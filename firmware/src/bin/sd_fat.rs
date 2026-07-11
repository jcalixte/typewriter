//! Spike 3 — SD card (FAT) over the EPD's shared SPI2 bus.
//!
//! A small standalone bench program (separate binary from the editor firmware)
//! that proves the storage stack the persistence module will sit on:
//!
//!   1. Bring up SPI2 with the SD's four lines. Three are shared with the EPD
//!      (SCK 12, MOSI 11) plus a MISO line (13) the write-only EPD never used,
//!      and the SD gets its own chip-select (10); the EPD's CS is 7.
//!   2. Mount a FAT filesystem on the card at `/sd` via `esp_vfs_fat_sdspi_mount`.
//!   3. Exercise the exact atomic-save pattern the persistence module specifies
//!      (ADR-007): write `*.tmp`, fsync, rename over the target, then read back
//!      and byte-compare. Report the card's negotiated clock and FAT usage.
//!
//! Why SD-only (no EPD in the same pass): the EPD driver uses esp-idf-hal's
//! `SpiBusDriver`, whose constructor calls `spi_device_acquire_bus(BLOCK)` and
//! holds that *exclusive* bus lock for the driver's whole lifetime (it needs CS
//! held across a cmd→data sequence while DC toggles). While that lock is held,
//! any other device on SPI2 — i.e. the SD — blocks. So the EPD and an arbitrated
//! SD device can't both be live on one host as things stand; proving the SD
//! stack + wiring first is the useful de-risking step. The shared-bus
//! arbitration question (release/re-acquire around EPD ops, or give the SD its
//! own SPI3 — the risk-table fallback) is what this spike hands data to.
//!
//! Two esp-idf notes baked in below:
//!   - The `SDSPI_HOST_DEFAULT()` / `SDSPI_DEVICE_CONFIG_DEFAULT()` C macros are
//!     dropped by bindgen, so the descriptors are filled by hand. The
//!     `SDMMC_HOST_FLAG_*` values are `BIT(n)` macros bindgen can't fold either,
//!     so they're inlined with a reference to sd_protocol_types.h.
//!   - The `.tmp` rename target (`notes.md.tmp`) is not a valid 8.3 name, and
//!     FatFS defaults to 8.3-only. `CONFIG_FATFS_LFN_HEAP=y` (sdkconfig.defaults)
//!     turns on long filenames — required here and by the real persistence path.
//!
//! Flash with `just flash-sd`. Needs no `.env` (unlike the Wi-Fi spike).

use std::ffi::CStr;
use std::fs;
use std::io::{Read, Write};
use std::mem::MaybeUninit;
use std::ptr;

use anyhow::{bail, Context, Result};
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::sys::{self, esp};

/// Injected by build.rs so serial output identifies the exact build.
const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

// SPI2 wiring. SCK/MOSI are shared with the EPD (epd.rs: SCK 12, MOSI 11); the
// SD adds MISO 13 (EPD is write-only, never wired it) and its own CS 10.
const PIN_SCK: i32 = 12;
const PIN_MOSI: i32 = 11;
const PIN_MISO: i32 = 13;
const PIN_CS: i32 = 10;

/// The EPD's chip-select (epd.rs). It sits on this same bus; we don't drive the
/// panel here, so we pin it HIGH (deselected) instead of leaving it floating.
const EPD_CS: i32 = 7;

/// SD clock. Deliberately conservative: the EPD was validated at 4 MHz on these
/// bench jumper wires, and SDSPI's 20 MHz default is prone to CRC errors on the
/// same wiring (stub reflections off the EPD's MOSI/SCK taps) — which would look
/// like a stack failure when it's really signal integrity. 10 MHz keeps margin
/// while staying a real speed; raise toward 20 MHz once on a clean PCB.
const SD_FREQ_KHZ: i32 = 10_000;

/// Host flags from sd_protocol_types.h — `BIT(3)` / `BIT(5)`. Inlined because
/// bindgen doesn't fold the nested `BIT()` macro into a constant.
const SDMMC_HOST_FLAG_SPI: u32 = 1 << 3;
const SDMMC_HOST_FLAG_DEINIT_ARG: u32 = 1 << 5;

/// VFS mount point. `MOUNT` is the C string handed to esp-idf; `MOUNT_STR` is
/// the same path for std::fs.
const MOUNT: &CStr = c"/sd";
const MOUNT_STR: &str = "/sd";

fn main() -> Result<()> {
    // Required once before any esp-idf-svc call; some runtime patches only link
    // if this symbol is referenced. See esp-idf-template#71.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Typoena — Spike 3 (SD/FAT on shared SPI2), {BUILD_TAG}");

    match run() {
        Ok(()) => {
            log::info!("✅ Spike 3 complete — mount + atomic write/fsync/rename/read-back on shared bus")
        }
        Err(e) => log::error!("❌ Spike 3 failed: {e:?}"),
    }

    // Idle instead of returning, so the result stays on the monitor.
    loop {
        FreeRtos::delay_ms(1000);
    }
}

fn run() -> Result<()> {
    let card = mount_sd().context("mounting SD over SPI2")?;

    // SAFETY: `card` is a live handle returned by a successful mount.
    let (max_khz, real_khz) = unsafe { ((*card).max_freq_khz, (*card).real_freq_khz) };
    log::info!("card mounted at /sd — max {max_khz} kHz, negotiated {real_khz} kHz");

    let (total, free) = fs_info().context("reading FAT usage")?;
    log::info!(
        "FAT usage — {} MiB total, {} MiB free",
        total / (1024 * 1024),
        free / (1024 * 1024)
    );

    file_roundtrip().context("atomic write/fsync/rename/read-back")?;
    list_root(); // best-effort, informational
    Ok(())
}

/// Init the shared SPI2 bus and mount the card. Returns the card handle (kept
/// alive for the program's lifetime; the spike never unmounts).
fn mount_sd() -> Result<*mut sys::sdmmc_card_t> {
    // 0) Deselect the EPD. It shares SCK/MOSI; its CS is GPIO 7 and the panel is
    //    write-only (can't contend on MISO), but a floating CS while we clock the
    //    shared lines is a variable worth removing. Pin it HIGH.
    esp!(unsafe { sys::gpio_reset_pin(EPD_CS) }).context("reset EPD CS")?;
    esp!(unsafe { sys::gpio_set_direction(EPD_CS, sys::gpio_mode_t_GPIO_MODE_OUTPUT) })
        .context("EPD CS as output")?;
    esp!(unsafe { sys::gpio_set_level(EPD_CS, 1) }).context("EPD CS high")?;

    // 1) Initialize SPI2 with the SD's lines. This is the bus the EPD also uses;
    //    the difference vs. epd.rs's init is the added MISO line (SD data-out).
    // SAFETY: zeroed spi_bus_config_t is valid (all pins default 0); we then set
    // the used pins and mark the quad lines unused (-1).
    let mut bus: sys::spi_bus_config_t = unsafe { MaybeUninit::zeroed().assume_init() };
    bus.__bindgen_anon_1.mosi_io_num = PIN_MOSI;
    bus.__bindgen_anon_2.miso_io_num = PIN_MISO;
    bus.sclk_io_num = PIN_SCK;
    bus.__bindgen_anon_3.quadwp_io_num = -1;
    bus.__bindgen_anon_4.quadhd_io_num = -1;
    bus.max_transfer_sz = 4096;
    esp!(unsafe {
        sys::spi_bus_initialize(
            sys::spi_host_device_t_SPI2_HOST,
            &bus,
            sys::spi_common_dma_t_SPI_DMA_CH_AUTO as _,
        )
    })
    .context("spi_bus_initialize(SPI2)")?;

    // 1b) Enable internal pull-ups on the SD lines. The SD spec wants ~10 kΩ
    //     pull-ups on the data lines; the bench jumpers have none, so MISO
    //     floats between response bytes and a stray bit reads back as a spurious
    //     R1 "illegal command" (ESP_ERR_NOT_SUPPORTED) that fails init. The
    //     ESP32's internal ~45 kΩ pull-ups are usually enough on short wires;
    //     an external 10 kΩ MISO→3V3 is the proper fix on a real board. Set
    //     after bus init so the SPI pin config doesn't clobber it (CS gets
    //     reconfigured by the mount below — harmless; MISO is the one that
    //     matters).
    for pin in [PIN_SCK, PIN_MOSI, PIN_MISO, PIN_CS] {
        esp!(unsafe { sys::gpio_set_pull_mode(pin, sys::gpio_pull_mode_t_GPIO_PULLUP_ONLY) })
            .with_context(|| format!("pull-up on GPIO {pin}"))?;
    }

    // 2) SDSPI host descriptor — hand-rolled SDSPI_HOST_DEFAULT(). The function
    //    pointers are esp-idf's sdspi_host_* ops; the driver calls them to drive
    //    the card. `slot` picks the SPI host the device attaches to.
    // SAFETY: zeroed is a valid starting point (all fn-pointer Options = None);
    // we fill exactly the fields the C macro sets.
    let mut host: sys::sdmmc_host_t = unsafe { MaybeUninit::zeroed().assume_init() };
    host.flags = SDMMC_HOST_FLAG_SPI | SDMMC_HOST_FLAG_DEINIT_ARG;
    host.slot = sys::spi_host_device_t_SPI2_HOST as i32;
    host.max_freq_khz = SD_FREQ_KHZ;
    host.io_voltage = 3.3;
    host.driver_strength = sys::sdmmc_driver_strength_t_SDMMC_DRIVER_STRENGTH_B;
    host.current_limit = sys::sdmmc_current_limit_t_SDMMC_CURRENT_LIMIT_200MA;
    host.init = Some(sys::sdspi_host_init);
    host.set_card_clk = Some(sys::sdspi_host_set_card_clk);
    host.do_transaction = Some(sys::sdspi_host_do_transaction);
    host.__bindgen_anon_1.deinit_p = Some(sys::sdspi_host_remove_device);
    host.io_int_enable = Some(sys::sdspi_host_io_int_enable);
    host.io_int_wait = Some(sys::sdspi_host_io_int_wait);
    host.get_real_freq = Some(sys::sdspi_host_get_real_freq);
    host.input_delay_phase = sys::sdmmc_delay_phase_t_SDMMC_DELAY_PHASE_0;
    host.check_buffer_alignment = Some(sys::sdspi_host_check_buffer_alignment);

    // 3) Device (slot) config — CS 10, no card-detect / write-protect / SDIO int.
    // SAFETY: zeroed is valid; we set the host, CS, and mark the rest unused.
    let mut slot: sys::sdspi_device_config_t = unsafe { MaybeUninit::zeroed().assume_init() };
    slot.host_id = sys::spi_host_device_t_SPI2_HOST;
    slot.gpio_cs = PIN_CS;
    slot.gpio_cd = -1;
    slot.gpio_wp = -1;
    slot.gpio_int = -1;

    // 4) Mount config. format_if_mount_failed = true here (spike only): a fresh
    //    bench card that's exFAT or unformatted gets reformatted to FAT on the
    //    device instead of failing, so no Mac-side prep is needed. This fires
    //    only on a *filesystem* mount failure, not on the earlier CMD59 protocol
    //    rejection (that still bails with the actionable message below).
    //    The real persistence module MUST keep this false — it must never wipe
    //    the user's card on a transient mount hiccup. allocation_unit_size is
    //    used when formatting, so the 16 KiB below now applies.
    let mount = sys::esp_vfs_fat_mount_config_t {
        format_if_mount_failed: true,
        max_files: 4,
        allocation_unit_size: 16 * 1024,
        disk_status_check_enable: false,
        use_one_fat: false,
    };

    let mut card: *mut sys::sdmmc_card_t = ptr::null_mut();
    let rc = unsafe {
        sys::esp_vfs_fat_sdspi_mount(MOUNT.as_ptr(), &host, &slot, &mount, &mut card)
    };

    // Turn the driver's opaque error into something actionable. The one we hit
    // in practice is a card that rejects CMD59 (SPI-mode CRC on/off): init gets
    // through CMD0/CMD8 cleanly, then the CRC-enable step returns NOT_SUPPORTED.
    // That's a card-firmware limitation (common on large/counterfeit SDXC), not
    // a wiring fault — and we deliberately keep CRC required rather than run the
    // user's notes over an unchecked bus, so we reject the card with guidance.
    if rc == sys::ESP_ERR_NOT_SUPPORTED {
        bail!(
            "SD card rejected CMD59 (SPI-mode CRC). CMD0/CMD8 succeeded, so wiring is \
             fine — this card's firmware just doesn't support CRC in SPI mode (common on \
             large/counterfeit SDXC). Use a genuine card, ideally ≤32 GB. We keep CRC \
             required on purpose: a writing device shouldn't run over an unchecked bus."
        );
    }
    esp!(rc).context("esp_vfs_fat_sdspi_mount (card present? inserted? FAT-formatted?)")?;
    Ok(card)
}

/// The persistence module's atomic save (ADR-007), proven end to end: write to
/// a temp file, fsync, rename over the target, then reopen and byte-compare.
fn file_roundtrip() -> Result<()> {
    let path = format!("{MOUNT_STR}/spike3.md");
    let tmp = format!("{path}.tmp"); // two dots → exercises long-filename support
    let payload =
        format!("typoena spike 3\n{BUILD_TAG}\nshared SPI2: SCK12 MOSI11 MISO13, SD CS10\n");

    {
        let mut f = fs::File::create(&tmp).context("create tmp")?;
        f.write_all(payload.as_bytes()).context("write tmp")?;
        f.sync_all().context("fsync tmp")?; // FatFS f_sync — flush before rename
    }
    // FatFS's f_rename — unlike POSIX rename(2) — refuses to overwrite an
    // existing destination and returns FR_EXIST (EEXIST). So the classic
    // write-tmp → rename-over-target idiom needs an explicit unlink of the
    // target first on FAT. That opens a crash window: the target is briefly
    // gone while `tmp` holds the complete, fsync'd new content. The real
    // persistence module must pair this with boot recovery — a lingering
    // `*.tmp` means the last save didn't finish and should be promoted. See
    // ADR-007. (Tolerate a missing target so the first save works too.)
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).context("remove existing target before rename"),
    }
    fs::rename(&tmp, &path).context("rename tmp -> final")?;

    let mut back = String::new();
    fs::File::open(&path)
        .context("reopen final")?
        .read_to_string(&mut back)
        .context("read back")?;

    if back != payload {
        bail!(
            "read-back mismatch: wrote {} bytes, read {} bytes",
            payload.len(),
            back.len()
        );
    }
    log::info!(
        "round-trip OK — {} bytes: create {tmp} → fsync → rename {path} → read back identical",
        payload.len()
    );
    Ok(())
}

/// FAT total/free bytes for the mount.
fn fs_info() -> Result<(u64, u64)> {
    let mut total: u64 = 0;
    let mut free: u64 = 0;
    esp!(unsafe { sys::esp_vfs_fat_info(MOUNT.as_ptr(), &mut total, &mut free) })
        .context("esp_vfs_fat_info")?;
    Ok((total, free))
}

/// Log the root directory (informational — shows the card's existing content
/// and confirms our file landed).
fn list_root() {
    match fs::read_dir(MOUNT_STR) {
        Ok(entries) => {
            log::info!("/sd contents:");
            for entry in entries.flatten() {
                let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
                log::info!("  {} ({len} B)", entry.file_name().to_string_lossy());
            }
        }
        Err(e) => log::warn!("could not list /sd: {e}"),
    }
}
