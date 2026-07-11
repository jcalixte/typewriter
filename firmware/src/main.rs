mod usb_kbd;

use std::time::Instant;

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{AnyIOPin, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::spi::config::{Config, DriverConfig};
use esp_idf_svc::hal::spi::{Dma, SpiBusDriver, SpiDriver};
use esp_idf_svc::hal::units::FromValueType;

use display::Frame;
use editor::{Editor, Effect, Mode, Scope, CH, LOCAL_DIR, REPO_DIR};
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
    // The boot note is Tracked (`/sd/repo/notes.md`); `:e` / the palette (v0.5)
    // open others, Tracked or Local.
    let mut ed = Editor::with_file(NOTES.to_string(), Scope::Tracked, saved);
    // Confirm the boot-load on the panel (no serial console in normal use):
    // "loaded <name>" using the note's filename without its suffix (notes.md ->
    // notes). Cleared by the first keystroke, like any snackbar.
    let name = std::path::Path::new(NOTES)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("notes");
    ed.set_notice(format!("loaded {name}"));
    // Feed the file palette (Ctrl-P). Enumerated once at boot — the v0.5 slices
    // that create/delete files (`:enew`, delete) will re-feed it then.
    ed.set_file_list(enumerate_files());
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
        while let Some(k) = usb_kbd::next_key() {
            ed.handle(k);
            keys += 1;
        }

        // Service the host-side effects the batch queued, in order. A file open
        // queues a Save of the outgoing dirty buffer *then* a Load of the target;
        // `:sync` queues a Save of the current buffer *then* Publish. Save/Load
        // are inline (fast SD IO); Publish hands off to the git thread — behind
        // the `git` feature, so a light build carries no libgit2/git2.
        //
        // Drain to empty rather than once: servicing a Load can itself queue an
        // eviction Save (when the swap pushes a dirty parked buffer out of the
        // ≤3 window), and that must be persisted now, not deferred to the next
        // keystroke where a power-off could lose it. The queue strictly shrinks
        // (a Save/Publish/Pull queues nothing; a Load queues at most one Save),
        // so this terminates.
        loop {
            let effects = ed.take_effects();
            if effects.is_empty() {
                break;
            }
            for effect in effects {
                match effect {
                    Effect::Save { path, contents, .. } => {
                        save_buffer(&storage, &mut ed, &path, &contents)
                    }
                    Effect::Load { path, scope } => open_buffer(&storage, &mut ed, path, scope),
                    Effect::Publish => {
                        // Non-blocking, so the ~10 s push never stalls the editor.
                        // The outcome returns on `git_rx` and updates the snackbar
                        // (see the idle branch below). The Save that preceded this
                        // in the batch already persisted the buffer, so this is a
                        // pure git push.
                        #[cfg(feature = "git")]
                        match git_tx.send(firmware::git_sync::PublishRequest) {
                            Ok(()) => ed.set_notice("syncing..."),
                            Err(_) => ed.set_notice("sync: git thread down"),
                        }
                        #[cfg(not(feature = "git"))]
                        log::info!(":sync — saved; light build (no `git` feature) — push skipped");
                    }
                    Effect::Pull => {
                        // `:gl` — fetch + fast-forward from the remote. The
                        // on-device fetch/fast-forward on the git thread is v0.7
                        // work (git_sync only exposes push today), so acknowledge
                        // and no-op for now.
                        ed.set_notice("pull: not wired yet (v0.7)");
                    }
                    Effect::Delete { path, scope } => delete_buffer(&storage, &mut ed, path, scope),
                }
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
            // exactly like a failed `save_buffer`. Drop this frame, leave `shown`
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

/// Persist a buffer to SD at `path`. Errors are logged, never propagated: the
/// in-RAM buffer is the source of truth and must survive a failed write (e.g. a
/// card pulled mid-session) so the user can fix the card and retry `:w`. On
/// success the editor's dirty flag for that path is cleared.
fn save_buffer(storage: &Storage, ed: &mut Editor, path: &str, contents: &str) {
    match storage.save_path(path, contents) {
        Ok(()) => {
            log::info!(":w — saved {} bytes to {path}", contents.len());
            ed.mark_saved(path);
            ed.set_notice("saved");
        }
        Err(e) => {
            log::error!("save FAILED ({e:#}); buffer kept in RAM, retry :w");
            ed.set_notice("save FAILED - retry :w");
        }
    }
}

/// Read `path` from SD and install it as the active buffer (the multi-file open
/// path, from `:e` / the palette). A read failure keeps the current buffer and
/// surfaces the reason on the snackbar rather than swapping to an empty screen.
fn open_buffer(storage: &Storage, ed: &mut Editor, path: String, scope: Scope) {
    match storage.load_path(&path) {
        Ok(text) => {
            log::info!("opened {path} ({} bytes, {scope:?})", text.len());
            let name = file_stem(&path);
            ed.set_notice(format!("loaded {name}"));
            ed.install_loaded(path, scope, text);
        }
        Err(e) => {
            log::error!("open {path} FAILED ({e:#})");
            ed.set_notice(format!("can't open {}", file_stem(&path)));
        }
    }
}

/// Unlink a file from the card (`:delete`). The editor has already dropped it
/// from its model and switched away, so this is pure IO plus the snackbar. For a
/// Tracked file the removal is left in the git working copy — the next `:sync`'s
/// `add --all` stages the deletion — so nothing git-specific happens here. A
/// failure keeps the file on disk and says so; the buffer has still switched, so
/// the file is recoverable by re-opening it.
fn delete_buffer(storage: &Storage, ed: &mut Editor, path: String, scope: Scope) {
    // Scope-qualified label (`repo/notes.md`), so the snackbar names exactly which
    // file left the card — and, for a Tracked file, that the removal is only local
    // until the next `:sync` publishes it (deleting from the card alone never
    // touches the remote — that mirrors how a Save is local until Publish).
    let label = path.strip_prefix("/sd/").unwrap_or(&path);
    match storage.delete_path(&path) {
        Ok(()) => {
            log::info!("deleted {path} ({scope:?})");
            ed.set_notice(match scope {
                Scope::Tracked => format!("deleted {label} - :sync to publish"),
                Scope::Local => format!("deleted {label}"),
            });
        }
        Err(e) => {
            log::error!("delete {path} FAILED ({e:#})");
            ed.set_notice(format!("delete FAILED: {label}"));
        }
    }
}

/// Enumerate the palette's openable files: the top-level regular files in
/// `/sd/repo` and `/sd/local`, as absolute paths. Skips dotfiles (so `.git`,
/// `.typoena.toml`, and the like never show) and anything that isn't a plain
/// file. Best-effort: an unreadable directory (e.g. no `/sd/local` yet)
/// contributes nothing rather than failing. The editor sorts and dedupes.
fn enumerate_files() -> Vec<String> {
    let mut out = Vec::new();
    for dir in [REPO_DIR, LOCAL_DIR] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            // Stat rather than trust d_type — FatFS's dirent type can read back
            // as unknown; a plain metadata call is reliable here.
            if !std::fs::metadata(&path).map(|m| m.is_file()).unwrap_or(false) {
                continue;
            }
            if let Some(p) = path.to_str() {
                out.push(p.to_string());
            }
        }
    }
    out
}

/// A file's display name — its basename without extension (`/sd/repo/notes.md`
/// → `notes`), for the snackbar. Falls back to the raw path if it has no stem.
fn file_stem(path: &str) -> &str {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
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
