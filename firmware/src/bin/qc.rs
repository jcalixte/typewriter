//! Bench QC firmware — a go/no-go bring-up fixture for the hand-soldered carrier
//! PCB. It exercises every connection the ESP32-S3 can reach and reports
//! OK / NOK per subsystem, so a freshly-assembled board is validated (or its
//! bad joints located) in one flash. Full spec: `docs/bench-qc.md`.
//!
//! Principles (from the spec):
//! - **Run to completion** — every check is isolated; a NOK is recorded and the
//!   suite continues, so one flash yields the whole fault matrix.
//! - **Layered output** — serial (authoritative) + an EPD checklist mirror once
//!   the panel is proven. Visual checks are confirmed with the **BOOT button
//!   (GPIO0)**. (The WS2812 aggregate indicator is deferred — see the LED note.)
//!
//! Flash with `just qc`. Needs no `.env`, no `git` feature (light build).

use std::time::Instant;

use anyhow::Result;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{AnyIOPin, Input, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::spi::config::{Config, DriverConfig};
use esp_idf_svc::hal::spi::{Dma, SpiBusDriver, SpiDriver};
use esp_idf_svc::hal::units::FromValueType;
use esp_idf_svc::sys;

use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::iso_8859_15::FONT_10X20;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use display::{Frame, HEIGHT, WIDTH};
use firmware::drivers::keyboard_usb as usb_kbd;
use firmware::drivers::screen_epd::Epd;
use firmware::infrastructure::storage_sd::Storage;

// ─── PCB-specific config — fill from the schematic ───────────────────────────

/// Charger status pin (`CHRG`, open-drain, low = charging) if wired to a GPIO.
/// `None` → the charger is a manual multimeter check and its test reports SKIP.
/// The spec suggests GPIO21. See `docs/bench-qc.md` prerequisite #2.
const CHARGER_CHRG_PIN: Option<i32> = None;

/// GPIOs that should be electrically **isolated** on the PCB (no bus, no device)
/// — the short/open scan drives each and reads the rest for solder bridges.
/// Left EMPTY on purpose: driving a pin that is actually a device output would
/// cause bus contention. Fill from the schematic's expected-net table before
/// relying on scan #8. Never list a bus pin (EPD/SD/USB) here.
const SCAN_PINS: &[i32] = &[];

/// Seconds to wait for a USB keyboard to enumerate before calling #5 a NOK.
const KBD_ENUM_TIMEOUT_S: u64 = 15;
/// Seconds to wait for the operator's BOOT-button confirmation before leaving a
/// visual check unresolved (`??`, non-gating).
const CONFIRM_TIMEOUT_S: u64 = 30;

/// Injected by build.rs so serial output identifies the exact build.
const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

// ─── Verdict / report harness ────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Ok,
    Nok,
    Skip,
    /// Operator did not confirm a visual check within the timeout (non-gating).
    Unconfirmed,
}

impl Verdict {
    fn tag(self) -> &'static str {
        match self {
            Verdict::Ok => "[ OK ]",
            Verdict::Nok => "[NOK ]",
            Verdict::Skip => "[SKIP]",
            Verdict::Unconfirmed => "[ ?? ]",
        }
    }
}

/// One checklist line: subsystem name, verdict, and a short detail/diagnosis.
struct Row {
    name: &'static str,
    verdict: Verdict,
    detail: String,
}

#[derive(Default)]
struct Report {
    rows: Vec<Row>,
}

impl Report {
    fn add(&mut self, name: &'static str, verdict: Verdict, detail: impl Into<String>) {
        let row = Row { name, verdict, detail: detail.into() };
        log::info!("{} {:<14} {}", row.verdict.tag(), row.name, row.detail);
        self.rows.push(row);
    }

    /// Any hard failure? Unconfirmed visual checks do NOT count — they are
    /// non-gating, per the spec.
    fn any_nok(&self) -> bool {
        self.rows.iter().any(|r| r.verdict == Verdict::Nok)
    }

    fn print(&self) {
        log::info!("──────────────────────────── QC RESULTS ────────────────────────────");
        for r in &self.rows {
            log::info!("{} {:<14} {}", r.verdict.tag(), r.name, r.detail);
        }
        let n_nok = self.rows.iter().filter(|r| r.verdict == Verdict::Nok).count();
        if n_nok == 0 {
            log::info!("───────────────────────────── ALL OK ───────────────────────────────");
        } else {
            log::info!("──────────────────────── {n_nok} FAULT(S) — see NOK ─────────────────────");
        }
    }

    fn lines(&self) -> Vec<String> {
        self.rows
            .iter()
            .map(|r| format!("{} {} {}", r.verdict.tag(), r.name, r.detail))
            .collect()
    }
}

type BootButton<'d> = PinDriver<'d, Input>;

fn main() -> Result<()> {
    // Required once before any esp-idf-svc call (see esp-idf-template#71).
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Typoena — bench QC fixture, {BUILD_TAG}");

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;
    let mut report = Report::default();

    // BOOT button (GPIO0) — the operator-confirm input for visual checks.
    let mut boot: BootButton = PinDriver::input(pins.gpio0, Pull::Up)?;

    // ── #2/#3 EPD (SPI2, Spike 2 wiring). Kept alive for the results mirror. ──
    let spi = SpiDriver::new(
        peripherals.spi2,
        pins.gpio12,                               // SCK
        pins.gpio11,                               // MOSI
        None::<AnyIOPin>,                          // no MISO — panel is write-only
        &DriverConfig::new().dma(Dma::Auto(4096)),
    )?;
    let bus = SpiBusDriver::new(spi, &Config::new().baudrate(20.MHz().into()))?;
    let mut epd = Epd::new(
        bus,
        PinDriver::output(pins.gpio6)?,            // DC
        PinDriver::output(pins.gpio5)?,            // RST
        PinDriver::output(pins.gpio7)?,            // CS
        PinDriver::input(pins.gpio4, Pull::Down)?, // BUSY
    );
    check_epd(&mut epd, &mut report, &mut boot);

    // ── #1 LED ────────────────────────────────────────────────────────────────
    // WS2812 on GPIO48. Deferred: esp-idf-hal 0.46 replaced the RMT API with the
    // ESP-IDF 5 encoder-based one, and the coarse LED indicator is the lowest-
    // value layer on a bench with serial attached. Tracked in docs/bench-qc.md.
    report.add("LED", Verdict::Skip, "WS2812/GPIO48 aggregate deferred (new RMT API)");

    // ── #4 SD ─────────────────────────────────────────────────────────────────
    check_sd(&mut report);

    // ── #5 USB-C keyboard ───────────────────────────────────────────────────
    check_keyboard(&mut report, &mut boot);

    // ── #6 Wi-Fi ──────────────────────────────────────────────────────────────
    check_wifi(peripherals.modem, &mut report);

    // ── #7 Charger / battery ────────────────────────────────────────────────
    check_charger(&mut report);

    // ── #8 GPIO short/open scan ───────────────────────────────────────────────
    check_gpio_scan(&mut report);

    // ── Report + panel mirror ─────────────────────────────────────────────────
    report.print();
    mirror_to_panel(&mut epd, &report);

    log::info!("QC complete — power-cycle to re-run.");
    loop {
        FreeRtos::delay_ms(1000);
    }
}

// ─── EPD (#2 handshake, #3 pattern) ────────────────────────────────────────────

fn check_epd(epd: &mut Epd, report: &mut Report, boot: &mut BootButton) {
    if let Err(e) = epd.reset().and_then(|()| epd.init()) {
        report.add("EPD handshake", Verdict::Nok, format!("reset/init errored: {e:?}"));
        return;
    }

    // A full refresh drives the whole panel and blocks on BUSY (~2.2 s on a good
    // panel). The wall-clock time is a real diagnostic of the BUSY/RST wiring:
    //  • ~1.9–2.4 s  → BUSY toggled correctly (handshake OK)
    //  • ≥ 2.45 s    → BUSY never dropped (driver caps at 2.5 s) = stuck high
    //                  (RST not releasing, or BUSY open reading high)
    //  • < 1.5 s     → BUSY read low the whole time (open BUSY line, pulled down)
    //                  — the panel almost certainly did not actually refresh
    let t = Instant::now();
    let refresh = epd.clear_screen(0x00); // all black, full refresh
    let ms = t.elapsed().as_millis();
    match refresh {
        Err(e) => report.add("EPD handshake", Verdict::Nok, format!("refresh errored: {e:?}")),
        Ok(()) if ms >= 2450 => report.add(
            "EPD handshake",
            Verdict::Nok,
            format!("BUSY stuck high ({ms} ms) — check RST/BUSY"),
        ),
        Ok(()) if ms < 1500 => report.add(
            "EPD handshake",
            Verdict::Nok,
            format!("BUSY stuck low ({ms} ms) — open BUSY line? panel likely blank"),
        ),
        Ok(()) => report.add("EPD handshake", Verdict::Ok, format!("refresh {ms} ms")),
    }

    // #3 visual pattern — exercises both controller halves + the seam at x=396.
    if let Err(e) = epd.display_frame(qc_pattern().bytes()) {
        report.add("EPD pattern", Verdict::Nok, format!("blit errored: {e:?}"));
        return;
    }
    let v = confirm(boot, "EPD: both halves filled, seam clean, text sharp? tap=OK / hold=NOK");
    report.add("EPD pattern", v, "noise=CS, missing half=MOSI/SCK, bad seam=controller");
}

/// A frame that reveals a dead controller half (left vs right of x=396), a bad
/// seam, and MOSI/SCK/CS faults (noise / missing bands).
fn qc_pattern() -> Frame {
    let mut f = Frame::new_white();
    let thick = PrimitiveStyle::with_stroke(BinaryColor::On, 3);
    // Full-panel border — a missing edge means that controller half is dark.
    Rectangle::new(Point::new(2, 2), Size::new((WIDTH - 4) as u32, (HEIGHT - 4) as u32))
        .into_styled(thick)
        .draw(&mut f)
        .ok();
    // Seam marker at x=396 (the dual-controller boundary).
    Line::new(Point::new(396, 0), Point::new(396, HEIGHT as i32))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 2))
        .draw(&mut f)
        .ok();
    // Text spanning the seam — the "R" must land on the right controller.
    let big = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
    Text::new("typoena QC   L | R", Point::new(230, HEIGHT as i32 / 2), big)
        .draw(&mut f)
        .ok();
    f
}

// ─── SD (#4) ───────────────────────────────────────────────────────────────────

fn check_sd(report: &mut Report) {
    let sd = match Storage::mount() {
        Ok(sd) => sd,
        Err(e) => {
            report.add("SD", Verdict::Nok, format!("mount failed: {e} — check 13/14/15/10, card"));
            return;
        }
    };
    let (max_khz, real_khz) = sd.negotiated_khz();

    // Write → read → compare a small blob, then clean up. A swapped/open data
    // line makes the round-trip mismatch even if the mount somehow succeeds.
    // `sd` (the mount guard) stays alive to the end of this scope.
    let path = "/sd/qc_probe";
    let payload: &[u8] = b"typoena-qc-roundtrip-0123456789";
    let round_trip = (|| -> std::io::Result<bool> {
        std::fs::write(path, payload)?;
        let back = std::fs::read(path)?;
        let _ = std::fs::remove_file(path);
        Ok(back == payload)
    })();
    match round_trip {
        Ok(true) => report.add(
            "SD",
            Verdict::Ok,
            format!("mounted, {real_khz}/{max_khz} kHz, round-trip identical"),
        ),
        Ok(false) => {
            report.add("SD", Verdict::Nok, "round-trip MISMATCH — data-line fault (MOSI/MISO swap?)")
        }
        Err(e) => {
            let _ = std::fs::remove_file(path);
            report.add("SD", Verdict::Nok, format!("mounted but I/O failed: {e}"));
        }
    }
}

// ─── USB-C keyboard (#5) ───────────────────────────────────────────────────────

fn check_keyboard(report: &mut Report, boot: &mut BootButton) {
    if let Err(e) = usb_kbd::start() {
        report.add("USB keyboard", Verdict::Nok, format!("host stack install failed: {e:?}"));
        return;
    }
    // Enumeration: wait for the host driver to open + set up a keyboard.
    log::info!("waiting up to {KBD_ENUM_TIMEOUT_S}s for a keyboard to enumerate…");
    let t = Instant::now();
    while !usb_kbd::keyboard_present() {
        if t.elapsed().as_secs() >= KBD_ENUM_TIMEOUT_S {
            report.add(
                "USB keyboard",
                Verdict::Nok,
                "no enumeration — check VBUS (5V sourced), D+/D- (20/19)",
            );
            return;
        }
        FreeRtos::delay_ms(100);
    }
    log::info!("keyboard enumerated — press any key to confirm decode…");

    // Drain any queued keys, then wait for a fresh keypress to confirm decode.
    while usb_kbd::next_key().is_some() {}
    let t = Instant::now();
    loop {
        if let Some(key) = usb_kbd::next_key() {
            report.add("USB keyboard", Verdict::Ok, format!("enumerated + decoded {key:?}"));
            log::info!("tip: flip the connector and confirm it re-enumerates (spec #5 orientation test)");
            return;
        }
        if t.elapsed().as_secs() >= CONFIRM_TIMEOUT_S {
            let v = confirm(boot, "keyboard enumerated but no key seen — tap=accept enum / hold=NOK");
            let v = if v == Verdict::Ok { Verdict::Unconfirmed } else { v };
            report.add("USB keyboard", v, "enumerated; keypress not decoded in time");
            return;
        }
        FreeRtos::delay_ms(50);
    }
}

// ─── Wi-Fi (#6) ────────────────────────────────────────────────────────────────

fn check_wifi(modem: esp_idf_svc::hal::modem::Modem, report: &mut Report) {
    use esp_idf_svc::eventloop::EspSystemEventLoop;
    use esp_idf_svc::nvs::EspDefaultNvsPartition;
    use esp_idf_svc::wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi};

    let scan = (|| -> Result<(usize, Option<i8>)> {
        let sys_loop = EspSystemEventLoop::take()?;
        let nvs = EspDefaultNvsPartition::take()?;
        let mut wifi =
            BlockingWifi::wrap(EspWifi::new(modem, sys_loop.clone(), Some(nvs))?, sys_loop)?;
        wifi.set_configuration(&Configuration::Client(ClientConfiguration::default()))?;
        wifi.start()?;
        let aps = wifi.scan()?;
        let best = aps.iter().map(|a| a.signal_strength).max();
        Ok((aps.len(), best))
    })();

    match scan {
        Ok((0, _)) => report.add("Wi-Fi", Verdict::Nok, "scan found 0 APs — antenna/RF?"),
        Ok((n, best)) => report.add(
            "Wi-Fi",
            Verdict::Ok,
            format!("scan found {n} AP(s), best RSSI {} dBm", best.unwrap_or(0)),
        ),
        Err(e) => report.add("Wi-Fi", Verdict::Nok, format!("scan failed: {e:#}")),
    }
}

// ─── Charger / battery (#7) ─────────────────────────────────────────────────────

fn check_charger(report: &mut Report) {
    let Some(pin) = CHARGER_CHRG_PIN else {
        report.add(
            "Charger",
            Verdict::Skip,
            "no CHRG pin wired — manual: unplug USB, device should stay alive on battery",
        );
        return;
    };
    set_input_pull(pin, sys::gpio_pull_mode_t_GPIO_PULLUP_ONLY);
    FreeRtos::delay_ms(5);
    let charging = unsafe { sys::gpio_get_level(pin) } == 0; // open-drain, low = charging
    report.add(
        "Charger",
        Verdict::Ok,
        format!("CHRG read: {}", if charging { "charging" } else { "not charging / done" }),
    );
}

// ─── GPIO short/open scan (#8) ──────────────────────────────────────────────────

fn check_gpio_scan(report: &mut Report) {
    if SCAN_PINS.is_empty() {
        report.add(
            "GPIO scan",
            Verdict::Skip,
            "no SCAN_PINS declared — fill the expected-net table from the schematic",
        );
        return;
    }

    let mut faults: Vec<String> = Vec::new();

    // Short test: drive one pin high with the rest as inputs pulled DOWN; any
    // follower that reads high is bridged to the driven pin.
    for &drv in SCAN_PINS {
        for &other in SCAN_PINS {
            if other != drv {
                set_input_pull(other, sys::gpio_pull_mode_t_GPIO_PULLDOWN_ONLY);
            }
        }
        unsafe {
            let _ = sys::gpio_set_direction(drv, sys::gpio_mode_t_GPIO_MODE_OUTPUT);
            let _ = sys::gpio_set_level(drv, 1);
        }
        FreeRtos::delay_ms(2);
        for &other in SCAN_PINS {
            if other != drv && unsafe { sys::gpio_get_level(other) } == 1 {
                faults.push(format!("short {drv}~{other}"));
            }
        }
        set_input_pull(drv, sys::gpio_pull_mode_t_GPIO_PULLDOWN_ONLY);
    }

    // Open/stuck test: each pin should follow its own pull (up→high, down→low);
    // if it doesn't, it's shorted to a rail or being driven.
    for &p in SCAN_PINS {
        set_input_pull(p, sys::gpio_pull_mode_t_GPIO_PULLUP_ONLY);
        FreeRtos::delay_ms(2);
        let hi = unsafe { sys::gpio_get_level(p) } == 1;
        set_input_pull(p, sys::gpio_pull_mode_t_GPIO_PULLDOWN_ONLY);
        FreeRtos::delay_ms(2);
        let lo = unsafe { sys::gpio_get_level(p) } == 0;
        if !(hi && lo) {
            faults.push(format!("stuck {p}"));
        }
    }

    if faults.is_empty() {
        report.add(
            "GPIO scan",
            Verdict::Ok,
            format!("{} isolated pin(s): no bridges, all float", SCAN_PINS.len()),
        );
    } else {
        report.add("GPIO scan", Verdict::Nok, faults.join(", "));
    }
}

fn set_input_pull(pin: i32, pull: sys::gpio_pull_mode_t) {
    unsafe {
        let _ = sys::gpio_reset_pin(pin);
        let _ = sys::gpio_set_direction(pin, sys::gpio_mode_t_GPIO_MODE_INPUT);
        let _ = sys::gpio_set_pull_mode(pin, pull);
    }
}

// ─── Panel results mirror ────────────────────────────────────────────────────

fn mirror_to_panel(epd: &mut Epd, report: &Report) {
    let mut f = Frame::new_white();
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let mut y = 11i32;
    let header = if report.any_nok() {
        "Typoena bench QC — FAULTS (see below)"
    } else {
        "Typoena bench QC — ALL OK"
    };
    Text::new(header, Point::new(4, y), style).draw(&mut f).ok();
    y += 14;
    for line in report.lines() {
        Text::new(line.as_str(), Point::new(4, y), style).draw(&mut f).ok();
        y += 11;
        if y > HEIGHT as i32 - 4 {
            break;
        }
    }
    if let Err(e) = epd.display_frame(f.bytes()) {
        log::warn!("panel mirror blit failed: {e:?}");
    }
}

// ─── Operator confirm via BOOT (GPIO0) ──────────────────────────────────────────

/// Wait for the operator to press BOOT: a short tap = OK, a hold (≥1 s) = NOK.
/// No press within [`CONFIRM_TIMEOUT_S`] leaves the check `Unconfirmed`
/// (non-gating). BOOT is pulled up, so `is_low()` = pressed.
fn confirm(boot: &mut BootButton, prompt: &str) -> Verdict {
    log::info!("CONFIRM: {prompt}");
    let start = Instant::now();
    while boot.is_high() {
        if start.elapsed().as_secs() >= CONFIRM_TIMEOUT_S {
            log::warn!("confirm timed out — leaving unconfirmed");
            return Verdict::Unconfirmed;
        }
        FreeRtos::delay_ms(20);
    }
    let pressed_at = Instant::now();
    while boot.is_low() {
        FreeRtos::delay_ms(10);
    }
    if pressed_at.elapsed().as_millis() >= 1000 {
        Verdict::Nok
    } else {
        Verdict::Ok
    }
}
