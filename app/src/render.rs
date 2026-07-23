//! The panel render engine — the editor's e-paper refresh machinery, shared by
//! the device firmware (`main.rs`) and the no-SD `demo` bin so both drive the
//! panel through one copy of the hard-won refresh logic.
//!
//! [`Panel`] owns the [`Screen`] and the two reused framebuffers, and encapsulates
//! every paint the editor loop performs: the windowed/additive/full-area
//! decision for an edit batch ([`Panel::render_batch`]), the debounced Insert
//! caret ([`Panel::caret_if_due`]), the periodic panel-longevity full refresh
//! ([`Panel::longevity_full`]), the focus-mode rest card ([`Panel::rest_if_due`]),
//! and the failed-paint → forced-full recovery threaded through all of them.
//! Each method is a faithful lift of what used to be inline in `main.rs`'s loop,
//! so the two callers get identical panel behaviour; they differ only in the
//! *orchestration* around these calls (the device polls a git thread and a
//! palette file-walk that the demo has no equivalent of).
//!
//! The bins keep their own loop skeleton — draining the keyboard, servicing
//! [`editor::Effect`]s, sequencing the idle steps below — because those genuinely
//! diverge between a persisting device and a throwaway demo. What lives here is
//! only what is identical between them: the pixels.

use std::time::Instant;

use display::typo;
use display::{Frame, FB_BYTES_W, HEIGHT};
use editor::{Editor, Mode, CH, CW};

use hal::Screen;

/// Occasional full refresh, mainly for panel longevity — partial updates on this
/// panel stay visually clean far longer, so this is deliberately rare. Once this
/// many partials have accumulated, [`Panel::longevity_full`] runs the full
/// refresh at the next typing pause (the counter only advances while typing, so
/// promoting a keystroke repaint would guarantee the ~2 s flash landed
/// mid-sentence).
pub const FULL_REFRESH_EVERY: u32 = 64;

/// The [`FULL_REFRESH_EVERY`] cadence when the experimental fast partial waveform
/// (`Prefs::fast_partial`) is active — halved, because a shorter custom LUT ghosts
/// faster and is not guaranteed DC-balanced, so the clean full refresh that
/// re-launders accumulated charge has to come around sooner. Guardrail 2 of the
/// fast-waveform experiment (the panel-damage mitigation reMarkable relies on too:
/// fast dirty flips during motion, a DC-balanced clean pass when idle).
pub const FULL_REFRESH_EVERY_FAST: u32 = 32;

/// How long typing must pause before the Insert-mode caret is shown. There is no
/// caret while actively typing (it would ghost under windowed refresh); it
/// reappears once you settle. 2 s, not shorter: at 750 ms ordinary mid-sentence
/// pauses triggered the caret, and each show/re-suppress pair cost two ~630 ms
/// panel passes right as typing resumed (the 2026-07-16 "toggling" trace).
pub const CURSOR_DEBOUNCE_MS: u128 = 2000;

/// A "genuine break" in typing — long enough that the writer has stepped back to
/// think or walked away, not just paused mid-sentence. At this point
/// [`Panel::longevity_full`] launders *any* accumulated ghosting with a full
/// refresh, even below the [`FULL_REFRESH_EVERY`] budget: the flash is unobtrusive
/// when you are not typing, and you return to a clean panel. 30 s, comfortably
/// past the 2 s caret debounce so an ordinary think-pause never triggers it.
pub const DEEP_IDLE_MS: u128 = 30_000;

/// Focus mode (Pomodoro) block length: 25 minutes of writing before the rest
/// card drops. Silent — never shown as a live countdown (an e-ink no-go, and the
/// whole point). See docs/v0.7.5-focus-mode.md.
pub const FOCUS_LEN_MS: u128 = 25 * 60 * 1000;
/// The same 25 on a **seconds** clock, for the `:focusdebug` time-base
/// ([`Editor::focus_debug`]) — makes the whole cycle testable in seconds.
pub const FOCUS_DEBUG_LEN_MS: u128 = 25 * 1000;
/// Grace past the block length: if the writer never pauses (so the pause-gated
/// drop can't fire), force the break this long after it comes due.
pub const FOCUS_GRACE_MS: u128 = 2 * 60 * 1000;
/// The `:focusdebug` equivalent of [`FOCUS_GRACE_MS`].
pub const FOCUS_DEBUG_GRACE_MS: u128 = 2 * 1000;

/// Focus-mode (Pomodoro) block timer: `Some(start)` while a block is active (its
/// monotonic start — no wall clock needed), `None` when off. It stays active
/// through the Rest break too; the due-check is gated on `mode != Rest`, not on
/// this. `words0` is the word count at the block's start, for the "words this
/// block" figure. Driven by the editor's FocusStart/FocusStop effects.
#[derive(Default)]
pub struct FocusTimer {
    start: Option<Instant>,
    words0: usize,
}

impl FocusTimer {
    /// Begin — or, after a break, restart — a focus block: start the monotonic
    /// timer and snapshot the word count for the rest card. (`FocusStart`.)
    pub fn start(&mut self, words0: usize) {
        self.start = Some(Instant::now());
        self.words0 = words0;
    }

    /// End the session. (`FocusStop`.)
    pub fn stop(&mut self) {
        self.start = None;
    }
}

/// The panel and its refresh state. Owns the [`Screen`] and the only two
/// framebuffers the editor loop ever uses: every repaint renders into `back`
/// (reusing its allocation via `draw_into`) and swaps it with `shown` on
/// success, so a repaint never allocates — a background `:gp` push can take the
/// heap to the floor, and a failed `Vec` alloc aborts the whole app (the
/// 2026-07-13 OOM: 66 s into a push, one HalfPageUp repaint died on a 27 KB
/// framebuffer).
///
/// Generic over the [`Screen`] port rather than owning the concrete `Epd`, so
/// the render engine is decoupled from esp-idf (and, once relocated, testable
/// off the xtensa target).
pub struct Panel<S: Screen> {
    screen: S,
    /// The frame currently on the panel.
    shown: Frame,
    /// Scratch frame for the next repaint; swapped with `shown` on success.
    back: Frame,
    /// Partial refreshes since the last full one — [`Panel::longevity_full`]
    /// fires when this reaches [`FULL_REFRESH_EVERY`].
    partials_since_full: u32,
    /// One-shot: the first editor frame is a partial painted over the boot
    /// splash, so a full refresh at the first typing pause launders the residual
    /// wordmark ghost. Armed in [`Panel::new`], cleared the first time
    /// [`Panel::longevity_full`] fires — independent of the partial-count cadence,
    /// so the ghost clears within a second or two of boot, not after 64 partials.
    boot_cleanup_pending: bool,
    /// Whether the caret is currently on the panel. Drives whether an
    /// erase-in-one-cell edit counts as additive (the debounced caret bar being
    /// re-suppressed), and is reset to `true` after any whole-panel repaint.
    cursor_shown: bool,
    /// Set when a paint fails: the next paint does a full refresh to re-establish
    /// both RAM banks, since a partial that died mid-transfer may have desynced
    /// them.
    force_full: bool,
    /// Monotonic refresh counter, for the serial trace.
    updates: u32,
    /// Where Typo is in his post-flash humor rotation ([`typo::POOL`]): every
    /// frame painted with a FULL refresh carries the next of the six, so the
    /// flash never plays the same beat twice. Wraps forever.
    pool_idx: usize,
}

impl<S: Screen> Panel<S> {
    /// First editor render — the moment the boot splash disappears. Draws the
    /// opening frame and paints it as a full-area *partial* (~630 ms) rather than
    /// a second full refresh: the partial first waits out the splash's waveform
    /// (`wait_ready`, which the boot work overlapped), so the splash→editor swap
    /// rides the partial and shaves ~1.3 s off cold boot. Allocates both
    /// framebuffers here at boot.
    ///
    /// Takes the [`Screen`] by value: the caller keeps it for the boot splash and
    /// any boot-error screen, then hands it over here once the first editor frame
    /// is ready — after which every panel op goes through the returned `Panel`.
    ///
    /// Arms [`boot_cleanup_pending`](Self::boot_cleanup_pending): this first paint
    /// is a partial over the splash wordmark, so a full refresh at the first
    /// typing pause is scheduled to wipe its residual ghost.
    pub fn new(mut screen: S, ed: &mut Editor) -> Result<Self, S::Error> {
        let shown = ed.draw(true);
        screen.display_frame_partial_window(shown.bytes(), 0, HEIGHT)?;
        Ok(Self {
            screen,
            shown,
            back: Frame::new_white(),
            partials_since_full: 0,
            boot_cleanup_pending: true,
            cursor_shown: true, // the initial render includes the caret
            force_full: false,
            updates: 0,
            pool_idx: 0,
        })
    }

    /// Typo's refresh-cycle transitions — the whole reason the faces are free:
    /// they are only ever swapped into a frame whose repaint is already paid for.
    ///
    /// Rotate to the next post-flash humor, for a frame about to be painted with
    /// a FULL refresh: the new face rides the flash, and what comes back after
    /// the black/white cycle is Typo with a fresh take on "keep going".
    fn companion_pool_mood(&mut self, ed: &mut Editor) {
        if !ed.prefs().companion {
            return;
        }
        ed.set_companion_mood(typo::POOL[self.pool_idx % typo::POOL.len()]);
        self.pool_idx = self.pool_idx.wrapping_add(1);
    }

    /// At a typing-pause repaint (already a silent full-area partial): once
    /// ghosting has built past **half** the full-refresh budget, Typo turns
    /// frustrated — at the residue dusting *his* feathers, never at the writer.
    /// He stays that way until the longevity flash launders the panel and the
    /// pool rotation above answers with a humor.
    fn companion_ghost_mood(&mut self, ed: &mut Editor) {
        if ed.prefs().companion && self.partials_since_full >= self.full_budget(ed) / 2 {
            ed.set_companion_mood(typo::Mood::Frustrated);
        }
    }

    /// Repaint after a batch of keystrokes. Renders the editor into `back`, then
    /// paints only the band that changed: a purely additive Insert edit (no
    /// cursor, no scroll) takes the fast windowed partial; anything else —
    /// deletes, caret moves, scrolling, mode switches — takes a clean full-area
    /// partial; a `force_full` recovery or leaving the Rest curtain takes a FULL
    /// refresh. `prev_mode` is the mode captured before the batch (to detect
    /// leaving Rest); `keys` is only for the trace. On a paint failure the frame
    /// is dropped and `force_full` is armed for the next paint — never fatal, the
    /// buffer is the source of truth and safe in RAM.
    pub fn render_batch(&mut self, ed: &mut Editor, prev_mode: Mode, keys: u32) {
        // Non-Insert actions (Normal edits, mode switches) aren't rapid typing,
        // so the panel word count can refresh immediately; in Insert the snapshot
        // stays frozen until the typing-pause path refreshes it.
        if ed.mode() != Mode::Insert {
            ed.refresh_stats();
        }
        // Suppress the Insert bar caret while typing (fast, no ghost); Normal and
        // View render their caret regardless of this flag.
        let insert_cursor_on = ed.mode() != Mode::Insert;
        // The experimental fast partial waveform is scoped to the additive path
        // below (guardrail 1); read live so a bench toggle takes effect at once.
        let fast_partial = ed.prefs().fast_partial;

        // A full-screen card (the rest curtain, or the `:about` splash) swapping
        // to or from the editor is a big ink change: force a clean full refresh so
        // it doesn't ghost. Rest only ever *leaves* through here (the focus timer
        // drops it in via `rest_if_due`); `:about` both enters and leaves by
        // keystroke, so either of its transitions counts. Checked before the
        // render (the keys are already applied, so both modes are known) so that
        // a frame headed for a FULL refresh can carry Typo's next pool humor.
        let was_card = prev_mode == Mode::Rest || prev_mode == Mode::About;
        let is_card = ed.mode() == Mode::About; // Rest never enters via a key batch
        if was_card != is_card {
            self.force_full = true;
        }
        if self.force_full {
            self.companion_pool_mood(ed);
        }

        let prev_scroll = ed.scroll_top();
        ed.draw_into(&mut self.back, insert_cursor_on);
        let scrolled = ed.scroll_top() != prev_scroll;

        // Only the rows that changed since the last shown frame need updating.
        let Some((y0, y1)) = changed_rows(self.shown.bytes(), self.back.bytes()) else {
            self.cursor_shown = ed.mode() != Mode::Insert;
            return; // no visible change (frames identical — no swap needed)
        };
        // Snap the band to whole text lines so a partial-window boundary never
        // lands mid-glyph — otherwise the boundary gate crops tall characters.
        let ch = CH as u16;
        let y0 = y0 / ch * ch;
        let y1 = (y1 / ch * ch + ch - 1).min(HEIGHT - 1);

        self.updates += 1;
        // One tolerated erase: the debounced caret bar (2×CH px, one cell) being
        // re-suppressed as typing resumes — its ghost risk is negligible, and
        // promoting it made every post-pause keystroke drive the whole panel. Any
        // wider erase (a backspaced glyph spans the caret's cell plus its own)
        // still falls back to the clean full-area pass.
        let additive = ed.mode() == Mode::Insert
            && !scrolled
            && match erase_bbox(self.shown.bytes(), self.back.bytes(), y0, y1) {
                None => true,
                Some((ex0, ex1, ey0, ey1)) => {
                    self.cursor_shown && ex1 - ex0 < CW as u16 && ey1 - ey0 < CH as u16
                }
            };

        let t0 = Instant::now();
        let (result, refresh) = if self.force_full {
            (self.screen.display_frame(self.back.bytes()), "FULL")
        } else if additive {
            let h = y1 - y0 + 1;
            if fast_partial {
                (self.screen.display_frame_partial_window_fast(self.back.bytes(), y0, h), "windowed-fast")
            } else {
                (self.screen.display_frame_partial_window(self.back.bytes(), y0, h), "windowed")
            }
        } else {
            (self.screen.display_frame_partial_window(self.back.bytes(), 0, HEIGHT), "full-area")
        };
        let ms = t0.elapsed().as_millis();
        if let Err(e) = result {
            log::warn!(
                "{refresh} refresh #{} FAILED ({e}); frame dropped, full refresh next",
                self.updates
            );
            self.force_full = true;
            return;
        }
        self.force_full = false;
        if refresh == "FULL" {
            self.partials_since_full = 0;
        } else {
            self.partials_since_full += 1;
        }
        log::info!(
            "{refresh} refresh #{} [{:?}]: {ms} ms (rows {y0}..={y1}, {keys} key(s))",
            self.updates,
            ed.mode()
        );
        std::mem::swap(&mut self.shown, &mut self.back);
        self.cursor_shown = ed.mode() != Mode::Insert;
    }

    /// Focus mode: if a running block has reached its length, drop the rest card
    /// at this typing pause — never mid-keystroke — or at the grace cap if the
    /// writer never pauses. FULL refresh: the curtain is a deliberate, unmissable
    /// state change, and the clean flash avoids ghosting the big black/white
    /// swap. Returns `true` if it painted (the caller should `continue`), `false`
    /// if nothing was due. Skipped once Rest is already showing.
    pub fn rest_if_due(&mut self, ed: &mut Editor, focus: &FocusTimer, last_activity: Instant) -> bool {
        let Some(start) = focus.start else {
            return false;
        };
        if ed.mode() == Mode::Rest {
            return false;
        }
        let (len, grace, div) = if ed.focus_debug() {
            (FOCUS_DEBUG_LEN_MS, FOCUS_DEBUG_GRACE_MS, 1000u128)
        } else {
            (FOCUS_LEN_MS, FOCUS_GRACE_MS, 60_000u128)
        };
        let el = start.elapsed().as_millis();
        let paused = last_activity.elapsed().as_millis() >= CURSOR_DEBOUNCE_MS;
        if !(el >= len && (paused || el >= len + grace)) {
            return false;
        }
        let words = ed.word_count().saturating_sub(focus.words0);
        ed.enter_rest(words, (el / div) as u32);
        ed.draw_into(&mut self.back, true);
        let t0 = Instant::now();
        if let Err(e) = self.screen.display_frame(self.back.bytes()) {
            log::warn!("rest-card refresh FAILED ({e}); full refresh next");
            self.force_full = true;
            return true;
        }
        self.partials_since_full = 0;
        log::info!("focus: rest after {el} ms ({words} words); {} ms", t0.elapsed().as_millis());
        std::mem::swap(&mut self.shown, &mut self.back);
        self.cursor_shown = true;
        true
    }

    /// Repaint the whole panel with a silent full-area partial (caret shown),
    /// for a notice that arrived while idle — no keystroke will come to trigger a
    /// repaint. Returns `true` (the caller should `continue`); on a paint failure
    /// it arms `force_full` for the next paint.
    pub fn show_notice(&mut self, ed: &mut Editor) -> bool {
        ed.draw_into(&mut self.back, true);
        if let Err(e) = self.screen.display_frame_partial_window(self.back.bytes(), 0, HEIGHT) {
            log::warn!("notice repaint FAILED ({e}); full refresh next");
            self.force_full = true;
            return true;
        }
        std::mem::swap(&mut self.shown, &mut self.back);
        self.cursor_shown = true;
        true
    }

    /// Repaint only if the freshly-drawn frame actually differs from what's on
    /// the panel — for a background file-list update, which is only visible
    /// through the (usually closed) palette overlay, so a no-op full-area partial
    /// would be a pointless ~630 ms panel drive. Caret visibility is preserved
    /// (not forced on), so this can't reveal a debounced Insert caret early.
    pub fn repaint_if_changed(&mut self, ed: &mut Editor) -> bool {
        ed.draw_into(&mut self.back, self.cursor_shown);
        if changed_rows(self.shown.bytes(), self.back.bytes()).is_some() {
            if let Err(e) = self.screen.display_frame_partial_window(self.back.bytes(), 0, HEIGHT) {
                log::warn!("palette repaint FAILED ({e}); full refresh next");
                self.force_full = true;
                return true;
            }
            std::mem::swap(&mut self.shown, &mut self.back);
        }
        true
    }

    /// A keyboard connect/disconnect while idle must still repaint the panel's
    /// disconnect flag — no keystroke will arrive to trigger it. No-op (returns
    /// `false`) when the attach state hasn't changed. `kbd` is the current state,
    /// for the trace line.
    pub fn kbd_repaint(&mut self, ed: &mut Editor, kbd_changed: bool, kbd: bool) -> bool {
        if !kbd_changed {
            return false;
        }
        ed.draw_into(&mut self.back, true);
        if let Err(e) = self.screen.display_frame_partial_window(self.back.bytes(), 0, HEIGHT) {
            log::warn!("kbd-flag repaint FAILED ({e}); full refresh next");
            self.force_full = true;
            return true;
        }
        std::mem::swap(&mut self.shown, &mut self.back);
        self.cursor_shown = true;
        log::info!("keyboard {}", if kbd { "connected" } else { "disconnected" });
        true
    }

    /// The full-refresh partial budget for the active waveform — halved when the
    /// experimental fast partial is on, since a shorter custom LUT ghosts faster
    /// (see [`FULL_REFRESH_EVERY_FAST`]).
    fn full_budget(&self, ed: &Editor) -> u32 {
        if ed.prefs().fast_partial {
            FULL_REFRESH_EVERY_FAST
        } else {
            FULL_REFRESH_EVERY
        }
    }

    /// Piggyback a panel-cleaning full refresh onto a palette file-switch. A switch
    /// already repaints the whole writing column and reads as "loading a document",
    /// so it is a free moment to launder accumulated ghosting — masking the ~2 s
    /// flash behind an expected transition instead of spending it on a standalone
    /// idle pass. Only once ghosting has built past half the longevity budget, so
    /// rapid browsing right after a clean pass stays on the fast full-area partial.
    /// The runtime calls this right after servicing an `Effect::Load`, before the
    /// switch repaint runs through [`render_batch`](Self::render_batch).
    pub fn full_refresh_on_switch(&mut self, ed: &Editor) {
        if self.partials_since_full >= self.full_budget(ed) / 2 {
            self.force_full = true;
        }
    }

    /// Deferred full refresh, at a typing pause — three triggers share this one
    /// mechanism (and the caret draw, so a pause costs one flash, not flash + caret
    /// pass):
    ///   * **boot-splash cleanup** — the one-shot
    ///     [`boot_cleanup_pending`](Self::boot_cleanup_pending) flag, at the first
    ///     short pause, to wipe the residual splash-wordmark ghost.
    ///   * **longevity** — [`FULL_REFRESH_EVERY`] partials accumulated, at a short
    ///     pause, to re-launder accumulated charge.
    ///   * **deep idle** — *any* ghosting after a genuine break
    ///     ([`DEEP_IDLE_MS`]), so you return from stepping away to a clean panel.
    /// All defer to a pause because the ~2 s flash must never land mid-typing; the
    /// partial counter only advances on keystroke repaints, so promoting in-band
    /// would mean it could ONLY land mid-sentence. Returns `true` if it painted (or
    /// attempted to — the caller should `continue`), `false` when not yet due.
    pub fn longevity_full(&mut self, ed: &mut Editor, last_activity: Instant) -> bool {
        let every = self.full_budget(ed);
        let elapsed = last_activity.elapsed().as_millis();
        let due = (self.boot_cleanup_pending && elapsed >= CURSOR_DEBOUNCE_MS)
            || (self.partials_since_full >= every && elapsed >= CURSOR_DEBOUNCE_MS)
            || (self.partials_since_full > 0 && elapsed >= DEEP_IDLE_MS);
        if !due {
            return false;
        }
        // Capture the reason before the refresh clears the flag / resets the count.
        let reason = if self.boot_cleanup_pending {
            "boot-splash cleanup"
        } else if self.partials_since_full >= every {
            "longevity"
        } else {
            "deep-idle"
        };
        // One-shot regardless of the refresh outcome: on a failed paint `force_full`
        // is armed below, so the next paint is a full refresh that cleans it anyway.
        self.boot_cleanup_pending = false;
        ed.refresh_stats();
        self.companion_pool_mood(ed); // the humor rides the flash, for free
        ed.draw_into(&mut self.back, true);
        self.updates += 1;
        let t0 = Instant::now();
        if let Err(e) = self.screen.display_frame(self.back.bytes()) {
            log::warn!("idle FULL refresh #{} FAILED ({e}); full refresh next", self.updates);
            self.force_full = true;
            self.partials_since_full = 0;
            return true;
        }
        self.partials_since_full = 0;
        log::info!(
            "idle FULL refresh #{} ({reason}): {} ms",
            self.updates,
            t0.elapsed().as_millis()
        );
        std::mem::swap(&mut self.shown, &mut self.back);
        self.cursor_shown = true;
        true
    }

    /// Debounced caret, Insert mode only: once typing has paused long enough,
    /// bring the bar caret back and refresh the panel word count with a silent
    /// full-area partial (no flash). Returns `true` when the caret was due (it
    /// painted, or tried and armed `force_full`), `false` when nothing was due —
    /// in which case the caller should briefly yield the CPU. The platform sleep
    /// is the composition root's concern, kept out of this pure render engine.
    /// The tail of the idle sequence — always call it last.
    pub fn caret_if_due(&mut self, ed: &mut Editor, last_activity: Instant) -> bool {
        if !(ed.mode() == Mode::Insert
            && !self.cursor_shown
            && last_activity.elapsed().as_millis() >= CURSOR_DEBOUNCE_MS)
        {
            return false;
        }
        ed.refresh_stats();
        self.companion_ghost_mood(ed); // this repaint is whole-panel already
        ed.draw_into(&mut self.back, true);
        if let Err(e) = self.screen.display_frame_partial_window(self.back.bytes(), 0, HEIGHT) {
            log::warn!("caret repaint FAILED ({e}); full refresh next");
            self.force_full = true;
        } else {
            std::mem::swap(&mut self.shown, &mut self.back);
            self.cursor_shown = true;
            log::info!("caret shown");
        }
        true
    }

    /// Paint an editor frame with a blocking full refresh and no swap bookkeeping
    /// — for a notice that must be on the bistable panel *before* a reset fires
    /// (`:setup`'s "restarting..." line). The caller reboots immediately after,
    /// so leaving `shown`/`back` unswapped is intentional.
    pub fn blit_editor_full(&mut self, ed: &mut Editor) {
        ed.draw_into(&mut self.back, true);
        let _ = self.screen.display_frame(self.back.bytes());
    }

    /// Paint a static frame (the branded reboot splash) with a blocking full
    /// refresh, so it is on the panel before the caller calls `esp_restart`.
    pub fn blit_full(&mut self, frame: &Frame) {
        let _ = self.screen.display_frame(frame.bytes());
    }
}

/// First and last (inclusive) framebuffer rows that differ between two frames,
/// or `None` if identical. Lets a partial refresh target just the band a
/// keystroke touched instead of all 272 rows.
pub fn changed_rows(a: &[u8], b: &[u8]) -> Option<(u16, u16)> {
    let w = FB_BYTES_W;
    let mut first: Option<u16> = None;
    let mut last = 0u16;
    for y in 0..HEIGHT as usize {
        if a[y * w..(y + 1) * w] != b[y * w..(y + 1) * w] {
            first.get_or_insert(y as u16);
            last = y as u16;
        }
    }
    first.map(|f| (f, last))
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::Prefs;
    use hal::Key;
    use std::cell::RefCell;
    use std::convert::Infallible;
    use std::rc::Rc;
    use std::time::Duration;

    /// A [`Screen`] that records which refresh method fired, in order, so a test can
    /// assert the fast waveform is reached only on the additive per-keystroke path.
    #[derive(Clone, Default)]
    struct RecordScreen(Rc<RefCell<Vec<&'static str>>>);
    impl Screen for RecordScreen {
        type Error = Infallible;
        fn display_frame(&mut self, _fb: &[u8]) -> Result<(), Infallible> {
            self.0.borrow_mut().push("full");
            Ok(())
        }
        fn display_frame_partial_window(&mut self, _fb: &[u8], _y0: u16, _h: u16) -> Result<(), Infallible> {
            self.0.borrow_mut().push("partial");
            Ok(())
        }
        fn display_frame_partial_window_fast(&mut self, _fb: &[u8], _y0: u16, _h: u16) -> Result<(), Infallible> {
            self.0.borrow_mut().push("partial-fast");
            Ok(())
        }
    }

    type Log = Rc<RefCell<Vec<&'static str>>>;

    /// A panel on a fresh empty editor with `fast_partial` set as given, already in
    /// Insert mode with the boot/entry paints drained from the log.
    fn insert_panel(fast: bool) -> (Panel<RecordScreen>, Editor, Log) {
        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let mut ed = Editor::with_text(String::new());
        ed.set_prefs(Prefs { fast_partial: fast, ..Prefs::default() });
        let mut panel = Panel::new(RecordScreen(log.clone()), &mut ed).expect("boot paint");
        ed.handle(Key::Char('i')); // Normal -> Insert
        panel.render_batch(&mut ed, Mode::Normal, 1); // caret-suppression transition
        (panel, ed, log)
    }

    /// Append `c` at the caret in Insert mode and return the refresh(es) it drove.
    fn type_char(panel: &mut Panel<RecordScreen>, ed: &mut Editor, log: &Log, c: char) -> Vec<&'static str> {
        log.borrow_mut().clear();
        ed.handle(Key::Char(c));
        panel.render_batch(ed, Mode::Insert, 1);
        log.borrow().clone()
    }

    #[test]
    fn fast_partial_pref_routes_the_additive_keystroke_to_the_fast_waveform() {
        let (mut panel, mut ed, log) = insert_panel(true);
        type_char(&mut panel, &mut ed, &log, 'a'); // prime past the entry transition
        type_char(&mut panel, &mut ed, &log, 'b');
        // A clean append at end of line — purely additive — takes the fast waveform.
        assert_eq!(type_char(&mut panel, &mut ed, &log, 'c'), ["partial-fast"]);
    }

    #[test]
    fn without_the_pref_the_same_keystroke_takes_the_ordinary_partial() {
        let (mut panel, mut ed, log) = insert_panel(false);
        type_char(&mut panel, &mut ed, &log, 'a');
        type_char(&mut panel, &mut ed, &log, 'b');
        assert_eq!(type_char(&mut panel, &mut ed, &log, 'c'), ["partial"]);
    }

    #[test]
    fn fast_waveform_never_fires_off_the_additive_path() {
        // Guardrail 1: even with the pref on, a non-additive edit (a delete, which
        // erases ink) must take the clean full-area partial, never the fast one.
        let (mut panel, mut ed, log) = insert_panel(true);
        type_char(&mut panel, &mut ed, &log, 'a');
        type_char(&mut panel, &mut ed, &log, 'b');
        type_char(&mut panel, &mut ed, &log, 'c');
        log.borrow_mut().clear();
        ed.handle(Key::Backspace); // erase 'c' -> wide erase -> not additive
        panel.render_batch(&mut ed, Mode::Insert, 1);
        let paints = log.borrow().clone();
        assert!(!paints.is_empty(), "the delete should have repainted");
        assert!(
            !paints.contains(&"partial-fast"),
            "a delete must not use the fast waveform; got {paints:?}"
        );
    }

    #[test]
    fn boot_splash_ghost_is_cleared_by_one_shot_full_refresh_at_first_pause() {
        // Panel::new paints the first editor frame as a partial *over* the boot
        // splash wordmark, arming the one-shot cleanup. At the first typing pause
        // (activity older than the debounce) longevity_full fires a FULL refresh to
        // launder the residual ghost — even with no partials accumulated yet.
        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let mut ed = Editor::with_text(String::new());
        let mut panel = Panel::new(RecordScreen(log.clone()), &mut ed).expect("boot paint");
        assert_eq!(log.borrow().clone(), ["partial"], "boot frame is a partial over the splash");

        let paused = Instant::now() - Duration::from_millis(CURSOR_DEBOUNCE_MS as u64 + 100);

        // First pause: the armed cleanup fires a full refresh despite 0 partials.
        log.borrow_mut().clear();
        assert!(panel.longevity_full(&mut ed, paused), "cleanup should paint at the first pause");
        assert_eq!(log.borrow().clone(), ["full"], "boot cleanup is a full refresh");

        // One-shot: a second pause does not re-fire (flag cleared, and the partial
        // count is nowhere near the longevity cadence).
        log.borrow_mut().clear();
        assert!(!panel.longevity_full(&mut ed, paused), "cleanup must not repeat");
        assert!(log.borrow().is_empty(), "no second full refresh: {:?}", log.borrow());
    }

    #[test]
    fn file_switch_promotes_to_a_full_refresh_only_once_ghosting_has_accumulated() {
        let (mut panel, ed, _log) = insert_panel(false);
        // Just below half the budget: a switch stays on the fast full-area partial.
        panel.partials_since_full = FULL_REFRESH_EVERY / 2 - 1;
        panel.full_refresh_on_switch(&ed);
        assert!(!panel.force_full, "light ghosting: no gratuitous full refresh on switch");
        // At half the budget: use the masked moment to launder the panel.
        panel.partials_since_full = FULL_REFRESH_EVERY / 2;
        panel.full_refresh_on_switch(&ed);
        assert!(panel.force_full, "accumulated ghosting: switch promotes to a full refresh");
    }

    #[test]
    fn deep_idle_launders_light_ghosting_only_after_a_long_break() {
        let (mut panel, mut ed, log) = insert_panel(false);
        panel.boot_cleanup_pending = false; // isolate from the boot one-shot
        panel.partials_since_full = 3; // light ghosting, well below the budget
        log.borrow_mut().clear();

        // A short (caret-debounce) pause is NOT enough for light ghosting — only
        // the full longevity budget fires at a short pause.
        let short = Instant::now() - Duration::from_millis(CURSOR_DEBOUNCE_MS as u64 + 100);
        assert!(!panel.longevity_full(&mut ed, short), "short pause + light ghost: no refresh");
        assert!(log.borrow().is_empty());

        // A genuine break launders even light ghosting.
        let deep = Instant::now() - Duration::from_millis(DEEP_IDLE_MS as u64 + 100);
        assert!(panel.longevity_full(&mut ed, deep), "deep pause: clean the light ghost");
        assert_eq!(log.borrow().clone(), ["full"]);

        // Panel now clean (count reset) — a second deep pause does nothing.
        log.borrow_mut().clear();
        assert!(!panel.longevity_full(&mut ed, deep), "clean panel: no repeat flash");
    }

    #[test]
    fn every_full_refresh_rotates_typo_through_the_humor_pool() {
        let (mut panel, mut ed, _log) = insert_panel(false);
        let paused = Instant::now() - Duration::from_millis(CURSOR_DEBOUNCE_MS as u64 + 100);

        // Boot-cleanup full: the first flash already carries the first humor.
        assert!(panel.longevity_full(&mut ed, paused));
        assert_eq!(ed.companion_mood(), typo::POOL[0]);

        // Each further full refresh steps the rotation — never the same beat twice.
        for expect in typo::POOL.iter().skip(1) {
            panel.partials_since_full = FULL_REFRESH_EVERY;
            assert!(panel.longevity_full(&mut ed, paused));
            assert_eq!(ed.companion_mood(), *expect);
        }
        // ...and the seventh wraps back to the first.
        panel.partials_since_full = FULL_REFRESH_EVERY;
        assert!(panel.longevity_full(&mut ed, paused));
        assert_eq!(ed.companion_mood(), typo::POOL[0]);
    }

    #[test]
    fn typo_turns_frustrated_at_a_pause_once_ghosting_passes_half_the_budget() {
        let (mut panel, mut ed, _log) = insert_panel(false);
        let paused = Instant::now() - Duration::from_millis(CURSOR_DEBOUNCE_MS as u64 + 100);

        // Below half the budget the pause repaint leaves the face alone.
        panel.partials_since_full = FULL_REFRESH_EVERY / 2 - 1;
        panel.cursor_shown = false;
        assert!(panel.caret_if_due(&mut ed, paused), "caret was due");
        assert_eq!(ed.companion_mood(), typo::Mood::Neutral, "light ghosting: no frown");

        // At half the budget the same free repaint carries the frustrated face.
        panel.partials_since_full = FULL_REFRESH_EVERY / 2;
        panel.cursor_shown = false;
        assert!(panel.caret_if_due(&mut ed, paused));
        assert_eq!(ed.companion_mood(), typo::Mood::Frustrated);
    }

    #[test]
    fn companion_off_freezes_the_face_machinery() {
        let (mut panel, mut ed, _log) = insert_panel(false);
        ed.set_prefs(Prefs { companion: false, ..Prefs::default() });
        let paused = Instant::now() - Duration::from_millis(CURSOR_DEBOUNCE_MS as u64 + 100);

        assert!(panel.longevity_full(&mut ed, paused), "the flash itself still runs");
        panel.partials_since_full = FULL_REFRESH_EVERY / 2;
        panel.cursor_shown = false;
        assert!(panel.caret_if_due(&mut ed, paused));
        assert_eq!(ed.companion_mood(), typo::Mood::Neutral, "pref off: no mood swaps");
    }
}

/// Bounding box (x0, x1, y0, y1 — pixels, inclusive) of the ink *erased* going
/// from frame `a` to `b` within rows `y0..=y1`, or `None` when the change only
/// adds ink. Windowed partial refresh renders added ink cleanly but leaves
/// ghosts where ink is erased, so erasing edits fall back to a clean full-area
/// partial — except an erase confined to one character cell with the caret on
/// screen, which the caller reads as the debounced caret bar being re-suppressed.
/// Bit convention: 1 = white, 0 = black ink.
pub fn erase_bbox(a: &[u8], b: &[u8], y0: u16, y1: u16) -> Option<(u16, u16, u16, u16)> {
    let w = FB_BYTES_W;
    let mut bbox: Option<(u16, u16, u16, u16)> = None;
    for y in y0 as usize..=y1 as usize {
        for xb in 0..w {
            // Bits set in b but clear in a went black→white — erased ink.
            let erased = b[y * w + xb] & !a[y * w + xb];
            if erased == 0 {
                continue;
            }
            let x_lo = (xb * 8) as u16 + erased.leading_zeros() as u16;
            let x_hi = (xb * 8) as u16 + 7 - erased.trailing_zeros() as u16;
            let bb = bbox.get_or_insert((x_lo, x_hi, y as u16, y as u16));
            bb.0 = bb.0.min(x_lo);
            bb.1 = bb.1.max(x_hi);
            bb.3 = y as u16; // rows scan top-down, so y is always the new max
        }
    }
    bbox
}
