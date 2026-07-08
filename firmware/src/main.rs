mod editor;
mod epd;
mod usb_kbd;

use std::time::Instant;

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{AnyIOPin, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::spi::config::{Config, DriverConfig};
use esp_idf_svc::hal::spi::{Dma, SpiBusDriver, SpiDriver};
use esp_idf_svc::hal::units::FromValueType;

use editor::{Editor, Mode, CH};
use epd::Epd;

/// Injected by build.rs so serial output identifies the exact build.
const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

/// Occasional full refresh, mainly for panel longevity — partial updates on
/// this panel stay visually clean far longer, so this is deliberately rare.
const FULL_REFRESH_EVERY: u32 = 64;

/// How long typing must pause before the Insert-mode caret is shown. There is no
/// caret while actively typing (it would ghost under windowed refresh); it
/// reappears once you settle. Normal/View draw their own caret every action.
const CURSOR_DEBOUNCE_MS: u128 = 750;

fn main() -> anyhow::Result<()> {
    // Required once before any esp-idf-svc call; some runtime patches
    // only link if this symbol is referenced. See esp-idf-template#71.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Typoena — modal editor (vim modes), {BUILD_TAG}");

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;

    // GDEY0579T93 on S3-safe GPIOs (Spike 2 wiring):
    //   SCK 12 · DIN/MOSI 11 · CS 7 · DC 6 · RST 5 · BUSY 4
    let spi = SpiDriver::new(
        peripherals.spi2,
        pins.gpio12,
        pins.gpio11,
        None::<AnyIOPin>,
        &DriverConfig::new().dma(Dma::Auto(4096)),
    )?;
    let bus = SpiBusDriver::new(spi, &Config::new().baudrate(4.MHz().into()))?;
    let cs = PinDriver::output(pins.gpio7)?;
    let dc = PinDriver::output(pins.gpio6)?;
    let rst = PinDriver::output(pins.gpio5)?;
    let busy = PinDriver::input(pins.gpio4, Pull::Down)?;
    let mut epd = Epd::new(bus, dc, rst, cs, busy);

    log::info!("EPD reset + init…");
    epd.reset()?;
    epd.init()?;
    epd.clear_screen(0xFF)?; // white baseline; establishes the previous bank

    // Bring up the USB keyboard in the background; keys arrive via next_key().
    usb_kbd::start()?;

    let mut ed = Editor::new();
    let mut updates: u32 = 0;
    let mut cursor_shown = true; // the initial render includes the caret
    let mut last_activity = Instant::now();

    // Keyboard attach/detach state drives the panel's disconnect flag; seed it
    // (and the word-count snapshot) before the first render.
    let mut last_kbd = usb_kbd::keyboard_present();
    ed.set_keyboard_present(last_kbd);
    ed.refresh_stats();

    // First render is full (establishes the on-screen baseline for partials).
    let mut shown = ed.draw(true);
    epd.display_frame(shown.bytes())?;

    loop {
        // Drain all queued keystrokes (type-ahead absorbed during a refresh),
        // apply them, then do a single refresh for the batch.
        let mut keys = 0;
        while let Some(k) = usb_kbd::next_key() {
            ed.handle(k);
            keys += 1;
        }

        // Keyboard attach/detach feeds the panel's disconnect flag.
        let kbd = usb_kbd::keyboard_present();
        ed.set_keyboard_present(kbd);
        let kbd_changed = kbd != last_kbd;
        last_kbd = kbd;

        if keys == 0 {
            // A connect/disconnect while idle must still repaint the panel flag —
            // no keystroke will arrive to trigger it otherwise.
            if kbd_changed {
                let f = ed.draw(true);
                epd.display_frame_partial_window(f.bytes(), 0, epd::HEIGHT)?;
                shown = f;
                cursor_shown = true;
                log::info!("keyboard {}", if kbd { "connected" } else { "disconnected" });
                continue;
            }
            // Debounced caret, Insert mode only: once typing pauses, bring the
            // bar caret back and refresh the panel word count with a silent
            // full-area partial (no flash). Normal/View draw their caret on action.
            if ed.mode() == Mode::Insert
                && !cursor_shown
                && last_activity.elapsed().as_millis() >= CURSOR_DEBOUNCE_MS
            {
                ed.refresh_stats();
                let f = ed.draw(true);
                epd.display_frame_partial_window(f.bytes(), 0, epd::HEIGHT)?;
                shown = f;
                cursor_shown = true;
                log::info!("caret shown");
            } else {
                FreeRtos::delay_ms(8);
            }
            continue;
        }

        last_activity = Instant::now();
        // Non-Insert actions (Normal edits, mode switches) aren't rapid typing,
        // so the panel word count can refresh immediately; in Insert the snapshot
        // stays frozen until the typing-pause path above refreshes it.
        if ed.mode() != Mode::Insert {
            ed.refresh_stats();
        }
        // Suppress the Insert bar caret while typing (fast, no ghost); Normal
        // and View render their caret regardless of this flag.
        let insert_cursor_on = ed.mode() != Mode::Insert;
        let prev_scroll = ed.scroll_top();
        let frame = ed.draw(insert_cursor_on);
        let scrolled = ed.scroll_top() != prev_scroll;

        // Only the rows that changed since the last shown frame need updating.
        let Some((y0, y1)) = changed_rows(shown.bytes(), frame.bytes()) else {
            shown = frame;
            cursor_shown = ed.mode() != Mode::Insert;
            continue; // no visible change
        };
        // Snap the band to whole text lines so a partial-window boundary never
        // lands mid-glyph — otherwise the boundary gate crops tall characters.
        let ch = CH as u16;
        let y0 = y0 / ch * ch;
        let y1 = (y1 / ch * ch + ch - 1).min(epd::HEIGHT - 1);

        updates += 1;
        // A purely additive Insert edit (no cursor, no scroll) uses the fast
        // windowed partial; anything else — deletes, caret moves, scrolling,
        // mode switches — uses a clean full-area partial, with a periodic full
        // refresh for panel longevity.
        let periodic = updates % FULL_REFRESH_EVERY == 0;
        let additive = ed.mode() == Mode::Insert
            && !scrolled
            && only_adds_ink(shown.bytes(), frame.bytes(), y0, y1);

        let t0 = Instant::now();
        let refresh = if periodic {
            epd.display_frame(frame.bytes())?;
            "FULL"
        } else if additive {
            epd.display_frame_partial_window(frame.bytes(), y0, y1 - y0 + 1)?;
            "windowed"
        } else {
            epd.display_frame_partial_window(frame.bytes(), 0, epd::HEIGHT)?;
            "full-area"
        };
        let ms = t0.elapsed().as_millis();
        log::info!(
            "{refresh} refresh #{updates} [{:?}]: {ms} ms (rows {y0}..={y1}, {keys} key(s))",
            ed.mode()
        );
        shown = frame;
        cursor_shown = ed.mode() != Mode::Insert;
    }
}

/// First and last (inclusive) framebuffer rows that differ between two frames,
/// or `None` if identical. Lets the partial refresh target just the band a
/// keystroke touched instead of all 272 rows.
fn changed_rows(a: &[u8], b: &[u8]) -> Option<(u16, u16)> {
    let w = epd::FB_BYTES_W;
    let mut first: Option<u16> = None;
    let mut last = 0u16;
    for y in 0..epd::HEIGHT as usize {
        if a[y * w..(y + 1) * w] != b[y * w..(y + 1) * w] {
            first.get_or_insert(y as u16);
            last = y as u16;
        }
    }
    first.map(|f| (f, last))
}

/// True if going from frame `a` to `b` only *adds* ink within rows `y0..=y1`
/// (no black pixel becomes white). Windowed partial refresh renders added ink
/// cleanly but leaves ghosts where ink is erased, so erasing edits fall back to
/// a clean full-area partial. Bit convention: 1 = white, 0 = black ink.
fn only_adds_ink(a: &[u8], b: &[u8], y0: u16, y1: u16) -> bool {
    let w = epd::FB_BYTES_W;
    for i in y0 as usize * w..(y1 as usize + 1) * w {
        // A bit set in b but clear in a went black→white — an erase.
        if b[i] & !a[i] != 0 {
            return false;
        }
    }
    true
}
