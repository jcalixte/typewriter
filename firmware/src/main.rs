#[cfg(feature = "git")]
mod wizard_io;

use std::time::Instant;

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{AnyIOPin, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::spi::config::{Config, DriverConfig};
use esp_idf_svc::hal::spi::{Dma, SpiBusDriver, SpiDriver};
use esp_idf_svc::hal::units::FromValueType;

use display::Frame;
use editor::{
    Date, Editor, Effect, Mode, Prefs, Scope, Snippets, LOCAL_DIR, PREFS_PATH, REPO_DIR,
    SNIPPETS_PATH,
};
use firmware::epd::Epd;
use firmware::persistence::{Storage, CONF_PATH, NOTES};
use firmware::ui::{FocusTimer, Panel};
use firmware::usb_kbd;

/// Injected by build.rs so serial output identifies the exact build.
const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

/// How long input must pause before `save_on_idle` persists a dirty buffer.
/// The save is silent (no snackbar, no forced e-ink flash) — a safety net
/// against power loss, not a user action — so unlike the caret it can afford
/// to fire during a mid-sentence pause. The caret / longevity / focus timing
/// constants now live with the render engine ([`firmware::ui`]).
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
    // EPD SPI clock. Was 4 MHz; the panel (SSD1683) takes 10–20 MHz, and this
    // clock only affects the pixel clock-out, not the waveform BUSY time — so it
    // trims the pre-kick band write (~43 ms full-area at 4 MHz) off perceived
    // latency on the erase/caret/scroll path. Sweep higher (16/20 MHz) only
    // while watching the panel for signal-integrity glitches (garbled/missing
    // bands). See docs/tradeoff-curves/epd-refresh-latency.md.
    let bus = SpiBusDriver::new(spi, &Config::new().baudrate(20.MHz().into()))?;
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
    let storage = boot_storage(&mut epd);

    // The light build has no wizard (it can't clone), so it keeps the old
    // no-repo halt; the git build's repo check happens in the wizard gate
    // below, where a missing repo *enters setup* instead of halting.
    #[cfg(not(feature = "git"))]
    if !storage.repo_present() {
        let _ = CONF_PATH; // conf is consumed by the git build only
        boot_halt(
            &mut epd,
            "No repo on the SD card",
            "Provision it on your computer (just init) and reboot.",
        );
    }

    // Bring up the USB keyboard in the background; keys arrive via next_key().
    // Before the wizard gate — first-boot setup types on this keyboard.
    usb_kbd::start()?;

    // Device runtime config + the first-boot wizard gate (v0.9 onboarding).
    // The card's typoena.conf overrides the .env-baked TW_* per field
    // (slice 0). If the effective config is incomplete or the repo is missing,
    // the wizard runs *instead of* the editor (slice 2) and hands back the
    // completed conf; either way the result is installed before the git
    // thread spawns. Secrets stay out of the log — only which keys exist.
    #[cfg(feature = "git")]
    let (sys_loop, nvs, modem) = {
        use esp_idf_svc::eventloop::EspSystemEventLoop;
        use esp_idf_svc::nvs::EspDefaultNvsPartition;

        let sys_loop = EspSystemEventLoop::take()?;
        let nvs = EspDefaultNvsPartition::take()?;
        let mut modem = peripherals.modem;

        let card = match std::fs::read_to_string(CONF_PATH) {
            Ok(body) => conf::Conf::parse(&body),
            Err(_) => conf::Conf::default(),
        };
        let provided: Vec<&str> = conf::Field::ALL
            .iter()
            .filter(|f| !card.get(**f).trim().is_empty())
            .map(|f| f.conf_key())
            .collect();
        log::info!(
            "typoena.conf on card provides: {}",
            if provided.is_empty() { "nothing".into() } else { provided.join(", ") }
        );

        let effective = firmware::git_sync::effective_conf_from(&card);
        let unconfigured = !effective.missing_required().is_empty() || !storage.repo_present();
        // `:setup` reboots into the wizard prefilled (the running editor can't
        // reclaim the radio from the git thread). One-shot: clear the marker on
        // read so a power-pull mid-setup boots the editor, not setup again.
        let setup_requested = storage.setup_requested();
        if setup_requested {
            storage.clear_setup_request();
        }
        let final_conf = if unconfigured || setup_requested {
            if unconfigured {
                log::info!("unconfigured card (conf incomplete or repo missing) — entering the onboarding wizard");
            } else {
                log::info!(":setup requested — reopening the wizard prefilled from the card conf");
            }
            // The gate above asks "is the device usable?" (baked dev values can
            // answer yes). The wizard provisions the *card*, so it resumes from
            // the card's own state — never the baked fallback, which would skip
            // the very steps a blank card needs (and, on the author's device,
            // mask the whole flow by jumping straight to the repo step). `:setup`
            // (configured card, marker set) opens the reset menu instead.
            match wizard_io::run(&mut epd, &storage, card, setup_requested && !unconfigured, &sys_loop, &nvs, &mut modem) {
                Ok(c) => c,
                Err(e) => boot_halt(&mut epd, "Setup stopped", &format!("{e:#}")),
            }
        } else {
            card
        };
        firmware::git_sync::set_card_conf(final_conf);
        (sys_loop, nvs, modem)
    };

    // Editor preferences (.typoena.toml, git-tracked). Read before the boot
    // buffer is chosen (`open_last_on_boot` decides which file that is) and
    // before the first render (`line_numbers` shapes the opening frame). A
    // missing / unreadable / partial file falls back to defaults, so a fresh
    // card just works.
    let prefs = match storage.load_path(PREFS_PATH) {
        Ok(src) => Prefs::parse(&src),
        Err(_) => Prefs::default(),
    };
    log::info!("prefs: {prefs:?}");
    // Apply the configured timezone before anything reads the wall clock, so
    // `localtime_r` — and thus the `:inbox` note's dated name/title — reflects the
    // local calendar day. Empty (the default) leaves the ESP clock at UTC.
    if !prefs.timezone.is_empty() {
        apply_timezone(&prefs.timezone);
    }
    let (boot_path, boot_scope, saved) = boot_note(&mut epd, &storage, &prefs);

    // Spawn the dedicated git thread — the `:gp` publish transport. It owns
    // the Wi-Fi stack (brought up lazily on the first `:gp`, so the radio
    // stays off until you publish) and parks on `git_tx` until signalled; the
    // push runs off the UI loop, and its outcome returns on `git_rx` for the
    // snackbar. Behind the `git` feature so a light build carries no libgit2.
    #[cfg(feature = "git")]
    let (git_tx, git_rx) = {
        use firmware::git_sync::{run_git_service, GitOutcome, GitRequest, GIT_STACK};

        // sys_loop / nvs / modem come from the wizard-gate block above — the
        // wizard borrows the modem for its join test, then the git thread
        // owns all three for good.
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

    // Seed the editor from the boot note (`boot_note` above: the default
    // `/sd/repo/notes.md`, or the resumed last file when `open_last_on_boot`
    // is set). Boots in Normal mode with the caret on the last character (the
    // resume point) — press `i`/`a`/`o` to write.
    let mut ed = Editor::with_file(boot_path.clone(), boot_scope, saved);
    // Confirm the boot-load on the panel (no serial console in normal use):
    // "loaded <name>" using the note's filename without its suffix (notes.md ->
    // notes). Cleared by the first keystroke, like any snackbar.
    ed.set_notice(format!("loaded {}", file_stem(&boot_path)));
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
    // Loop state that isn't the panel's own. `idle_saved`: whether
    // `save_on_idle` already persisted the current idle window, so it fires once
    // per typing burst (and doesn't retry-storm if a save fails); reset on the
    // next activity. `last_file`: what the last-file marker was last written with
    // — starts empty so the first loop pass records the boot buffer, and the
    // marker then always names the active file, whether `open_last_on_boot`
    // currently reads it or not (flipping the pref on works from the very next
    // boot). `last_activity`: monotonic time of the last keystroke, for the caret
    // / save-on-idle / longevity debounces.
    let mut idle_saved = false;
    let mut last_file = String::new();
    let mut last_activity = Instant::now();
    // Focus mode (Pomodoro): off until `:focus`, then a silent monotonic block
    // timer (no wall clock, no live countdown). Driven by the FocusStart/Stop
    // effects below; the rest-card drop is `Panel::rest_if_due` in the idle branch.
    let mut focus = FocusTimer::default();

    // Keyboard attach/detach state drives the panel's disconnect flag; seed it
    // (and the word-count snapshot) before the first render.
    let mut last_kbd = usb_kbd::keyboard_present();
    ed.set_keyboard_present(last_kbd);
    ed.refresh_stats();

    // First editor render — the moment the splash disappears. Everything
    // mandatory is ready here: SD mounted, note loaded, prefs applied, input
    // running (the palette walk continues in the background). `Panel::new` draws
    // the opening frame and paints it as a full-area *partial* (~630 ms) that
    // first waits out the splash's waveform (which the boot work above
    // overlapped), so the splash→editor swap rides the partial instead of a
    // second full refresh — shaving ~1.3 s off cold boot. From here the panel
    // owns the EPD and both reused framebuffers (a repaint never allocates — a
    // background `:gp` can take the heap to the floor); every repaint goes
    // through it. See [`firmware::ui::Panel`].
    let mut panel = Panel::new(epd, &mut ed)?;

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

    // Feed the file palette (Ctrl-P) from a background walk — spawned only now,
    // AFTER the first editor frame is on the panel. Enumerating /sd/repo +
    // /sd/local takes seconds on a big tree (readdir-over-SPI bound) and the
    // palette isn't needed to type. Spawned earlier (pre-render), the walk
    // thread monopolized the FatFS volume lock and starved the main task's own
    // boot SD reads — the synchronous snippets load blocked ~5.4 s behind it
    // (2026-07-17 trace: cursor-ready 4.2 s → 7.3 s, and the 240 MHz CPU made
    // it worse by tightening the walk's readdir loop). By here every
    // critical-path read (prefs, note, snippets) and the first paint are done,
    // so the walk finally runs off the hot path. The list lands on `walk_rx`;
    // the idle branch feeds it to the editor (recents-only until then). A pull
    // re-feeds it the same way.
    let (walk_tx, walk_rx) = std::sync::mpsc::channel::<String>();
    spawn_file_walk(walk_tx.clone());

    loop {
        // Feed today's date from the wall clock each pass, so `:inbox` names/dates
        // the note for the current day even if the session crosses midnight or the
        // clock is only set mid-session by the first sync. `None` until the clock
        // is trustworthy (see `today_date`).
        ed.set_today(today_date());

        // Drain all queued keystrokes (type-ahead absorbed during a refresh),
        // apply them, then do a single refresh for the batch.
        let prev_mode = ed.mode(); // to detect leaving the Rest curtain below
        let mut keys = 0;
        while let Some(k) = usb_kbd::next_key() {
            let was_rest = ed.mode() == Mode::Rest;
            ed.handle(k);
            keys += 1;
            // Leaving the rest curtain (Ctrl-C / q / Esc) drops the rest of this
            // batch: a keyboard bump that triggered the exit can't then poke the
            // editor, so one accidental touch only ever lands you on a clean
            // Normal screen, never an edit.
            if was_rest && ed.mode() != Mode::Rest {
                while usb_kbd::next_key().is_some() {}
                break;
            }
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
                    Effect::Setup => {
                        // Reboot into the boot-time wizard: the running editor
                        // can't reclaim the radio from the git thread. The editor
                        // already refused if anything was unsaved, so the reboot
                        // loses nothing. Paint a notice with a blocking full
                        // refresh (visible before the reset), then restart — the
                        // boot gate sees the marker and re-enters the wizard.
                        #[cfg(feature = "git")]
                        match storage.request_setup() {
                            Ok(()) => {
                                ed.set_notice("opening setup - restarting...");
                                panel.blit_editor_full(&mut ed);
                                log::info!(":setup — rebooting into the wizard");
                                unsafe { esp_idf_svc::sys::esp_restart() };
                            }
                            Err(e) => {
                                log::warn!(":setup marker write failed: {e:#}");
                                ed.set_notice("setup: could not save marker");
                            }
                        }
                        #[cfg(not(feature = "git"))]
                        ed.set_notice(":setup needs the full firmware");
                    }
                    Effect::Reboot => {
                        // Clean restart (`:reboot`). No marker or radio hand-off
                        // like `:setup` needs — just paint the branded splash (the
                        // same circle+wordmark the panel boots back to, plus a
                        // "restarting..." line) so the reboot reads as intentional
                        // rather than a frozen frame. The editor already refused if
                        // anything was unsaved. Blocking full refresh so the frame
                        // is on the bistable panel before the reset fires; it then
                        // carries over the whole reboot into the boot splash.
                        log::info!(":reboot — restarting");
                        panel.blit_full(&Frame::reboot());
                        unsafe { esp_idf_svc::sys::esp_restart() };
                    }
                    Effect::FocusStart => {
                        // Begin (or, after a break, restart) a focus block: start
                        // the silent monotonic timer and snapshot the word count
                        // for the rest card. See docs/v0.7.5-focus-mode.md.
                        focus.start(ed.word_count());
                    }
                    Effect::FocusStop => focus.stop(),
                }
            }
        }

        // Keep the last-file marker on the active named buffer: any switch
        // (`:e`, palette pick, `:delete`'s fallback) lands here once its
        // effects have drained. An unnamed `:enew` scratch (empty path) keeps
        // the previous marker — there is nothing to resume into.
        if !ed.path().is_empty() && ed.path() != last_file {
            last_file = ed.path().to_string();
            storage.record_last_file(&last_file);
        }

        // Keyboard attach/detach feeds the panel's disconnect flag.
        let kbd = usb_kbd::keyboard_present();
        ed.set_keyboard_present(kbd);
        let kbd_changed = kbd != last_kbd;
        last_kbd = kbd;

        if keys == 0 {
            // Focus mode: a running block that has reached its length drops the
            // rest card at this typing pause — never mid-keystroke — or at a grace
            // cap if the writer never pauses (Panel::rest_if_due).
            if panel.rest_if_due(&mut ed, &focus, last_activity) {
                continue;
            }
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
                    GitOutcome::Pull(outcome) => {
                        // Pulled and Rebased both move the working copy under us
                        // (Rebased applies origin's tree *and* replants our commit
                        // on top); LocalAhead / UpToDate leave the tree untouched.
                        let moved_working_copy = matches!(
                            outcome,
                            PullOutcome::Pulled(_) | PullOutcome::Rebased(_)
                        );
                        let notice = match outcome {
                            PullOutcome::Pulled(oid) => format!("pulled {oid}"),
                            PullOutcome::Rebased(oid) => format!("rebased {oid} - :gp to publish"),
                            PullOutcome::UpToDate => "up to date".to_string(),
                            PullOutcome::LocalAhead => "ahead - :gp to publish".to_string(),
                            PullOutcome::Failed(reason) => reason,
                        };
                        if moved_working_copy {
                            // Stale resident buffers must re-read the disk. Clean
                            // parked buffers are dropped (they reload on the next
                            // switch), the clean active buffer is re-read now, and
                            // a RAM-dirty buffer is left alone — its edits win,
                            // last-writer-wins like the publish reconcile. The
                            // palette list is re-walked in the background for files
                            // the pull added or removed (it lands on `walk_rx` a
                            // few seconds later, instead of stalling the UI).
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
                        }
                        notice
                    }
                };
                ed.set_notice(notice);
                // Behind the rest curtain the panel is masked: keep the state
                // settlement above but defer the repaint — the notice shows when
                // the writer presses `c` and the editor repaints normally.
                if ed.mode() == Mode::Rest {
                    continue;
                }
                panel.show_notice(&mut ed);
                continue;
            }
            // A finished background file walk (boot or post-pull) feeds the
            // palette; repaint only if the visible frame changed — the list is
            // only visible through the (usually closed) palette overlay, so a
            // no-op full-area partial would be a pointless ~630 ms panel drive
            // (Panel::repaint_if_changed, which also preserves caret visibility).
            if let Ok(files) = walk_rx.try_recv() {
                ed.set_file_list_joined(files);
                panel.repaint_if_changed(&mut ed);
                continue;
            }
            // A connect/disconnect while idle must still repaint the panel flag —
            // no keystroke will arrive to trigger it otherwise.
            if panel.kbd_repaint(&mut ed, kbd_changed, kbd) {
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
            // Panel-longevity full refresh, deferred to a typing pause
            // (Panel::longevity_full), then the debounced Insert caret or a brief
            // CPU-yielding sleep (Panel::caret_or_sleep) — the tail of the idle
            // sequence, always last since there is nothing after it.
            if panel.longevity_full(&mut ed, last_activity) {
                continue;
            }
            panel.caret_or_sleep(&mut ed, last_activity);
            continue;
        }

        last_activity = Instant::now();
        idle_saved = false; // fresh activity reopens the save_on_idle window
        // Repaint the batch: the windowed/additive/full-area decision, the
        // force_full recovery, and the leaving-Rest full refresh all live in the
        // panel engine now (Panel::render_batch). `prev_mode` lets it detect the
        // Rest→editor swap; `keys` is only for the trace.
        panel.render_batch(&mut ed, prev_mode, keys);
    }
}

/// Apply a POSIX `TZ` string to libc so `localtime_r` reads the local calendar
/// day (see `Prefs::timezone`). newlib carries no zoneinfo database, so `tz` must
/// be the POSIX form (`CET-1CEST,M3.5.0,M10.5.0/3`), never an IANA name
/// (`Europe/Paris`) — the latter silently stays UTC. Best-effort: an interior NUL
/// or a failed `setenv` just leaves the previous zone (UTC) in place.
fn apply_timezone(tz: &str) {
    let Ok(c_tz) = std::ffi::CString::new(tz) else {
        log::warn!("timezone {tz:?} has an interior NUL; left at UTC");
        return;
    };
    // SAFETY: both pointers are valid C strings for the call; `tzset` reads the
    // `TZ` env var we just set. `1` = overwrite any existing value.
    unsafe {
        esp_idf_svc::sys::setenv(c"TZ".as_ptr(), c_tz.as_ptr(), 1);
        esp_idf_svc::sys::tzset();
    }
    log::info!("timezone applied: TZ={tz}");
}

/// Today's date from the wall clock, or `None` when the clock is not yet
/// trustworthy. The editor boot path never runs SNTP, so the clock sits at the
/// epoch until a `:gl`/`:gp` sync sets it this power cycle (no battery-backed
/// RTC) — a year before 2020 means "unset", and `:inbox` refuses rather than
/// dating a note `1970-01-01`. Honours the timezone applied by `apply_timezone`.
fn today_date() -> Option<Date> {
    let mut now: esp_idf_svc::sys::time_t = 0;
    let mut t: esp_idf_svc::sys::tm = unsafe { core::mem::zeroed() };
    // SAFETY: `now`/`t` are valid, owned locals; `time` fills `now`, `localtime_r`
    // fills `t` from it (the reentrant form writes into our `t`, no shared state).
    unsafe {
        esp_idf_svc::sys::time(&mut now);
        esp_idf_svc::sys::localtime_r(&now, &mut t);
    }
    let year = t.tm_year + 1900;
    if year < 2020 {
        return None; // clock unset (still at the epoch) — no sync yet this boot
    }
    Some(Date {
        year,
        month: (t.tm_mon + 1) as u32, // tm_mon is 0-11
        day: t.tm_mday as u32,
    })
}

/// Mount the SD card, or halt with the reason on the panel. A missing CARD is
/// fatal by design (see the boot-sequence comment in `main`): the note is the
/// whole point of the appliance, so we refuse to run in a state where the next
/// save could destroy it. A missing REPO is the caller's call — the git
/// build's wizard gate enters first-boot setup, the light build halts.
fn boot_storage(epd: &mut Epd) -> Storage {
    // A git build shares this mount with the git thread, and libgit2 keeps the
    // pack + idx descriptors open across a publish — that overruns the
    // editor's tight 4-FD budget, so mount with the 16-FD one (persistence.rs,
    // MAX_FILES_GIT). The light build keeps the editor's own budget.
    #[cfg(feature = "git")]
    let mounted = Storage::mount_for_git();
    #[cfg(not(feature = "git"))]
    let mounted = Storage::mount();
    match mounted {
        Ok(s) => s,
        Err(e) => boot_halt(epd, "SD card not ready", &format!("{e:#}")),
    }
}

/// Choose and load the boot buffer. With `open_last_on_boot` set and a marker
/// naming a still-existing file (`Storage::last_file`), resume that file;
/// otherwise the default note. Only the default note is fatal (`boot_halt`) —
/// a stale or unreadable last file falls back rather than refusing to boot.
fn boot_note(epd: &mut Epd, storage: &Storage, prefs: &Prefs) -> (String, Scope, String) {
    if prefs.open_last_on_boot {
        if let Some(path) = storage.last_file() {
            match storage.load_path(&path) {
                Ok(text) => {
                    log::info!("boot: resumed {path} ({} bytes)", text.len());
                    let scope = if path.starts_with(LOCAL_DIR) { Scope::Local } else { Scope::Tracked };
                    return (path, scope, text);
                }
                // Unreadable (e.g. grown past MAX_FILE_BYTES on a computer) —
                // the default note still boots.
                Err(e) => log::warn!("boot: can't resume {path} ({e:#}); falling back to {NOTES}"),
            }
        }
    }
    let note = match storage.load() {
        Ok(text) => text,
        Err(e) => boot_halt(epd, "Could not read your note", &format!("{e:#}")),
    };
    log::info!("boot: loaded {} bytes from {NOTES}", note.len());
    (NOTES.to_string(), Scope::Tracked, note)
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

// The panel-diff helpers (`changed_rows` / `erase_bbox`) and the render loop's
// paint machinery now live in [`firmware::ui`], shared with the `demo` bin.
