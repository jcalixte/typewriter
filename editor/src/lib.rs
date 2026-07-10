//! Modal text editor core: a vim-style buffer with Normal / Insert (edit) /
//! View (read-only) modes, rendered onto the e-paper [`Frame`].
//!
//! The buffer is plain ASCII — the US-QWERTY decoder only ever produces ASCII
//! and Tab expands to spaces on insert — so a byte offset into the `String` is
//! also a character index, and `caret` is that offset. Motions and edits work
//! on the logical (`\n`-delimited) buffer; word-wrapping and scrolling are a
//! render-time concern handled by [`Editor::draw`].

// ISO-8859-15 (Latin-9) rather than the ascii subset: same glyph cells, but it
// carries the accented Latin glyphs (à é ê ç … plus œ €) that international
// input will emit. ASCII rendering is byte-for-byte unchanged.
use embedded_graphics::mono_font::iso_8859_15::{FONT_6X10, FONT_10X20};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Baseline, Text};

use display::{Frame, HEIGHT};
use keymap::Key;

/// FONT_10X20 cell size (writing column) and the grid it tiles into.
pub const CW: i32 = 10;
pub const CH: i32 = 20;
/// Writing-column width, in characters. The panel is split into a left
/// **writing column** (this wide) and a right **side panel** for metadata (see
/// CONTEXT.md § Screen regions). 60 cols × 10 px = 600 px of text; the driver's
/// `x = 396` seam runs through it invisibly. The remaining 192 px sit entirely
/// in the master half (right of the seam) and hold the side panel.
const WRITE_COLS: usize = 60;
/// Visible writing rows. 13 × 20 px = 260 px; the bottom 12 px is the transient
/// `:` command line (the only thing left of the old status band).
const ROWS: usize = (HEIGHT / 20) as usize; // 13
/// x of the 1 px rule dividing writing column from side panel, and the left edge
/// of panel text (a small gutter past the rule).
const DIVIDER_X: i32 = WRITE_COLS as i32 * CW; // 600
const PANEL_X: i32 = DIVIDER_X + 8; // 608
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
    /// Enter runs it, Esc cancels. Currently just `:fmt`.
    Command,
}

/// A pending operator awaiting a motion or text object (`d`elete / `c`hange).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Delete,
    Change,
}

/// The editor state: buffer, caret, mode, viewport, and pending command state.
pub struct Editor {
    text: String,
    /// Byte offset of the caret (== char index; the buffer is ASCII). Ranges
    /// over `0..=text.len()`.
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
}

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
            mode: Mode::Insert, // writing appliance: power-on = ready to type
            scroll_top: 0,
            count: 0,
            pending_op: None,
            pending_obj: None,
            pending_g: false,
            cmdline: String::new(),
            shown_words: 0,
            keyboard_present: false,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
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

    /// Whitespace-delimited word count of the whole buffer.
    fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }

    /// Dispatch one decoded key event according to the current mode.
    pub fn handle(&mut self, key: Key) {
        match self.mode {
            Mode::Insert => self.insert_key(key),
            Mode::Normal => self.normal_key(key),
            Mode::View => self.view_key(key),
            Mode::Command => self.command_key(key),
        }
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
            Key::Escape => {
                self.mode = Mode::Normal;
                // vim drops the caret onto the last inserted char.
                if self.caret > self.line_start(self.caret) {
                    self.caret -= 1;
                }
            }
        }
    }

    // --- Normal mode -------------------------------------------------------

    fn normal_key(&mut self, key: Key) {
        let c = match key {
            Key::Char(c) => c,
            // Esc and non-character events cancel any pending command.
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
                'd' if op == Op::Delete => (0..n).for_each(|_| self.delete_current_line()),
                'c' if op == Op::Change => self.change_current_line(),
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
                    self.apply_op(op, self.caret, t + 1);
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
            'x' => (0..n).for_each(|_| self.delete_at_caret()),
            'd' => {
                self.pending_op = Some(Op::Delete);
                return;
            }
            'c' => {
                self.pending_op = Some(Op::Change);
                return;
            }
            'i' => self.mode = Mode::Insert,
            'a' => {
                self.move_right_append();
                self.mode = Mode::Insert;
            }
            'A' => {
                self.caret = self.line_end(self.caret);
                self.mode = Mode::Insert;
            }
            'I' => {
                self.caret = self.line_start(self.caret);
                self.mode = Mode::Insert;
            }
            'o' => {
                self.caret = self.line_end(self.caret);
                self.insert_char('\n');
                self.mode = Mode::Insert;
            }
            'O' => {
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

    fn command_key(&mut self, key: Key) {
        match key {
            Key::Char(c) => self.cmdline.push(c),
            Key::Backspace => {
                // Backspace on the empty command line cancels back to Normal.
                if self.cmdline.pop().is_none() {
                    self.mode = Mode::Normal;
                }
            }
            Key::Enter => {
                self.execute_command();
                self.cmdline.clear();
                self.mode = Mode::Normal;
            }
            Key::Escape => {
                self.cmdline.clear();
                self.mode = Mode::Normal;
            }
            // Word/line deletes and Tab aren't meaningful on a short command line.
            _ => {}
        }
    }

    /// Run the typed `:` command. Unknown commands are silently ignored.
    fn execute_command(&mut self) {
        match self.cmdline.trim() {
            "fmt" => self.format_buffer(),
            _ => {}
        }
    }

    /// `:fmt` — normalize the buffer (align tables, collapse duplicate blank
    /// lines, strip trailing whitespace) and keep the caret on roughly the same
    /// line (buffer length changes, so exact restoration isn't possible).
    fn format_buffer(&mut self) {
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

    // --- View mode ---------------------------------------------------------

    fn view_key(&mut self, key: Key) {
        match key {
            Key::Char('j') => self.scroll_top += 1, // clamped in draw()
            Key::Char('k') => self.scroll_top = self.scroll_top.saturating_sub(1),
            Key::Char(' ') => self.scroll_top += ROWS,
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

    fn move_left(&mut self) {
        if self.caret > self.line_start(self.caret) {
            self.caret -= 1;
        }
    }

    fn move_right(&mut self) {
        if self.caret < self.line_end(self.caret) {
            self.caret += 1;
        }
    }

    /// Like `l` but allowed to land one past the last char (for `a`).
    fn move_right_append(&mut self) {
        if self.caret < self.line_end(self.caret) {
            self.caret += 1;
        }
    }

    fn move_down(&mut self) {
        let col = self.caret - self.line_start(self.caret);
        let le = self.line_end(self.caret);
        if le >= self.text.len() {
            return; // already on the last line
        }
        let next_start = le + 1;
        let next_end = self.line_end(next_start);
        self.caret = (next_start + col).min(next_end);
    }

    fn move_up(&mut self) {
        let ls = self.line_start(self.caret);
        if ls == 0 {
            return; // already on the first line
        }
        let col = self.caret - ls;
        let prev_start = self.line_start(ls - 1);
        let prev_end = ls - 1; // the '\n' that ends the previous line
        self.caret = (prev_start + col).min(prev_end);
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

    /// End of the current/next word (lands on its last char).
    fn word_end_pos(&self, from: usize) -> usize {
        let b = self.text.as_bytes();
        let n = b.len();
        let mut i = from + 1;
        if i >= n {
            return from;
        }
        while i < n && b[i].is_ascii_whitespace() {
            i += 1;
        }
        while i < n && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        i.saturating_sub(1)
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
            self.caret -= 1;
            self.text.remove(self.caret);
        }
    }

    /// `x` — delete the char under the caret (never a newline).
    fn delete_at_caret(&mut self) {
        let b = self.text.as_bytes();
        if self.caret < b.len() && b[self.caret] != b'\n' {
            self.text.remove(self.caret);
            // Keep the caret on a char: if it fell off the line end, step back.
            if self.caret >= self.line_end(self.caret) && self.caret > self.line_start(self.caret) {
                self.caret -= 1;
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
        let ls = self.line_start(self.caret);
        let le = self.line_end(self.caret);
        self.text.replace_range(ls..le, "");
        self.caret = ls;
        self.mode = Mode::Insert;
    }

    /// Apply a pending operator over the buffer range `[start, end)` (order
    /// independent). Delete removes it; Change removes it and enters insert.
    fn apply_op(&mut self, op: Op, start: usize, end: usize) {
        let s = start.min(end);
        let e = start.max(end).min(self.text.len());
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

    /// Wrap the buffer into display lines, tracking each line's buffer offset.
    /// Soft-wrap at word boundaries: a logical line too long for `WRITE_COLS`
    /// breaks at the last space that fits, so words are never split — except a
    /// single word wider than the writing column, hard-broken at `WRITE_COLS`. Buffer is
    /// ASCII (1 byte = 1 char), so a char index within a line is also a byte
    /// offset (matches the rest of `editor.rs`; UTF-8 correctness is v0.2 work).
    fn layout(&self) -> Vec<Line> {
        let mut lines: Vec<Line> = Vec::new();
        let mut base = 0usize; // buffer offset of the current logical line's start
        for logical in self.text.split('\n') {
            let chars: Vec<char> = logical.chars().collect();
            if chars.is_empty() {
                lines.push(Line { start: base, text: String::new() });
            } else {
                let mut c = 0usize; // char index within `logical`
                while c < chars.len() {
                    let remaining = chars.len() - c;
                    let take = if remaining <= WRITE_COLS {
                        remaining
                    } else {
                        // Break at the last space within the COLS-wide window;
                        // include that space on this line. No space → hard break.
                        let window = c + WRITE_COLS;
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
                            _ => WRITE_COLS,
                        }
                    };
                    lines.push(Line {
                        start: base + c,
                        text: chars[c..c + take].iter().collect(),
                    });
                    c += take;
                }
            }
            base += chars.len() + 1; // + the '\n' that `split` consumed
        }
        lines
    }

    /// Display (row, col) of the caret within `lay`.
    fn caret_rc(&self, lay: &[Line]) -> (usize, usize) {
        let mut row = 0;
        for (i, l) in lay.iter().enumerate() {
            if l.start <= self.caret {
                row = i;
            } else {
                break;
            }
        }
        (row, self.caret - lay[row].start)
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

    /// Render the current state into a frame. `insert_cursor_on` gates the
    /// Insert-mode bar caret (suppressed while typing, shown after a pause);
    /// Normal draws a block caret and View draws none, regardless.
    pub fn draw(&mut self, insert_cursor_on: bool) -> Frame {
        let lay = self.layout();
        let (crow, ccol) = self.caret_rc(&lay);
        self.adjust_scroll(crow, lay.len());

        let mut f = Frame::new_white();
        let text_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
        let end = (self.scroll_top + ROWS).min(lay.len());
        for (vis, li) in (self.scroll_top..end).enumerate() {
            let y = vis as i32 * CH;
            Text::with_baseline(&lay[li].text, Point::new(0, y), text_style, Baseline::Top)
                .draw(&mut f)
                .unwrap();
            // Markdown heading (`#`..`######` + space): faux-bold by double-
            // striking the whole display line 1px to the right (no bold Latin-9
            // font exists). Checks the logical line so wrapped headings stay bold.
            if self.is_heading_at(self.line_start(lay[li].start)) {
                Text::with_baseline(&lay[li].text, Point::new(1, y), text_style, Baseline::Top)
                    .draw(&mut f)
                    .unwrap();
            }
        }

        if crow >= self.scroll_top && crow < self.scroll_top + ROWS {
            let x = ccol.min(WRITE_COLS - 1) as i32 * CW;
            let y = (crow - self.scroll_top) as i32 * CH;
            match self.mode {
                Mode::Normal => {
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
                Mode::Insert if insert_cursor_on => {
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

    /// Type a run of characters in Insert mode (the power-on mode).
    fn typed(s: &str) -> Editor {
        let mut e = Editor::new();
        for c in s.chars() {
            e.handle(Key::Char(c));
        }
        e
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
}
