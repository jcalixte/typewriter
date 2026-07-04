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

/// A decoded key-down event. Beyond plain characters, the decoder recognises a
/// few editing combos (resolved here so the main loop only sees intents) and a
/// dual-role Caps Lock: held it acts as Ctrl, tapped it emits `Escape`.
#[derive(Debug, Clone, Copy)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    /// Ctrl+Backspace or Ctrl+W — delete the word before the caret.
    DeleteWord,
    /// Cmd/GUI+Backspace — delete back to the start of the current line.
    DeleteLine,
    /// Caps Lock tapped on its own. A no-op for now; groundwork for a future
    /// vim-style normal mode.
    Escape,
}

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
/// Control-transfer completion, published by `ctrl_cb` to the setup routine.
static CTRL_DONE: AtomicBool = AtomicBool::new(false);
static CTRL_STATUS: AtomicU32 = AtomicU32::new(0);

/// Queue of decoded key-down events, drained by the main thread. A plain
/// mutex-guarded queue rather than a channel because `mpsc::Sender` is not
/// `Sync` and so can't live in a `static`.
static KEY_QUEUE: OnceLock<Mutex<VecDeque<Key>>> = OnceLock::new();
/// Keycodes held in the previous report, for key-down edge detection. Only
/// ever touched from the single client thread's `report_cb`.
static PREV_KEYS: Mutex<[u8; 6]> = Mutex::new([0; 6]);
/// Caps Lock dual-role tracking: set while Caps is held once any other key is
/// pressed, so releasing Caps only emits `Escape` on a clean tap. Only touched
/// from the client thread's `report_cb`.
static CAPS_USED: AtomicBool = AtomicBool::new(false);

/// Pop the next decoded key-down event, if any.
pub fn next_key() -> Option<Key> {
    KEY_QUEUE.get()?.lock().unwrap().pop_front()
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
            match setup_keyboard(client, addr) {
                Ok((dev, xfer)) => {
                    open_dev = dev;
                    report_xfer = xfer;
                }
                Err(e) => log::error!("keyboard setup failed: {e:?}"),
            }
        }
        if DEV_GONE.swap(false, Ordering::SeqCst) && !open_dev.is_null() {
            log::info!("keyboard unplugged; releasing interface and closing");
            // Order per the USB Host Library: free transfers, release
            // interfaces, then close the device.
            if !report_xfer.is_null() {
                unsafe { usb_host_transfer_free(report_xfer) };
                report_xfer = ptr::null_mut();
            }
            unsafe { usb_host_interface_release(client, open_dev, KBD_INTERFACE) };
            unsafe { usb_host_device_close(client, open_dev) };
            open_dev = ptr::null_mut();
            *PREV_KEYS.lock().unwrap() = [0; 6];
        }
    }
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
    let t = unsafe { &mut *transfer };
    if t.status == usb_transfer_status_t_USB_TRANSFER_STATUS_COMPLETED {
        let n = (t.actual_num_bytes as usize).min(BOOT_REPORT_LEN);
        let report = unsafe { core::slice::from_raw_parts(t.data_buffer, n) };
        handle_report(report);
        let err = unsafe { usb_host_transfer_submit(transfer) };
        if err != 0 {
            log::error!("interrupt resubmit failed: {err}");
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

/// Caps Lock usage ID — repurposed as a dual-role Ctrl/Escape key.
const CAPS: u8 = 0x39;

/// Edge-detect key-downs in an 8-byte boot report and enqueue translated keys.
/// Layout: [modifiers, reserved, key1..key6]; 0 means "no key".
fn handle_report(report: &[u8]) {
    if report.len() < 3 {
        return;
    }
    let mods = report[0];
    let shift = mods & 0x22 != 0; // LShift 0x02 | RShift 0x20
    let cmd = mods & 0x88 != 0; // LGUI 0x08 | RGUI 0x80
    let current = &report[2..];

    let mut prev = PREV_KEYS.lock().unwrap();

    // Caps Lock is a normal key in the boot report (not a modifier bit), so we
    // track its down/up edges here. Held, it acts as Ctrl; tapped alone, it
    // emits Escape.
    let caps_now = current.contains(&CAPS);
    let caps_before = prev.contains(&CAPS);
    let ctrl = mods & 0x11 != 0 || caps_now; // LCtrl 0x01 | RCtrl 0x10, or Caps
    // Any other key down while Caps is held means it was used as Ctrl — so its
    // release must not fire Escape.
    if caps_now && current.iter().any(|&k| k != 0 && k != CAPS) {
        CAPS_USED.store(true, Ordering::SeqCst);
    }

    for &k in current {
        if k == 0 || k == CAPS || prev.contains(&k) {
            continue; // empty slot, the Caps key itself, or already held
        }
        if let Some(key) = translate(k, shift, ctrl, cmd) {
            enqueue(key);
        }
    }

    // Caps released as a clean tap (nothing else pressed while it was down) →
    // Escape. Reset the used-flag on both the press and release edges.
    if caps_before && !caps_now {
        if !CAPS_USED.swap(false, Ordering::SeqCst) {
            enqueue(Key::Escape);
        }
    } else if caps_now && !caps_before {
        CAPS_USED.store(false, Ordering::SeqCst);
    }

    let mut next = [0u8; 6];
    for (slot, &k) in next.iter_mut().zip(current.iter()) {
        *slot = k;
    }
    *prev = next;
}

/// Translate a HID keyboard usage ID to a key event using a US QWERTY layout.
/// Editing combos (Ctrl/Cmd chords) resolve to intents here and take priority
/// over character insertion; other keys with Ctrl or Cmd held are swallowed.
fn translate(usage: u8, shift: bool, ctrl: bool, cmd: bool) -> Option<Key> {
    match usage {
        0x2a => {
            // Backspace: Cmd = delete line, Ctrl = delete word, else one char.
            return Some(if cmd {
                Key::DeleteLine
            } else if ctrl {
                Key::DeleteWord
            } else {
                Key::Backspace
            });
        }
        0x1a if ctrl => return Some(Key::DeleteWord), // Ctrl+W, readline-style
        _ => {}
    }

    // With Ctrl or Cmd held and no combo matched above, insert nothing — so
    // Caps+J or Cmd+S don't type a stray character.
    if ctrl || cmd {
        return None;
    }

    let key = match usage {
        0x04..=0x1d => {
            let base = b'a' + (usage - 0x04);
            Key::Char(if shift { base.to_ascii_uppercase() } else { base } as char)
        }
        0x1e..=0x27 => {
            const UNSHIFTED: [char; 10] = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];
            const SHIFTED: [char; 10] = ['!', '@', '#', '$', '%', '^', '&', '*', '(', ')'];
            let i = (usage - 0x1e) as usize;
            Key::Char(if shift { SHIFTED[i] } else { UNSHIFTED[i] })
        }
        0x28 => Key::Enter,
        0x2a => Key::Backspace,
        0x2b => Key::Char('\t'),
        0x2c => Key::Char(' '),
        0x2d => Key::Char(if shift { '_' } else { '-' }),
        0x2e => Key::Char(if shift { '+' } else { '=' }),
        0x2f => Key::Char(if shift { '{' } else { '[' }),
        0x30 => Key::Char(if shift { '}' } else { ']' }),
        0x31 => Key::Char(if shift { '|' } else { '\\' }),
        0x33 => Key::Char(if shift { ':' } else { ';' }),
        0x34 => Key::Char(if shift { '"' } else { '\'' }),
        0x35 => Key::Char(if shift { '~' } else { '`' }),
        0x36 => Key::Char(if shift { '<' } else { ',' }),
        0x37 => Key::Char(if shift { '>' } else { '.' }),
        0x38 => Key::Char(if shift { '?' } else { '/' }),
        _ => return None,
    };
    Some(key)
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
    esp!(unsafe { usb_host_transfer_submit_control(client, xfer) })?;
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
    esp!(unsafe { usb_host_transfer_submit(xfer) })?;
    Ok(xfer)
}
