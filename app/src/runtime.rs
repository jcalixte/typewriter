//! The editor run loop, lifted from `firmware/src/main.rs` into a host-testable
//! application service.
//!
//! [`Runtime`] owns the [`Editor`], the [`Panel`], the [`FocusTimer`], and every
//! injected port, and drives one iteration per [`tick`](Runtime::tick). It is a
//! faithful lift of what used to be inline in the binary's loop — the same key
//! drain, the same effect servicing, the same idle sequence and ordering — with
//! the concrete hardware calls replaced by port calls. The composition root
//! (the firmware binary) keeps only what is genuinely platform: bringing up the
//! peripherals, spawning the git and file-walk threads, and choosing which port
//! adapters to inject.
//!
//! It is generic only over the [`Screen`] (the per-pixel hot path stays static
//! dispatch, via [`Panel`]); the remaining ports are trait objects built once at
//! composition, so the struct has a single type parameter and the tests inject
//! in-memory doubles.

use std::time::Instant;

use display::Frame;
use editor::{Editor, Effect, Mode, Scope, PREFS_PATH};
use hal::{Keyboard, Screen};

use crate::ports::{
    Clock, FileIndex, PublishDispatch, PublishOutcome, PullDispatch, PullOutcome, SetupDispatch,
    Storage, NetOutcome, NetService, System, UpdateDispatch, UpdateOutcome,
};
use crate::render::{FocusTimer, Panel};

/// How long input must pause before `save_on_idle` persists a dirty buffer. The
/// save is silent (no snackbar, no forced e-ink flash) — a safety net against
/// power loss, not a user action — so unlike the caret it can fire during a
/// mid-sentence pause.
const IDLE_SAVE_MS: u128 = 1500;

/// The editor run loop: owns the editor, the panel, and the injected ports.
pub struct Runtime<S: Screen> {
    ed: Editor,
    panel: Panel<S>,
    keyboard: Box<dyn Keyboard>,
    storage: Box<dyn Storage>,
    net: Box<dyn NetService>,
    clock: Box<dyn Clock>,
    system: Box<dyn System>,
    files: Box<dyn FileIndex>,
    /// Focus-mode (Pomodoro) block timer — off until `:focus`.
    focus: FocusTimer,
    /// Monotonic time of the last keystroke, for the caret / save-on-idle /
    /// longevity debounces.
    last_activity: Instant,
    /// Whether `save_on_idle` already persisted the current idle window, so it
    /// fires once per typing burst; reset on the next activity.
    idle_saved: bool,
    /// What the last-file marker was last written with — starts empty so the
    /// first pass records the boot buffer.
    last_file: String,
    /// Keyboard attach state, for the panel disconnect flag.
    last_kbd: bool,
    /// The current-pass keyboard state and whether it changed (set in `tick`,
    /// read by the idle branch's kbd-flag repaint).
    kbd: bool,
    kbd_changed: bool,
}

impl<S: Screen> Runtime<S> {
    /// Assemble the runtime after boot: the editor is already seeded (boot note,
    /// prefs, snippets, keyboard flag, first stats) and the panel has painted the
    /// first frame. Seeds the loop bookkeeping from the keyboard's current state.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ed: Editor,
        panel: Panel<S>,
        keyboard: Box<dyn Keyboard>,
        storage: Box<dyn Storage>,
        net: Box<dyn NetService>,
        clock: Box<dyn Clock>,
        system: Box<dyn System>,
        files: Box<dyn FileIndex>,
    ) -> Self {
        let last_kbd = keyboard.keyboard_present();
        Self {
            ed,
            panel,
            keyboard,
            storage,
            net,
            clock,
            system,
            files,
            focus: FocusTimer::default(),
            last_activity: Instant::now(),
            idle_saved: false,
            last_file: String::new(),
            last_kbd,
            kbd: last_kbd,
            kbd_changed: false,
        }
    }

    /// Run forever. The only exits are `:reboot`/`:setup`, which restart the
    /// device (so [`System::reboot`] diverges); the loop itself never returns.
    pub fn run(&mut self) -> ! {
        loop {
            self.tick();
        }
    }

    /// One loop iteration: drain the keyboard, service the queued effects, then
    /// either repaint the batch or run the idle sequence. Split out so the host
    /// tests can drive single passes.
    pub fn tick(&mut self) {
        // Feed today's date each pass, so a session crossing midnight (or one
        // whose clock is only set mid-session by the first sync) sees the current
        // day. `None` until the clock is trustworthy.
        self.ed.set_today(self.clock.today());

        let prev_mode = self.ed.mode(); // to detect leaving the Rest curtain
        let keys = self.drain_keys();

        // Service the effects the batch queued, draining to empty: servicing a
        // Load can itself queue an eviction Save that must be persisted now.
        self.service_effects();

        // Keep the last-file marker on the active named buffer. An unnamed
        // scratch (empty path) keeps the previous marker.
        if !self.ed.path().is_empty() && self.ed.path() != self.last_file {
            self.last_file = self.ed.path().to_string();
            self.storage.record_last_file(&self.last_file);
        }

        // Keyboard attach/detach feeds the panel's disconnect flag.
        self.kbd = self.keyboard.keyboard_present();
        self.ed.set_keyboard_present(self.kbd);
        self.kbd_changed = self.kbd != self.last_kbd;
        self.last_kbd = self.kbd;

        if keys == 0 {
            self.idle_step();
            return;
        }

        self.last_activity = Instant::now();
        self.idle_saved = false; // fresh activity reopens the save_on_idle window
        self.panel.render_batch(&mut self.ed, prev_mode, keys);
    }

    /// Drain all queued keystrokes (type-ahead absorbed during a refresh), apply
    /// them, and return the count. Leaving the Rest curtain drops the rest of the
    /// batch so an accidental bump only ever lands on a clean Normal screen.
    fn drain_keys(&mut self) -> u32 {
        let mut keys = 0;
        while let Some(k) = self.keyboard.next_key() {
            let was_rest = self.ed.mode() == Mode::Rest;
            self.ed.handle(k);
            keys += 1;
            if was_rest && self.ed.mode() != Mode::Rest {
                while self.keyboard.next_key().is_some() {}
                break;
            }
        }
        keys
    }

    /// Service the host-side effects the key batch queued, in order, draining to
    /// empty. The queue strictly shrinks (a Save/Publish/Pull queues nothing; a
    /// Load queues at most one eviction Save), so this terminates.
    fn service_effects(&mut self) {
        loop {
            let effects = self.ed.take_effects();
            if effects.is_empty() {
                break;
            }
            for effect in effects {
                self.service_one(effect);
            }
        }
    }

    fn service_one(&mut self, effect: Effect) {
        match effect {
            Effect::Save { path, contents, .. } => self.save_buffer(&path, &contents),
            Effect::Load { path, scope } => self.open_buffer(path, scope),
            // Non-blocking: the ~10 s push never stalls the editor; the outcome
            // returns via `poll_outcome` in the idle branch. The Save that
            // preceded this in the batch already persisted the buffer.
            Effect::Publish => match self.net.publish() {
                PublishDispatch::Dispatched => self.ed.set_notice("syncing..."),
                PublishDispatch::ThreadDown => self.ed.set_notice("sync: git thread down"),
            },
            Effect::Pull { commit_dirty } => match self.net.pull(commit_dirty) {
                PullDispatch::Dispatched => self.ed.set_notice("pulling..."),
                // Unpublished saves: ask before folding them into a commit. On
                // `y` the editor re-queues Pull { commit_dirty: true }.
                PullDispatch::NeedsCommitConfirm => self.ed.confirm_pull_commit(),
                PullDispatch::ThreadDown => self.ed.set_notice("pull: git thread down"),
            },
            Effect::Delete { path, scope } => self.delete_buffer(path, scope),
            Effect::SavePrefs { contents } => self.save_prefs(&contents),
            Effect::Setup => match self.system.prepare_setup() {
                SetupDispatch::Ready => {
                    // Paint the notice with a blocking full refresh (visible
                    // before the reset), then restart into the boot-time wizard.
                    self.ed.set_notice("opening setup - restarting...");
                    self.panel.blit_editor_full(&mut self.ed);
                    log::info!(":setup — rebooting into the wizard");
                    self.system.reboot();
                }
                SetupDispatch::MarkerFailed => self.ed.set_notice("setup: could not save marker"),
            },
            Effect::Reboot => {
                // Paint the branded splash (so the reboot reads as intentional),
                // then restart; the bistable panel carries it into the boot splash.
                log::info!(":reboot — restarting");
                self.panel.blit_full(&Frame::reboot());
                self.system.reboot();
            }
            // Non-blocking, like Publish: the multi-second download + flash runs on
            // the radio-owning thread while the editor keeps running; the terminal
            // outcome returns via `poll_outcome` in the idle branch. `:update` was
            // gated on a clean buffer set in the editor, so the eventual reboot on
            // success (see `handle_net_outcome`) can't strand unsaved edits.
            Effect::Update => match self.net.update() {
                UpdateDispatch::Dispatched => self.ed.set_notice("checking for update..."),
                UpdateDispatch::ThreadDown => self.ed.set_notice("update: git thread down"),
            },
            Effect::FocusStart => self.focus.start(self.ed.word_count()),
            Effect::FocusStop => self.focus.stop(),
        }
    }

    /// The idle branch: the same sequence and ordering as the old inline loop.
    /// Each rung that paints returns early (the old `continue`); `save_on_idle`
    /// deliberately falls through to the longevity/caret tail.
    fn idle_step(&mut self) {
        // Focus mode: a running block that has reached its length drops the rest
        // card at this typing pause.
        if self.panel.rest_if_due(&mut self.ed, &self.focus, self.last_activity) {
            return;
        }
        // A finished git operation reports its outcome here (it ran on the git
        // thread while we idled).
        if let Some(outcome) = self.net.poll_outcome() {
            self.handle_net_outcome(outcome);
            return;
        }
        // A finished background file walk (boot or post-pull) feeds the palette;
        // repaint only if the visible frame changed.
        if let Some(files) = self.files.poll_result() {
            self.ed.set_file_list_joined(files);
            self.panel.repaint_if_changed(&mut self.ed);
            return;
        }
        // A connect/disconnect while idle must still repaint the panel flag.
        if self.panel.kbd_repaint(&mut self.ed, self.kbd_changed, self.kbd) {
            return;
        }
        // save_on_idle: once input has paused, quietly persist a dirty named
        // buffer. Silent — no snackbar, no forced flash. Fires once per idle
        // window, so a failing save can't busy-loop. Falls through afterwards.
        if !self.idle_saved
            && self.ed.prefs().save_on_idle
            && self.ed.dirty()
            && !self.ed.path().is_empty()
            && self.last_activity.elapsed().as_millis() >= IDLE_SAVE_MS
        {
            self.idle_saved = true;
            let path = self.ed.path().to_string();
            match self.storage.save_path(&path, self.ed.text()) {
                Ok(()) => {
                    log::info!("idle-save: {} bytes to {path}", self.ed.text().len());
                    self.ed.mark_saved(&path);
                }
                Err(e) => log::warn!("idle-save FAILED ({e:#}); buffer kept in RAM"),
            }
        }
        // Panel-longevity full refresh, deferred to a typing pause, then the
        // debounced Insert caret or a brief CPU-yielding sleep — the tail.
        if self.panel.longevity_full(&mut self.ed, self.last_activity) {
            return;
        }
        if !self.panel.caret_if_due(&mut self.ed, self.last_activity) {
            self.clock.idle_yield();
        }
    }

    /// Handle a finished sync operation's outcome: settle the notice, and (for a
    /// pull that moved the working copy) reload the stale active buffer and
    /// re-walk the palette. The dirty-journal settlement already happened inside
    /// the sync backend before this returned.
    fn handle_net_outcome(&mut self, outcome: NetOutcome) {
        let notice = match outcome {
            NetOutcome::Publish(o) => publish_notice(&o),
            NetOutcome::Pull(o) => {
                // Pulled and Rebased both move the working copy under us; the
                // stale resident buffers must re-read the disk.
                let moved_working_copy =
                    matches!(o, PullOutcome::Pulled(_) | PullOutcome::Rebased(_));
                let notice = pull_notice(&o);
                if moved_working_copy {
                    // Clean parked buffers are dropped (they reload on the next
                    // switch); the clean active buffer is re-read now; a RAM-dirty
                    // buffer is left alone — its edits win, last-writer-wins.
                    self.ed.drop_clean_parked();
                    if self.ed.dirty() {
                        log::info!(
                            "post-pull: {} is RAM-dirty — kept (its edits win)",
                            self.ed.path()
                        );
                    } else if !self.ed.path().is_empty() {
                        match self.storage.load_path(self.ed.path()) {
                            Ok(text) => self.ed.refresh_active(text),
                            Err(e) => log::warn!(
                                "post-pull reload of {} FAILED ({e:#}); buffer kept",
                                self.ed.path()
                            ),
                        }
                    }
                    self.files.request_rewalk();
                }
                notice
            }
            NetOutcome::Update(o) => match o {
                UpdateOutcome::Installed(ver) => {
                    // The new image is written and is now the boot slot. Paint the
                    // notice with a blocking full refresh (visible before the
                    // reset), then restart into it; the boot path self-tests and
                    // marks it valid, or the bootloader rolls back to this slot.
                    self.ed.set_notice(format!("updated to {ver} - restarting..."));
                    self.panel.blit_editor_full(&mut self.ed);
                    log::info!(":update — installed {ver}; rebooting into the new image");
                    self.system.reboot();
                }
                UpdateOutcome::UpToDate => "firmware up to date".to_string(),
                UpdateOutcome::Failed(reason) => reason,
            },
        };
        self.ed.set_notice(notice);
        // Behind the rest curtain the panel is masked: settle the state but defer
        // the repaint — the notice shows when the writer leaves Rest.
        if self.ed.mode() == Mode::Rest {
            return;
        }
        self.panel.show_notice(&mut self.ed);
    }

    /// Persist a buffer to `path`. Errors are logged, never propagated: the
    /// in-RAM buffer is the source of truth and must survive a failed write.
    fn save_buffer(&mut self, path: &str, contents: &str) {
        match self.storage.save_path(path, contents) {
            Ok(()) => {
                log::info!(":w — saved {} bytes to {path}", contents.len());
                self.ed.mark_saved(path);
                self.ed.set_notice("saved");
            }
            Err(e) => {
                log::error!("save FAILED ({e:#}); buffer kept in RAM, retry :w");
                self.ed.set_notice("save FAILED - retry :w");
            }
        }
    }

    /// Persist the preferences file after a palette `>` command changed a pref.
    fn save_prefs(&mut self, contents: &str) {
        match self.storage.save_path(PREFS_PATH, contents) {
            Ok(()) => log::info!("prefs saved to {PREFS_PATH}"),
            Err(e) => {
                log::error!("prefs save FAILED ({e:#})");
                self.ed.set_notice("prefs save FAILED");
            }
        }
    }

    /// Read `path` and install it as the active buffer. A read failure keeps the
    /// current buffer and surfaces the reason rather than swapping to an empty one.
    fn open_buffer(&mut self, path: String, scope: Scope) {
        match self.storage.load_path(&path) {
            Ok(text) => {
                log::info!("opened {path} ({} bytes, {scope:?})", text.len());
                let name = file_stem(&path);
                self.ed.set_notice(format!("loaded {name}"));
                self.ed.install_loaded(path, scope, text);
            }
            Err(e) => {
                log::error!("open {path} FAILED ({e:#})");
                self.ed.set_notice(format!("can't open {}", file_stem(&path)));
            }
        }
    }

    /// Unlink a file from the card. The editor has already dropped it from its
    /// model and switched away, so this is pure IO plus the snackbar.
    fn delete_buffer(&mut self, path: String, scope: Scope) {
        // Scope-qualified label (`repo/notes.md`), so the snackbar names exactly
        // which file left the card and, for Tracked, that it's local until `:gp`.
        let label = path.strip_prefix("/sd/").unwrap_or(&path);
        match self.storage.delete_path(&path) {
            Ok(()) => {
                log::info!("deleted {path} ({scope:?})");
                self.ed.set_notice(match scope {
                    Scope::Tracked => format!("deleted {label} - :gp to publish"),
                    Scope::Local => format!("deleted {label}"),
                });
            }
            Err(e) => {
                log::error!("delete {path} FAILED ({e:#})");
                self.ed.set_notice(format!("delete FAILED: {label}"));
            }
        }
    }
}

/// A file's display name — its basename without extension (`/sd/repo/notes.md`
/// → `notes`), for the snackbar. Falls back to the raw path if it has no stem.
/// Pure; shared with the firmware boot path.
pub fn file_stem(path: &str) -> &str {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
}

/// The snackbar line for a finished publish. Pure — the notice mapping only.
fn publish_notice(o: &PublishOutcome) -> String {
    match o {
        PublishOutcome::Pushed(oid) => format!("synced {oid}"),
        PublishOutcome::UpToDate => "up to date".to_string(),
        PublishOutcome::Failed(reason) => reason.clone(),
    }
}

/// The snackbar line for a finished pull. Pure — the notice mapping only.
fn pull_notice(o: &PullOutcome) -> String {
    match o {
        PullOutcome::Pulled(oid) => format!("pulled {oid}"),
        PullOutcome::Rebased(oid) => format!("rebased {oid} - :gp to publish"),
        PullOutcome::UpToDate => "up to date".to_string(),
        PullOutcome::LocalAhead => "ahead - :gp to publish".to_string(),
        PullOutcome::Failed(reason) => reason.clone(),
    }
}

#[cfg(test)]
mod tests;
