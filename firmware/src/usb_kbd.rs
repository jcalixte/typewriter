//! USB host HID boot keyboard → key-event queue.
//!
//! Drives the ESP-IDF USB Host Library directly through the raw `esp-idf-sys`
//! bindings (the convenience HID class driver is a managed component that isn't
//! vendored in mainline, and a boot keyboard doesn't need it). `start()`
//! installs the host stack, spawns the daemon + client event pumps on their own
//! threads, and returns immediately; decoded key-down events are pushed onto a
//! queue the caller drains with `next_key()`. This keeps the USB pumps off the
//! main thread so the main thread can own the e-paper panel.
//!
//! On attach it opens the device, dumps its descriptors, claims the boot
//! keyboard interface (interface 0 / EP 0x81 / 8-byte reports, confirmed by
//! Spike 4's enumeration of VID:PID 19f5:3255), switches it to boot protocol,
//! and polls the interrupt-IN endpoint. Each report is edge-detected against
//! the previous one so a held key yields a single key-down, then translated
//! through a US QWERTY layout.
//!
//! Logging goes over the CP2102 UART bridge (console = UART0), independent of
//! the USB PHY, so the host library and the serial monitor coexist.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use esp_idf_svc::sys::esp;
use esp_idf_svc::sys::{
    usb_config_desc_t, usb_device_desc_t, usb_device_handle_t, usb_host_client_config_t,
    usb_host_client_event_msg_t, usb_host_client_event_t_USB_HOST_CLIENT_EVENT_DEV_GONE,
    usb_host_client_event_t_USB_HOST_CLIENT_EVENT_NEW_DEV, usb_host_client_handle_events,
    usb_host_client_handle_t, usb_host_client_register, usb_host_config_t, usb_host_device_close,
    usb_host_device_open, usb_host_get_active_config_descriptor, usb_host_get_device_descriptor,
    usb_host_install, usb_host_interface_claim, usb_host_interface_release,
    usb_host_lib_handle_events, usb_host_transfer_alloc, usb_host_transfer_free,
    usb_host_transfer_submit, usb_host_transfer_submit_control, usb_print_config_descriptor,
    usb_print_device_descriptor, usb_transfer_status_t_USB_TRANSFER_STATUS_COMPLETED,
    usb_transfer_t, EspError, ESP_INTR_FLAG_LEVEL1,
};

/// A decoded key-down event, re-exported from the `keymap` crate. The decode
/// logic (edge detection + US-QWERTY translation) lives there — pure, with no
/// esp/std deps — so it is host-testable and fuzzable off the xtensa target.
/// This module only bridges it to the USB transport. See MEMORY_AUDIT.md.
pub use keymap::Key;

/// Boot-keyboard parameters, confirmed by Spike 4's enumeration.
const KBD_INTERFACE: u8 = 0;
const KBD_ALT_SETTING: u8 = 0;
const KBD_EP_IN: u8 = 0x81;
const BOOT_REPORT_LEN: usize = 8;

/// HID class control requests. bmRequestType 0x21 = host→device | class |
/// interface recipient; wIndex (byte 4) = the interface number.
const SET_PROTOCOL_BOOT: [u8; 8] = [0x21, 0x0b, 0x00, 0x00, KBD_INTERFACE, 0x00, 0x00, 0x00];
const SET_IDLE_INFINITE: [u8; 8] = [0x21, 0x0a, 0x00, 0x00, KBD_INTERFACE, 0x00, 0x00, 0x00];

/// Address of a freshly-attached device, published by the client event
/// callback and consumed by the client loop. 0 means "nothing pending".
static NEW_DEV_ADDR: AtomicU8 = AtomicU8::new(0);
/// Set when the open device is unplugged.
static DEV_GONE: AtomicBool = AtomicBool::new(false);
/// Whether a keyboard is currently open (attached + set up). Unlike `DEV_GONE`
/// (a one-shot detach event the client loop consumes), this is the persistent
/// connection state the side-panel disconnect flag reads via `keyboard_present`.
static KBD_PRESENT: AtomicBool = AtomicBool::new(false);
/// Control-transfer completion, published by `ctrl_cb` to the setup routine.
static CTRL_DONE: AtomicBool = AtomicBool::new(false);
static CTRL_STATUS: AtomicU32 = AtomicU32::new(0);

/// Queue of decoded key-down events, drained by the main thread. A plain
/// mutex-guarded queue rather than a channel because `mpsc::Sender` is not
/// `Sync` and so can't live in a `static`.
static KEY_QUEUE: OnceLock<Mutex<VecDeque<Key>>> = OnceLock::new();
/// Edge-detecting decode state (previous report + Caps dual-role), owned here
/// as one `keymap::Decoder` rather than the pair of loose statics it used to
/// be. Only ever touched from the single client thread's `report_cb`; the mutex
/// is for the `static`, not contention. The decode logic itself is the
/// host-tested `keymap` crate.
static DECODER: Mutex<keymap::Decoder> = Mutex::new(keymap::Decoder::new());
/// US-International dead-key accent composition, downstream of the decoder: it
/// folds a dead key (`'` `` ` `` `^` `"` `~`) plus the next letter into one
/// accented `Key::Char`. Like `DECODER`, only ever touched from the client
/// thread. Safe to feed the editor only because its buffer is UTF-8-correct —
/// this emits non-ASCII Latin-9 characters. Host-tested in the `keymap` crate.
static COMPOSER: Mutex<keymap::Composer> = Mutex::new(keymap::Composer::new());
/// Whether the interrupt-IN report transfer is currently in-flight (submitted
/// and awaiting completion). Set on submit, cleared the moment `report_cb`
/// fires. Read on unplug to quiesce the transfer before freeing it — freeing an
/// in-flight transfer races the library's pending completion into a
/// use-after-free (MEMORY_AUDIT.md finding #1).
static REPORT_INFLIGHT: AtomicBool = AtomicBool::new(false);

/// Pop the next decoded key-down event, if any.
pub fn next_key() -> Option<Key> {
    KEY_QUEUE.get()?.lock().unwrap().pop_front()
}

/// Whether a USB keyboard is currently attached and set up. Read by the main
/// loop to drive the side-panel disconnect flag.
pub fn keyboard_present() -> bool {
    KBD_PRESENT.load(Ordering::SeqCst)
}

/// Install the USB Host Library and spawn the daemon + client event pumps.
/// Returns once the stack is up; key events then arrive via `next_key()`.
pub fn start() -> Result<(), EspError> {
    // Internal PHY (skip_phy_setup = false), root port powered on install,
    // default full-speed peripheral (BIT0 — the S3 has a single USB-OTG).
    let mut host_config: usb_host_config_t = unsafe { core::mem::zeroed() };
    host_config.intr_flags = ESP_INTR_FLAG_LEVEL1 as i32;
    host_config.peripheral_map = 1 << 0;
    esp!(unsafe { usb_host_install(&host_config) })?;
    log::info!("USB Host Library installed; waiting for a keyboard…");

    let _ = KEY_QUEUE.set(Mutex::new(VecDeque::new()));

    // The daemon pump services enumeration and root-port events. It must run
    // continuously or an attach never completes.
    std::thread::Builder::new()
        .stack_size(4096)
        .name("usb_host_daemon".into())
        .spawn(|| loop {
            let mut event_flags: u32 = 0;
            unsafe { usb_host_lib_handle_events(u32::MAX, &mut event_flags) };
        })
        .expect("spawn usb host daemon thread");

    // The client pump registers the client, handles attach/detach, and (via
    // report_cb, called from within its handle_events) decodes key events.
    std::thread::Builder::new()
        .stack_size(8192)
        .name("usb_client".into())
        .spawn(client_loop)
        .expect("spawn usb client thread");

    Ok(())
}

/// Client event pump: register the client and service device attach/detach
/// forever. Runs on its own thread.
fn client_loop() {
    let mut client_config: usb_host_client_config_t = unsafe { core::mem::zeroed() };
    client_config.max_num_event_msg = 5;
    client_config.__bindgen_anon_1.async_.client_event_callback = Some(client_event_cb);
    client_config.__bindgen_anon_1.async_.callback_arg = ptr::null_mut();
    let mut client: usb_host_client_handle_t = ptr::null_mut();
    let err = unsafe { usb_host_client_register(&client_config, &mut client) };
    if err != 0 {
        log::error!("usb_host_client_register failed: {err}");
        return;
    }

    let mut open_dev: usb_device_handle_t = ptr::null_mut();
    let mut report_xfer: *mut usb_transfer_t = ptr::null_mut();
    loop {
        // Blocks until a client event; the callbacks (attach/detach, control
        // completion, interrupt reports) all fire from within here.
        unsafe { usb_host_client_handle_events(client, u32::MAX) };

        let addr = NEW_DEV_ADDR.swap(0, Ordering::SeqCst);
        if addr != 0 {
            // A new attach while a device is still open means we missed the
            // detach event; tear the old one down first so its transfer and
            // handle aren't leaked and overwritten (MEMORY_AUDIT.md finding #3).
            if !open_dev.is_null() {
                log::warn!("new device while one is still open; closing the previous keyboard");
                close_device(client, &mut open_dev, &mut report_xfer);
            }
            match setup_keyboard(client, addr) {
                Ok((dev, xfer)) => {
                    open_dev = dev;
                    report_xfer = xfer;
                    KBD_PRESENT.store(true, Ordering::SeqCst);
                }
                Err(e) => log::error!("keyboard setup failed: {e:?}"),
            }
        }
        if DEV_GONE.swap(false, Ordering::SeqCst) && !open_dev.is_null() {
            log::info!("keyboard unplugged; releasing interface and closing");
            close_device(client, &mut open_dev, &mut report_xfer);
        }
    }
}

/// Tear down the open keyboard: quiesce + free the report transfer, release the
/// interface, close the device, then reset the decode + presence state. Order
/// per the USB Host Library: free transfers, release interfaces, then close.
///
/// The report transfer is freed only once it is no longer in-flight. On unplug
/// the library completes the pending interrupt transfer with a canceled status
/// and fires `report_cb` (which clears `REPORT_INFLIGHT`); we pump client events
/// until that happens so we never hand `usb_host_transfer_free` a transfer the
/// lower layer still owns — doing so would race the pending completion into a
/// use-after-free (MEMORY_AUDIT.md finding #1). Bounded so a wedged transfer
/// can't spin the client loop forever; if it never quiesces we leak it rather
/// than risk the free.
fn close_device(
    client: usb_host_client_handle_t,
    open_dev: &mut usb_device_handle_t,
    report_xfer: &mut *mut usb_transfer_t,
) {
    if !(*report_xfer).is_null() {
        let mut spins = 0;
        while REPORT_INFLIGHT.load(Ordering::SeqCst) && spins < 100 {
            unsafe { usb_host_client_handle_events(client, 10) };
            spins += 1;
        }
        if REPORT_INFLIGHT.load(Ordering::SeqCst) {
            log::error!(
                "report transfer still in-flight after drain; leaking it rather than \
                 freeing (a free here would be a use-after-free)"
            );
        } else {
            let err = unsafe { usb_host_transfer_free(*report_xfer) };
            if err != 0 {
                log::warn!("usb_host_transfer_free(report) returned {err}");
            }
        }
        *report_xfer = ptr::null_mut();
    }
    unsafe { usb_host_interface_release(client, *open_dev, KBD_INTERFACE) };
    unsafe { usb_host_device_close(client, *open_dev) };
    *open_dev = ptr::null_mut();
    DECODER.lock().unwrap().reset();
    COMPOSER.lock().unwrap().reset(); // drop any half-typed accent
    KBD_PRESENT.store(false, Ordering::SeqCst);
}

/// Client event callback — runs inside `usb_host_client_handle_events`. Keep
/// it minimal: stash what happened and let the client loop do the FFI work.
unsafe extern "C" fn client_event_cb(msg: *const usb_host_client_event_msg_t, _arg: *mut c_void) {
    let msg = unsafe { &*msg };
    #[allow(non_upper_case_globals)]
    match msg.event {
        usb_host_client_event_t_USB_HOST_CLIENT_EVENT_NEW_DEV => {
            let addr = unsafe { msg.__bindgen_anon_1.new_dev.address };
            NEW_DEV_ADDR.store(addr, Ordering::SeqCst);
        }
        usb_host_client_event_t_USB_HOST_CLIENT_EVENT_DEV_GONE => {
            DEV_GONE.store(true, Ordering::SeqCst);
        }
        _ => {}
    }
}

/// Control-transfer completion callback. Publishes status to the setup routine
/// waiting in `control_request`.
unsafe extern "C" fn ctrl_cb(transfer: *mut usb_transfer_t) {
    let status = unsafe { (*transfer).status };
    CTRL_STATUS.store(status as u32, Ordering::SeqCst);
    CTRL_DONE.store(true, Ordering::SeqCst);
}

/// Interrupt-IN completion callback: decode the boot report into key-down
/// events and resubmit to keep polling. Runs inside the client loop's
/// `usb_host_client_handle_events`. On any non-completed status (e.g. the
/// device was unplugged and the transfer canceled) it stops resubmitting.
unsafe extern "C" fn report_cb(transfer: *mut usb_transfer_t) {
    // A completion fired, so the transfer is no longer in-flight. Clear the flag
    // first — the non-completed (canceled-on-unplug) path below returns without
    // resubmitting, and leaving it false is what lets close_device free the
    // transfer safely (MEMORY_AUDIT.md finding #1).
    REPORT_INFLIGHT.store(false, Ordering::SeqCst);
    let t = unsafe { &mut *transfer };
    if t.status == usb_transfer_status_t_USB_TRANSFER_STATUS_COMPLETED {
        let n = (t.actual_num_bytes as usize).min(BOOT_REPORT_LEN);
        // SAFETY: data_buffer was allocated with BOOT_REPORT_LEN bytes and `n`
        // is clamped to that, so the slice stays within the allocation even if
        // the device reports a bogus actual_num_bytes.
        let report = unsafe { core::slice::from_raw_parts(t.data_buffer, n) };
        // Decode HID → keys, then fold dead-key accents before enqueuing.
        DECODER
            .lock()
            .unwrap()
            .feed(report, |k| COMPOSER.lock().unwrap().feed(k, enqueue));
        let err = unsafe { usb_host_transfer_submit(transfer) };
        if err != 0 {
            log::error!("interrupt resubmit failed: {err}");
        } else {
            REPORT_INFLIGHT.store(true, Ordering::SeqCst);
        }
    } else {
        log::info!("interrupt transfer stopped, status {}", t.status as u32);
    }
}

/// Log and enqueue a decoded key event for the main thread to drain.
fn enqueue(key: Key) {
    log::info!("key: {key:?}");
    if let Some(q) = KEY_QUEUE.get() {
        q.lock().unwrap().push_back(key);
    }
}

/// Open a newly-attached device, dump its descriptors, claim the keyboard
/// interface, put it in boot protocol, and start polling for reports.
fn setup_keyboard(
    client: usb_host_client_handle_t,
    addr: u8,
) -> Result<(usb_device_handle_t, *mut usb_transfer_t), EspError> {
    let dev = open_and_dump(client, addr)?;

    log::info!("claiming interface {KBD_INTERFACE}");
    esp!(unsafe { usb_host_interface_claim(client, dev, KBD_INTERFACE, KBD_ALT_SETTING) })?;

    // Boot protocol gives the fixed 8-byte report; SET_IDLE(0) means "report
    // only on change" (no auto-repeat spam). A keyboard may STALL either — we
    // log and continue, since interface 0 reports boot format regardless.
    control_request(client, dev, &SET_PROTOCOL_BOOT, "SET_PROTOCOL(boot)")?;
    control_request(client, dev, &SET_IDLE_INFINITE, "SET_IDLE(0)")?;

    let xfer = start_report_polling(dev)?;
    log::info!("polling EP {KBD_EP_IN:#04x} — keyboard ready");
    Ok((dev, xfer))
}

/// Open a device and print its descriptors over the console.
fn open_and_dump(
    client: usb_host_client_handle_t,
    addr: u8,
) -> Result<usb_device_handle_t, EspError> {
    log::info!("keyboard attached at address {addr}; opening");
    let mut dev: usb_device_handle_t = ptr::null_mut();
    esp!(unsafe { usb_host_device_open(client, addr, &mut dev) })?;

    // usb_device_desc_t is a union { #[repr(C, packed)] struct; [u8; 18] }.
    // Copy the struct out and then each field into aligned locals — packed
    // fields can't be referenced (and the format machinery takes references).
    let mut dev_desc: *const usb_device_desc_t = ptr::null();
    esp!(unsafe { usb_host_get_device_descriptor(dev, &mut dev_desc) })?;
    let d = unsafe { (*dev_desc).__bindgen_anon_1 };
    let (vid, pid, class, sub, proto, ncfg) = (
        d.idVendor,
        d.idProduct,
        d.bDeviceClass,
        d.bDeviceSubClass,
        d.bDeviceProtocol,
        d.bNumConfigurations,
    );
    log::info!(
        "VID:PID {vid:04x}:{pid:04x}  class {class:02x}/{sub:02x}/{proto:02x}  {ncfg} configuration(s)"
    );
    unsafe { usb_print_device_descriptor(dev_desc) };

    let mut cfg_desc: *const usb_config_desc_t = ptr::null();
    esp!(unsafe { usb_host_get_active_config_descriptor(dev, &mut cfg_desc) })?;
    unsafe { usb_print_config_descriptor(cfg_desc, None) };

    Ok(dev)
}

/// Send an 8-byte control request (setup packet, no data stage) and block
/// until it completes, pumping client events so the callback can fire.
fn control_request(
    client: usb_host_client_handle_t,
    dev: usb_device_handle_t,
    setup: &[u8; 8],
    label: &str,
) -> Result<(), EspError> {
    let mut xfer: *mut usb_transfer_t = ptr::null_mut();
    esp!(unsafe { usb_host_transfer_alloc(64, 0, &mut xfer) })?;
    unsafe {
        let t = &mut *xfer;
        // First 8 bytes of a control transfer's buffer are the setup packet.
        core::ptr::copy_nonoverlapping(setup.as_ptr(), t.data_buffer, 8);
        t.num_bytes = 8; // setup packet only, no data stage
        t.device_handle = dev;
        t.bEndpointAddress = 0; // control endpoint EP0
        t.callback = Some(ctrl_cb);
        t.context = ptr::null_mut();
    }

    CTRL_DONE.store(false, Ordering::SeqCst);
    if let Err(e) = esp!(unsafe { usb_host_transfer_submit_control(client, xfer) }) {
        // Free the transfer we allocated before bailing, or it leaks
        // (MEMORY_AUDIT.md finding #3).
        unsafe { usb_host_transfer_free(xfer) };
        return Err(e);
    }
    while !CTRL_DONE.load(Ordering::SeqCst) {
        unsafe { usb_host_client_handle_events(client, u32::MAX) };
    }

    let status = CTRL_STATUS.load(Ordering::SeqCst);
    unsafe { usb_host_transfer_free(xfer) };
    if status == usb_transfer_status_t_USB_TRANSFER_STATUS_COMPLETED as u32 {
        log::info!("{label} ok");
    } else {
        log::warn!("{label} completed with status {status} (continuing)");
    }
    Ok(())
}

/// Allocate and submit the interrupt-IN transfer for boot reports. The
/// `report_cb` resubmits it on each completion to keep polling.
fn start_report_polling(dev: usb_device_handle_t) -> Result<*mut usb_transfer_t, EspError> {
    let mut xfer: *mut usb_transfer_t = ptr::null_mut();
    esp!(unsafe { usb_host_transfer_alloc(BOOT_REPORT_LEN, 0, &mut xfer) })?;
    unsafe {
        let t = &mut *xfer;
        t.num_bytes = BOOT_REPORT_LEN as i32; // must be a multiple of wMaxPacketSize (8)
        t.device_handle = dev;
        t.bEndpointAddress = KBD_EP_IN;
        t.callback = Some(report_cb);
        t.context = ptr::null_mut();
    }
    if let Err(e) = esp!(unsafe { usb_host_transfer_submit(xfer) }) {
        // Free the transfer we allocated before bailing, or it leaks
        // (MEMORY_AUDIT.md finding #3).
        unsafe { usb_host_transfer_free(xfer) };
        return Err(e);
    }
    REPORT_INFLIGHT.store(true, Ordering::SeqCst);
    Ok(xfer)
}
