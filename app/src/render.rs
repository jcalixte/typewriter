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

/// How long typing must pause before the Insert-mode caret is shown. There is no
/// caret while actively typing (it would ghost under windowed refresh); it
/// reappears once you settle. 2 s, not shorter: at 750 ms ordinary mid-sentence
/// pauses triggered the caret, and each show/re-suppress pair cost two ~630 ms
/// panel passes right as typing resumed (the 2026-07-16 "toggling" trace).
pub const CURSOR_DEBOUNCE_MS: u128 = 2000;

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
    pub fn new(mut screen: S, ed: &mut Editor) -> Result<Self, S::Error> {
        let shown = ed.draw(true);
        screen.display_frame_partial_window(shown.bytes(), 0, HEIGHT)?;
        Ok(Self {
            screen,
            shown,
            back: Frame::new_white(),
            partials_since_full: 0,
            cursor_shown: true, // the initial render includes the caret
            force_full: false,
            updates: 0,
        })
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
        let prev_scroll = ed.scroll_top();
        ed.draw_into(&mut self.back, insert_cursor_on);
        let scrolled = ed.scroll_top() != prev_scroll;

        // A full-screen card (the rest curtain, or the `:about` splash) swapping
        // to or from the editor is a big ink change: force a clean full refresh so
        // it doesn't ghost. Rest only ever *leaves* through here (the focus timer
        // drops it in via `rest_if_due`); `:about` both enters and leaves by
        // keystroke, so either of its transitions counts.
        let was_card = prev_mode == Mode::Rest || prev_mode == Mode::About;
        let is_card = ed.mode() == Mode::About; // Rest never enters via a key batch
        if was_card != is_card {
            self.force_full = true;
        }

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
            (self.screen.display_frame_partial_window(self.back.bytes(), y0, y1 - y0 + 1), "windowed")
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

    /// Panel-longevity full refresh, deferred to a typing pause. The partial
    /// counter only advances on keystroke repaints, so promoting in-band would
    /// mean the ~2 s flash could ONLY land mid-typing. Draws the caret itself, so
    /// the pause costs one flash, not flash + caret pass. Returns `true` if it
    /// painted (or attempted to — the caller should `continue`), `false` when not
    /// yet due.
    pub fn longevity_full(&mut self, ed: &mut Editor, last_activity: Instant) -> bool {
        if !(self.partials_since_full >= FULL_REFRESH_EVERY
            && last_activity.elapsed().as_millis() >= CURSOR_DEBOUNCE_MS)
        {
            return false;
        }
        ed.refresh_stats();
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
        log::info!("idle FULL refresh #{}: {} ms", self.updates, t0.elapsed().as_millis());
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
