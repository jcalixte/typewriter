mod usb_kbd;

use std::time::Instant;

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{AnyIOPin, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::spi::config::{Config, DriverConfig};
use esp_idf_svc::hal::spi::{Dma, SpiBusDriver, SpiDriver};
use esp_idf_svc::hal::units::FromValueType;

use display::Frame;
use editor::{
    Editor, Effect, Mode, Prefs, Scope, Snippets, CH, LOCAL_DIR, PREFS_PATH, REPO_DIR,
    SNIPPETS_PATH,
};
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

/// How long input must pause before `save_on_idle` persists a dirty buffer.
/// Longer than the caret debounce so autosave settles after typing, not during
/// a mid-sentence pause. The save is silent (no snackbar, no forced e-ink
/// flash) — a safety net against power loss, not a user action.
const IDLE_SAVE_MS: u128 = 1500;

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
    // Boot splash (Spike 9): the Typoena mark, kicked off *async* — the ~2.2 s
    // full-refresh waveform runs while the SD mounts and the note loads below,
    // so the splash starts painting as early as the app can drive it and its
    // wait overlaps the mandatory boot work instead of preceding it. Its full
    // refresh doubles as the baseline the old white clear used to establish
    // (writes both RAM banks); the first editor render further down implicitly
    // waits it out (`wait_ready`) and then replaces it.
    epd.display_frame_async(Frame::splash().bytes())?;

    // Mount the SD and load the saved note. We bring the SD up *after* the EPD —
    // the doc's boot order is SD-first, but a dead panel can't explain a missing
    // card — and treat a missing card / repo / unreadable note as fatal: a
    // writing appliance that silently started empty would clobber the note on
    // the next `:w`. See docs/v0.1-mvp-technical.md, boot sequence.
    let (storage, saved) = boot_storage(&mut epd);

    // Feed the file palette (Ctrl-P) from a background walk. Enumerating
    // /sd/repo + /sd/local takes seconds on a big tree (4.3 s at 1098 files,
    // readdir-over-SPI bound) and the palette is not needed to type, so it
    // must not hold up the first editor frame. The list lands on `walk_rx` and
    // the idle branch of the main loop feeds it to the editor; until then the
    // palette shows recents only. A pull re-feeds it the same way.
    let (walk_tx, walk_rx) = std::sync::mpsc::channel::<String>();
    spawn_file_walk(walk_tx.clone());

    // Bring up the USB keyboard in the background; keys arrive via next_key().
    usb_kbd::start()?;

    // Spawn the dedicated git thread — the `:gp` publish transport. It owns
    // the Wi-Fi stack (brought up lazily on the first `:gp`, so the radio
    // stays off until you publish) and parks on `git_tx` until signalled; the
    // push runs off the UI loop, and its outcome returns on `git_rx` for the
    // snackbar. Behind the `git` feature so a light build carries no libgit2.
    #[cfg(feature = "git")]
    let (git_tx, git_rx) = {
        use esp_idf_svc::eventloop::EspSystemEventLoop;
        use esp_idf_svc::nvs::EspDefaultNvsPartition;
        use firmware::git_sync::{run_git_service, GitOutcome, GitRequest, GIT_STACK};

        let sys_loop = EspSystemEventLoop::take()?;
        let nvs = EspDefaultNvsPartition::take()?;
        let modem = peripherals.modem;
        let (req_tx, req_rx) = std::sync::mpsc::channel::<GitRequest>();
        let (res_tx, res_rx) = std::sync::mpsc::channel::<GitOutcome>();
        std::thread::Builder::new()
            .name("git".into())
            .stack_size(GIT_STACK)
            .spawn(move || run_git_service(modem, sys_loop, nvs, req_rx, res_tx))?;
        log::info!(
            "git thread up ({} KB stack); Wi-Fi comes up on the first :gp",
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
    // Editor preferences (.typoena.toml, git-tracked). Read before the first
    // render so `line_numbers` shapes the opening frame. A missing / unreadable /
    // partial file falls back to defaults, so a fresh card just works.
    let prefs = match storage.load_path(PREFS_PATH) {
        Ok(src) => Prefs::parse(&src),
        Err(_) => Prefs::default(),
    };
    log::info!("prefs: {prefs:?}");
    ed.set_prefs(prefs);
    // Snippet library (.typoena.snippets.json, git-tracked). Parsed with
    // serde_json in the editor crate; a missing / unreadable / malformed file is
    // non-fatal — the editor simply has no snippets and runs unchanged.
    let snippets = match storage.load_path(SNIPPETS_PATH) {
        Ok(src) => match Snippets::parse(&src) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("snippets parse FAILED ({e}); none loaded");
                Snippets::default()
            }
        },
        Err(_) => Snippets::default(),
    };
    log::info!("snippets: {} loaded", snippets.0.len());
    ed.set_snippets(snippets);
    let mut updates: u32 = 0;
    let mut cursor_shown = true; // the initial render includes the caret
    let mut last_activity = Instant::now();
    // Whether `save_on_idle` already persisted the current idle window, so it
    // fires once per typing burst (and doesn't retry-storm if a save fails).
    // Reset on the next activity.
    let mut idle_saved = false;
    // Set when a paint fails (see the refresh block below): the next paint then
    // does a full refresh to re-establish both RAM banks, since a partial that
    // died mid-transfer may have left them inconsistent.
    let mut force_full = false;

    // Keyboard attach/detach state drives the panel's disconnect flag; seed it
    // (and the word-count snapshot) before the first render.
    let mut last_kbd = usb_kbd::keyboard_present();
    ed.set_keyboard_present(last_kbd);
    ed.refresh_stats();

    // First editor render — the moment the splash disappears. Everything
    // mandatory is ready here: SD mounted, note loaded, prefs applied, input
    // running (the palette walk continues in the background). The splash's
    // full refresh already seeded both RAM banks (its image is the `0x26`
    // "previous" baseline) — the partial below first waits out its waveform
    // (`wait_ready`), which the boot work above overlapped — so the editor
    // comes up with a full-area *partial* (~630 ms) instead of a second full
    // refresh (~1.9 s): the splash→editor swap rides the partial waveform,
    // shaving ~1.3 s off cold boot. This large-area partial is the one boot
    // refresh worth eyeballing for ghosting; the loop's periodic full refresh
    // (every FULL_REFRESH_EVERY updates) clears any residue.
    let mut shown = ed.draw(true);
    epd.display_frame_partial_window(shown.bytes(), 0, epd::HEIGHT)?;
    // The only two framebuffers the loop ever uses, both allocated here at
    // boot: every repaint below renders into `back` (`draw_into` reuses its
    // allocation) and swaps it with `shown` on success. A repaint must never
    // allocate — a background `:gp` push can take the heap to the floor, and
    // a failed `Vec` alloc aborts the whole app (the 2026-07-13 OOM: 66 s into
    // the push, one HalfPageUp repaint died on a 27 KB framebuffer).
    let mut back = Frame::new_white();

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
        // `:gp` queues a Save of the current buffer *then* Publish. Save/Load
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
                        // pure git publish of the recorded dirty paths — the
                        // outcome decides whether the snapshot is forgotten
                        // (publish_succeeded) or retried (publish_failed).
                        #[cfg(feature = "git")]
                        {
                            use firmware::git_sync::{GitRequest, PublishRequest};
                            let paths = storage.take_dirty();
                            match git_tx.send(GitRequest::Publish(PublishRequest { paths })) {
                                Ok(()) => ed.set_notice("syncing..."),
                                Err(_) => {
                                    // Thread gone — nothing will report back, so
                                    // return the snapshot to pending ourselves.
                                    storage.publish_failed();
                                    ed.set_notice("sync: git thread down");
                                }
                            }
                        }
                        #[cfg(not(feature = "git"))]
                        log::info!(":gp — saved; light build (no `git` feature) — push skipped");
                    }
                    Effect::Pull => {
                        // `:gl` — fetch + fast-forward, on the git thread like a
                        // publish. Gated on an empty dirty journal: unpublished
                        // saves would fight the checkout, and `:gp` first is
                        // the appliance's natural order anyway. (A RAM-dirty
                        // buffer that was never saved doesn't gate — its edits
                        // simply win over the pulled state, see the outcome
                        // handler below.)
                        #[cfg(feature = "git")]
                        {
                            use firmware::git_sync::GitRequest;
                            if storage.has_dirty() {
                                // Log it too — on the 2026-07-14 run this gate
                                // firing looked like a silent no-op in the
                                // serial log.
                                log::info!(":gl refused — dirty journal non-empty; :gp first");
                                ed.set_notice("pull: unsynced changes - :gp first");
                            } else {
                                match git_tx.send(GitRequest::Pull) {
                                    Ok(()) => ed.set_notice("pulling..."),
                                    Err(_) => ed.set_notice("pull: git thread down"),
                                }
                            }
                        }
                        #[cfg(not(feature = "git"))]
                        log::info!(":gl — light build (no `git` feature) — pull skipped");
                    }
                    Effect::Delete { path, scope } => delete_buffer(&storage, &mut ed, path, scope),
                    Effect::SavePrefs { contents } => save_prefs(&storage, &mut ed, &contents),
                }
            }
        }

        // Keyboard attach/detach feeds the panel's disconnect flag.
        let kbd = usb_kbd::keyboard_present();
        ed.set_keyboard_present(kbd);
        let kbd_changed = kbd != last_kbd;
        last_kbd = kbd;

        if keys == 0 {
            // A finished git operation reports its outcome here (it ran on the
            // git thread while we idled). Show it in the snackbar with a silent
            // full-area partial — no keystroke will arrive to trigger a repaint.
            #[cfg(feature = "git")]
            if let Ok(outcome) = git_rx.try_recv() {
                use firmware::git_sync::{GitOutcome, PublishOutcome, PullOutcome};
                let notice = match outcome {
                    GitOutcome::Publish(outcome) => {
                        // Settle the dirty snapshot this publish took: confirmed
                        // published (or up to date) → forget it; failed → back to
                        // pending so the next :gp retries the same paths.
                        match &outcome {
                            PublishOutcome::Pushed(_) | PublishOutcome::UpToDate => {
                                storage.publish_succeeded()
                            }
                            PublishOutcome::Failed(_) => storage.publish_failed(),
                        }
                        match outcome {
                            PublishOutcome::Pushed(oid) => format!("synced {oid}"),
                            PublishOutcome::UpToDate => "up to date".to_string(),
                            PublishOutcome::Failed(reason) => reason,
                        }
                    }
                    GitOutcome::Pull(outcome) => match outcome {
                        // The working copy moved under us: stale resident
                        // buffers must re-read the disk. Clean parked buffers
                        // are dropped (they reload on the next switch), the
                        // clean active buffer is re-read now, and a RAM-dirty
                        // buffer is left alone — its edits win, last-writer-
                        // wins like the publish reconcile. The palette list is
                        // re-walked in the background for files the pull added
                        // or removed (it lands on `walk_rx` a few seconds
                        // later, instead of stalling the UI for the walk).
                        PullOutcome::Pulled(oid) => {
                            ed.drop_clean_parked();
                            if ed.dirty() {
                                log::info!(
                                    "post-pull: {} is RAM-dirty — kept (its edits win)",
                                    ed.path()
                                );
                            } else if !ed.path().is_empty() {
                                match storage.load_path(ed.path()) {
                                    Ok(text) => ed.refresh_active(text),
                                    Err(e) => log::warn!(
                                        "post-pull reload of {} FAILED ({e:#}); buffer kept",
                                        ed.path()
                                    ),
                                }
                            }
                            spawn_file_walk(walk_tx.clone());
                            format!("pulled {oid}")
                        }
                        PullOutcome::UpToDate => "up to date".to_string(),
                        PullOutcome::LocalAhead => "ahead - :gp to publish".to_string(),
                        PullOutcome::Diverged => "diverged - resolve on a computer".to_string(),
                        PullOutcome::Failed(reason) => reason,
                    },
                };
                ed.set_notice(notice);
                ed.draw_into(&mut back, true);
                if let Err(e) = epd.display_frame_partial_window(back.bytes(), 0, epd::HEIGHT) {
                    log::warn!("sync-notice repaint FAILED ({e}); full refresh next");
                    force_full = true;
                    continue;
                }
                std::mem::swap(&mut shown, &mut back);
                cursor_shown = true;
                continue;
            }
            // A finished background file walk (boot or post-pull) feeds the
            // palette. Repaint only if the visible frame changed — the list
            // is only visible through the palette overlay, which is usually
            // closed, and a no-op full-area partial would be a pointless
            // ~630 ms panel drive. Caret visibility is passed through
            // unchanged so this can't reveal a debounced Insert caret early.
            if let Ok(files) = walk_rx.try_recv() {
                ed.set_file_list_joined(files);
                ed.draw_into(&mut back, cursor_shown);
                if changed_rows(shown.bytes(), back.bytes()).is_some() {
                    if let Err(e) = epd.display_frame_partial_window(back.bytes(), 0, epd::HEIGHT) {
                        log::warn!("palette repaint FAILED ({e}); full refresh next");
                        force_full = true;
                        continue;
                    }
                    std::mem::swap(&mut shown, &mut back);
                }
                continue;
            }
            // A connect/disconnect while idle must still repaint the panel flag —
            // no keystroke will arrive to trigger it otherwise.
            if kbd_changed {
                ed.draw_into(&mut back, true);
                if let Err(e) = epd.display_frame_partial_window(back.bytes(), 0, epd::HEIGHT) {
                    log::warn!("kbd-flag repaint FAILED ({e}); full refresh next");
                    force_full = true;
                    continue;
                }
                std::mem::swap(&mut shown, &mut back);
                cursor_shown = true;
                log::info!("keyboard {}", if kbd { "connected" } else { "disconnected" });
                continue;
            }
            // save_on_idle: once input has paused, quietly persist a dirty named
            // buffer so a power pull can't cost more than the last couple seconds.
            // Silent — no snackbar and no forced e-ink flash (a safety net, not an
            // action; `:w` is the loud save). Unformatted: fmt only runs on an
            // explicit `:w`/`:gp`, never reflowing text mid-session. Fires once
            // per idle window (`idle_saved`), so a failing save can't busy-loop.
            if !idle_saved
                && ed.prefs().save_on_idle
                && ed.dirty()
                && !ed.path().is_empty()
                && last_activity.elapsed().as_millis() >= IDLE_SAVE_MS
            {
                idle_saved = true;
                let path = ed.path().to_string();
                match storage.save_path(&path, ed.text()) {
                    Ok(()) => {
                        log::info!("idle-save: {} bytes to {path}", ed.text().len());
                        ed.mark_saved(&path);
                    }
                    Err(e) => log::warn!("idle-save FAILED ({e:#}); buffer kept in RAM"),
                }
                // No repaint: `dirty` clearing has no visible effect, and a flash
                // here would defeat the point. Fall through to the caret/idle path.
            }
            // Debounced caret, Insert mode only: once typing pauses, bring the
            // bar caret back and refresh the panel word count with a silent
            // full-area partial (no flash). Normal/View draw their caret on action.
            if ed.mode() == Mode::Insert
                && !cursor_shown
                && last_activity.elapsed().as_millis() >= CURSOR_DEBOUNCE_MS
            {
                ed.refresh_stats();
                ed.draw_into(&mut back, true);
                if let Err(e) = epd.display_frame_partial_window(back.bytes(), 0, epd::HEIGHT) {
                    log::warn!("caret repaint FAILED ({e}); full refresh next");
                    force_full = true;
                } else {
                    std::mem::swap(&mut shown, &mut back);
                    cursor_shown = true;
                    log::info!("caret shown");
                }
            } else {
                FreeRtos::delay_ms(8);
            }
            continue;
        }

        last_activity = Instant::now();
        idle_saved = false; // fresh activity reopens the save_on_idle window
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
        ed.draw_into(&mut back, insert_cursor_on);
        let scrolled = ed.scroll_top() != prev_scroll;

        // Only the rows that changed since the last shown frame need updating.
        let Some((y0, y1)) = changed_rows(shown.bytes(), back.bytes()) else {
            cursor_shown = ed.mode() != Mode::Insert;
            continue; // no visible change (the frames are identical — no swap needed)
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
            && only_adds_ink(shown.bytes(), back.bytes(), y0, y1);

        let t0 = Instant::now();
        // `force_full` promotes to a full refresh after a failed paint: it
        // rewrites both RAM banks, recovering from a partial that may have died
        // mid-transfer and desynced them.
        let (result, refresh) = if periodic || force_full {
            (epd.display_frame(back.bytes()), "FULL")
        } else if additive {
            (epd.display_frame_partial_window(back.bytes(), y0, y1 - y0 + 1), "windowed")
        } else {
            (epd.display_frame_partial_window(back.bytes(), 0, epd::HEIGHT), "full-area")
        };
        let ms = t0.elapsed().as_millis();
        if let Err(e) = result {
            // Never fatal — the buffer is the source of truth and safe in RAM,
            // exactly like a failed `save_buffer`. Drop this frame, leave `shown`
            // untouched so the next paint repaints the same diff, and force a
            // clean full refresh then. Typical cause: internal DMA-capable RAM
            // briefly starved by Wi-Fi/TLS during a background `:gp`; it frees
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
        std::mem::swap(&mut shown, &mut back);
        cursor_shown = ed.mode() != Mode::Insert;
    }
}

/// Mount the SD card and load the saved note, or halt with the reason on the
/// panel. Everything here is fatal by design (see the boot-sequence comment in
/// `main`): the note is the whole point of the appliance, so we refuse to run
/// in a state where the next save could destroy it.
fn boot_storage(epd: &mut Epd) -> (Storage, String) {
    // A git build shares this mount with the git thread, and libgit2 keeps the
    // pack + idx descriptors open across a publish — that overruns the
    // editor's tight 4-FD budget, so mount with the 16-FD one (persistence.rs,
    // MAX_FILES_GIT). The light build keeps the editor's own budget.
    #[cfg(feature = "git")]
    let mounted = Storage::mount_for_git();
    #[cfg(not(feature = "git"))]
    let mounted = Storage::mount();
    let storage = match mounted {
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

/// Persist the preferences file after a palette `>` command changed a pref
/// (`Effect::SavePrefs`). The editor already applied the change live and
/// serialized it; this is a plain atomic write to the fixed `.typoena.toml`
/// path. Under `/sd/repo`, so it rides the next `:gp` to other devices.
fn save_prefs(storage: &Storage, ed: &mut Editor, contents: &str) {
    match storage.save_path(PREFS_PATH, contents) {
        Ok(()) => log::info!("prefs saved to {PREFS_PATH}"),
        Err(e) => {
            log::error!("prefs save FAILED ({e:#})");
            ed.set_notice("prefs save FAILED");
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
/// Tracked file the removal is left in the git working copy — the next `:gp`'s
/// `add --all` stages the deletion — so nothing git-specific happens here. A
/// failure keeps the file on disk and says so; the buffer has still switched, so
/// the file is recoverable by re-opening it.
fn delete_buffer(storage: &Storage, ed: &mut Editor, path: String, scope: Scope) {
    // Scope-qualified label (`repo/notes.md`), so the snackbar names exactly which
    // file left the card — and, for a Tracked file, that the removal is only local
    // until the next `:gp` publishes it (deleting from the card alone never
    // touches the remote — that mirrors how a Save is local until Publish).
    let label = path.strip_prefix("/sd/").unwrap_or(&path);
    match storage.delete_path(&path) {
        Ok(()) => {
            log::info!("deleted {path} ({scope:?})");
            ed.set_notice(match scope {
                Scope::Tracked => format!("deleted {label} - :gp to publish"),
                Scope::Local => format!("deleted {label}"),
            });
        }
        Err(e) => {
            log::error!("delete {path} FAILED ({e:#})");
            ed.set_notice(format!("delete FAILED: {label}"));
        }
    }
}

/// Enumerate the palette's openable files: the regular files under `/sd/repo`
/// and `/sd/local`, recursively, as absolute paths — **one newline-joined
/// blob**, not a `Vec<String>`. 1099 paths as individual small `String`s
/// measured 182 KB of *internal* DRAM resident (each stays under the 16 KB
/// SPIRAM-malloc threshold, plus per-alloc overhead), which starved the SD DMA
/// pool during the first on-device pull (2026-07-14). The blob is seeded past
/// the threshold so it and its growth reallocs land in PSRAM. Skips dot
/// entries at every level (so `.git` and its thousands of object files never
/// get walked). Best-effort: an unreadable directory (e.g. no `/sd/local`
/// yet) contributes nothing rather than failing. The editor sorts and dedupes
/// span-side. Runs on the `walk` thread (`spawn_file_walk`); on a big repo
/// the FAT directory IO is the cost to watch (~4 ms/file over SPI).
fn enumerate_files() -> String {
    let start = std::time::Instant::now();
    // 64 KB seed: comfortably past the 16 KB SPIRAM threshold and roomy enough
    // that a ~1100-file tree never reallocs.
    let mut out = String::with_capacity(64 * 1024);
    let mut count = 0usize;
    for dir in [REPO_DIR, LOCAL_DIR] {
        walk_files(std::path::Path::new(dir), 0, &mut out, &mut count);
    }
    log::info!("file walk: {count} files in {}ms", start.elapsed().as_millis());
    out
}

/// Run [`enumerate_files`] on its own short-lived thread and send the result
/// over `tx`; the main loop's idle branch feeds it to the editor. Off the boot
/// path (and off the UI loop on a post-pull re-walk) because the walk takes
/// seconds on a big tree and the palette is not mandatory for typing. The
/// walk is pure directory reads, serialized against the editor's and the git
/// thread's SD traffic by the FatFS volume lock. Bracketed with internal-DRAM
/// readings to confirm the interned blob keeps the list out of internal
/// (pre-interning: 182 KB resident; expected now: ~0, the spans only).
fn spawn_file_walk(tx: std::sync::mpsc::Sender<String>) {
    // Explicit stack: the default pthread stack (4 KB) is tight for 8 levels
    // of readdir recursion plus FatFS underneath.
    let spawned = std::thread::Builder::new()
        .name("walk".into())
        .stack_size(16 * 1024)
        .spawn(move || {
            let dram_before = internal_free_heap();
            let files = enumerate_files();
            let dram_after = internal_free_heap();
            log::info!(
                "file list: internal heap {dram_before} -> {dram_after} ({} KB consumed), blob {} KB",
                dram_before.saturating_sub(dram_after) / 1024,
                files.len() / 1024
            );
            let _ = tx.send(files); // receiver gone = shutting down; nothing to do
        });
    if let Err(e) = spawned {
        log::warn!("file-walk thread spawn FAILED ({e}); palette list not refreshed");
    }
}

/// Depth bound for [`walk_files`] — belt-and-braces against pathological
/// nesting on a hand-edited card; notes trees are a couple of levels deep.
const WALK_MAX_DEPTH: usize = 8;

/// Recursive helper for [`enumerate_files`]: push `dir`'s files onto `out`,
/// then descend into its subdirectories. Reads each directory fully before
/// recursing (the `remove_dir_recursive` pattern in `git_sync`), so only one
/// FatFS directory handle is open at a time regardless of depth — relevant on
/// the FD-bounded SD mount.
fn walk_files(dir: &std::path::Path, depth: usize, out: &mut String, count: &mut usize) {
    if depth > WALK_MAX_DEPTH {
        log::warn!("file walk: {} exceeds depth {WALK_MAX_DEPTH}, skipped", dir.display());
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    // Keep the dirent's own file type — a per-entry `metadata()` stat re-walks
    // the directory by path every time (~32ms/file on the SD card; it turned a
    // 1098-file walk into 35s). But the type needs decoding: esp-idf's
    // dirent.h says DT_REG=1 / DT_DIR=2, and std was built against libc
    // 0.2.178, which had no espidf overrides (they arrived in 0.2.186) and
    // falls back to the generic unix table — DT_FIFO=1, DT_CHR=2, DT_DIR=4,
    // DT_REG=8. Through std's eyes every card file is a "fifo" and every
    // directory a "char device": is_file()/is_dir() never matched, and the
    // 2026-07-13 walk dropped all 1157 files in 49ms. FAT can't hold fifos or
    // device nodes, so reading fifo-as-file / chardev-as-dir is unambiguous
    // here, and the is_file()/is_dir() arms take over the day the toolchain's
    // libc catches up. A type matching neither pair pays the one stat rather
    // than being silently dropped.
    use std::os::unix::fs::FileTypeExt;
    let children: Vec<_> = entries
        .flatten()
        .filter_map(|e| e.file_type().ok().map(|t| (e.path(), t)))
        .collect();
    for (path, ftype) in children {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let (is_file, is_dir) = if ftype.is_file() || ftype.is_fifo() {
            (true, false)
        } else if ftype.is_dir() || ftype.is_char_device() {
            (false, true)
        } else {
            match std::fs::metadata(&path) {
                Ok(m) => (m.is_file(), m.is_dir()),
                Err(_) => continue,
            }
        };
        if is_file {
            if let Some(p) = path.to_str() {
                out.push_str(p);
                out.push('\n');
                *count += 1;
            }
        } else if is_dir {
            walk_files(&path, depth + 1, out, count);
        }
    }
}

/// Free internal DRAM (excludes the 8 MB PSRAM pool, which dominates the total
/// free-heap number and masks DRAM exhaustion). Same reading `git_sync` logs.
fn internal_free_heap() -> u32 {
    use esp_idf_svc::sys;
    unsafe { sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL) as u32 }
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
