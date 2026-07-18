//! Full editor on the panel — the real experience, no SD card.
//!
//! The device firmware ([`main.rs`]) minus persistence: it brings up the
//! e-paper (Spike 2 wiring) and the shared [`firmware::usb_kbd`] host stack,
//! then runs the actual [`editor::Editor`] through the **same panel engine as
//! `main.rs`** — [`app::Panel`] — so the panel behaves exactly like the
//! shipping device (boot splash, windowed/additive partials, the debounced
//! Insert caret, the periodic panel-longevity full refresh, focus mode). Only
//! the SD card is gone: the loop below is the storage/git-free half of main's,
//! sequencing the same [`Panel`] calls without the git thread or palette walk.
//!
//! Because there is no card, storage is stubbed: it mounts nothing (so the
//! editor firmware's "SD card not ready" boot halt never applies), starts from
//! an in-RAM scratch buffer, and runs on default prefs and an empty snippet
//! library. Storage commands are inert — `:w`/`:e`/`:delete` just flash a
//! notice, `:gp`/`:gl` do nothing, prefs cycled with the palette `>` still apply
//! live but aren't persisted, and there is no file-palette walk or save-on-idle.
//! Nothing is saved: edits live in RAM and vanish on reset. `:reboot` still
//! works (it needs no card). It's the full editor for a bench check or a demo,
//! not a place to write real notes. Flash with `just flash-demo`, then plug a
//! keyboard into the OTG port and press `i`.

use std::time::Instant;

use esp_idf_svc::hal::gpio::{AnyIOPin, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::spi::config::{Config, DriverConfig};
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::spi::{Dma, SpiBusDriver, SpiDriver};
use esp_idf_svc::hal::units::FromValueType;

use app::{FocusTimer, Panel};
use display::Frame;
use editor::{Editor, Effect, Mode, Prefs, Snippets};
use firmware::epd::Epd;
use firmware::usb_kbd;

/// Injected by build.rs so serial output identifies the exact build.
const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

fn main() -> anyhow::Result<()> {
    // Required once before any esp-idf-svc call; some runtime patches only link
    // if this symbol is referenced. See esp-idf-template#71.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Typoena — full editor, no SD card, {BUILD_TAG}");

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;

    // GDEY0579T93 on the same S3-safe GPIOs main.rs uses (Spike 2 wiring):
    //   SCK 12 · DIN/MOSI 11 · CS 7 · DC 6 · RST 5 · BUSY 4
    let spi = SpiDriver::new(
        peripherals.spi2,
        pins.gpio12,
        pins.gpio11,
        None::<AnyIOPin>,
        &DriverConfig::new().dma(Dma::Auto(4096)),
    )?;
    let bus = SpiBusDriver::new(spi, &Config::new().baudrate(20.MHz().into()))?;
    let cs = PinDriver::output(pins.gpio7)?;
    let dc = PinDriver::output(pins.gpio6)?;
    let rst = PinDriver::output(pins.gpio5)?;
    let busy = PinDriver::input(pins.gpio4, Pull::Down)?;
    let mut epd = Epd::new(bus, dc, rst, cs, busy);

    log::info!("EPD reset + init…");
    epd.reset()?;
    epd.init()?;
    // Boot splash (Spike 9), kicked off async so its ~2.2 s waveform overlaps the
    // keyboard bring-up below — exactly as main.rs overlaps it with the SD mount.
    // The first editor render (Panel::new) waits it out and replaces it.
    epd.display_frame_async(Frame::splash().bytes())?;

    // Bring up the USB keyboard in the background; keys arrive via next_key().
    usb_kbd::start()?;

    // Seed an in-RAM scratch buffer (no SD to load a note from). A short hint
    // proves the panel immediately and tells the tester how to start; it clears
    // on the first keystroke like any buffer content. The editor opens in Normal
    // mode (vim), so `i` enters Insert.
    let hint = "Typoena - full editor, no SD card\n\n\
        Press  i  to type, Esc to stop.\n\
        Storage is off: nothing is saved and :w / :e / :gp are inert.\n";
    let mut ed = Editor::with_text(hint.to_string());
    // Default prefs and an empty snippet library — there is no card to read
    // .typoena.toml / .typoena.snippets.json from. Prefs cycled live with the
    // palette `>` still take effect this session; they just aren't persisted.
    ed.set_prefs(Prefs::default());
    ed.set_snippets(Snippets::default());

    // Keyboard attach/detach drives the panel's disconnect flag; seed it before
    // the first render.
    let mut last_kbd = usb_kbd::keyboard_present();
    ed.set_keyboard_present(last_kbd);
    ed.refresh_stats();

    // Hand the panel to the render engine; Panel::new does the first editor frame
    // (a full-area partial riding out the splash waveform) and owns it from here.
    let mut panel = Panel::new(epd, &mut ed)?;
    let total_ms = unsafe { esp_idf_svc::sys::esp_log_timestamp() };
    log::info!("boot: cursor ready — {total_ms} ms since power-on");

    let mut last_activity = Instant::now();
    // Focus mode (Pomodoro): off until `:focus`. Driven by the FocusStart/Stop
    // effects below; the rest-card drop is Panel::rest_if_due in the idle branch.
    let mut focus = FocusTimer::default();

    loop {
        // Drain all queued keystrokes (type-ahead absorbed during a refresh),
        // apply them, then do a single refresh for the batch.
        let prev_mode = ed.mode(); // to detect leaving the Rest curtain below
        let mut keys = 0;
        while let Some(k) = usb_kbd::next_key() {
            let was_rest = ed.mode() == Mode::Rest;
            ed.handle(k);
            keys += 1;
            // Leaving the rest curtain (Ctrl-C / q / Esc) drops the rest of this
            // batch, so an accidental bump that exits only ever lands on a clean
            // Normal screen, never an edit.
            if was_rest && ed.mode() != Mode::Rest {
                while usb_kbd::next_key().is_some() {}
                break;
            }
        }

        // Service the host-side effects the batch queued. With no SD and no git,
        // storage effects are inert — they only flash a notice — while focus and
        // reboot behave exactly as on the device. Drain to empty for structural
        // parity with main.rs (nothing here re-queues, so it terminates at once).
        loop {
            let effects = ed.take_effects();
            if effects.is_empty() {
                break;
            }
            for effect in effects {
                match effect {
                    // No card to write to: acknowledge but don't persist. The
                    // buffer stays dirty in RAM (honest — nothing was saved).
                    Effect::Save { .. } => ed.set_notice("demo - not saved (no SD)"),
                    Effect::Load { path, .. } => {
                        log::info!("demo - open {path} skipped (no SD)");
                        ed.set_notice("demo - can't open (no SD)");
                    }
                    Effect::Delete { .. } => ed.set_notice("demo - not deleted (no SD)"),
                    // The pref change already applied live in the editor; there is
                    // just nowhere to persist it. No notice — the visible effect
                    // (e.g. line numbers toggling) is its own feedback.
                    Effect::SavePrefs { .. } => {}
                    Effect::Publish | Effect::Pull => ed.set_notice("demo - no sync (no SD)"),
                    Effect::Setup => ed.set_notice("demo - setup needs the full firmware"),
                    Effect::Reboot => {
                        // Needs no card: paint the branded splash so the reboot
                        // reads as intentional, then restart.
                        log::info!(":reboot — restarting");
                        panel.blit_full(&Frame::reboot());
                        unsafe { esp_idf_svc::sys::esp_restart() };
                    }
                    Effect::FocusStart => focus.start(ed.word_count()),
                    Effect::FocusStop => focus.stop(),
                }
            }
        }

        // Keyboard attach/detach feeds the panel's disconnect flag.
        let kbd = usb_kbd::keyboard_present();
        ed.set_keyboard_present(kbd);
        let kbd_changed = kbd != last_kbd;
        last_kbd = kbd;

        if keys == 0 {
            // Idle: the same sequence as main.rs's idle branch, minus the
            // git-outcome and palette-walk rungs the demo has no equivalent of.
            // Each returns true when it painted, so we stop at the first that did.
            if panel.rest_if_due(&mut ed, &focus, last_activity) {
                continue;
            }
            if panel.kbd_repaint(&mut ed, kbd_changed, kbd) {
                continue;
            }
            if panel.longevity_full(&mut ed, last_activity) {
                continue;
            }
            if !panel.caret_if_due(&mut ed, last_activity) {
                FreeRtos::delay_ms(8);
            }
            continue;
        }

        last_activity = Instant::now();
        panel.render_batch(&mut ed, prev_mode, keys);
    }
}
