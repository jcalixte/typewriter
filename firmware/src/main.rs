mod usb_kbd;

use std::time::Instant;

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{AnyIOPin, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::spi::config::{Config, DriverConfig};
use esp_idf_svc::hal::spi::{Dma, SpiBusDriver, SpiDriver};
use esp_idf_svc::hal::units::FromValueType;

use display::Frame;
use editor::{Editor, Effect, Mode, CH};
use firmware::epd::{self, Epd};
use firmware::persistence::{Storage, NOTES};

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
    // Boot splash (Spike 9): the Typoena mark, shown while the SD mounts and the
    // note loads below. Its full refresh doubles as the baseline the old white
    // clear used to establish (writes both RAM banks); the editor's first render
    // further down cleanly replaces it with a second full refresh.
    epd.display_frame(Frame::splash().bytes())?;

    // Mount the SD and load the saved note. We bring the SD up *after* the EPD —
    // the doc's boot order is SD-first, but a dead panel can't explain a missing
    // card — and treat a missing card / repo / unreadable note as fatal: a
    // writing appliance that silently started empty would clobber the note on
    // the next `:w`. See docs/v0.1-mvp-technical.md, boot sequence.
    let (storage, saved) = boot_storage(&mut epd);

    // Bring up the USB keyboard in the background; keys arrive via next_key().
    usb_kbd::start()?;

    // Spawn the dedicated git thread — the `:sync` publish transport. It owns
    // the Wi-Fi stack (brought up lazily on the first `:sync`, so the radio
    // stays off until you publish) and parks on `git_tx` until signalled; the
    // push runs off the UI loop, and its outcome returns on `git_rx` for the
    // snackbar. Behind the `git` feature so a light build carries no libgit2.
    #[cfg(feature = "git")]
    let (git_tx, git_rx) = {
        use esp_idf_svc::eventloop::EspSystemEventLoop;
        use esp_idf_svc::nvs::EspDefaultNvsPartition;
        use firmware::git_sync::{run_git_service, PublishOutcome, PublishRequest, GIT_STACK};

        let sys_loop = EspSystemEventLoop::take()?;
        let nvs = EspDefaultNvsPartition::take()?;
        let modem = peripherals.modem;
        let (req_tx, req_rx) = std::sync::mpsc::channel::<PublishRequest>();
        let (res_tx, res_rx) = std::sync::mpsc::channel::<PublishOutcome>();
        std::thread::Builder::new()
            .name("git".into())
            .stack_size(GIT_STACK)
            .spawn(move || run_git_service(modem, sys_loop, nvs, req_rx, res_tx))?;
        log::info!(
            "git thread up ({} KB stack); Wi-Fi comes up on the first :sync",
            GIT_STACK / 1024
        );
        (req_tx, res_rx)
    };

    // Seed the editor from the saved note. Boots in Normal mode with the caret
    // on the last character (the resume point) — press `i`/`a`/`o` to write.
    let mut ed = Editor::with_text(saved);
    // Confirm the boot-load on the panel (no serial console in normal use):
    // "loaded <name>" using the note's filename without its suffix (notes.md ->
    // notes). Cleared by the first keystroke, like any snackbar.
    let name = std::path::Path::new(NOTES)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("notes");
    ed.set_notice(format!("loaded {name}"));
    let mut updates: u32 = 0;
    let mut cursor_shown = true; // the initial render includes the caret
    let mut last_activity = Instant::now();
    // Set when a paint fails (see the refresh block below): the next paint then
    // does a full refresh to re-establish both RAM banks, since a partial that
    // died mid-transfer may have left them inconsistent.
    let mut force_full = false;

    // Keyboard attach/detach state drives the panel's disconnect flag; seed it
    // (and the word-count snapshot) before the first render.
    let mut last_kbd = usb_kbd::keyboard_present();
    ed.set_keyboard_present(last_kbd);
    ed.refresh_stats();

    // First editor render. The splash's full refresh above already seeded both
    // RAM banks (its image is the `0x26` "previous" baseline), so the editor
    // comes up with a full-area *partial* (~630 ms) instead of a second full
    // refresh (~1.9 s): the splash→editor swap rides the partial waveform,
    // shaving ~1.3 s off cold boot. This large-area partial is the one boot
    // refresh worth eyeballing for ghosting; the loop's periodic full refresh
    // (every FULL_REFRESH_EVERY updates) clears any residue.
    let mut shown = ed.draw(true);
    epd.display_frame_partial_window(shown.bytes(), 0, epd::HEIGHT)?;

    // Boot-time measurement (the ≤ 5 s v0.1 / ≤ 3 s v1.0 target). Two clocks, and
    // they disagree by ~1.4 s here, so report both. `esp_log_timestamp()` counts
    // from ~power-on (same value as this line's own log prefix) → the real
    // cold-boot number. `esp_timer_get_time()` only starts ~1.4 s in, after the
    // 2nd-stage bootloader + the ~0.74 s PSRAM memtest, so it captures just the
    // app-side init, not total boot. "Cursor ready" = first editor frame on the
    // panel, input loop below about to poll.
    let total_ms = unsafe { esp_idf_svc::sys::esp_log_timestamp() };
    let app_ms = (unsafe { esp_idf_svc::sys::esp_timer_get_time() } / 1000) as u32;
    log::info!("boot: cursor ready — {total_ms} ms since power-on ({app_ms} ms app-side)");

    loop {
        // Drain all queued keystrokes (type-ahead absorbed during a refresh),
        // apply them, then do a single refresh for the batch.
        let mut keys = 0;
        let mut effect = Effect::None;
        while let Some(k) = usb_kbd::next_key() {
            // A `:` command (only) yields an Effect; keep the last one in the batch.
            match ed.handle(k) {
                Effect::None => {}
                e => effect = e,
            }
            keys += 1;
        }

        // Carry out any host-side effect a `:` command asked for. Save is inline
        // (fast). `:sync` persists, then hands off to the git thread — the push
        // is behind the `git` feature, so a light build carries no libgit2/git2.
        match effect {
            Effect::None => {}
            Effect::Save => save_note(&storage, &mut ed),
            Effect::Publish => {
                // Publishing an unsaved buffer is meaningless, so persist first.
                save_note(&storage, &mut ed);
                // Then signal the git thread — non-blocking, so the ~10 s push
                // never stalls the editor. The outcome returns on `git_rx` and
                // updates the snackbar (see the idle branch below).
                #[cfg(feature = "git")]
                match git_tx.send(firmware::git_sync::PublishRequest) {
                    Ok(()) => ed.set_notice("syncing..."),
                    Err(_) => ed.set_notice("sync: git thread down"),
                }
                #[cfg(not(feature = "git"))]
                log::info!(":sync — saved; light build (no `git` feature) — push skipped");
            }
            Effect::Pull => {
                // `:gl` — fetch + fast-forward from the remote. The on-device
                // fetch/fast-forward on the git thread is v0.7 work (git_sync
                // only exposes push today), so acknowledge and no-op for now.
                ed.set_notice("pull: not wired yet (v0.7)");
            }
        }

        // Keyboard attach/detach feeds the panel's disconnect flag.
        let kbd = usb_kbd::keyboard_present();
        ed.set_keyboard_present(kbd);
        let kbd_changed = kbd != last_kbd;
        last_kbd = kbd;

        if keys == 0 {
            // A finished publish reports its outcome here (the push ran on the
            // git thread while we idled). Show it in the snackbar with a silent
            // full-area partial — no keystroke will arrive to trigger a repaint.
            #[cfg(feature = "git")]
            if let Ok(outcome) = git_rx.try_recv() {
                use firmware::git_sync::PublishOutcome::*;
                ed.set_notice(match outcome {
                    Pushed(oid) => format!("synced {oid}"),
                    UpToDate => "up to date".to_string(),
                    Failed(reason) => reason,
                });
                let f = ed.draw(true);
                if let Err(e) = epd.display_frame_partial_window(f.bytes(), 0, epd::HEIGHT) {
                    log::warn!("sync-notice repaint FAILED ({e}); full refresh next");
                    force_full = true;
                    continue;
                }
                shown = f;
                cursor_shown = true;
                continue;
            }
            // A connect/disconnect while idle must still repaint the panel flag —
            // no keystroke will arrive to trigger it otherwise.
            if kbd_changed {
                let f = ed.draw(true);
                if let Err(e) = epd.display_frame_partial_window(f.bytes(), 0, epd::HEIGHT) {
                    log::warn!("kbd-flag repaint FAILED ({e}); full refresh next");
                    force_full = true;
                    continue;
                }
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
                if let Err(e) = epd.display_frame_partial_window(f.bytes(), 0, epd::HEIGHT) {
                    log::warn!("caret repaint FAILED ({e}); full refresh next");
                    force_full = true;
                } else {
                    shown = f;
                    cursor_shown = true;
                    log::info!("caret shown");
                }
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
        // `force_full` promotes to a full refresh after a failed paint: it
        // rewrites both RAM banks, recovering from a partial that may have died
        // mid-transfer and desynced them.
        let (result, refresh) = if periodic || force_full {
            (epd.display_frame(frame.bytes()), "FULL")
        } else if additive {
            (epd.display_frame_partial_window(frame.bytes(), y0, y1 - y0 + 1), "windowed")
        } else {
            (epd.display_frame_partial_window(frame.bytes(), 0, epd::HEIGHT), "full-area")
        };
        let ms = t0.elapsed().as_millis();
        if let Err(e) = result {
            // Never fatal — the buffer is the source of truth and safe in RAM,
            // exactly like a failed `save_note`. Drop this frame, leave `shown`
            // untouched so the next paint repaints the same diff, and force a
            // clean full refresh then. Typical cause: internal DMA-capable RAM
            // briefly starved by Wi-Fi/TLS during a background `:sync`; it frees
            // the moment the push finishes.
            log::warn!("{refresh} refresh #{updates} FAILED ({e}); frame dropped, full refresh next");
            force_full = true;
            continue;
        }
        force_full = false;
        log::info!(
            "{refresh} refresh #{updates} [{:?}]: {ms} ms (rows {y0}..={y1}, {keys} key(s))",
            ed.mode()
        );
        shown = frame;
        cursor_shown = ed.mode() != Mode::Insert;
    }
}

/// Mount the SD card and load the saved note, or halt with the reason on the
/// panel. Everything here is fatal by design (see the boot-sequence comment in
/// `main`): the note is the whole point of the appliance, so we refuse to run
/// in a state where the next save could destroy it.
fn boot_storage(epd: &mut Epd) -> (Storage, String) {
    let storage = match Storage::mount() {
        Ok(s) => s,
        Err(e) => boot_halt(epd, "SD card not ready", &format!("{e:#}")),
    };
    if !storage.repo_present() {
        boot_halt(
            epd,
            "No repo on the SD card",
            "Provision it on your computer (just init) and reboot.",
        );
    }
    let note = match storage.load() {
        Ok(text) => text,
        Err(e) => boot_halt(epd, "Could not read your note", &format!("{e:#}")),
    };
    log::info!("boot: loaded {} bytes from {NOTES}", note.len());
    (storage, note)
}

/// Show a terminal boot error on the panel and idle forever. Rebooting into the
/// same missing card would just thrash, so we stop and explain instead.
fn boot_halt(epd: &mut Epd, headline: &str, detail: &str) -> ! {
    log::error!("boot halt — {headline}: {detail}");
    if let Err(e) = show_message(epd, &format!("{headline}\n\n{detail}\n")) {
        log::error!("(could not paint the boot error either: {e:#})");
    }
    loop {
        FreeRtos::delay_ms(1000);
    }
}

/// Render a plain full-frame message by borrowing the editor purely as a
/// text-layout engine, so boot failures surface on the panel, not a dead screen.
fn show_message(epd: &mut Epd, msg: &str) -> anyhow::Result<()> {
    let frame = Editor::with_text(msg.to_string()).draw(false);
    epd.display_frame(frame.bytes())?;
    Ok(())
}

/// Persist the buffer to SD. Errors are logged, never propagated: the in-RAM
/// buffer is the source of truth and must survive a failed write (e.g. a card
/// pulled mid-session) so the user can fix the card and retry `:w`.
fn save_note(storage: &Storage, ed: &mut Editor) {
    let n = ed.text().len();
    match storage.save(ed.text()) {
        Ok(()) => {
            log::info!(":w — saved {n} bytes to {NOTES}");
            ed.set_notice("saved");
        }
        Err(e) => {
            log::error!(":w — save FAILED ({e:#}); buffer kept in RAM, retry :w");
            ed.set_notice("save FAILED - retry :w");
        }
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
