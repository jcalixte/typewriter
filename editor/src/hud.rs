//! HUD painters: the on-screen furniture drawn around and over the writing
//! column, kept out of [`crate::render`] (which owns grid geometry, display-line
//! layout, and the buffer painting). Each is a `draw_*` method on [`Editor`],
//! called from [`Editor::draw_into`]:
//!
//! * [`Editor::draw_panel`] — the persistent right-hand side panel.
//! * [`Editor::draw_cmdline`] — the transient `:` command line.
//! * [`Editor::draw_palette`] — the modal file / command / snippet palette.
//! * [`Editor::draw_rest_card`] / [`Editor::draw_about_card`] — full-screen cards.
//!
//! `hud` is a sibling of `render` under the crate root, so it reaches `Editor`'s
//! private fields and the shared grid constants/helpers through the same
//! `use super::*;` — no code inside the painters changed when they moved here.

use super::*;

impl Editor {
    /// Draw the side panel: a full-height rule plus three stacked tiers grouped
    /// by subject, separated by a blank row (structure by spacing, not header
    /// labels):
    ///
    /// * **File** (top-anchored): the active file name, then the word count with
    ///   a trailing `*` when the buffer has unsaved edits.
    /// * **Sync** (below the file tier, after a gap): the buffer [`Scope`]
    ///   (`Tracked`/`Local` — whether it syncs at all), and beneath it the
    ///   transient push/pull/save `notice` ("snackbar") when one is present.
    /// * **Vim** (bottom-anchored): the focus marker, the mode indicator +
    ///   pending-command echo, and a keyboard-disconnect flag / snippet hint just
    ///   above the mode line.
    ///
    /// FONT_9X15 throughout. Word count is a throttled snapshot and everything
    /// else is event-driven, so the panel never repaints per keystroke. The file
    /// tier grows down and the vim tier is pinned up, with the (capped) sync tier
    /// between them, so a long notice can no longer run into the mode strip.
    pub(crate) fn draw_panel(&self, f: &mut Frame) {
        // The rule dividing writing column from panel, full panel height.
        Rectangle::new(Point::new(DIVIDER_X, 0), Size::new(1, HEIGHT as u32))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(f)
            .unwrap();

        let style = MonoTextStyle::new(&FONT_9X15, BinaryColor::On);

        // Active file name on the top line(s) — the file you're looking at, made
        // friendlier (`friendly_filename`: leading date kept, hyphens → spaces,
        // `.md` dropped) — or `[no name]` for an unnamed scratch buffer. Wrapped to
        // the panel width rather than truncated, so a long title stays readable;
        // capped at `FILENAME_MAX_LINES` so it can't crowd out the rows below.
        let name = self
            .path
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("[no name]");
        let name_lines = wrap_text(&friendly_filename(name), PANEL_COLS);
        let name_rows = name_lines.len().min(FILENAME_MAX_LINES);
        for (i, line) in name_lines.iter().take(FILENAME_MAX_LINES).enumerate() {
            let y = 2 + i as i32 * PANEL_CH;
            Text::with_baseline(line, Point::new(PANEL_X, y), style, Baseline::Top)
                .draw(f)
                .unwrap();
        }

        // Word count on the line below the (possibly wrapped) name, from the
        // throttled snapshot (never per keystroke). A trailing `*` — vim's
        // modified marker — flags unsaved edits (`dirty`); `·` is already the
        // focus marker, so `*` keeps the two unambiguous.
        let words_y = 2 + name_rows as i32 * PANEL_CH;
        // Thousands-grouped ("5,002 words") so it reads as the same figure as the
        // milestone notice ("5,000 words!"). Worst realistic case fits the 15-col
        // panel: "99,999 words *" is 14 chars.
        let words = if self.dirty {
            format!("{} words *", group_thousands(self.shown_words))
        } else {
            format!("{} words", group_thousands(self.shown_words))
        };
        Text::with_baseline(&words, Point::new(PANEL_X, words_y), style, Baseline::Top)
            .draw(f)
            .unwrap();

        // ── Sync tier ────────────────────────────────────────────────────────
        // One blank row below the file tier. The buffer scope is the persistent
        // signal — whether this file syncs at all (`:gp` is refused in Local) —
        // so it anchors the tier even with no notice. There is no ahead/behind
        // state to show: push/pull results only ever arrive as the transient
        // notice below.
        let scope_y = words_y + 2 * PANEL_CH;
        let scope_label = match self.scope {
            Scope::Tracked => "Tracked",
            Scope::Local => "Local",
        };
        Text::with_baseline(scope_label, Point::new(PANEL_X, scope_y), style, Baseline::Top)
            .draw(f)
            .unwrap();

        // Transient notice ("snackbar") directly under the scope: the last
        // save/push/pull result. Word-wrapped to the panel width (so a message
        // like "save FAILED - retry :w" keeps its actionable tail instead of
        // clipping mid-word) and capped at a few lines; cleared on the next
        // keystroke.
        if let Some(msg) = &self.notice {
            let notice_top = scope_y + PANEL_CH;
            for (i, line) in wrap_text(msg, PANEL_COLS)
                .into_iter()
                .take(NOTICE_MAX_LINES)
                .enumerate()
            {
                let y = notice_top + i as i32 * PANEL_CH;
                Text::with_baseline(&line, Point::new(PANEL_X, y), style, Baseline::Top)
                    .draw(f)
                    .unwrap();
            }
        }

        // ── Companion tier ───────────────────────────────────────────────────
        // Typo, resident between the sync and vim tiers: the current mood face
        // (mirrored, watching the text) at [`FACE_SCALE`], horizontally centred
        // in the panel at the fixed [`FACE_Y`]. His moods ride the refresh
        // cycle (see `app::Panel`) and the word-count milestones; an empty
        // buffer always gets the neutral face plus the blank-page nudge. A
        // notice whose wrapped lines would run into the face box wins — it is
        // transient, and the next keystroke clears it and brings Typo back.
        if self.prefs.companion {
            let notice_rows = self
                .notice
                .as_ref()
                .map_or(0, |m| wrap_text(m, PANEL_COLS).len().min(NOTICE_MAX_LINES));
            let notice_end = scope_y + (1 + notice_rows as i32) * PANEL_CH;
            if notice_end <= FACE_Y {
                let empty = self.text.is_empty();
                let mood = if empty { typo::Mood::Neutral } else { self.companion_mood };
                let face = mood.face();
                let fw = face.w as i32 * FACE_SCALE;
                let fx = PANEL_X + (WIDTH as i32 - PANEL_X - fw) / 2;
                typo::blit_sprite(f, fx, FACE_Y, face, FACE_SCALE);
                // The empty-file nudge, centred under the face. Only when the
                // rows beneath are actually free: the focus marker and the
                // NO KBD flag own them when shown (a snippet hint can't — it
                // needs typed text, and the buffer is empty).
                if empty && self.keyboard_present && !self.pomodoro_on {
                    let caption_top = FACE_Y + face.h as i32 * FACE_SCALE + 4;
                    for (i, line) in ["plenty of", "beak left."].iter().enumerate() {
                        let w = line.chars().count() as i32 * PANEL_CW;
                        let x = PANEL_X + (WIDTH as i32 - PANEL_X - w) / 2;
                        let y = caption_top + i as i32 * PANEL_CH;
                        Text::with_baseline(line, Point::new(x, y), style, Baseline::Top)
                            .draw(f)
                            .unwrap();
                    }
                }
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
                // Rest and About mask the panel (draw_into early-returns), so
                // these are never reached; listed for exhaustiveness.
                Mode::Rest => "REST",
                Mode::About => "ABOUT",
                Mode::Confirm => "CONFIRM",
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

    /// The `:about` splash ([`Mode::About`]): a full-screen card with the product
    /// wordmark and running firmware version centred, plus the credit and the
    /// leave hint pinned near the bottom. Painted black-on-white; the caller's
    /// dark-theme invert turns it into the black card (as with the rest card).
    /// Latin-9 glyphs only — no heart glyph (it isn't in the font), so the credit
    /// spells "with love". `version` is host-injected; empty (host tests) shows
    /// "version unknown" rather than a bare `v`.
    pub(crate) fn draw_about_card(&self, f: &mut Frame) {
        f.clear_white();
        let style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
        let centre = |f: &mut Frame, text: &str, y: i32| {
            let x = (WIDTH as i32 - text.chars().count() as i32 * CW) / 2;
            Text::with_baseline(text, Point::new(x, y), style, Baseline::Top)
                .draw(f)
                .unwrap();
        };

        // Wordmark + version, vertically centred.
        let version = if self.version.is_empty() {
            "version unknown".to_string()
        } else {
            format!("v{}", self.version)
        };
        let top = (HEIGHT as i32 - 2 * CH) / 2;
        centre(f, "typoena", top);
        centre(f, &version, top + CH);

        // Credit + leave hint, pinned a row above the bottom edge.
        centre(f, "Made with love by Julien Calixte & Emmanuel Colas", HEIGHT as i32 - 3 * CH);
        centre(f, "Enter or q to leave", HEIGHT as i32 - 2 * CH);
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
        // Live font preview: when the highlighted `>` command is Font, reserve the
        // row above the hint for a sample line drawn in the selected font. The
        // palette covers the writing column, so this is the only way to see a font
        // while cycling it.
        let sel = self.palette_sel.min(matches.len().saturating_sub(1));
        let font_preview = command_mode
            && matches.get(sel).map_or(false, |&idx| matches!(PALETTE_CMDS[idx], PaletteCmd::Font));
        let preview_y = hint_y - CH;
        let list_bottom = if font_preview { preview_y } else { hint_y };
        let visible = ((list_bottom - list_top) / CH).max(1) as usize;

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
            // Scroll the window so the selection stays visible.
            let start = if sel >= visible { sel - visible + 1 } else { 0 };
            for (row, &idx) in matches.iter().enumerate().skip(start).take(visible) {
                let y = list_top + (row - start) as i32 * CH;
                let label: String = if snippet_mode {
                    Self::snippet_label(&self.snippets[idx]).chars().take(max_chars).collect()
                } else if command_mode {
                    self.command_label(PALETTE_CMDS[idx])
                } else {
                    // Prettify the basename only (`friendly_filename`), keeping any
                    // scope/dir prefix as-is so the date check anchors on the name.
                    let raw = palette_label(self.file_at(idx));
                    let pretty = match raw.rsplit_once('/') {
                        Some((dir, base)) => format!("{dir}/{}", friendly_filename(base)),
                        None => friendly_filename(raw),
                    };
                    pretty.chars().take(max_chars).collect()
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

        // The reserved preview row (see `font_preview`): a pangram in the chosen
        // body font, under a thin rule mirroring the prompt rule above.
        if font_preview {
            Rectangle::new(Point::new(0, preview_y - 1), Size::new(DIVIDER_X as u32, 1))
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                .draw(f)
                .unwrap();
            let preview = MonoTextStyle::new(display::body_font(&self.prefs.font), BinaryColor::On);
            let sample: String = "The quick brown fox jumps over the lazy dog"
                .chars()
                .take(WRITE_COLS - 1)
                .collect();
            Text::with_baseline(&sample, Point::new(2, preview_y), preview, Baseline::Top)
                .draw(f)
                .unwrap();
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
