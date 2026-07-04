//! Modal text editor core: a vim-style buffer with Normal / Insert (edit) /
//! View (read-only) modes, rendered onto the e-paper [`Frame`].
//!
//! The buffer is plain ASCII — the US-QWERTY decoder only ever produces ASCII
//! and Tab expands to spaces on insert — so a byte offset into the `String` is
//! also a character index, and `caret` is that offset. Motions and edits work
//! on the logical (`\n`-delimited) buffer; word-wrapping and scrolling are a
//! render-time concern handled by [`Editor::draw`].

use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_10X20};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Baseline, Text};

use crate::epd::{self, Frame};
use crate::usb_kbd::Key;

/// FONT_10X20 cell size and the grid it tiles the panel into.
pub const CW: i32 = 10;
pub const CH: i32 = 20;
const COLS: usize = (epd::WIDTH / 10) as usize; // 79 characters per line
const ROWS: usize = (epd::HEIGHT / 20) as usize; // 13 text rows; bottom 12 px = status
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
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn scroll_top(&self) -> usize {
        self.scroll_top
    }

    /// Dispatch one decoded key event according to the current mode.
    pub fn handle(&mut self, key: Key) {
        match self.mode {
            Mode::Insert => self.insert_key(key),
            Mode::Normal => self.normal_key(key),
            Mode::View => self.view_key(key),
        }
    }

    // --- Insert mode -------------------------------------------------------

    fn insert_key(&mut self, key: Key) {
        match key {
            Key::Char('\t') => self.insert_str(TAB),
            Key::Char(c) => self.insert_char(c),
            Key::Enter => self.insert_char('\n'),
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
            _ => {}
        }
        self.count = 0;
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
    fn layout(&self) -> Vec<Line> {
        let mut lines = vec![Line {
            start: 0,
            text: String::new(),
        }];
        let mut idx = 0usize;
        for ch in self.text.chars() {
            if ch == '\n' {
                idx += 1;
                lines.push(Line {
                    start: idx,
                    text: String::new(),
                });
                continue;
            }
            if lines.last().unwrap().text.chars().count() >= COLS {
                lines.push(Line {
                    start: idx,
                    text: String::new(),
                });
            }
            lines.last_mut().unwrap().text.push(ch);
            idx += 1;
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
            Text::with_baseline(
                &lay[li].text,
                Point::new(0, vis as i32 * CH),
                text_style,
                Baseline::Top,
            )
            .draw(&mut f)
            .unwrap();
        }

        if crow >= self.scroll_top && crow < self.scroll_top + ROWS {
            let x = ccol.min(COLS - 1) as i32 * CW;
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

        self.draw_status(&mut f);
        f
    }

    /// Draw the mode indicator (and any pending count/operator) in the bottom
    /// strip, in the small 6×10 font so it fits below the 13 text rows.
    fn draw_status(&self, f: &mut Frame) {
        let name = match self.mode {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::View => "VIEW",
        };
        let mut s = format!(" -- {name} --");
        if self.count > 0 {
            s.push_str(&format!("  {}", self.count));
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
        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        Text::with_baseline(&s, Point::new(2, ROWS as i32 * CH + 1), style, Baseline::Top)
            .draw(f)
            .unwrap();
    }
}
