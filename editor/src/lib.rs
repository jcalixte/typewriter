//! Modal text editor core: a vim-style buffer with Normal / Insert (edit) /
//! View (read-only) modes, rendered onto the e-paper [`Frame`].
//!
//! The buffer is a UTF-8 `String` (the keyboard's dead-key composer feeds it
//! accented Latin-9 characters). `caret` is a byte offset that always sits on a
//! char boundary: motions and edits step whole characters via `next_char` /
//! `prev_char`, and display columns are character counts, so a two-byte `é`
//! never traps the caret mid-character. Motions and edits work on the logical
//! (`\n`-delimited) buffer; word-wrapping and scrolling are a render-time
//! concern handled by [`Editor::draw`].

// ISO-8859-15 (Latin-9) rather than the ascii subset: same glyph cells, but it
// carries the accented Latin glyphs (à é ê ç … plus œ €) that international
// input will emit. ASCII rendering is byte-for-byte unchanged.
use embedded_graphics::mono_font::iso_8859_15::{FONT_6X10, FONT_10X20};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Baseline, Text};

use display::{Frame, HEIGHT, WIDTH};
use keymap::Key;

/// FONT_10X20 cell size (writing column) and the grid it tiles into.
pub const CW: i32 = 10;
pub const CH: i32 = 20;
/// Writing-region width, in characters: the left region holding the
/// line-number gutter **and** the text column (the gutter steals from it — see
/// [`Editor::text_cols`]). The right **side panel** holds metadata (see
/// CONTEXT.md § Screen regions). 63 cols × 10 px = 630 px; the driver's
/// `x = 396` seam runs through it invisibly. The remaining 162 px (right of the
/// divider) hold the ~25-col side panel. Widened from 60 so the gutter doesn't
/// narrow the text: a ≤ 99-line file keeps a full 60-col text column.
const WRITE_COLS: usize = 63;
/// Minimum digit columns in the line-number gutter (before the 1-col separator).
/// Files up to 99 lines still get a 2-wide gutter so short notes don't jitter.
const GUTTER_MIN_DIGITS: usize = 2;
/// Visible writing rows. 13 × 20 px = 260 px; the bottom 12 px is the transient
/// `:` command line (the only thing left of the old status band).
const ROWS: usize = (HEIGHT / 20) as usize; // 13
/// Half-page scroll distance for `Ctrl-d`/`Ctrl-u`, in **display rows** — vim's
/// `'scroll'` default (half the visible window). Fixed, not configurable: a
/// resizable `'scroll'` is meaningless on a fixed 13-row panel.
const HALF_PAGE: usize = ROWS / 2; // 6
/// x of the 1 px rule dividing writing column from side panel, and the left edge
/// of panel text (a small gutter past the rule).
const DIVIDER_X: i32 = WRITE_COLS as i32 * CW; // 630
const PANEL_X: i32 = DIVIDER_X + 8; // 638
/// Side-panel text width in 6 px (`FONT_6X10`) columns, for clamping panel
/// strings — the snackbar notice, word count — so they never draw past the
/// right edge of the panel.
const PANEL_COLS: usize = (WIDTH as usize - PANEL_X as usize) / 6; // 25
/// Tab stop, in spaces. Tabs never enter the buffer — they expand on insert so
/// the buffer stays 1 char = 1 column.
const TAB: &str = "    ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Navigation and commands (hjkl, w/b/e, dd, x, …).
    Normal,
    /// Text entry — keys insert at the caret.
    Insert,
    /// Read-only reading: keys scroll the viewport, edits are locked out.
    View,
    /// `:` command line — keys accumulate a command shown in the status strip;
    /// Enter runs it, Esc cancels. Handles `:fmt` (in-core) plus `:w`/`:sync`
    /// (which ask the host to persist/publish via an [`Effect`]).
    Command,
}

/// A side effect the host (firmware) must carry out after a `:` command. The
/// editor core is pure and does no IO, so persistence and publishing can't
/// happen here — they're signalled out through [`Editor::handle`]'s return
/// value and actioned by the main loop. `:fmt` is pure text work and stays
/// in-core, so it yields [`Effect::None`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Nothing for the host to do — ordinary keys, `:fmt`, unknown commands.
    None,
    /// `:w` (and the `:wq`/`:x` aliases) — persist the buffer to storage.
    Save,
    /// `:sync` — publish the buffer (save, then git push). The host saves
    /// first: publishing an unsaved buffer is meaningless.
    Publish,
    /// `:gl` — pull from the remote: fetch, then **fast-forward only**. The host
    /// refuses (and surfaces) a divergence rather than merging, and never
    /// touches local commits. Complements `:sync` (push) as the download half.
    Pull,
}

/// A pending operator awaiting a motion or text object (`d`elete / `c`hange /
/// `y`ank).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Delete,
    Change,
    Yank,
}

/// The editor state: buffer, caret, mode, viewport, and pending command state.
pub struct Editor {
    text: String,
    /// Byte offset of the caret, always on a UTF-8 char boundary. Ranges over
    /// `0..=text.len()`; step it only via `next_char`/`prev_char`.
    caret: usize,
    mode: Mode,
    /// Index of the first visible display line.
    scroll_top: usize,
    /// Pending numeric count prefix (`0` = none), e.g. the `3` in `3j`.
    count: usize,
    /// Operator awaiting a motion/text object (`dd`, `dw`, `ciw`, `di(`, …).
    pending_op: Option<Op>,
    /// After an operator, an `i`/`a` text-object prefix awaiting the object
    /// char. `Some(false)` = inner (`i`), `Some(true)` = around (`a`).
    pending_obj: Option<bool>,
    /// First `g` of a `gg` awaiting the second.
    pending_g: bool,
    /// The `:` command line being typed (valid only in `Mode::Command`).
    cmdline: String,
    /// Word count as of the last stats refresh. The panel shows this snapshot,
    /// not a live count, so ordinary typing never repaints the panel row — it is
    /// refreshed on a typing pause / non-Insert action via `refresh_stats`.
    shown_words: usize,
    /// Whether a USB keyboard is attached; drives the panel disconnect flag.
    /// Fed from `usb_kbd::keyboard_present()` by the main loop.
    keyboard_present: bool,
    /// Transient side-panel message ("snackbar") — the last host event
    /// (save/publish result). Shown until the next keystroke dismisses it
    /// (cleared in [`Editor::handle`]); `None` means nothing to show.
    notice: Option<String>,
    /// Run `:fmt` on the buffer before persisting on `:w`/`:sync`, so `:sync`
    /// is fmt → save → commit → push. Defaults on; the v0.5 `.typoena.toml`
    /// `format_on_save` key will drive it.
    format_on_save: bool,
    /// The unnamed register: the last yanked or deleted text, replayed by
    /// `p`/`P`. `y`, `d`, `c`, and `x` all fill it (vim's unnamed register), so
    /// `dd`…`p` moves a line. There is one register — no named registers yet.
    register: String,
    /// Whether [`register`](Self::register) holds whole **lines** (from `yy`/`dd`,
    /// stored with a trailing `\n`) rather than a character span (`yw`/`x`). It
    /// decides how `p`/`P` reinsert: linewise pastes open a new line, charwise
    /// paste inline next to the caret.
    register_linewise: bool,
    /// Undo history: `(text, caret)` snapshots, one per change-group, oldest
    /// first. We snapshot the whole buffer rather than journal diffs — prose
    /// notes are small and PSRAM is ample (8 MB), so a full copy per edit is
    /// cheap and far simpler to reason about. Bounded to [`UNDO_DEPTH`] groups.
    undo: Vec<(String, usize)>,
    /// Redo history: states popped by `u`, replayable with `Ctrl-r`. Cleared the
    /// moment a fresh edit records a new undo baseline (a new branch of history).
    redo: Vec<(String, usize)>,
    /// The last completed change, as the exact key sequence that produced it —
    /// replayed verbatim by `.`. Recording keystrokes (rather than a structured
    /// op) is what lets `.` repeat an insert session like `ciwfoo<Esc>`.
    dot: Vec<Key>,
    /// The change currently being recorded, if one is in progress (from the
    /// initiating key through the key that completes it). Committed to [`dot`] on
    /// completion. `None` between changes.
    dot_recording: Option<Vec<Key>>,
    /// True while `.` is replaying [`dot`], so the replayed keys are neither
    /// re-recorded nor able to re-trigger `.`.
    replaying: bool,
}

/// Maximum undo depth (change-groups). A full-buffer snapshot per group means
/// worst-case memory is `UNDO_DEPTH × buffer size`; for note-sized files on the
/// 8 MB PSRAM this is negligible, and prose editing rarely nears 100 groups
/// between saves anyway.
const UNDO_DEPTH: usize = 100;

/// One wrapped display line: its text and the buffer offset of its first char.
struct Line {
    start: usize,
    text: String,
}

impl Editor {
    pub fn new() -> Self {
        Editor {
            text: String::new(),
            caret: 0,
            mode: Mode::Normal, // power-on = Normal (vim-style); `with_text` boots the same
            scroll_top: 0,
            count: 0,
            pending_op: None,
            pending_obj: None,
            pending_g: false,
            cmdline: String::new(),
            shown_words: 0,
            keyboard_present: false,
            notice: None,
            format_on_save: true,
            register: String::new(),
            register_linewise: false,
            undo: Vec::new(),
            redo: Vec::new(),
            dot: Vec::new(),
            dot_recording: None,
            replaying: false,
        }
    }

    /// Seed a fresh editor from previously saved text — the boot-load path
    /// (`storage.load()` → `Editor`). Boots in **Normal** mode (vim opens a file
    /// in Normal, not Insert) with the caret on the *last* character — the
    /// resume point — matching the Esc→Normal convention rather than sitting one
    /// cell past the end. The first [`Editor::draw`] scrolls it into view. An
    /// empty string is equivalent to [`Editor::new`].
    pub fn with_text(text: String) -> Self {
        let mut ed = Editor { text, ..Editor::new() };
        ed.caret = ed.text.len();
        if ed.caret > ed.line_start(ed.caret) {
            ed.caret = ed.prev_char(ed.caret);
        }
        ed
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The full buffer contents, for the host to persist on `:w`/`:sync`.
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn scroll_top(&self) -> usize {
        self.scroll_top
    }

    /// Recompute the panel word-count snapshot from the buffer. The main loop
    /// calls this on a typing pause and on non-Insert actions, so the panel
    /// count stays current without repainting on every keystroke.
    pub fn refresh_stats(&mut self) {
        self.shown_words = self.word_count();
    }

    /// Tell the editor whether a keyboard is attached (for the panel flag).
    pub fn set_keyboard_present(&mut self, present: bool) {
        self.keyboard_present = present;
    }

    /// Post a transient side-panel notice ("snackbar") — e.g. the result of a
    /// save or publish. Shown from the next [`Editor::draw`] until the next
    /// keystroke dismisses it (see [`Editor::handle`]). The host calls this from
    /// its `:` command effect handlers.
    pub fn set_notice(&mut self, msg: impl Into<String>) {
        self.notice = Some(msg.into());
    }

    /// Whitespace-delimited word count of the whole buffer.
    fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }

    /// Dispatch one decoded key event according to the current mode, returning
    /// any [`Effect`] the host must carry out (only `:` commands produce one;
    /// every other key yields [`Effect::None`]).
    pub fn handle(&mut self, key: Key) -> Effect {
        // Any keystroke dismisses the transient notice ("snackbar"). The host
        // sets a fresh one *after* handle() returns (on a `:` command's effect),
        // so a save/publish message survives to the next draw, then clears the
        // moment you move on — no timed repaint (which on e-ink would cost a
        // full ~630 ms flash just to erase text).
        self.notice = None;

        // `.` repeats the last change — intercepted before dispatch (in Normal,
        // not mid-command, not already replaying) so the '.' keystroke itself is
        // never inserted or recorded. In Insert mode '.' falls through as a
        // literal character.
        if !self.replaying
            && self.mode == Mode::Normal
            && self.pending_op.is_none()
            && self.pending_obj.is_none()
            && self.dot_recording.is_none()
            && key == Key::Char('.')
        {
            self.repeat_last_change();
            return Effect::None;
        }

        // State before dispatch, so `record_dot` can read the transition a key
        // caused (entered Insert, started an operator, …).
        let before_mode = self.mode;
        let before_pending = self.pending_op.is_some() || self.pending_obj.is_some();

        let effect = match self.mode {
            Mode::Insert => {
                self.insert_key(key);
                Effect::None
            }
            Mode::Normal => {
                self.normal_key(key);
                Effect::None
            }
            Mode::View => {
                self.view_key(key);
                Effect::None
            }
            Mode::Command => self.command_key(key),
        };

        if !self.replaying {
            self.record_dot(key, before_mode, before_pending);
        }
        effect
    }

    /// Record `key` into the in-progress change for `.`. Called after dispatch
    /// with the mode/operator state as it was *before*. A change is recorded
    /// from its initiating key (an edit `x`/`p`/`P`, an operator `d`/`c`, or any
    /// key that enters Insert) through the key that completes it — an operator
    /// resolving back to Normal, or `Esc` ending an insert session. Yank (`y`)
    /// and pure motions never start a recording, so `.` ignores them. The leading
    /// count is not captured (so `3x` then `.` deletes one), but a count *inside*
    /// an operator is (`d2w` records in full).
    fn record_dot(&mut self, key: Key, before_mode: Mode, before_pending: bool) {
        if self.dot_recording.is_some() {
            self.dot_recording.as_mut().unwrap().push(key);
            if self.change_complete() {
                self.dot = self.dot_recording.take().unwrap();
            }
            return;
        }
        // Not yet recording: does this key begin a change? Only from a clean
        // Normal state (no operator already pending — that key belongs to an
        // in-progress command we'd have been recording already).
        if before_mode == Mode::Normal && !before_pending {
            let starts = matches!(key, Key::Char('x') | Key::Char('p') | Key::Char('P'))
                || self.mode == Mode::Insert
                || matches!(self.pending_op, Some(Op::Delete) | Some(Op::Change));
            if starts {
                self.dot_recording = Some(vec![key]);
                if self.change_complete() {
                    self.dot = self.dot_recording.take().unwrap();
                }
            }
        }
    }

    /// A recorded change is complete once we're back in Normal with no operator
    /// still pending (an immediate edit, a resolved operator, or a finished
    /// insert session).
    fn change_complete(&self) -> bool {
        self.mode == Mode::Normal && self.pending_op.is_none() && self.pending_obj.is_none()
    }

    /// `.` — replay the last recorded change. Sets [`replaying`](Self::replaying)
    /// so the replayed keys are not themselves recorded and cannot recurse.
    fn repeat_last_change(&mut self) {
        if self.dot.is_empty() {
            return;
        }
        self.replaying = true;
        for k in self.dot.clone() {
            self.handle(k);
        }
        self.replaying = false;
    }

    // --- Insert mode -------------------------------------------------------

    fn insert_key(&mut self, key: Key) {
        match key {
            Key::Char('\t') => self.insert_str(TAB),
            Key::Char(c) => self.insert_char(c),
            Key::Enter => self.insert_newline(),
            Key::Backspace => self.backspace(),
            Key::DeleteWord => self.delete_word_before(),
            Key::DeleteLine => self.delete_to_line_start(),
            // Half-page scroll is a navigation gesture — Normal/View only. In
            // Insert it's a no-op rather than yanking the caret off the text
            // you're typing. Redo (Ctrl-r) is likewise Normal-only here.
            Key::HalfPageDown | Key::HalfPageUp | Key::Redo => {}
            Key::Escape => {
                self.mode = Mode::Normal;
                // vim drops the caret onto the last inserted char.
                if self.caret > self.line_start(self.caret) {
                    self.caret = self.prev_char(self.caret);
                }
            }
        }
    }

    // --- Normal mode -------------------------------------------------------

    fn normal_key(&mut self, key: Key) {
        let c = match key {
            Key::Char(c) => c,
            // Ctrl-d/u: scroll half a screen by *display* rows (see
            // `move_display_rows`). Like any non-motion key, they abandon a
            // pending count/operator first.
            Key::HalfPageDown => {
                self.reset_pending();
                self.move_display_rows(HALF_PAGE as isize);
                return;
            }
            Key::HalfPageUp => {
                self.reset_pending();
                self.move_display_rows(-(HALF_PAGE as isize));
                return;
            }
            // Ctrl-r redo: like any non-motion key it abandons a pending command.
            Key::Redo => {
                self.reset_pending();
                self.redo();
                return;
            }
            // Esc and other non-character events cancel any pending command.
            _ => {
                self.reset_pending();
                return;
            }
        };

        // Operator pending (d/c): expect a text object, motion, or doubled op.
        if let Some(op) = self.pending_op {
            // After an i/a prefix, `c` is the text-object selector.
            if let Some(around) = self.pending_obj {
                self.pending_obj = None;
                self.pending_op = None;
                if let Some((s, e)) = self.text_object(c, around) {
                    self.apply_op(op, s, e);
                }
                self.count = 0;
                return;
            }
            // A count between the operator and its motion (e.g. `d2w`).
            if c.is_ascii_digit() && !(c == '0' && self.count == 0) {
                self.count = self.count.saturating_mul(10) + (c as usize - '0' as usize);
                return;
            }
            let n = self.count.max(1);
            match c {
                'i' => {
                    self.pending_obj = Some(false);
                    self.count = 0;
                    return;
                }
                'a' => {
                    self.pending_obj = Some(true);
                    self.count = 0;
                    return;
                }
                'd' if op == Op::Delete => {
                    self.checkpoint(); // one snapshot for the whole `ndd`
                    self.register_lines(n); // yank the lines before removing them
                    (0..n).for_each(|_| self.delete_current_line());
                }
                'c' if op == Op::Change => self.change_current_line(),
                'y' if op == Op::Yank => self.register_lines(n), // `yy` — caret stays put
                'w' => {
                    let mut t = self.caret;
                    (0..n).for_each(|_| t = self.word_forward_pos(t));
                    self.apply_op(op, self.caret, t);
                }
                'b' => {
                    let mut t = self.caret;
                    (0..n).for_each(|_| t = self.word_back_pos(t));
                    self.apply_op(op, self.caret, t);
                }
                'e' => {
                    let mut t = self.caret;
                    (0..n).for_each(|_| t = self.word_end_pos(t));
                    // Inclusive of the last char: end the range past it.
                    self.apply_op(op, self.caret, self.next_char(t));
                }
                '0' => self.apply_op(op, self.line_start(self.caret), self.caret),
                '$' => self.apply_op(op, self.caret, self.line_end(self.caret)),
                _ => {}
            }
            self.pending_op = None;
            self.count = 0;
            return;
        }

        if self.pending_g {
            self.pending_g = false;
            if c == 'g' {
                self.caret = 0;
            }
            self.count = 0;
            return;
        }

        // Count prefix: a leading `0` is the line-start motion, not a digit.
        if c.is_ascii_digit() && !(c == '0' && self.count == 0) {
            self.count = self.count.saturating_mul(10) + (c as usize - '0' as usize);
            return;
        }
        let n = self.count.max(1);

        match c {
            'h' => (0..n).for_each(|_| self.move_left()),
            'l' => (0..n).for_each(|_| self.move_right()),
            'j' => (0..n).for_each(|_| self.move_down()),
            'k' => (0..n).for_each(|_| self.move_up()),
            'w' => (0..n).for_each(|_| self.caret = self.word_forward_pos(self.caret)),
            'b' => (0..n).for_each(|_| self.caret = self.word_back_pos(self.caret)),
            'e' => (0..n).for_each(|_| self.caret = self.word_end_pos(self.caret)),
            '0' => self.caret = self.line_start(self.caret),
            '$' => self.caret = self.line_end(self.caret),
            'G' => self.caret = self.line_start(self.text.len()),
            'g' => {
                self.pending_g = true;
                return;
            }
            'x' => {
                self.checkpoint();
                // Yank the chars we're about to delete (charwise), so `x`…`p`
                // works. `x` never crosses the line end.
                let s = self.caret;
                let le = self.line_end(s);
                let mut e = s;
                for _ in 0..n {
                    if e >= le {
                        break;
                    }
                    e = self.next_char(e);
                }
                self.register = self.text[s..e].to_string();
                self.register_linewise = false;
                (0..n).for_each(|_| self.delete_at_caret());
            }
            'u' => self.undo(),
            'd' => {
                self.pending_op = Some(Op::Delete);
                return;
            }
            'c' => {
                self.pending_op = Some(Op::Change);
                return;
            }
            'y' => {
                self.pending_op = Some(Op::Yank);
                return;
            }
            'p' => self.paste_after(n),
            'P' => self.paste_before(n),
            // Entering Insert snapshots once here; the whole session (up to Esc)
            // is one undo group, so `u` reverts an entire typed run at a time.
            'i' => {
                self.checkpoint();
                self.mode = Mode::Insert;
            }
            'a' => {
                self.checkpoint();
                self.move_right_append();
                self.mode = Mode::Insert;
            }
            'A' => {
                self.checkpoint();
                self.caret = self.line_end(self.caret);
                self.mode = Mode::Insert;
            }
            'I' => {
                self.checkpoint();
                self.caret = self.line_start(self.caret);
                self.mode = Mode::Insert;
            }
            'o' => {
                self.checkpoint();
                self.caret = self.line_end(self.caret);
                self.insert_char('\n');
                self.mode = Mode::Insert;
            }
            'O' => {
                self.checkpoint();
                let p = self.line_start(self.caret);
                self.text.insert(p, '\n');
                self.caret = p;
                self.mode = Mode::Insert;
            }
            'v' | 'V' => self.mode = Mode::View,
            ':' => {
                self.reset_pending();
                self.cmdline.clear();
                self.mode = Mode::Command;
                return;
            }
            _ => {}
        }
        self.count = 0;
    }

    // --- Command mode (`:`) ------------------------------------------------

    fn command_key(&mut self, key: Key) -> Effect {
        match key {
            Key::Char(c) => self.cmdline.push(c),
            Key::Backspace => {
                // Backspace on the empty command line cancels back to Normal.
                if self.cmdline.pop().is_none() {
                    self.mode = Mode::Normal;
                }
            }
            Key::Enter => {
                let effect = self.execute_command();
                self.cmdline.clear();
                self.mode = Mode::Normal;
                return effect;
            }
            Key::Escape => {
                self.cmdline.clear();
                self.mode = Mode::Normal;
            }
            Key::DeleteWord => {
                // Readline Ctrl-W: drop trailing spaces, then the word before the
                // caret — editing the `:` command line while typing it. Unlike
                // Backspace, emptying the line does not cancel back to Normal.
                while self.cmdline.ends_with(' ') {
                    self.cmdline.pop();
                }
                while !self.cmdline.is_empty() && !self.cmdline.ends_with(' ') {
                    self.cmdline.pop();
                }
            }
            // Cmd+Backspace: clear the whole command line, staying in Command.
            Key::DeleteLine => self.cmdline.clear(),
            // Tab isn't meaningful on a short command line.
            _ => {}
        }
        Effect::None
    }

    /// Run the typed `:` command, returning any [`Effect`] the host must carry
    /// out. Unknown commands are silently ignored. The `:q` quit family is
    /// deliberately absent — an always-on writing appliance has nothing to
    /// quit to; `:wq`/`:x` therefore just save (the "quit" half is dropped).
    fn execute_command(&mut self) -> Effect {
        match self.cmdline.trim() {
            "fmt" => {
                self.format_buffer();
                Effect::None
            }
            "w" | "wq" | "x" => {
                if self.format_on_save {
                    self.format_buffer();
                }
                Effect::Save
            }
            "sync" => {
                // fmt → save → commit → push: format in-core here, then the host
                // persists and publishes the already-formatted buffer.
                if self.format_on_save {
                    self.format_buffer();
                }
                Effect::Publish
            }
            "gl" => Effect::Pull,
            _ => Effect::None,
        }
    }

    /// `:fmt` — normalize the buffer (align tables, collapse duplicate blank
    /// lines, strip trailing whitespace) and keep the caret on roughly the same
    /// line (buffer length changes, so exact restoration isn't possible).
    fn format_buffer(&mut self) {
        self.checkpoint(); // `:fmt` (and format-on-save) is undoable
        let row = self.text[..self.caret].bytes().filter(|&b| b == b'\n').count();
        self.text = format_markdown(&self.text);
        // Land the caret at the start of the same logical line, clamped.
        let total = self.text.bytes().filter(|&b| b == b'\n').count() + 1;
        let target = row.min(total - 1);
        self.caret = if target == 0 {
            0
        } else {
            let mut seen = 0;
            let mut off = self.text.len();
            for (i, b) in self.text.bytes().enumerate() {
                if b == b'\n' {
                    seen += 1;
                    if seen == target {
                        off = i + 1;
                        break;
                    }
                }
            }
            off
        };
    }

    fn reset_pending(&mut self) {
        self.count = 0;
        self.pending_op = None;
        self.pending_obj = None;
        self.pending_g = false;
    }

    // --- Undo / redo -------------------------------------------------------

    /// Record the current `(text, caret)` as an undo baseline, at the *start* of
    /// a change-group, and drop the redo history (a new edit forks the timeline).
    /// Called once per change: on entering Insert (the whole session undoes
    /// together), and before each Normal-mode edit (`x`, `dd`, operators, paste,
    /// `:fmt`). If the buffer is unchanged since the last baseline it is a no-op,
    /// so calling it more than once before a mutation records only one group.
    fn checkpoint(&mut self) {
        if self.undo.last().is_some_and(|(t, _)| t == &self.text) {
            return; // nothing changed since the last baseline
        }
        self.undo.push((self.text.clone(), self.caret));
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0); // drop the oldest group
        }
        self.redo.clear();
    }

    /// `u` — restore the most recent undo baseline, pushing the current state to
    /// the redo stack. Lands in Normal mode with the caret clamped onto a char
    /// boundary. No-op with nothing to undo.
    fn undo(&mut self) {
        if let Some((text, caret)) = self.undo.pop() {
            self.redo.push((self.text.clone(), self.caret));
            self.restore(text, caret);
        }
    }

    /// `Ctrl-r` — reapply the most recently undone state. No-op with nothing to
    /// redo.
    fn redo(&mut self) {
        if let Some((text, caret)) = self.redo.pop() {
            self.undo.push((self.text.clone(), self.caret));
            self.restore(text, caret);
        }
    }

    /// Swap in a snapshot's buffer + caret, landing in Normal on a char boundary.
    fn restore(&mut self, text: String, caret: usize) {
        self.text = text;
        self.caret = caret.min(self.text.len());
        while self.caret > 0 && !self.text.is_char_boundary(self.caret) {
            self.caret -= 1;
        }
        self.mode = Mode::Normal;
        self.reset_pending();
    }

    // --- View mode ---------------------------------------------------------

    fn view_key(&mut self, key: Key) {
        match key {
            Key::Char('j') => self.scroll_top += 1, // clamped in draw()
            Key::Char('k') => self.scroll_top = self.scroll_top.saturating_sub(1),
            Key::Char(' ') => self.scroll_top += ROWS,
            // Half-page scroll, mirroring Normal mode — here it's a pure
            // viewport move (View has no caret to chase). Clamped in draw().
            Key::HalfPageDown => self.scroll_top += HALF_PAGE,
            Key::HalfPageUp => self.scroll_top = self.scroll_top.saturating_sub(HALF_PAGE),
            Key::Char('G') => {
                let total = self.layout().len();
                self.scroll_top = total.saturating_sub(ROWS);
            }
            Key::Char('g') => {
                if self.pending_g {
                    self.scroll_top = 0;
                    self.pending_g = false;
                } else {
                    self.pending_g = true;
                }
            }
            Key::Escape => {
                self.mode = Mode::Normal;
                self.pending_g = false;
            }
            _ => {}
        }
    }

    // --- Motions (all on the logical buffer) -------------------------------

    /// Offset of the start of the line containing `pos`.
    fn line_start(&self, pos: usize) -> usize {
        let b = self.text.as_bytes();
        let mut i = pos;
        while i > 0 && b[i - 1] != b'\n' {
            i -= 1;
        }
        i
    }

    /// Offset of the end of the line containing `pos` (the `\n`, or buffer end).
    fn line_end(&self, pos: usize) -> usize {
        let b = self.text.as_bytes();
        let mut i = pos;
        while i < b.len() && b[i] != b'\n' {
            i += 1;
        }
        i
    }

    /// Byte offset one character right of `i`, clamped to the buffer end. `i`
    /// must be a char boundary (every caret position is one).
    fn next_char(&self, i: usize) -> usize {
        self.text[i..].chars().next().map_or(i, |c| i + c.len_utf8())
    }

    /// Byte offset one character left of `i`, clamped to 0.
    fn prev_char(&self, i: usize) -> usize {
        self.text[..i].chars().next_back().map_or(i, |c| i - c.len_utf8())
    }

    /// Byte offset `col` characters into the text starting at `start`, clamped
    /// to `end` (so a shorter target line lands the caret at its end).
    fn advance_chars(&self, start: usize, col: usize, end: usize) -> usize {
        let mut pos = start;
        for _ in 0..col {
            if pos >= end {
                break;
            }
            pos = self.next_char(pos);
        }
        pos.min(end)
    }

    fn move_left(&mut self) {
        if self.caret > self.line_start(self.caret) {
            self.caret = self.prev_char(self.caret);
        }
    }

    fn move_right(&mut self) {
        if self.caret < self.line_end(self.caret) {
            self.caret = self.next_char(self.caret);
        }
    }

    /// Like `l` but allowed to land one past the last char (for `a`).
    fn move_right_append(&mut self) {
        if self.caret < self.line_end(self.caret) {
            self.caret = self.next_char(self.caret);
        }
    }

    fn move_down(&mut self) {
        let ls = self.line_start(self.caret);
        let col = self.text[ls..self.caret].chars().count();
        let le = self.line_end(self.caret);
        if le >= self.text.len() {
            return; // already on the last line
        }
        let next_start = le + 1;
        let next_end = self.line_end(next_start);
        self.caret = self.advance_chars(next_start, col, next_end);
    }

    fn move_up(&mut self) {
        let ls = self.line_start(self.caret);
        if ls == 0 {
            return; // already on the first line
        }
        let col = self.text[ls..self.caret].chars().count();
        let prev_start = self.line_start(ls - 1);
        let prev_end = ls - 1; // the '\n' that ends the previous line
        self.caret = self.advance_chars(prev_start, col, prev_end);
    }

    /// Move the caret by `delta` **display** (soft-wrapped) rows, keeping the
    /// column where the target row is long enough. This is the `Ctrl-d`/`Ctrl-u`
    /// step: unlike `j`/`k` (which move by *logical* line and so jump over
    /// wrapped continuation rows), it walks the rendered layout, so half a page
    /// is half the visible window no matter how the prose wraps. In Normal mode
    /// the caret is always kept on-screen, so moving it *is* the scroll — the
    /// viewport follows via `adjust_scroll` at draw time.
    fn move_display_rows(&mut self, delta: isize) {
        let lay = self.layout();
        if lay.is_empty() {
            return;
        }
        let (row, col) = self.caret_rc(&lay);
        let target = (row as isize + delta).clamp(0, lay.len() as isize - 1) as usize;
        let line = &lay[target];
        let row_end = line.start + line.text.len();
        self.caret = self.advance_chars(line.start, col, row_end);
    }

    /// Start of the next whitespace-delimited word after `from`.
    fn word_forward_pos(&self, from: usize) -> usize {
        let b = self.text.as_bytes();
        let n = b.len();
        let mut i = from;
        while i < n && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        while i < n && b[i].is_ascii_whitespace() {
            i += 1;
        }
        i
    }

    /// Start of the word at or before `from`.
    fn word_back_pos(&self, from: usize) -> usize {
        let b = self.text.as_bytes();
        let mut i = from;
        while i > 0 && b[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        while i > 0 && !b[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        i
    }

    /// Byte offset of the last character of the current/next word — vim `e`
    /// lands the caret on that char. Skips any leading whitespace, then runs to
    /// the word's end; whitespace includes `\n`, so it can cross lines.
    fn word_end_pos(&self, from: usize) -> usize {
        let start = self.next_char(from);
        if start >= self.text.len() {
            return from;
        }
        let mut last = from;
        let mut in_word = false;
        for (off, c) in self.text[start..].char_indices() {
            if c.is_ascii_whitespace() {
                if in_word {
                    break;
                }
            } else {
                in_word = true;
                last = start + off;
            }
        }
        last
    }

    // --- Edits -------------------------------------------------------------

    fn insert_char(&mut self, c: char) {
        self.text.insert(self.caret, c);
        self.caret += c.len_utf8();
    }

    fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.caret, s);
        self.caret += s.len();
    }

    /// Enter in Insert mode, with Markdown list continuation. At the END of a
    /// list line (`- `/`* `/`+ ` or `N. `), start the next item automatically —
    /// same bullet, or the next number — preserving indentation. Enter on an
    /// otherwise-empty item strips the marker instead (exits the list). Anywhere
    /// else (mid-line, or a non-list line) it's a plain newline.
    fn insert_newline(&mut self) {
        let le = self.line_end(self.caret);
        if self.caret == le {
            let ls = self.line_start(self.caret);
            if let Some((next, cur_len, content_empty)) = list_marker(&self.text[ls..le]) {
                if content_empty {
                    // Empty item: drop the marker, leaving a blank line.
                    self.text.replace_range(ls..ls + cur_len, "");
                    self.caret = ls;
                } else {
                    self.insert_str(&format!("\n{next}"));
                }
                return;
            }
        }
        self.insert_char('\n');
    }

    fn backspace(&mut self) {
        if self.caret > 0 {
            self.caret = self.prev_char(self.caret);
            self.text.remove(self.caret); // removes the whole char at the caret
        }
    }

    /// `x` — delete the char under the caret (never a newline).
    fn delete_at_caret(&mut self) {
        let b = self.text.as_bytes();
        if self.caret < b.len() && b[self.caret] != b'\n' {
            self.text.remove(self.caret);
            // Keep the caret on a char: if it fell off the line end, step back.
            if self.caret >= self.line_end(self.caret) && self.caret > self.line_start(self.caret) {
                self.caret = self.prev_char(self.caret);
            }
        }
    }

    /// `dd` — delete the current logical line, including its newline (or the
    /// preceding one for the last line, so no blank line is left behind).
    fn delete_current_line(&mut self) {
        let ls = self.line_start(self.caret);
        let le = self.line_end(self.caret);
        let (start, end) = if le < self.text.len() {
            (ls, le + 1) // eat the trailing newline
        } else if ls > 0 {
            (ls - 1, le) // last line: eat the preceding newline instead
        } else {
            (ls, le) // whole buffer
        };
        self.text.replace_range(start..end, "");
        self.caret = self.line_start(start.min(self.text.len()));
    }

    /// `cc` — clear the current line's text and drop into insert.
    fn change_current_line(&mut self) {
        self.checkpoint();
        let ls = self.line_start(self.caret);
        let le = self.line_end(self.caret);
        self.register = format!("{}\n", &self.text[ls..le]); // linewise, like dd
        self.register_linewise = true;
        self.text.replace_range(ls..le, "");
        self.caret = ls;
        self.mode = Mode::Insert;
    }

    /// Yank `n` logical lines from the caret's line into the register, linewise
    /// (each line carries its trailing `\n`, synthesised for a final line that
    /// lacks one). Backs both `yy`/`nyy` and `dd`/`ndd`'s register capture; does
    /// not move the caret or change the buffer.
    fn register_lines(&mut self, n: usize) {
        let ls = self.line_start(self.caret);
        let mut e = ls;
        for _ in 0..n {
            let le = self.line_end(e);
            e = if le < self.text.len() { le + 1 } else { le };
        }
        let mut block = self.text[ls..e].to_string();
        if !block.ends_with('\n') {
            block.push('\n'); // the last line has no trailing newline; add one
        }
        self.register = block;
        self.register_linewise = true;
    }

    /// `p` — paste the register `n` times after the caret. Linewise content
    /// opens new line(s) below the current line (caret to the first pasted
    /// line); charwise content goes in just after the caret char (caret on the
    /// last pasted char). No-op on an empty register.
    fn paste_after(&mut self, n: usize) {
        if self.register.is_empty() {
            return;
        }
        self.checkpoint();
        let content = self.register.repeat(n);
        if self.register_linewise {
            let le = self.line_end(self.caret);
            if le < self.text.len() {
                let at = le + 1; // start of the following line
                self.text.insert_str(at, &content);
                self.caret = at;
            } else {
                // Last line has no trailing newline: prefix one, drop the
                // block's trailing newline so we don't leave a blank line.
                let block = content.strip_suffix('\n').unwrap_or(&content);
                self.text.insert_str(le, &format!("\n{block}"));
                self.caret = le + 1;
            }
        } else {
            let at = if self.text.is_empty() { 0 } else { self.next_char(self.caret) };
            self.text.insert_str(at, &content);
            self.caret = self.prev_char(at + content.len()); // onto the last char
        }
    }

    /// `P` — paste the register `n` times before the caret. Linewise content
    /// opens new line(s) above the current line; charwise content goes in at the
    /// caret (caret on the last pasted char). No-op on an empty register.
    fn paste_before(&mut self, n: usize) {
        if self.register.is_empty() {
            return;
        }
        self.checkpoint();
        let content = self.register.repeat(n);
        if self.register_linewise {
            let ls = self.line_start(self.caret);
            self.text.insert_str(ls, &content);
            self.caret = ls;
        } else {
            let at = self.caret;
            self.text.insert_str(at, &content);
            self.caret = self.prev_char(at + content.len()); // onto the last char
        }
    }

    /// Apply a pending operator over the buffer range `[start, end)` (order
    /// independent). All three fill the unnamed register (charwise) with the
    /// range. Yank copies and leaves the text; Delete removes it; Change removes
    /// it and enters insert. Yank leaves the caret at the range start (vim `yw`),
    /// the others land it there because the text collapses to that point.
    fn apply_op(&mut self, op: Op, start: usize, end: usize) {
        let s = start.min(end);
        let e = start.max(end).min(self.text.len());
        self.register = self.text[s..e].to_string();
        self.register_linewise = false;
        if op == Op::Yank {
            self.caret = s;
            return;
        }
        self.checkpoint(); // Delete/Change mutate — snapshot for undo
        self.text.replace_range(s..e, "");
        self.caret = s.min(self.text.len());
        if op == Op::Change {
            self.mode = Mode::Insert;
        }
    }

    /// Resolve a text object to a buffer range. `around` selects `a` (include
    /// delimiters / trailing space) vs `i` (inner). Returns `None` if there's
    /// no matching object under the caret.
    fn text_object(&self, obj: char, around: bool) -> Option<(usize, usize)> {
        match obj {
            'w' => Some(self.word_object(around)),
            '(' | ')' | 'b' => self.pair_object(b'(', b')', around),
            '{' | '}' | 'B' => self.pair_object(b'{', b'}', around),
            '[' | ']' => self.pair_object(b'[', b']', around),
            '<' | '>' => self.pair_object(b'<', b'>', around),
            '"' => self.quote_object(b'"', around),
            '\'' => self.quote_object(b'\'', around),
            '`' => self.quote_object(b'`', around),
            _ => None,
        }
    }

    /// `iw`/`aw`: the run of same-class chars (word vs space, never crossing a
    /// newline) under the caret. `aw` also takes the trailing run of spaces, or
    /// the leading one if there is no trailing space. Word class is
    /// whitespace-delimited (so this behaves like vim's `iW`/`aW`).
    fn word_object(&self, around: bool) -> (usize, usize) {
        let b = self.text.as_bytes();
        let n = b.len();
        if n == 0 {
            return (0, 0);
        }
        let pos = self.caret.min(n - 1);
        let ws = |c: u8| c == b' ' || c == b'\t';
        let target_ws = ws(b[pos]);
        let same = |c: u8| ws(c) == target_ws && c != b'\n';
        let mut s = pos;
        while s > 0 && same(b[s - 1]) {
            s -= 1;
        }
        let mut e = pos + 1;
        while e < n && same(b[e]) {
            e += 1;
        }
        if around && !target_ws {
            let mut a = e;
            while a < n && ws(b[a]) {
                a += 1;
            }
            if a > e {
                return (s, a);
            }
            let mut ls = s;
            while ls > 0 && ws(b[ls - 1]) {
                ls -= 1;
            }
            return (ls, e);
        }
        (s, e)
    }

    /// `i(`/`a(` and friends: the range between the bracket pair enclosing the
    /// caret, nesting-aware. `around` includes the brackets themselves.
    fn pair_object(&self, open: u8, close: u8, around: bool) -> Option<(usize, usize)> {
        let b = self.text.as_bytes();
        let n = b.len();
        if n == 0 {
            return None;
        }
        let start = self.caret.min(n - 1);
        // Scan left for the enclosing open bracket.
        let mut depth = 0i32;
        let mut i = start;
        let open_idx = loop {
            let ch = b[i];
            if ch == close && i != start {
                depth += 1;
            } else if ch == open {
                if depth == 0 {
                    break Some(i);
                }
                depth -= 1;
            }
            if i == 0 {
                break None;
            }
            i -= 1;
        }?;
        // Scan right for its matching close.
        let mut depth = 0i32;
        let mut j = open_idx + 1;
        let close_idx = loop {
            if j >= n {
                break None;
            }
            let ch = b[j];
            if ch == open {
                depth += 1;
            } else if ch == close {
                if depth == 0 {
                    break Some(j);
                }
                depth -= 1;
            }
            j += 1;
        }?;
        Some(if around {
            (open_idx, close_idx + 1)
        } else {
            (open_idx + 1, close_idx)
        })
    }

    /// `i"`/`a"` and friends: the range between a matching quote pair on the
    /// current line. `around` includes the quotes.
    fn quote_object(&self, q: u8, around: bool) -> Option<(usize, usize)> {
        let b = self.text.as_bytes();
        let ls = self.line_start(self.caret);
        let le = self.line_end(self.caret);
        let quotes: Vec<usize> = (ls..le).filter(|&i| b[i] == q).collect();
        // Pair them left-to-right; take the first pair closing at/after the caret.
        let mut k = 0;
        while k + 1 < quotes.len() {
            let (a, z) = (quotes[k], quotes[k + 1]);
            if self.caret <= z {
                return Some(if around { (a, z + 1) } else { (a + 1, z) });
            }
            k += 2;
        }
        None
    }

    /// Insert-mode Ctrl+W / Ctrl+Backspace: delete the word before the caret.
    fn delete_word_before(&mut self) {
        let b = self.text.as_bytes();
        let mut i = self.caret;
        while i > 0 && (b[i - 1] == b' ' || b[i - 1] == b'\t') {
            i -= 1;
        }
        while i > 0 && !b[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        self.text.replace_range(i..self.caret, "");
        self.caret = i;
    }

    /// Insert-mode Cmd+Backspace: delete back to the start of the line, or the
    /// preceding newline if already there.
    fn delete_to_line_start(&mut self) {
        let ls = self.line_start(self.caret);
        if ls == self.caret {
            if self.caret > 0 {
                self.caret -= 1;
                self.text.remove(self.caret);
            }
        } else {
            self.text.replace_range(ls..self.caret, "");
            self.caret = ls;
        }
    }

    // --- Rendering ---------------------------------------------------------

    /// Number of logical lines in the buffer (1 + newline count). Used to size
    /// the line-number gutter.
    fn logical_lines(&self) -> usize {
        self.text.bytes().filter(|&b| b == b'\n').count() + 1
    }

    /// Width of the absolute line-number gutter, in display columns: enough
    /// digits for the buffer's largest line number (min [`GUTTER_MIN_DIGITS`])
    /// plus a 1-column separator before the text. Sized from the *total* line
    /// count, not the visible range, so it stays fixed while scrolling — only
    /// crossing a power of ten (100, 1000, …) reflows the wrap, which is rare.
    fn gutter_cols(&self) -> usize {
        let digits = self.logical_lines().to_string().len().max(GUTTER_MIN_DIGITS);
        digits + 1
    }

    /// Character columns left for text once the gutter is reserved. The writing
    /// region is fixed at [`WRITE_COLS`]; the gutter steals from it, so text
    /// soft-wraps narrower.
    fn text_cols(&self) -> usize {
        WRITE_COLS - self.gutter_cols()
    }

    /// Wrap the buffer into display lines, tracking each line's buffer offset.
    /// Soft-wrap at word boundaries: a logical line too long for [`text_cols`]
    /// (the writing width left after the line-number gutter) breaks at the last
    /// space that fits, so words are never split — except a single word wider
    /// than the column, hard-broken at [`text_cols`].
    /// Wrapping counts characters (one per display cell), while `Line.start` is
    /// a byte offset into the buffer, so caret math stays correct for multi-byte
    /// (accented) characters.
    fn layout(&self) -> Vec<Line> {
        let cols = self.text_cols(); // writing width after the gutter is reserved
        let mut lines: Vec<Line> = Vec::new();
        let mut base = 0usize; // byte offset of the current logical line's start
        for logical in self.text.split('\n') {
            let chars: Vec<char> = logical.chars().collect();
            if chars.is_empty() {
                lines.push(Line { start: base, text: String::new() });
            } else {
                let mut c = 0usize; // char index within `logical`
                let mut byte = 0usize; // byte offset of chars[c] within `logical`
                while c < chars.len() {
                    let remaining = chars.len() - c;
                    let take = if remaining <= cols {
                        remaining
                    } else {
                        // Break at the last space within the COLS-wide window;
                        // include that space on this line. No space → hard break.
                        let window = c + cols;
                        let mut brk = None;
                        let mut p = window;
                        while p > c {
                            p -= 1;
                            if chars[p] == ' ' {
                                brk = Some(p);
                                break;
                            }
                        }
                        match brk {
                            Some(sp) if sp > c => sp + 1 - c,
                            _ => cols,
                        }
                    };
                    lines.push(Line {
                        start: base + byte,
                        text: chars[c..c + take].iter().collect(),
                    });
                    byte += chars[c..c + take].iter().map(|ch| ch.len_utf8()).sum::<usize>();
                    c += take;
                }
            }
            base += logical.len() + 1; // bytes + the '\n' that `split` consumed
        }
        lines
    }

    /// Display (row, col) of the caret within `lay`. `col` is a character count
    /// (display cells) from the row's start, not a byte offset, so it is correct
    /// for multi-byte characters and indexes `Line.text` via `chars().nth`.
    fn caret_rc(&self, lay: &[Line]) -> (usize, usize) {
        let mut row = 0;
        for (i, l) in lay.iter().enumerate() {
            if l.start <= self.caret {
                row = i;
            } else {
                break;
            }
        }
        let col = self.text[lay[row].start..self.caret].chars().count();
        (row, col)
    }

    /// Move the viewport so the caret stays visible (Normal/Insert), or just
    /// clamp it to the content (View).
    fn adjust_scroll(&mut self, caret_row: usize, total: usize) {
        match self.mode {
            Mode::View => {
                let max = total.saturating_sub(ROWS);
                if self.scroll_top > max {
                    self.scroll_top = max;
                }
            }
            _ => {
                if caret_row < self.scroll_top {
                    self.scroll_top = caret_row;
                } else if caret_row >= self.scroll_top + ROWS {
                    self.scroll_top = caret_row + 1 - ROWS;
                }
            }
        }
    }

    /// Is the logical line starting at `ls` a Markdown ATX heading — 1–6 `#`
    /// followed by a space? (Used to render heading lines bold.)
    fn is_heading_at(&self, ls: usize) -> bool {
        let b = self.text.as_bytes();
        let mut i = ls;
        while i < b.len() && b[i] == b'#' {
            i += 1;
        }
        let hashes = i - ls;
        (1..=6).contains(&hashes) && b.get(i) == Some(&b' ')
    }

    /// Render the current state into a frame. `cursor_on` gates the caret: the
    /// Insert bar caret is suppressed while typing and shown after a pause, and
    /// `false` also suppresses the Normal block caret so callers can render pure
    /// text (e.g. a boot message). View never draws a caret. In the main loop
    /// Normal always passes `true`, so its block caret is unaffected.
    pub fn draw(&mut self, cursor_on: bool) -> Frame {
        let lay = self.layout();
        let (crow, ccol) = self.caret_rc(&lay);
        self.adjust_scroll(crow, lay.len());

        let mut f = Frame::new_white();
        let text_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
        let gutter = self.gutter_cols();
        let cols = WRITE_COLS - gutter; // text columns after the gutter
        let gx = gutter as i32 * CW; // text (and cursor) x-origin, past the gutter
        let digits = gutter - 1; // number field width; the last col is the separator
        let end = (self.scroll_top + ROWS).min(lay.len());
        // Absolute line number of the first visible row's logical line, then
        // bumped as later logical lines scroll into view.
        let mut line_no = self.text.as_bytes()[..lay[self.scroll_top.min(lay.len() - 1)].start]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
            + 1;
        for (vis, li) in (self.scroll_top..end).enumerate() {
            let y = vis as i32 * CH;
            // Number a logical line only on its first display row; wrapped
            // continuation rows leave the gutter blank.
            let first_row = lay[li].start == self.line_start(lay[li].start);
            if li > self.scroll_top && first_row {
                line_no += 1;
            }
            if first_row {
                let label = format!("{line_no:>digits$}");
                Text::with_baseline(&label, Point::new(0, y), text_style, Baseline::Top)
                    .draw(&mut f)
                    .unwrap();
            }
            Text::with_baseline(&lay[li].text, Point::new(gx, y), text_style, Baseline::Top)
                .draw(&mut f)
                .unwrap();
            // Markdown heading (`#`..`######` + space): faux-bold by double-
            // striking the whole display line 1px to the right (no bold Latin-9
            // font exists). Checks the logical line so wrapped headings stay bold.
            if self.is_heading_at(self.line_start(lay[li].start)) {
                Text::with_baseline(&lay[li].text, Point::new(gx + 1, y), text_style, Baseline::Top)
                    .draw(&mut f)
                    .unwrap();
            }
        }

        if crow >= self.scroll_top && crow < self.scroll_top + ROWS {
            let x = gx + ccol.min(cols - 1) as i32 * CW;
            let y = (crow - self.scroll_top) as i32 * CH;
            match self.mode {
                Mode::Normal if cursor_on => {
                    // Block caret: fill the cell, redraw the glyph in white.
                    Rectangle::new(Point::new(x, y), Size::new(CW as u32, CH as u32))
                        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                        .draw(&mut f)
                        .unwrap();
                    if let Some(ch) = lay[crow].text.chars().nth(ccol) {
                        let mut buf = [0u8; 4];
                        let inv = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
                        Text::with_baseline(
                            ch.encode_utf8(&mut buf),
                            Point::new(x, y),
                            inv,
                            Baseline::Top,
                        )
                        .draw(&mut f)
                        .unwrap();
                    }
                }
                Mode::Insert if cursor_on => {
                    // Bar caret at the left edge of the cell.
                    Rectangle::new(Point::new(x, y), Size::new(2, CH as u32))
                        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                        .draw(&mut f)
                        .unwrap();
                }
                _ => {}
            }
        }

        self.draw_panel(&mut f);
        self.draw_cmdline(&mut f);
        f
    }

    /// Draw the side panel: a full-height rule, word count at the top, and the
    /// mode indicator + pending-command echo at the bottom-left, with a
    /// keyboard-disconnect flag just above the mode while the keyboard is
    /// dropped. Small 6×10 font. This is the surface every later field
    /// (filename, clock, Wi-Fi, publish state) will add to. Word count is a
    /// throttled snapshot and the rest is event-driven, so the panel never
    /// repaints per keystroke.
    fn draw_panel(&self, f: &mut Frame) {
        // The rule dividing writing column from panel, full panel height.
        Rectangle::new(Point::new(DIVIDER_X, 0), Size::new(1, HEIGHT as u32))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(f)
            .unwrap();

        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

        // Word count, from the throttled snapshot (never per keystroke).
        let words = format!("{} words", self.shown_words);
        Text::with_baseline(&words, Point::new(PANEL_X, 2), style, Baseline::Top)
            .draw(f)
            .unwrap();

        // Transient notice ("snackbar"), just under the word count: the last
        // save/publish result. Clamped to the panel width so a long message
        // can't spill past the right edge; cleared on the next keystroke.
        if let Some(msg) = &self.notice {
            let shown: String = msg.chars().take(PANEL_COLS).collect();
            Text::with_baseline(&shown, Point::new(PANEL_X, 16), style, Baseline::Top)
                .draw(f)
                .unwrap();
        }

        // Keyboard-disconnect flag, just above the mode line, shown only while
        // the keyboard is dropped. Latin-9 has no ⌨/✗ glyph, so plain text.
        if !self.keyboard_present {
            Text::with_baseline(
                "NO KBD",
                Point::new(PANEL_X, HEIGHT as i32 - 24),
                style,
                Baseline::Top,
            )
            .draw(f)
            .unwrap();
        }

        // Mode indicator + pending count/operator echo at the panel's bottom-
        // left. In Command mode the ':' line (bottom strip) takes over instead.
        // All event-driven — never repaints per keystroke.
        if self.mode != Mode::Command {
            let name = match self.mode {
                Mode::Normal => "NORMAL",
                Mode::Insert => "INSERT",
                Mode::View => "VIEW",
                Mode::Command => unreachable!(),
            };
            let mut s = format!("-- {name} --");
            if self.count > 0 {
                s.push_str(&format!(" {}", self.count));
            }
            match self.pending_op {
                Some(Op::Delete) => s.push('d'),
                Some(Op::Change) => s.push('c'),
                Some(Op::Yank) => s.push('y'),
                None => {}
            }
            match self.pending_obj {
                Some(false) => s.push('i'),
                Some(true) => s.push('a'),
                None => {}
            }
            if self.pending_g {
                s.push('g');
            }
            Text::with_baseline(
                &s,
                Point::new(PANEL_X, HEIGHT as i32 - 12),
                style,
                Baseline::Top,
            )
            .draw(f)
            .unwrap();
        }
    }

    /// The transient `:` command line, in the bottom strip below the writing
    /// column (vim-style). Shown only while composing a command.
    fn draw_cmdline(&self, f: &mut Frame) {
        if self.mode != Mode::Command {
            return;
        }
        let s = format!(":{}", self.cmdline);
        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        Text::with_baseline(&s, Point::new(2, ROWS as i32 * CH + 1), style, Baseline::Top)
            .draw(f)
            .unwrap();
    }
}

/// Parse a Markdown list marker at the start of `line`. Returns
/// `(next_marker, current_marker_len, content_empty)` where `next_marker` is what
/// the following item should start with (same bullet, or the incremented number,
/// preserving indentation), `current_marker_len` is the byte length of this
/// line's marker prefix, and `content_empty` is whether anything follows it.
/// Returns `None` when the line isn't a list item. ASCII throughout (leading
/// spaces, bullets, digits, `. ` are all single-byte).
fn list_marker(line: &str) -> Option<(String, usize, bool)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    for bullet in ["- ", "* ", "+ "] {
        if rest.starts_with(bullet) {
            let cur_len = indent + bullet.len();
            let content_empty = line[cur_len..].trim().is_empty();
            return Some((format!("{}{bullet}", &line[..indent]), cur_len, content_empty));
        }
    }
    // Ordered: <digits>`. ` → continue as the next number.
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && rest[digits..].starts_with(". ") {
        let cur_len = indent + digits + 2;
        let content_empty = line[cur_len..].trim().is_empty();
        let n: usize = rest[..digits].parse().unwrap_or(0);
        return Some((format!("{}{}. ", &line[..indent], n + 1), cur_len, content_empty));
    }
    None
}

// --- `:fmt` Markdown normalizer ----------------------------------------------

/// Column alignment parsed from a table's `|:--:|` separator row.
#[derive(Clone, Copy)]
enum Align {
    Left,
    Right,
    Center,
    None,
}

/// Normalize a Markdown buffer for `:fmt`: strip trailing whitespace, align
/// pipe tables, and collapse runs of blank lines to a single blank (dropping
/// trailing blanks). Deliberately does NOT reflow paragraphs — the buffer's
/// logical line breaks are the writer's, and display wrapping is soft (see
/// `layout`). ASCII throughout (widths are char counts).
fn format_markdown(text: &str) -> String {
    // 1. Trailing-whitespace strip, per line.
    let stripped: Vec<String> = text.split('\n').map(|l| l.trim_end().to_string()).collect();

    // 2. Reformat pipe-table blocks in place; pass everything else through.
    let mut piped: Vec<String> = Vec::with_capacity(stripped.len());
    let mut i = 0;
    while i < stripped.len() {
        if let Some(len) = table_block_len(&stripped[i..]) {
            piped.extend(format_table(&stripped[i..i + len]));
            i += len;
        } else {
            piped.push(stripped[i].clone());
            i += 1;
        }
    }

    // 3. Collapse 2+ consecutive blank lines to one; drop trailing blanks.
    let mut out: Vec<String> = Vec::with_capacity(piped.len());
    let mut blank_run = 0;
    for line in piped {
        if line.is_empty() {
            blank_run += 1;
            if blank_run == 1 {
                out.push(String::new());
            }
        } else {
            blank_run = 0;
            out.push(line);
        }
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// Split a table row into trimmed cells, dropping the empty cells that leading /
/// trailing `|` produce (`| a | b |` → `["a", "b"]`).
fn table_cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_string()).collect()
}

/// A separator row: every cell is dashes with optional edge colons (`:--`, `-:`,
/// `:-:`, `---`) and at least one dash.
fn is_separator_row(line: &str) -> bool {
    if !line.contains('|') {
        return false;
    }
    let cells = table_cells(line);
    !cells.is_empty()
        && cells.iter().all(|c| {
            !c.is_empty() && c.contains('-') && c.chars().all(|ch| ch == '-' || ch == ':')
        })
}

/// If `lines[0..]` starts a pipe table (header row + separator row + data rows),
/// return its length in lines; else `None`.
fn table_block_len(lines: &[String]) -> Option<usize> {
    if lines.len() < 2 || !lines[0].contains('|') || !is_separator_row(&lines[1]) {
        return None;
    }
    let mut n = 2;
    while n < lines.len() && !lines[n].is_empty() && lines[n].contains('|') {
        n += 1;
    }
    Some(n)
}

/// Reformat one detected table block: pad every cell to its column's width and
/// rebuild the separator row, honoring per-column alignment colons.
fn format_table(block: &[String]) -> Vec<String> {
    let rows: Vec<Vec<String>> = block.iter().map(|l| table_cells(l)).collect();
    let aligns: Vec<Align> = rows[1]
        .iter()
        .map(|c| match (c.starts_with(':'), c.ends_with(':')) {
            (true, true) => Align::Center,
            (true, false) => Align::Left,
            (false, true) => Align::Right,
            (false, false) => Align::None,
        })
        .collect();
    let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(aligns.len());

    // Column widths from content rows (min 3 so the separator stays readable).
    let mut width = vec![3usize; ncols];
    for (ri, row) in rows.iter().enumerate() {
        if ri == 1 {
            continue; // the separator's own width doesn't constrain the column
        }
        for (ci, cell) in row.iter().enumerate() {
            width[ci] = width[ci].max(cell.chars().count());
        }
    }
    let align_of = |ci: usize| aligns.get(ci).copied().unwrap_or(Align::None);

    let mut out = Vec::with_capacity(rows.len());
    for (ri, row) in rows.iter().enumerate() {
        let cells: Vec<String> = (0..ncols)
            .map(|ci| {
                let w = width[ci];
                if ri == 1 {
                    match align_of(ci) {
                        Align::Left => format!(":{}", "-".repeat(w - 1)),
                        Align::Right => format!("{}:", "-".repeat(w - 1)),
                        Align::Center => format!(":{}:", "-".repeat(w - 2)),
                        Align::None => "-".repeat(w),
                    }
                } else {
                    pad_cell(row.get(ci).map(String::as_str).unwrap_or(""), w, align_of(ci))
                }
            })
            .collect();
        out.push(format!("| {} |", cells.join(" | ")));
    }
    out
}

/// Pad `cell` to `w` columns per `align` (left/none pad right, right pads left,
/// center splits). Over-wide cells are returned unchanged.
fn pad_cell(cell: &str, w: usize, align: Align) -> String {
    let len = cell.chars().count();
    if len >= w {
        return cell.to_string();
    }
    let pad = w - len;
    match align {
        Align::Right => format!("{}{cell}", " ".repeat(pad)),
        Align::Center => {
            let l = pad / 2;
            format!("{}{cell}{}", " ".repeat(l), " ".repeat(pad - l))
        }
        _ => format!("{cell}{}", " ".repeat(pad)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Type a run of characters in Insert mode, entered with `i` from the
    /// power-on Normal mode.
    fn typed(s: &str) -> Editor {
        let mut e = Editor::new();
        e.handle(Key::Char('i')); // Normal -> Insert
        for c in s.chars() {
            e.handle(Key::Char(c));
        }
        e
    }

    /// From a fresh editor, run `:{cmd}<Enter>`, returning the editor and the
    /// [`Effect`] the Enter produced.
    fn command(cmd: &str) -> (Editor, Effect) {
        let mut e = Editor::new();
        e.handle(Key::Escape); // ensure Normal (power-on already is)
        e.handle(Key::Char(':')); // Normal -> Command
        for c in cmd.chars() {
            e.handle(Key::Char(c));
        }
        let effect = e.handle(Key::Enter);
        (e, effect)
    }

    #[test]
    fn insert_builds_buffer_and_advances_caret() {
        let e = typed("hello");
        assert_eq!(e.text, "hello");
        assert_eq!(e.caret, 5);
        assert_eq!(e.mode(), Mode::Insert);
    }

    #[test]
    fn backspace_deletes_previous_char() {
        let mut e = typed("hello");
        e.handle(Key::Backspace);
        assert_eq!(e.text, "hell");
        assert_eq!(e.caret, 4);
    }

    #[test]
    fn enter_splits_the_line() {
        let mut e = typed("ab");
        e.handle(Key::Enter);
        e.handle(Key::Char('c'));
        assert_eq!(e.text, "ab\nc");
        assert_eq!(e.caret, 4);
    }

    #[test]
    fn escape_enters_normal_and_steps_onto_last_char() {
        let mut e = typed("abc");
        e.handle(Key::Escape);
        assert_eq!(e.mode(), Mode::Normal);
        assert_eq!(e.caret, 2); // vim: caret drops onto the last inserted char
    }

    #[test]
    fn normal_h_and_l_step_one_char() {
        let mut e = typed("abc");
        e.handle(Key::Escape); // Normal, caret = 2
        e.handle(Key::Char('h'));
        assert_eq!(e.caret, 1);
        e.handle(Key::Char('h'));
        assert_eq!(e.caret, 0);
        e.handle(Key::Char('l'));
        assert_eq!(e.caret, 1);
    }

    #[test]
    fn normal_x_deletes_char_under_caret() {
        let mut e = typed("abc");
        e.handle(Key::Escape); // caret on 'c'
        e.handle(Key::Char('h')); // caret on 'b'
        e.handle(Key::Char('x'));
        assert_eq!(e.text, "ac");
    }

    #[test]
    fn word_forward_lands_on_next_word_start() {
        let mut e = typed("foo bar");
        e.handle(Key::Escape); // Normal
        e.handle(Key::Char('0')); // line start
        e.handle(Key::Char('w'));
        assert_eq!(e.caret, 4); // 'b' of "bar"
    }

    /// The buffer round-trips and `draw()` runs for a plain-ASCII buffer — the
    /// current, byte==char world. UTF-8 (accented-input) correctness is the next
    /// change; when it lands, add the accented-motion cases here.
    #[test]
    fn draw_produces_a_full_frame_for_ascii() {
        let mut e = typed("hello world");
        let frame = e.draw(true);
        assert_eq!(frame.bytes().len(), display::FB_BYTES);
    }

    // ---- UTF-8 correctness: accented (Latin-9) input the composer feeds ----

    #[test]
    fn insert_accented_char_advances_by_utf8_len() {
        let e = typed("é");
        assert_eq!(e.text, "é");
        assert_eq!(e.caret, 2); // 'é' is two bytes; caret is a byte offset
    }

    #[test]
    fn backspace_deletes_whole_multibyte_char() {
        let mut e = typed("café");
        e.handle(Key::Backspace);
        assert_eq!(e.text, "caf");
        assert_eq!(e.caret, 3);
    }

    #[test]
    fn normal_hl_step_over_multibyte_chars() {
        let mut e = typed("aéb"); // bytes: a(1) é(2) b(1)
        e.handle(Key::Escape); // Normal, caret onto 'b' at byte 3
        assert_eq!(e.caret, 3);
        e.handle(Key::Char('h')); // onto 'é'
        assert_eq!(e.caret, 1);
        e.handle(Key::Char('h')); // onto 'a'
        assert_eq!(e.caret, 0);
        e.handle(Key::Char('l')); // back onto 'é'
        assert_eq!(e.caret, 1);
        e.handle(Key::Char('l')); // onto 'b'
        assert_eq!(e.caret, 3);
    }

    #[test]
    fn delete_char_under_caret_removes_whole_multibyte() {
        let mut e = typed("aéb");
        e.handle(Key::Escape); // caret on 'b'
        e.handle(Key::Char('h')); // caret on 'é'
        e.handle(Key::Char('x'));
        assert_eq!(e.text, "ab");
    }

    #[test]
    fn de_deletes_through_end_of_accented_word() {
        let mut e = typed("café bar");
        e.handle(Key::Escape);
        e.handle(Key::Char('0')); // line start, on 'c'
        e.handle(Key::Char('d'));
        e.handle(Key::Char('e')); // delete to the end of "café"
        assert_eq!(e.text, " bar");
    }

    #[test]
    fn vertical_move_keeps_char_column_across_accents() {
        let mut e = typed("éé"); // line 0: two 2-byte chars
        e.handle(Key::Enter);
        for c in "xxx".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Escape); // Normal, on last 'x'
        e.handle(Key::Char('k')); // up to line 0 at the same character column
        assert!(e.text.is_char_boundary(e.caret)); // never lands mid-character
    }

    #[test]
    fn draw_runs_for_accented_buffer() {
        // Every glyph here is in ISO-8859-15, which the composer is limited to.
        let mut e = typed("café naïve garçon çÿ");
        let frame = e.draw(true);
        assert_eq!(frame.bytes().len(), display::FB_BYTES);
    }

    #[test]
    fn w_command_signals_save_and_returns_to_normal() {
        let (e, eff) = command("w");
        assert_eq!(eff, Effect::Save);
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn sync_command_signals_publish() {
        assert_eq!(command("sync").1, Effect::Publish);
    }

    #[test]
    fn gl_command_signals_pull() {
        assert_eq!(command("gl").1, Effect::Pull);
    }

    #[test]
    fn sync_formats_the_buffer_before_publishing() {
        // fmt → save → commit → push: `:sync` runs :fmt in-core first (default on).
        let mut e = Editor::with_text("hello   \nworld".to_string()); // trailing spaces
        e.handle(Key::Char(':'));
        for c in "sync".chars() {
            e.handle(Key::Char(c));
        }
        let eff = e.handle(Key::Enter);
        assert_eq!(eff, Effect::Publish);
        assert_eq!(e.text(), "hello\nworld"); // :fmt stripped the trailing whitespace
    }

    #[test]
    fn format_on_save_off_leaves_the_buffer_untouched() {
        let mut e = Editor::with_text("hello   \nworld".to_string());
        e.format_on_save = false;
        e.handle(Key::Char(':'));
        e.handle(Key::Char('w'));
        let eff = e.handle(Key::Enter);
        assert_eq!(eff, Effect::Save);
        assert_eq!(e.text(), "hello   \nworld"); // unchanged when the pref is off
    }

    #[test]
    fn wq_and_x_alias_save_dropping_the_quit() {
        assert_eq!(command("wq").1, Effect::Save);
        assert_eq!(command("x").1, Effect::Save);
    }

    #[test]
    fn fmt_stays_in_core_and_asks_the_host_for_nothing() {
        assert_eq!(command("fmt").1, Effect::None);
    }

    #[test]
    fn unknown_command_is_ignored() {
        let (e, eff) = command("q"); // quit is deliberately unimplemented
        assert_eq!(eff, Effect::None);
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn with_text_boots_normal_with_caret_on_last_char() {
        let e = Editor::with_text("resumed draft".to_string());
        assert_eq!(e.text(), "resumed draft");
        assert_eq!(e.caret, 12); // on the last char ('t'), the resume point
        assert_eq!(e.mode(), Mode::Normal); // vim-style: open a file in Normal
    }

    #[test]
    fn with_text_empty_matches_new() {
        let e = Editor::with_text(String::new());
        assert_eq!(e.text(), "");
        assert_eq!(e.caret, 0);
        assert_eq!(e.mode(), Mode::Normal);
    }

    // ---- Ctrl-d / Ctrl-u half-page scroll (v0.2) ----

    /// The core reason this isn't `HALF_PAGE × move_down`: on one long paragraph
    /// that soft-wraps, half-page-down steps *display* rows, advancing the caret
    /// half a window into the wrap — whereas `j` (logical-line) can't move
    /// within the single line at all.
    #[test]
    fn half_page_down_steps_display_rows_within_a_wrapped_line() {
        let mut e = Editor::with_text("a".repeat(WRITE_COLS * 10)); // one long wrapped line
        let cols = e.text_cols(); // wrap width shrinks by the gutter
        e.caret = 0;
        e.handle(Key::HalfPageDown);
        assert_eq!(e.caret, cols * HALF_PAGE); // down HALF_PAGE *display* rows

        // Contrast: `j` on the same single logical line is a no-op.
        let mut j = Editor::with_text("a".repeat(WRITE_COLS * 10));
        j.caret = 0;
        j.handle(Key::Char('j'));
        assert_eq!(j.caret, 0);
    }

    /// Up is the inverse of down within a wrapped line.
    #[test]
    fn half_page_up_is_the_inverse_within_a_wrapped_line() {
        let mut e = Editor::with_text("a".repeat(WRITE_COLS * 10));
        e.caret = e.text_cols() * HALF_PAGE; // start on a display-row boundary
        e.handle(Key::HalfPageUp);
        assert_eq!(e.caret, 0);
    }

    /// Clamps at both ends: up from the top stays; down past the bottom lands on
    /// the last row on a character boundary, never out of range.
    #[test]
    fn half_page_clamps_at_both_ends() {
        let mut e = Editor::with_text("a".repeat(WRITE_COLS * 3)); // 3 rows
        e.caret = 0;
        e.handle(Key::HalfPageUp);
        assert_eq!(e.caret, 0);
        e.handle(Key::HalfPageDown);
        e.handle(Key::HalfPageDown);
        assert!(e.caret <= e.text.len());
        assert!(e.text.is_char_boundary(e.caret));
    }

    /// The viewport follows the caret past the window: after enough half-pages,
    /// `scroll_top` advances (in draw) and the caret stays visible.
    #[test]
    fn half_page_down_scrolls_the_viewport() {
        let text = vec!["a"; 40].join("\n"); // 40 one-char lines = 40 display rows
        let mut e = Editor::with_text(text);
        e.caret = 0;
        for _ in 0..4 {
            e.handle(Key::HalfPageDown);
        }
        e.draw(true); // adjust_scroll runs here
        assert!(e.scroll_top() > 0, "viewport should have scrolled");
        let lay = e.layout();
        let (row, _) = e.caret_rc(&lay);
        assert!(row >= e.scroll_top() && row < e.scroll_top() + ROWS);
    }

    /// In View mode (read-only) half-page moves the viewport directly and leaves
    /// the caret alone.
    #[test]
    fn half_page_scrolls_viewport_in_view_mode() {
        let mut e = Editor::with_text(vec!["a"; 40].join("\n"));
        let caret_before = e.caret;
        e.handle(Key::Char('v')); // Normal -> View
        assert_eq!(e.mode(), Mode::View);
        e.handle(Key::HalfPageDown);
        assert_eq!(e.scroll_top(), HALF_PAGE);
        assert_eq!(e.caret, caret_before); // caret untouched in View
        e.handle(Key::HalfPageUp);
        assert_eq!(e.scroll_top(), 0);
    }

    /// Inert in Insert mode — it must not yank the caret off the text you're
    /// typing.
    #[test]
    fn half_page_is_a_noop_in_insert_mode() {
        let mut e = Editor::with_text(vec!["a"; 40].join("\n"));
        e.caret = 0;
        e.handle(Key::Char('i')); // Normal -> Insert
        e.handle(Key::HalfPageDown);
        assert_eq!(e.caret, 0);
        assert_eq!(e.mode(), Mode::Insert);
    }

    // ---- Absolute line-number gutter (v0.2) ----

    #[test]
    fn gutter_is_two_digits_plus_separator_for_small_files() {
        let e = Editor::with_text("one\ntwo\nthree".to_string()); // 3 logical lines
        assert_eq!(e.logical_lines(), 3);
        assert_eq!(e.gutter_cols(), 3); // 2 digit cols + 1 separator
        assert_eq!(e.text_cols(), WRITE_COLS - 3);
    }

    #[test]
    fn gutter_widens_past_ninety_nine_lines() {
        let e = Editor::with_text("x\n".repeat(120)); // 121 logical lines
        assert_eq!(e.gutter_cols(), 4); // 3 digit cols + 1 separator
        assert_eq!(e.text_cols(), WRITE_COLS - 4);
    }

    #[test]
    fn gutter_narrows_the_soft_wrap_width() {
        let e = Editor::with_text("a".repeat(WRITE_COLS)); // 60 chars, one logical line
        let cols = e.text_cols();
        assert!(cols < WRITE_COLS); // the gutter stole columns
        let lay = e.layout();
        assert_eq!(lay[0].text.chars().count(), cols); // first row fills the text width
        assert!(lay.len() >= 2); // 60 chars no longer fit one row
    }

    #[test]
    fn draw_with_gutter_produces_a_full_frame() {
        let mut e = Editor::with_text("line one\nline two\nline three".to_string());
        assert_eq!(e.draw(true).bytes().len(), display::FB_BYTES);
    }

    // ---- Command-line editing (Ctrl-W / Cmd-Backspace while typing `:`) ----

    #[test]
    fn ctrl_w_deletes_the_last_word_of_the_command_line() {
        let mut e = Editor::new();
        e.handle(Key::Char(':'));
        for c in "sync now".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::DeleteWord);
        assert_eq!(e.cmdline, "sync ");
        assert_eq!(e.mode(), Mode::Command); // stays on the command line
    }

    #[test]
    fn ctrl_w_on_a_one_word_command_does_not_cancel() {
        let mut e = Editor::new();
        e.handle(Key::Char(':'));
        e.handle(Key::Char('w'));
        e.handle(Key::DeleteWord);
        assert_eq!(e.cmdline, "");
        assert_eq!(e.mode(), Mode::Command); // unlike Backspace, does not exit
    }

    #[test]
    fn cmd_backspace_clears_the_command_line() {
        let mut e = Editor::new();
        e.handle(Key::Char(':'));
        for c in "fmt".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::DeleteLine);
        assert_eq!(e.cmdline, "");
        assert_eq!(e.mode(), Mode::Command);
    }

    // ---- Register + yank / paste (v0.3) ----

    #[test]
    fn yy_then_p_opens_a_copy_of_the_line_below() {
        let mut e = Editor::with_text("foo\nbar".to_string());
        e.handle(Key::Char('g'));
        e.handle(Key::Char('g')); // gg -> caret on line "foo"
        e.handle(Key::Char('y'));
        e.handle(Key::Char('y')); // yank the line, linewise
        e.handle(Key::Char('p')); // paste it after the current line
        assert_eq!(e.text(), "foo\nfoo\nbar");
    }

    #[test]
    fn yy_then_capital_p_pastes_the_line_above() {
        let mut e = Editor::with_text("foo\nbar".to_string()); // caret on line "bar"
        e.handle(Key::Char('y'));
        e.handle(Key::Char('y'));
        e.handle(Key::Char('P'));
        assert_eq!(e.text(), "foo\nbar\nbar");
    }

    #[test]
    fn dd_then_p_moves_a_line_down() {
        let mut e = Editor::with_text("one\ntwo\nthree".to_string());
        e.handle(Key::Char('g'));
        e.handle(Key::Char('g')); // caret on "one"
        e.handle(Key::Char('d'));
        e.handle(Key::Char('d')); // cut "one" into the register
        e.handle(Key::Char('p')); // paste after "two"
        assert_eq!(e.text(), "two\none\nthree");
    }

    #[test]
    fn count_dd_captures_all_lines_for_paste() {
        let mut e = Editor::with_text("a\nb\nc\nd".to_string());
        e.handle(Key::Char('g'));
        e.handle(Key::Char('g'));
        e.handle(Key::Char('3'));
        e.handle(Key::Char('d'));
        e.handle(Key::Char('d')); // 3dd — cut three lines
        assert_eq!(e.text(), "d");
        e.handle(Key::Char('p')); // paste all three back below "d"
        assert_eq!(e.text(), "d\na\nb\nc");
    }

    #[test]
    fn x_then_p_replays_the_deleted_char_after_the_caret() {
        let mut e = Editor::with_text("abc".to_string());
        e.handle(Key::Char('0')); // caret on 'a'
        e.handle(Key::Char('x')); // delete 'a' -> "bc", register = "a" (charwise)
        e.handle(Key::Char('p')); // paste after 'b'
        assert_eq!(e.text(), "bac");
    }

    #[test]
    fn yw_yanks_charwise_and_p_inserts_after_the_caret() {
        let mut e = Editor::with_text("foo bar".to_string());
        e.handle(Key::Char('0'));
        e.handle(Key::Char('y'));
        e.handle(Key::Char('w')); // yank "foo " (word + trailing space), caret stays put
        e.handle(Key::Char('p'));
        assert_eq!(e.text(), "ffoo oo bar"); // charwise paste after the cursor char
    }

    #[test]
    fn capital_p_pastes_a_char_before_the_caret() {
        let mut e = Editor::with_text("abc".to_string());
        e.handle(Key::Char('0'));
        e.handle(Key::Char('x')); // register = "a", text "bc", caret on 'b'
        e.handle(Key::Char('l')); // caret on 'c'
        e.handle(Key::Char('P')); // paste "a" before 'c'
        assert_eq!(e.text(), "bac");
    }

    #[test]
    fn paste_with_an_empty_register_is_a_noop() {
        let mut e = Editor::with_text("abc".to_string());
        e.handle(Key::Char('p'));
        e.handle(Key::Char('P'));
        assert_eq!(e.text(), "abc");
    }

    // ---- Undo / redo (v0.3) ----

    #[test]
    fn undo_reverts_a_whole_insert_session_at_once() {
        let mut e = Editor::new();
        e.handle(Key::Char('i'));
        for c in "hello".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Escape);
        assert_eq!(e.text(), "hello");
        e.handle(Key::Char('u'));
        assert_eq!(e.text(), ""); // the entire typed run, not one char
        assert_eq!(e.mode(), Mode::Normal); // undo always lands in Normal
    }

    #[test]
    fn redo_reapplies_an_undone_change() {
        let mut e = Editor::new();
        e.handle(Key::Char('i'));
        e.handle(Key::Char('x'));
        e.handle(Key::Escape); // "x"
        e.handle(Key::Char('u'));
        assert_eq!(e.text(), "");
        e.handle(Key::Redo); // Ctrl-r
        assert_eq!(e.text(), "x");
    }

    #[test]
    fn undo_reverts_dd() {
        let mut e = Editor::with_text("one\ntwo".to_string());
        e.handle(Key::Char('g'));
        e.handle(Key::Char('g'));
        e.handle(Key::Char('d'));
        e.handle(Key::Char('d'));
        assert_eq!(e.text(), "two");
        e.handle(Key::Char('u'));
        assert_eq!(e.text(), "one\ntwo");
    }

    #[test]
    fn undo_reverts_x_and_restores_the_caret() {
        let mut e = Editor::with_text("abc".to_string()); // caret on 'c'
        e.handle(Key::Char('x'));
        assert_eq!(e.text(), "ab");
        e.handle(Key::Char('u'));
        assert_eq!(e.text(), "abc");
        assert_eq!(e.caret, 2); // caret came back to where the change began
    }

    #[test]
    fn undo_reverts_a_paste() {
        let mut e = Editor::with_text("foo\nbar".to_string());
        e.handle(Key::Char('g'));
        e.handle(Key::Char('g'));
        e.handle(Key::Char('y'));
        e.handle(Key::Char('y'));
        e.handle(Key::Char('p'));
        assert_eq!(e.text(), "foo\nfoo\nbar");
        e.handle(Key::Char('u'));
        assert_eq!(e.text(), "foo\nbar");
    }

    #[test]
    fn a_fresh_edit_after_undo_clears_the_redo_history() {
        let mut e = Editor::new();
        e.handle(Key::Char('i'));
        e.handle(Key::Char('a'));
        e.handle(Key::Escape); // "a"
        e.handle(Key::Char('u')); // -> ""
        e.handle(Key::Char('i'));
        e.handle(Key::Char('b'));
        e.handle(Key::Escape); // new branch: "b"
        e.handle(Key::Redo); // nothing to redo — the "a" branch is gone
        assert_eq!(e.text(), "b");
    }

    #[test]
    fn successive_undos_walk_the_history_back() {
        let mut e = Editor::new();
        e.handle(Key::Char('i'));
        e.handle(Key::Char('a'));
        e.handle(Key::Escape); // "a"
        e.handle(Key::Char('A'));
        e.handle(Key::Char('b'));
        e.handle(Key::Escape); // "ab"
        e.handle(Key::Char('u'));
        assert_eq!(e.text(), "a");
        e.handle(Key::Char('u'));
        assert_eq!(e.text(), "");
    }

    #[test]
    fn undo_with_empty_history_is_a_noop() {
        let mut e = Editor::with_text("x".to_string());
        e.handle(Key::Char('u'));
        assert_eq!(e.text(), "x");
        e.handle(Key::Redo);
        assert_eq!(e.text(), "x");
    }

    // ---- `.` repeat (v0.3) ----

    #[test]
    fn dot_repeats_x() {
        let mut e = Editor::with_text("abcde".to_string());
        e.handle(Key::Char('0'));
        e.handle(Key::Char('x')); // "bcde"
        e.handle(Key::Char('.')); // "cde"
        e.handle(Key::Char('.')); // "de"
        assert_eq!(e.text(), "de");
    }

    #[test]
    fn dot_repeats_dd() {
        let mut e = Editor::with_text("a\nb\nc\nd".to_string());
        e.handle(Key::Char('g'));
        e.handle(Key::Char('g'));
        e.handle(Key::Char('d'));
        e.handle(Key::Char('d')); // delete "a"
        e.handle(Key::Char('.')); // delete "b"
        assert_eq!(e.text(), "c\nd");
    }

    #[test]
    fn dot_repeats_dw() {
        let mut e = Editor::with_text("foo bar baz".to_string());
        e.handle(Key::Char('0'));
        e.handle(Key::Char('d'));
        e.handle(Key::Char('w')); // "bar baz"
        e.handle(Key::Char('.')); // "baz"
        assert_eq!(e.text(), "baz");
    }

    #[test]
    fn dot_repeats_a_change_operator_with_its_inserted_text() {
        // The reason `.` records keystrokes: it must replay `ciw` *and* the text
        // typed in the insert session that followed.
        let mut e = Editor::with_text("foo bar".to_string());
        e.handle(Key::Char('0'));
        e.handle(Key::Char('c'));
        e.handle(Key::Char('i'));
        e.handle(Key::Char('w'));
        e.handle(Key::Char('X'));
        e.handle(Key::Escape); // "X bar"
        assert_eq!(e.text(), "X bar");
        e.handle(Key::Char('w')); // caret onto "bar"
        e.handle(Key::Char('.')); // repeat: change that word to "X" too
        assert_eq!(e.text(), "X X");
    }

    #[test]
    fn dot_repeats_a_paste() {
        let mut e = Editor::with_text("x\na\nb".to_string());
        e.handle(Key::Char('g'));
        e.handle(Key::Char('g'));
        e.handle(Key::Char('y'));
        e.handle(Key::Char('y')); // yank line "x"
        e.handle(Key::Char('p')); // "x\nx\na\nb"
        e.handle(Key::Char('.')); // paste again below
        assert_eq!(e.text(), "x\nx\nx\na\nb");
    }

    #[test]
    fn dot_ignores_pure_motions() {
        let mut e = Editor::with_text("abc".to_string());
        e.handle(Key::Char('0'));
        e.handle(Key::Char('l')); // motions only — nothing to repeat
        e.handle(Key::Char('.'));
        assert_eq!(e.text(), "abc");
    }

    #[test]
    fn a_yank_does_not_become_the_dot_change() {
        // `y` is not a `.`-repeatable change; the prior `x` must remain the dot.
        let mut e = Editor::with_text("abcdef".to_string());
        e.handle(Key::Char('0'));
        e.handle(Key::Char('x')); // dot = x; "bcdef"
        e.handle(Key::Char('y'));
        e.handle(Key::Char('w')); // yank — must not overwrite the dot
        e.handle(Key::Char('.')); // repeat the x, not the yank
        assert_eq!(e.text(), "cdef");
    }

    #[test]
    fn dot_in_insert_mode_is_a_literal_character() {
        let mut e = Editor::new();
        e.handle(Key::Char('i'));
        e.handle(Key::Char('.'));
        assert_eq!(e.text(), "."); // '.' only repeats from Normal
    }

    #[test]
    fn text_getter_reflects_edits() {
        let e = typed("hello");
        assert_eq!(e.text(), "hello");
    }

    #[test]
    fn a_notice_shows_until_the_next_key_dismisses_it() {
        let mut e = Editor::new();
        e.set_notice("saved");
        assert_eq!(e.notice.as_deref(), Some("saved"));
        e.handle(Key::Char('j')); // any key dismisses the snackbar
        assert_eq!(e.notice, None);
    }
}
