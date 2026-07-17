//! Rendering: the e-paper grid geometry, display-line layout, and all
//! `draw_*` painting onto the [`Frame`].

use super::*;

/// FONT_10X20 cell size (writing column) and the grid it tiles into.
pub const CW: i32 = 10;
pub const CH: i32 = 20;
/// Writing-region width, in characters: the left region holding the
/// line-number gutter **and** the text column (the gutter steals from it — see
/// [`Editor::text_cols`]). The right **side panel** holds metadata (see
/// CONTEXT.md § Screen regions). 63 cols × 10 px = 630 px; the driver's
/// `x = 396` seam runs through it invisibly. The remaining 162 px (right of the
/// divider) hold the ~17-col side panel (at its FONT_9X15 metadata font — see
/// [`PANEL_COLS`]). Widened from 60 so the gutter doesn't narrow the text: a
/// ≤ 99-line file keeps a full 60-col text column.
pub(crate) const WRITE_COLS: usize = 63;
/// Minimum digit columns in the line-number gutter (before the 1-col separator).
/// Files up to 99 lines still get a 2-wide gutter so short notes don't jitter.
pub(crate) const GUTTER_MIN_DIGITS: usize = 2;
/// Visible writing rows. 13 × 20 px = 260 px. The transient `:` command line is
/// drawn at body size over the **bottom** writing row (see [`Editor::draw_cmdline`]),
/// so no rows are permanently reserved for it.
pub(crate) const ROWS: usize = (HEIGHT / 20) as usize; // 13
/// Half-page scroll distance for `Ctrl-d`/`Ctrl-u`, in **display rows** — vim's
/// `'scroll'` default (half the visible window). Fixed, not configurable: a
/// resizable `'scroll'` is meaningless on a fixed 13-row panel.
pub(crate) const HALF_PAGE: usize = ROWS / 2; // 6
/// x of the 1 px rule dividing writing column from side panel, and the left edge
/// of panel text (a small gutter past the rule).
pub(crate) const DIVIDER_X: i32 = WRITE_COLS as i32 * CW; // 630
pub(crate) const PANEL_X: i32 = DIVIDER_X + 8; // 638
/// Side-panel font cell: **FONT_9X15** — a middle size between the old squint-y
/// 6×10 and the body 10×20. Legible metadata without eating as many columns as
/// the body font would (the `:` command line, being text you type, stays at the
/// body 10×20 — see [`Editor::draw_cmdline`]). Kept as its own pair (not reusing
/// `CW`/`CH`) so the panel font tunes independently of the writing font; change
/// these **and** the `MonoTextStyle` font in `draw_panel` together.
pub(crate) const PANEL_CW: i32 = 9;
pub(crate) const PANEL_CH: i32 = 15;
/// Side-panel text width in [`PANEL_CW`]-px columns, for clamping panel strings —
/// the snackbar notice, word count — so they never draw past the right edge of
/// the panel.
pub(crate) const PANEL_COLS: usize = (WIDTH as usize - PANEL_X as usize) / PANEL_CW as usize; // 15
/// Max wrapped lines the snackbar draws under the word count, so a long notice
/// can't run down into the bottom mode strip. Four PANEL_CH rows ≈ 60 chars,
/// enough for any current message.
pub(crate) const NOTICE_MAX_LINES: usize = 4;
/// Tab stop, in spaces. Tabs never enter the buffer — they expand on insert so
/// the buffer stays 1 char = 1 column.
pub(crate) const TAB: &str = "    ";


/// Word-wrap `text` to lines of at most `width` characters, for the side-panel
/// snackbar. Packs whole words greedily; a word longer than `width` is hard-split
/// across lines (so a long path or oid still shows in full rather than being
/// truncated). Empty input yields no lines.
pub(crate) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if word.chars().count() > width {
            if !cur.is_empty() {
                lines.push(core::mem::take(&mut cur));
            }
            let mut chars = word.chars().peekable();
            while chars.peek().is_some() {
                lines.push(chars.by_ref().take(width).collect());
            }
            continue;
        }
        let sep = usize::from(!cur.is_empty());
        if cur.chars().count() + sep + word.chars().count() > width {
            lines.push(core::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}


/// One wrapped display line: its text and the buffer offset of its first char.
pub(crate) struct Line {
    pub(crate) start: usize,
    pub(crate) text: String,
}


impl Editor {
    /// Number of logical lines in the buffer (1 + newline count). Used to size
    /// the line-number gutter.
    pub(crate) fn logical_lines(&self) -> usize {
        self.text.bytes().filter(|&b| b == b'\n').count() + 1
    }

    /// Width of the absolute line-number gutter, in display columns: enough
    /// digits for the buffer's largest line number (min [`GUTTER_MIN_DIGITS`])
    /// plus a 1-column separator before the text. Sized from the *total* line
    /// count, not the visible range, so it stays fixed while scrolling — only
    /// crossing a power of ten (100, 1000, …) reflows the wrap, which is rare.
    pub(crate) fn gutter_cols(&self) -> usize {
        if !self.prefs.line_numbers {
            return 0; // gutter off: text reclaims the full writing width
        }
        let digits = self.logical_lines().to_string().len().max(GUTTER_MIN_DIGITS);
        digits + 1
    }

    /// Character columns left for text once the gutter is reserved. The writing
    /// region is fixed at [`WRITE_COLS`]; the gutter steals from it, so text
    /// soft-wraps narrower.
    pub(crate) fn text_cols(&self) -> usize {
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
    pub(crate) fn layout(&self) -> Vec<Line> {
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
    pub(crate) fn caret_rc(&self, lay: &[Line]) -> (usize, usize) {
        // `lay` is sorted by `start`: the caret's row is the last line at or
        // before it.
        let row = lay.iter().rposition(|l| l.start <= self.caret).unwrap_or(0);
        let col = self.text[lay[row].start..self.caret].chars().count();
        (row, col)
    }

    /// Scroll so the display row holding byte offset `pos` is visible,
    /// bottom-aligning it when it sits below the viewport (never scrolls up).
    /// Called after an insert that can run past the fold — chiefly a multi-line
    /// paste — so the *whole* pasted block is revealed, not just the caret's
    /// first line. The caret is left where the edit put it; `draw`'s
    /// `adjust_scroll` won't override this as long as the caret stays within the
    /// resulting window (true for any block up to a screen tall).
    pub(crate) fn reveal(&mut self, pos: usize) {
        let lay = self.layout();
        if lay.is_empty() {
            return;
        }
        let pos = pos.min(self.text.len());
        let row = lay.iter().rposition(|l| l.start <= pos).unwrap_or(0);
        if row >= self.scroll_top + ROWS {
            self.scroll_top = row + 1 - ROWS;
        }
    }

    /// Move the viewport so the caret stays visible (Normal/Insert/Visual), or
    /// just clamp it to the content (View).
    ///
    /// Keeps [`Prefs::scroll_margin`] rows of context above and below the caret
    /// (vim's `scrolloff`). The margin is capped at `(ROWS - 1) / 2` so it can
    /// never squeeze the caret off a short panel, and it **collapses at both
    /// ends of the buffer**: `saturating_sub` lets the caret sit on the first
    /// rows, and the final clamp to `total - ROWS` keeps the last line pinned to
    /// the bottom rather than showing blank rows past it. `scroll_margin == 0`
    /// reproduces the old edge-triggered behaviour exactly.
    pub(crate) fn adjust_scroll(&mut self, caret_row: usize, total: usize) {
        match self.mode {
            Mode::View => {
                let max = total.saturating_sub(ROWS);
                if self.scroll_top > max {
                    self.scroll_top = max;
                }
            }
            _ => {
                let margin = self.prefs.scroll_margin.min((ROWS - 1) / 2);
                if caret_row < self.scroll_top + margin {
                    self.scroll_top = caret_row.saturating_sub(margin);
                } else if caret_row + margin >= self.scroll_top + ROWS {
                    self.scroll_top = (caret_row + margin + 1).saturating_sub(ROWS);
                }
                // Never scroll past the buffer's end: collapses the bottom margin
                // near EOF so the last line sits on the bottom row, not above it.
                let max = total.saturating_sub(ROWS);
                if self.scroll_top > max {
                    self.scroll_top = max;
                }
            }
        }
    }

    /// Is the logical line starting at `ls` a Markdown ATX heading — 1–6 `#`
    /// followed by a space? (Used to render heading lines bold.)
    pub(crate) fn is_heading_at(&self, ls: usize) -> bool {
        let b = self.text.as_bytes();
        let mut i = ls;
        while i < b.len() && b[i] == b'#' {
            i += 1;
        }
        let hashes = i - ls;
        (1..=6).contains(&hashes) && b.get(i) == Some(&b' ')
    }

    /// Paint the non-Latin-9 glyphs (`→ ≠ Σ … – — ' ' " " •`) that the base font
    /// can't draw, overlaying them onto the cells `embedded-graphics` just filled
    /// with fallback boxes. `line` is a laid-out display line; `x0` is its
    /// x-origin (`gx`, or `gx + 1` for the heading double-strike). Only the
    /// half-open display-column range `[col_start, col_end)` is painted, so the
    /// visual-selection pass can invert just the selected span. `ink = On` draws
    /// black-on-white; `ink = Off` draws white-on-black for reverse-video cells.
    /// Because every extra glyph is one cell wide (like the font), cell x is
    /// `x0 + col * CW` — the same grid the layout and caret math already use.
    pub(crate) fn overlay_extras(
        f: &mut Frame,
        line: &str,
        x0: i32,
        y: i32,
        col_start: usize,
        col_end: usize,
        ink: BinaryColor,
    ) {
        for (col, ch) in line.chars().enumerate() {
            if col < col_start || col >= col_end {
                continue;
            }
            if let Some(g) = extra_glyph(ch) {
                blit_glyph(f, x0 + col as i32 * CW, y, g, ink);
            }
        }
    }

    /// Render the current state into a frame. `cursor_on` gates the caret: the
    /// Insert bar caret is suppressed while typing and shown after a pause, and
    /// `false` also suppresses the Normal block caret so callers can render pure
    /// text (e.g. a boot message). View never draws a caret. In the main loop
    /// Normal always passes `true`, so its block caret is unaffected.
    pub fn draw(&mut self, cursor_on: bool) -> Frame {
        let mut f = Frame::empty();
        self.draw_into(&mut f, cursor_on);
        f
    }

    /// [`draw`](Self::draw) into a caller-owned frame, reusing its allocation.
    /// Firmware keeps two boot-time frames and round-trips them through here so
    /// a repaint never allocates: the editor must stay drawable even when a
    /// background `:gp` push has taken the heap to the floor — a failed
    /// framebuffer alloc aborts the whole app (the 2026-07-13 OOM).
    pub fn draw_into(&mut self, out: &mut Frame, cursor_on: bool) {
        // The focus-mode break masks the whole screen: draw the rest card
        // instead of the editor, then apply the theme flip so a dark theme turns
        // the white card into the black one. Nothing else (layout, scroll,
        // panel) runs — the buffer stays hidden behind the curtain.
        if self.mode == Mode::Rest {
            let mut f = std::mem::replace(out, Frame::empty());
            self.draw_rest_card(&mut f);
            if self.prefs.theme == "dark" {
                f.invert();
            }
            *out = f;
            return;
        }
        let lay = self.layout();
        let (crow, ccol) = self.caret_rc(&lay);
        self.adjust_scroll(crow, lay.len());

        // Take the caller's buffer (no copy, no alloc), render into it as an
        // owned frame — the body predates this signature — and hand it back.
        let mut f = std::mem::replace(out, Frame::empty());
        f.clear_white();
        let text_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
        let gutter = self.gutter_cols();
        let cols = WRITE_COLS - gutter; // text columns after the gutter
        let gx = gutter as i32 * CW; // text (and cursor) x-origin, past the gutter
        // Number field width (the last gutter col is the separator). Saturating so
        // a disabled gutter (`gutter == 0`, line_numbers off) can't underflow; the
        // number draw below is skipped in that case anyway.
        let digits = gutter.saturating_sub(1);
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
            if gutter > 0 && first_row {
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
            let heading = self.is_heading_at(self.line_start(lay[li].start));
            if heading {
                Text::with_baseline(&lay[li].text, Point::new(gx + 1, y), text_style, Baseline::Top)
                    .draw(&mut f)
                    .unwrap();
            }
            // Repaint any non-Latin-9 glyphs over the fallback boxes the font
            // left. Double-struck at gx+1 too on headings, to stay faux-bold.
            Self::overlay_extras(&mut f, &lay[li].text, gx, y, 0, usize::MAX, BinaryColor::On);
            if heading {
                Self::overlay_extras(&mut f, &lay[li].text, gx + 1, y, 0, usize::MAX, BinaryColor::On);
            }
        }

        // Visual selection: reverse-video the selected cells (black fill, glyphs
        // redrawn white). A second pass so the text loop above stays untouched;
        // on a 1-bit panel this inversion is the only selection affordance.
        if self.in_visual() {
            let (ss, se, lw) = self.visual_span();
            let inv = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
            for (vis, li) in (self.scroll_top..end).enumerate() {
                let y = vis as i32 * CH;
                let rs = lay[li].start;
                let re = rs + lay[li].text.len();
                let (col_a, col_b) = if rs.max(ss) < re.min(se) {
                    let a = rs.max(ss);
                    let b = re.min(se);
                    (self.text[rs..a].chars().count(), self.text[rs..b].chars().count())
                } else if lw && lay[li].text.is_empty() && rs >= ss && rs <= se {
                    // A blank line inside a linewise selection: a 1-cell mark so
                    // the empty row still reads as selected.
                    (0, 1)
                } else {
                    continue;
                };
                let x = gx + col_a as i32 * CW;
                let w = (col_b - col_a).max(1) as u32 * CW as u32;
                Rectangle::new(Point::new(x, y), Size::new(w, CH as u32))
                    .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                    .draw(&mut f)
                    .unwrap();
                let seg: String = lay[li].text.chars().skip(col_a).take(col_b - col_a).collect();
                if !seg.is_empty() {
                    Text::with_baseline(&seg, Point::new(x, y), inv, Baseline::Top)
                        .draw(&mut f)
                        .unwrap();
                }
                // Any extra glyphs in the selected span: repaint white-on-black.
                Self::overlay_extras(&mut f, &lay[li].text, gx, y, col_a, col_b, BinaryColor::Off);
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
                        if let Some(g) = extra_glyph(ch) {
                            blit_glyph(&mut f, x, y, g, BinaryColor::Off);
                        } else {
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
                }
                Mode::Insert if cursor_on => {
                    // Bar caret at the left edge of the cell.
                    Rectangle::new(Point::new(x, y), Size::new(2, CH as u32))
                        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                        .draw(&mut f)
                        .unwrap();
                }
                Mode::Visual | Mode::VisualLine if cursor_on => {
                    // The selection painted this cell inverted; punch the caret
                    // back to normal video (white cell, black glyph) so the
                    // active end stands out from the rest of the selection.
                    Rectangle::new(Point::new(x, y), Size::new(CW as u32, CH as u32))
                        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                        .draw(&mut f)
                        .unwrap();
                    if let Some(ch) = lay[crow].text.chars().nth(ccol) {
                        if let Some(g) = extra_glyph(ch) {
                            blit_glyph(&mut f, x, y, g, BinaryColor::On);
                        } else {
                            let mut buf = [0u8; 4];
                            Text::with_baseline(
                                ch.encode_utf8(&mut buf),
                                Point::new(x, y),
                                text_style,
                                Baseline::Top,
                            )
                            .draw(&mut f)
                            .unwrap();
                        }
                    }
                }
                _ => {}
            }
        }

        self.draw_panel(&mut f);
        self.draw_cmdline(&mut f);
        // The file palette is a modal transient panel: it paints over the whole
        // writing column (the side panel stays, showing the PALETTE mode). Drawn
        // last so it covers the buffer text/caret rendered above.
        if self.mode == Mode::Palette {
            self.draw_palette(&mut f);
        }
        // Dark theme: flip the whole frame in one pass, after everything else is
        // painted, so text, selection, caret, panel and palette all invert
        // together. Any value but "dark" stays native black-on-white.
        if self.prefs.theme == "dark" {
            f.invert();
        }
        *out = f;
    }

    /// Draw the side panel: a full-height rule, word count at the top, and the
    /// mode indicator + pending-command echo at the bottom-left, with a
    /// keyboard-disconnect flag just above the mode while the keyboard is
    /// dropped. Small 6×10 font. This is the surface every later field
    /// (filename, clock, Wi-Fi, publish state) will add to. Word count is a
    /// throttled snapshot and the rest is event-driven, so the panel never
    /// repaints per keystroke.
    pub(crate) fn draw_panel(&self, f: &mut Frame) {
        // The rule dividing writing column from panel, full panel height.
        Rectangle::new(Point::new(DIVIDER_X, 0), Size::new(1, HEIGHT as u32))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(f)
            .unwrap();

        let style = MonoTextStyle::new(&FONT_9X15, BinaryColor::On);

        // Word count, from the throttled snapshot (never per keystroke).
        let words = format!("{} words", self.shown_words);
        Text::with_baseline(&words, Point::new(PANEL_X, 2), style, Baseline::Top)
            .draw(f)
            .unwrap();

        // Transient notice ("snackbar"), just under the word count: the last
        // save/publish result. Word-wrapped to the panel width (so a message like
        // "save FAILED - retry :w" keeps its actionable tail instead of clipping
        // mid-word) and capped at a few lines so it can't reach the bottom mode
        // strip; cleared on the next keystroke.
        if let Some(msg) = &self.notice {
            for (i, line) in wrap_text(msg, PANEL_COLS)
                .into_iter()
                .take(NOTICE_MAX_LINES)
                .enumerate()
            {
                let y = 2 + PANEL_CH + 2 + i as i32 * PANEL_CH;
                Text::with_baseline(&line, Point::new(PANEL_X, y), style, Baseline::Top)
                    .draw(f)
                    .unwrap();
            }
        }

        // Just above the mode line: the keyboard-disconnect flag, or — while
        // typing a recognised prefix — a quiet snippet hint. Mutually exclusive:
        // the hint means you're typing, which needs the keyboard. Latin-9 has no
        // ⌨/✗ or ↹ glyph, so the flag is plain text and the hint leads with `»`.
        if !self.keyboard_present {
            Text::with_baseline(
                "NO KBD",
                Point::new(PANEL_X, HEIGHT as i32 - 2 * PANEL_CH),
                style,
                Baseline::Top,
            )
            .draw(f)
            .unwrap();
        } else if let Some(name) = &self.snippet_hint {
            let hint: String = format!("» {name}").chars().take(PANEL_COLS).collect();
            Text::with_baseline(
                &hint,
                Point::new(PANEL_X, HEIGHT as i32 - 2 * PANEL_CH),
                style,
                Baseline::Top,
            )
            .draw(f)
            .unwrap();
        }

        // Focus-mode marker: a quiet indicator that a Pomodoro session is
        // running, one row above the mode line. `(s)` while the debug time-base
        // is on. (During the break the rest card masks the panel entirely, so
        // this only ever shows in a focus block, never in Rest.)
        if self.pomodoro_on {
            let label = if self.focus_debug { "· focus (s)" } else { "· focus" };
            Text::with_baseline(
                label,
                Point::new(PANEL_X, HEIGHT as i32 - 3 * PANEL_CH),
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
                Mode::Visual => "VISUAL",
                Mode::VisualLine => "V-LINE",
                Mode::View => "VIEW",
                Mode::Palette => "PALETTE",
                // Rest masks the panel (draw_into early-returns), so this is
                // never reached; listed for exhaustiveness.
                Mode::Rest => "REST",
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
                Point::new(PANEL_X, HEIGHT as i32 - PANEL_CH),
                style,
                Baseline::Top,
            )
            .draw(f)
            .unwrap();
        }
    }

    /// The focus-mode **rest card** ([`Mode::Rest`]): a full-screen curtain that
    /// masks the editor during a Pomodoro break. Centred, body font. Painted
    /// black-on-white here; the caller's dark-theme invert turns it into the
    /// black card. Three lines — a `Rest` title, the finished block's
    /// `words · minutes`, and the `Ctrl-C continue · Ctrl-Q quit` hint (both are
    /// deliberate chords so a stray key can't end the break). Kept to Latin-9
    /// glyphs (the `·` is 0xB7, in the font; no em-dash, which isn't) so no
    /// extra-glyph overlay is needed.
    pub(crate) fn draw_rest_card(&self, f: &mut Frame) {
        f.clear_white();
        let style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
        let (words, mins) = self.rest_stats.unwrap_or((0, 0));
        let lines = [
            "Rest".to_string(),
            format!("{words} words · {mins} min"),
            "Ctrl-C continue  ·  Ctrl-Q quit".to_string(),
        ];
        // Vertically centre the three body-font rows; horizontally centre each.
        let block_h = lines.len() as i32 * CH;
        let mut y = (HEIGHT as i32 - block_h) / 2;
        for line in &lines {
            let w = line.chars().count() as i32 * CW;
            let x = (WIDTH as i32 - w) / 2;
            Text::with_baseline(line, Point::new(x, y), style, Baseline::Top)
                .draw(f)
                .unwrap();
            y += CH;
        }
    }

    /// The transient `:` command line, drawn at body size (FONT_10X20) along the
    /// bottom of the writing column (vim-style). Shown only while composing a
    /// command. At this size it no longer fits the old 12 px sliver, so it
    /// overlays the bottom writing row: blank that row to white first, then draw
    /// `:cmd` over it. The row's text reappears on the next render once the
    /// command finishes (Enter/Esc) — you never read the last line while typing a
    /// command. Blanks only the writing column (left of the divider), so the
    /// panel is untouched (and in Command mode its mode line isn't drawn anyway).
    pub(crate) fn draw_cmdline(&self, f: &mut Frame) {
        if self.mode != Mode::Command {
            return;
        }
        let band_top = (ROWS as i32 - 1) * CH; // start of the last writing row
        Rectangle::new(
            Point::new(0, band_top),
            Size::new(DIVIDER_X as u32, (HEIGHT as i32 - band_top) as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(f)
        .unwrap();

        let s = format!("{}{}", self.cmd_prompt, self.cmdline);
        let style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
        Text::with_baseline(&s, Point::new(2, HEIGHT as i32 - CH), style, Baseline::Top)
            .draw(f)
            .unwrap();
    }

    /// Draw the file palette (the modal transient panel, [`Mode::Palette`]) over
    /// the writing column — the side panel stays put. Top row: a `> query`
    /// prompt with a block caret. Then a rule, the fuzzy-ranked file list (the
    /// selected row in reverse video), and a key hint on the bottom row. The list
    /// scrolls to keep the selection visible. Body font (FONT_10X20) throughout.
    /// The whole column repaints, so the host renders this as one full-area
    /// partial — the Spike 11 transient-panel refresh worth eyeballing for
    /// e-ink ghosting.
    pub(crate) fn draw_palette(&self, f: &mut Frame) {
        // Cover the writing column with white (left of the divider only).
        Rectangle::new(Point::new(0, 0), Size::new(DIVIDER_X as u32, HEIGHT as u32))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
            .draw(f)
            .unwrap();
        let style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
        let inv = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);

        // The `> new file` input step: a labelled filename prompt with a block
        // caret and a create hint — no list. The query is the filename being
        // typed; an empty one shows a placeholder naming the scope-prefix rule.
        if self.palette_step == PaletteStep::NewFile {
            Text::with_baseline("New file:", Point::new(2, 0), style, Baseline::Top)
                .draw(f)
                .unwrap();
            let y = CH + 3;
            if self.palette_query.is_empty() {
                Text::with_baseline(
                    "name  (repo/ or local/ prefix)",
                    Point::new(2 + CW, y),
                    style,
                    Baseline::Top,
                )
                .draw(f)
                .unwrap();
            } else {
                Text::with_baseline(&self.palette_query, Point::new(2, y), style, Baseline::Top)
                    .draw(f)
                    .unwrap();
            }
            let cx = (2 + self.palette_query.chars().count() as i32 * CW).min(DIVIDER_X - 2);
            Rectangle::new(Point::new(cx, y), Size::new(2, CH as u32))
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                .draw(f)
                .unwrap();
            Text::with_baseline(
                "Enter create  Esc cancel",
                Point::new(2, HEIGHT as i32 - CH),
                style,
                Baseline::Top,
            )
            .draw(f)
            .unwrap();
            return;
        }

        // The query on the top row with a block caret at the end. A bare input is
        // "go to file" (VS Code Cmd-P); a leading `>` switches to the command list
        // (`command_mode`). An empty query is just the caret over a `Go to file`
        // placeholder that clears on the first keystroke — type `>` for commands.
        let command_mode = self.palette_command_mode();
        let snippet_mode = self.palette_snippet_mode();
        if self.palette_query.is_empty() {
            Text::with_baseline(
                "Go to file  ·  > settings  ·  $ snippets",
                Point::new(2 + CW, 0),
                style,
                Baseline::Top,
            )
            .draw(f)
            .unwrap();
        } else {
            Text::with_baseline(&self.palette_query, Point::new(2, 0), style, Baseline::Top)
                .draw(f)
                .unwrap();
        }
        let cx = (2 + self.palette_query.chars().count() as i32 * CW).min(DIVIDER_X - 2);
        Rectangle::new(Point::new(cx, 0), Size::new(2, CH as u32))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(f)
            .unwrap();

        // Rule under the prompt.
        Rectangle::new(Point::new(0, CH), Size::new(DIVIDER_X as u32, 1))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(f)
            .unwrap();

        // The list is the fuzzy-ranked files, the `>` command registry, or the `$`
        // snippet library, per the active sigil.
        let matches = if snippet_mode {
            self.palette_snippet_matches()
        } else if command_mode {
            self.palette_command_matches()
        } else {
            self.palette_matches()
        };
        let max_chars = WRITE_COLS - 1; // leave a right margin
        let list_top = CH + 3;
        let hint_y = HEIGHT as i32 - CH; // bottom row holds the key hint
        let visible = ((hint_y - list_top) / CH).max(1) as usize;

        if matches.is_empty() {
            let msg = if snippet_mode {
                if self.snippets.is_empty() {
                    "(no snippets)"
                } else {
                    "(no match)"
                }
            } else if command_mode {
                "(no command)"
            } else if self.file_spans.is_empty() {
                "(no files on card)"
            } else if self.palette_query.chars().count() < PALETTE_MIN_QUERY {
                // No recents yet and the query is below the search threshold —
                // the full list needs 2+ chars.
                "(type to search)"
            } else {
                "(no match)"
            };
            Text::with_baseline(msg, Point::new(2, list_top), style, Baseline::Top)
                .draw(f)
                .unwrap();
        } else {
            let sel = self.palette_sel.min(matches.len() - 1);
            // Scroll the window so the selection stays visible.
            let start = if sel >= visible { sel - visible + 1 } else { 0 };
            for (row, &idx) in matches.iter().enumerate().skip(start).take(visible) {
                let y = list_top + (row - start) as i32 * CH;
                let label: String = if snippet_mode {
                    Self::snippet_label(&self.snippets[idx]).chars().take(max_chars).collect()
                } else if command_mode {
                    self.command_label(PALETTE_CMDS[idx])
                } else {
                    palette_label(self.file_at(idx)).chars().take(max_chars).collect()
                };
                if row == sel {
                    // Reverse video: black fill across the column, white glyphs.
                    Rectangle::new(Point::new(0, y), Size::new(DIVIDER_X as u32, CH as u32))
                        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                        .draw(f)
                        .unwrap();
                    Text::with_baseline(&label, Point::new(2, y), inv, Baseline::Top)
                        .draw(f)
                        .unwrap();
                } else {
                    Text::with_baseline(&label, Point::new(2, y), style, Baseline::Top)
                        .draw(f)
                        .unwrap();
                }
            }
        }

        let hint = if snippet_mode {
            "^N/^P move  Enter insert  Esc close"
        } else if command_mode {
            "^N/^P move  Enter change  Esc close"
        } else {
            "^N/^P move  Enter open  Esc close"
        };
        Text::with_baseline(hint, Point::new(2, hint_y), style, Baseline::Top)
            .draw(f)
            .unwrap();
    }
}
