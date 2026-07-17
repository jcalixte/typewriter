//! The Cmd-P palette: file switching, the `>` command registry, and the `$`
//! snippet picker.

use super::*;

/// The palette's display label for an absolute path: `/sd/` stripped, so
/// `/sd/repo/notes.md` shows as `repo/notes.md` and `/sd/local/journal.md` as
/// `local/journal.md`. The scope dir (`repo`/`local`) stays, which both
/// disambiguates same-named files across scopes and reads as a scope tag. A path
/// not under `/sd/` is shown verbatim. Matching (`fuzzy_score`) runs on this
/// label, so you can filter by scope (`local`) or subpath, not just basename.
pub(crate) fn palette_label(path: &str) -> &str {
    path.strip_prefix("/sd/").unwrap_or(path)
}

/// A `>` palette command — a real action registry, not a settings box (v0.6).
/// Three dispatch shapes, distinguished by [`PaletteCmd::kind`]:
/// - a **[one-shot](CmdKind::OneShot)** ([`Format`](PaletteCmd::Format),
///   [`Publish`](PaletteCmd::Publish)) runs and closes the palette;
/// - a **[parameterised](CmdKind::Param)** command ([`NewFile`](PaletteCmd::NewFile))
///   morphs the palette into a filename input step;
/// - a **[toggle](CmdKind::Toggle)** — the boolean prefs and the preset
///   rotations ([`Theme`](PaletteCmd::Theme), [`AutoSync`](PaletteCmd::AutoSync),
///   [`ScrollMargin`](PaletteCmd::ScrollMargin)) —
///   applies live and keeps the list open, so several settings flip in a row. Each
///   toggle's *label* carries the pref's current state ([`Editor::command_label`]),
///   so the list still doubles as a settings readout. `auto_sync` has no behaviour
///   yet (v0.7); cycling it only changes the stored/displayed value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteCmd {
    NewFile,
    Format,
    Publish,
    Setup,
    Reboot,
    SaveOnIdle,
    FormatOnSave,
    LineNumbers,
    ScrollMargin,
    OpenLastOnBoot,
    Theme,
    AutoSync,
}

/// How a [`PaletteCmd`] behaves on Enter — see [`PaletteCmd::kind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CmdKind {
    /// Applies live and keeps the palette open (the pref toggles/rotations).
    Toggle,
    /// Runs once and closes the palette (`format`, `publish`).
    OneShot,
    /// Opens a second input step in the palette (`new file`).
    Param,
}

impl PaletteCmd {
    /// The command's dispatch shape, which decides what Enter does in
    /// [`Editor::palette_run_command`].
    fn kind(self) -> CmdKind {
        match self {
            PaletteCmd::NewFile => CmdKind::Param,
            PaletteCmd::Format | PaletteCmd::Publish | PaletteCmd::Setup | PaletteCmd::Reboot => {
                CmdKind::OneShot
            }
            _ => CmdKind::Toggle,
        }
    }
}

/// The palette command list, in display order (empty `>` query shows them all):
/// the actions first, the settings after.
pub(crate) const PALETTE_CMDS: [PaletteCmd; 12] = [
    PaletteCmd::NewFile,
    PaletteCmd::Format,
    PaletteCmd::Publish,
    PaletteCmd::Setup,
    PaletteCmd::Reboot,
    PaletteCmd::SaveOnIdle,
    PaletteCmd::FormatOnSave,
    PaletteCmd::LineNumbers,
    PaletteCmd::ScrollMargin,
    PaletteCmd::OpenLastOnBoot,
    PaletteCmd::Theme,
    PaletteCmd::AutoSync,
];

/// Which step the palette is showing. Most of its life it is a
/// [`List`](PaletteStep::List) — files, `>` commands, or `$` snippets, chosen by
/// the query's leading sigil. Selecting a [parameterised](CmdKind::Param) `>`
/// command switches it to an input step ([`NewFile`](PaletteStep::NewFile)), where
/// the query is a value (a filename) rather than a filter, and Enter commits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteStep {
    List,
    NewFile,
}


/// Query length (chars) at which the file palette searches the full file list.
/// Shorter queries show only the recents ([`MRU_MAX`]) — the list is a
/// recursive walk of the card, and one char can't rank hundreds of paths
/// usefully. `>` commands and `$` snippets are short curated lists, so the
/// threshold does not apply to them.
pub(crate) const PALETTE_MIN_QUERY: usize = 2;


impl Editor {
    /// `Ctrl-P` — open the file palette: empty query (full list, recents first),
    /// selection on the first row.
    pub(crate) fn open_palette(&mut self) {
        self.mode = Mode::Palette;
        self.palette_query.clear();
        self.palette_sel = 0;
        self.palette_step = PaletteStep::List;
    }

    /// `:settings` — open the palette straight into `>` command mode (the
    /// settings list), so the prefs are reachable in one command instead of
    /// `Cmd-P` then `>`. Same surface, same stay-open toggle behaviour.
    pub(crate) fn open_settings(&mut self) {
        self.mode = Mode::Palette;
        self.palette_query = ">".to_string();
        self.palette_sel = 0;
        self.palette_step = PaletteStep::List;
    }

    /// Leave the palette back to Normal, clearing its query, selection, and step.
    pub(crate) fn close_palette(&mut self) {
        self.mode = Mode::Normal;
        self.palette_query.clear();
        self.palette_sel = 0;
        self.palette_step = PaletteStep::List;
    }

    /// Dispatch a key in [`Mode::Palette`]. In the `New file` input step the keys
    /// build a filename ([`new_file_step_key`](Self::new_file_step_key)); otherwise
    /// typing fuzzy-filters, `Ctrl-n`/`Ctrl-p` (or `Ctrl-d`/`Ctrl-u`) move the
    /// selection, and Enter acts on it per the leading sigil (open a file, run a
    /// `>` command, or insert a `$` snippet). Esc or `Cmd-P` closes; Backspace on
    /// an empty query also closes (mirrors the `:` line). Any query edit resets the
    /// selection to the top.
    pub(crate) fn palette_key(&mut self, key: Key) {
        if self.palette_step == PaletteStep::NewFile {
            return self.new_file_step_key(key);
        }
        match key {
            Key::Char(c) => {
                self.palette_query.push(c);
                self.palette_sel = 0;
            }
            Key::Backspace => {
                if self.palette_query.pop().is_none() {
                    self.close_palette();
                } else {
                    self.palette_sel = 0;
                }
            }
            // Readline Ctrl-W: drop trailing spaces then the last word.
            Key::DeleteWord => {
                while self.palette_query.ends_with(' ') {
                    self.palette_query.pop();
                }
                while !self.palette_query.is_empty() && !self.palette_query.ends_with(' ') {
                    self.palette_query.pop();
                }
                self.palette_sel = 0;
            }
            Key::DeleteLine => {
                self.palette_query.clear();
                self.palette_sel = 0;
            }
            // Ctrl-n/Ctrl-p move the selection (fzf-style); Ctrl-d/Ctrl-u do too.
            // Wraps around the current result list (files, `>` commands, `$` snippets).
            Key::Down | Key::HalfPageDown => {
                let n = self.palette_len();
                if n > 0 {
                    self.palette_sel = (self.palette_sel + 1) % n;
                }
            }
            Key::Up | Key::HalfPageUp => {
                let n = self.palette_len();
                if n > 0 {
                    self.palette_sel = self.palette_sel.checked_sub(1).unwrap_or(n - 1);
                }
            }
            // Enter acts on the selection by mode: insert a `$` snippet, run a `>`
            // command, or open the selected file.
            Key::Enter => {
                if self.palette_snippet_mode() {
                    self.palette_insert_selected();
                } else if self.palette_command_mode() {
                    self.palette_run_command();
                } else {
                    self.palette_open_selected();
                }
            }
            // Esc, or Cmd-P again, closes the palette.
            Key::Escape | Key::Palette => self.close_palette(),
            // Redo has no meaning here; Cmd-S is handled in `handle` before
            // dispatch; Ctrl-C is a focus-break key. All no-ops in the palette
            // (unreachable/inert here, but the match must be exhaustive).
            Key::Redo | Key::Save | Key::FocusContinue | Key::FocusQuit => {}
        }
    }

    /// Keys in the `> new file` input step: the query is a filename, not a filter.
    /// Enter creates it (scope resolved from a `repo/`/`local/` prefix, exactly as
    /// `:enew` did) and closes; an empty name is a no-op that stays in the step.
    /// Backspacing past the start steps **back** to the `>` command list rather
    /// than closing, so the step is escapable without losing the palette. Esc or
    /// `Cmd-P` closes outright.
    pub(crate) fn new_file_step_key(&mut self, key: Key) {
        match key {
            Key::Char(c) => self.palette_query.push(c),
            Key::Backspace => {
                if self.palette_query.pop().is_none() {
                    // Nothing left to erase — return to the command list.
                    self.palette_step = PaletteStep::List;
                    self.palette_query = ">".to_string();
                    self.palette_sel = 0;
                }
            }
            Key::DeleteWord => {
                while self.palette_query.ends_with(' ') {
                    self.palette_query.pop();
                }
                while !self.palette_query.is_empty() && !self.palette_query.ends_with(' ') {
                    self.palette_query.pop();
                }
            }
            Key::DeleteLine => self.palette_query.clear(),
            Key::Enter => {
                let name = self.palette_query.trim().to_string();
                if name.is_empty() {
                    return; // nothing typed yet — stay in the step
                }
                self.close_palette();
                self.new_file(&name);
            }
            Key::Escape | Key::Palette => self.close_palette(),
            // No list to move over in this step; Cmd-S is handled upstream in
            // `handle`, Ctrl-C is a focus-break key (both inert here, but the
            // match must be exhaustive).
            Key::Up | Key::Down | Key::HalfPageUp | Key::HalfPageDown | Key::Redo | Key::Save
            | Key::FocusContinue | Key::FocusQuit => {}
        }
    }

    /// Open the palette's selected file (Enter). A no-op on an empty result set.
    /// Closes the palette first, then routes through [`open_path`](Self::open_path)
    /// exactly like `:e`, so the switch/park/evict/MRU path is shared.
    pub(crate) fn palette_open_selected(&mut self) {
        let idx = self.palette_matches().get(self.palette_sel).copied();
        self.close_palette();
        let Some(idx) = idx else { return };
        let (path, scope) = resolve_path(self.file_at(idx), self.scope);
        self.open_path(path, scope);
    }

    /// The palette's filtered, ranked result as indices into [`files`](Self::files).
    /// Base order is MRU-first (recents in use order, then the rest as sorted). A
    /// non-empty query keeps only fuzzy matches and stable-sorts them by score, so
    /// equal scores keep their MRU/base position. See [`fuzzy_score`].
    ///
    /// Below [`PALETTE_MIN_QUERY`] chars the candidate set is the recents only:
    /// the file list is a recursive walk of the whole card, too long to page
    /// through unranked, but the MRU keeps quick-switch (`Cmd-P`, `Enter`) one
    /// keystroke away. Two typed chars reveal the full list.
    pub(crate) fn palette_matches(&self) -> Vec<usize> {
        let mut order: Vec<usize> = Vec::with_capacity(self.file_count());
        for r in &self.recent {
            if let Some(i) = (0..self.file_count()).find(|&i| self.file_at(i) == r) {
                order.push(i);
            }
        }
        if self.palette_query.chars().count() >= PALETTE_MIN_QUERY {
            for i in 0..self.file_count() {
                if !order.contains(&i) {
                    order.push(i);
                }
            }
        }
        if self.palette_query.is_empty() {
            return order;
        }
        let mut scored: Vec<(usize, i32)> = order
            .into_iter()
            .filter_map(|i| {
                fuzzy_score(&self.palette_query, palette_label(self.file_at(i))).map(|s| (i, s))
            })
            .collect();
        // Stable sort by descending score — ties keep their MRU/base position.
        scored.sort_by_key(|&(_, s)| core::cmp::Reverse(s));
        scored.into_iter().map(|(i, _)| i).collect()
    }

    // --- Palette command mode (`>`) ----------------------------------------

    /// Whether the palette is in `>` command mode. VS Code semantics: a leading
    /// `>` in the query switches the file search to the command list. The `>` is
    /// part of [`palette_query`](Self::palette_query), so backspacing it off
    /// returns to file mode with no extra state.
    pub(crate) fn palette_command_mode(&self) -> bool {
        self.palette_query.starts_with('>')
    }

    /// The command filter: everything after the leading `>`, trimmed. `>` alone
    /// (or with only spaces) is an empty filter, which matches every command.
    pub(crate) fn command_filter(&self) -> &str {
        self.palette_query.strip_prefix('>').unwrap_or("").trim()
    }

    /// A command's display label. An action's label is a plain verb (with a
    /// trailing `...` on the parameterised `new file`, VS-Code-style, to flag the
    /// second step — ASCII dots, since Latin-9 has no `…` glyph); a toggle's label
    /// carries its pref's current state, so the list reads as a live settings panel
    /// and the effect is legible before and after. This is also the text
    /// [`fuzzy_score`] matches against.
    pub(crate) fn command_label(&self, cmd: PaletteCmd) -> String {
        let on = |b| if b { "on" } else { "off" };
        match cmd {
            PaletteCmd::NewFile => "new file...".to_string(),
            PaletteCmd::Format => "format".to_string(),
            PaletteCmd::Publish => "publish".to_string(),
            PaletteCmd::Setup => "setup...".to_string(),
            PaletteCmd::Reboot => "reboot".to_string(),
            PaletteCmd::SaveOnIdle => format!("save on idle: {}", on(self.prefs.save_on_idle)),
            PaletteCmd::FormatOnSave => format!("format on save: {}", on(self.prefs.format_on_save)),
            PaletteCmd::LineNumbers => format!("line numbers: {}", on(self.prefs.line_numbers)),
            PaletteCmd::ScrollMargin => format!("scroll margin: {}", self.prefs.scroll_margin),
            PaletteCmd::OpenLastOnBoot => {
                format!("open last on boot: {}", on(self.prefs.open_last_on_boot))
            }
            PaletteCmd::Theme => format!("theme: {}", self.prefs.theme),
            PaletteCmd::AutoSync => format!("auto sync: {}", self.prefs.auto_sync),
        }
    }

    /// Filtered, ranked command indices into [`PALETTE_CMDS`]. An empty filter
    /// keeps registry order; a non-empty one fuzzy-ranks by label, same matcher
    /// and stable-sort as the file list.
    pub(crate) fn palette_command_matches(&self) -> Vec<usize> {
        let filter = self.command_filter();
        let mut scored: Vec<(usize, i32)> = PALETTE_CMDS
            .iter()
            .enumerate()
            .filter_map(|(i, &cmd)| fuzzy_score(filter, &self.command_label(cmd)).map(|s| (i, s)))
            .collect();
        scored.sort_by_key(|&(_, s)| core::cmp::Reverse(s));
        scored.into_iter().map(|(i, _)| i).collect()
    }

    /// Enter in `>` command mode, dispatched by the selected command's
    /// [`kind`](PaletteCmd::kind):
    /// - a **[toggle](CmdKind::Toggle)** flips its pref and the palette **stays
    ///   open** (flip several in a row; the label updates in place);
    /// - a **[one-shot](CmdKind::OneShot)** (`format`/`publish`) runs and **closes**
    ///   — an action switches you back to writing, a toggle does not;
    /// - a **[parameterised](CmdKind::Param)** command (`new file`) opens the
    ///   filename input step ([`begin_new_file_step`](Self::begin_new_file_step)).
    ///
    /// A no-op on an empty result set (nothing selected), staying open so the
    /// query can be fixed.
    pub(crate) fn palette_run_command(&mut self) {
        let Some(&ci) = self.palette_command_matches().get(self.palette_sel) else {
            return;
        };
        let cmd = PALETTE_CMDS[ci];
        match cmd.kind() {
            CmdKind::Toggle => self.cycle_pref(cmd),
            CmdKind::OneShot => {
                self.close_palette();
                match cmd {
                    PaletteCmd::Format => {
                        self.format_buffer();
                        self.set_notice("formatted");
                    }
                    PaletteCmd::Publish => self.run_publish(),
                    PaletteCmd::Setup => self.request_setup(),
                    PaletteCmd::Reboot => self.request_reboot(),
                    _ => {}
                }
            }
            CmdKind::Param => self.begin_new_file_step(),
        }
    }

    /// Switch the open palette into its `new file` filename input step: the list
    /// gives way to a prompt, and the next Enter creates the typed file. Reached
    /// only from [`palette_run_command`](Self::palette_run_command), so the palette
    /// is already open.
    pub(crate) fn begin_new_file_step(&mut self) {
        self.palette_step = PaletteStep::NewFile;
        self.palette_query.clear();
        self.palette_sel = 0;
    }

    /// The publish path shared by `:gp` and the `>` `publish` command: format on
    /// save (if enabled), queue the buffer save, then the git push — the host
    /// services them in order. Tracked-only: a Local buffer never reaches the
    /// remote, so it is a no-op with a notice.
    pub(crate) fn run_publish(&mut self) {
        if self.scope == Scope::Local {
            self.set_notice("Publish unavailable (Local)");
            return;
        }
        if self.prefs.format_on_save {
            self.format_buffer();
        }
        self.request_save_active();
        self.requests.push(Effect::Publish);
    }

    /// Advance the pref a command targets to its next value, apply it live (the
    /// next [`draw`](Self::draw) reflects it — line numbers appear/vanish, the
    /// theme flips at once), queue the prefs-file write ([`Effect::SavePrefs`]),
    /// and confirm the new state on the snackbar. A boolean flips; a preset
    /// string ([`Theme`](PaletteCmd::Theme), [`AutoSync`](PaletteCmd::AutoSync))
    /// rotates to its next option and wraps — so from the palette every setting
    /// is "press Enter to change". The queued `SavePrefs` is what makes the
    /// change durable and lets it ride the next `:gp` to other devices.
    pub(crate) fn cycle_pref(&mut self, cmd: PaletteCmd) {
        match cmd {
            PaletteCmd::SaveOnIdle => self.prefs.save_on_idle = !self.prefs.save_on_idle,
            PaletteCmd::FormatOnSave => self.prefs.format_on_save = !self.prefs.format_on_save,
            PaletteCmd::LineNumbers => self.prefs.line_numbers = !self.prefs.line_numbers,
            PaletteCmd::OpenLastOnBoot => {
                self.prefs.open_last_on_boot = !self.prefs.open_last_on_boot
            }
            PaletteCmd::ScrollMargin => {
                self.prefs.scroll_margin =
                    next_usize_option(self.prefs.scroll_margin, &SCROLL_MARGIN_OPTIONS)
            }
            PaletteCmd::Theme => {
                self.prefs.theme = next_option(&self.prefs.theme, &THEME_OPTIONS).to_string()
            }
            PaletteCmd::AutoSync => {
                self.prefs.auto_sync = next_option(&self.prefs.auto_sync, &AUTO_SYNC_OPTIONS).to_string()
            }
            // Actions, not prefs: palette_run_command routes them away, so we never
            // arrive here. Return before the SavePrefs/notice below rather than
            // panicking the firmware on a would-be routing bug.
            PaletteCmd::NewFile
            | PaletteCmd::Format
            | PaletteCmd::Publish
            | PaletteCmd::Setup
            | PaletteCmd::Reboot => {
                debug_assert!(false, "cycle_pref called with a non-toggle command");
                return;
            }
        }
        self.requests.push(Effect::SavePrefs {
            contents: self.prefs.to_toml(),
        });
        // The label already reflects the just-changed state (e.g. "theme: dark").
        self.set_notice(format!("{} - saved", self.command_label(cmd)));
    }

    // --- Palette snippet mode (`$`) ----------------------------------------

    /// Whether the palette is in `$` snippet mode. Same sigil mechanism as `>`: a
    /// leading `$` in the query switches the file search to the snippet launcher,
    /// and backspacing it off returns to file mode with no extra state. `$` and `>`
    /// are mutually exclusive (a query starts with at most one).
    pub(crate) fn palette_snippet_mode(&self) -> bool {
        self.palette_query.starts_with('$')
    }

    /// The snippet filter: everything after the leading `$`, trimmed. `$` alone is
    /// an empty filter, which lists every snippet.
    pub(crate) fn snippet_filter(&self) -> &str {
        self.palette_query.strip_prefix('$').unwrap_or("").trim()
    }

    /// The text a snippet is fuzzy-matched against: name, prefix, and description
    /// together, so you find a snippet by whichever you remember. Matching runs on
    /// this joined haystack (see [`fuzzy_score`]); [`snippet_label`] is the shorter
    /// string actually drawn.
    pub(crate) fn snippet_haystack(s: &Snippet) -> String {
        format!("{} {} {}", s.name, s.prefix, s.description)
    }

    /// A snippet's palette row: the display name with its inline trigger in
    /// brackets (`Markdown link [link]`), so browsing also teaches the prefix you'd
    /// type for the fast inline path. Truncated to the column width by the caller.
    pub(crate) fn snippet_label(s: &Snippet) -> String {
        format!("{} [{}]", s.name, s.prefix)
    }

    /// Filtered, ranked snippet indices into [`snippets`](Self::snippets). An empty
    /// filter keeps the parsed order (sorted by name); a non-empty one fuzzy-ranks
    /// over [`snippet_haystack`], same matcher and stable-sort as the file list.
    pub(crate) fn palette_snippet_matches(&self) -> Vec<usize> {
        let filter = self.snippet_filter();
        let mut scored: Vec<(usize, i32)> = self
            .snippets
            .iter()
            .enumerate()
            .filter_map(|(i, s)| fuzzy_score(filter, &Self::snippet_haystack(s)).map(|score| (i, score)))
            .collect();
        scored.sort_by_key(|&(_, s)| core::cmp::Reverse(s));
        scored.into_iter().map(|(i, _)| i).collect()
    }

    /// Enter in `$` snippet mode: insert the selected snippet at the caret and start
    /// its tab-stop session. Unlike a `>` toggle (which stays open), this **closes**
    /// the palette — inserting content returns you to the buffer, in Insert on `$1`.
    /// Checkpoints so the whole insertion is one undo group. A no-op on an empty
    /// result set (nothing selected), which stays open so the query can be fixed.
    pub(crate) fn palette_insert_selected(&mut self) {
        let Some(&i) = self.palette_snippet_matches().get(self.palette_sel) else {
            return;
        };
        let body = self.snippets[i].body.clone();
        self.close_palette();
        self.checkpoint(); // baseline is the buffer before insertion — undo removes it whole
        self.insert_snippet(&body);
    }

    /// Row count of the palette's current result list, whichever sigil is active —
    /// the single source the selection clamps against.
    pub(crate) fn palette_len(&self) -> usize {
        if self.palette_snippet_mode() {
            self.palette_snippet_matches().len()
        } else if self.palette_command_mode() {
            self.palette_command_matches().len()
        } else {
            self.palette_matches().len()
        }
    }

    // --- Visual mode -------------------------------------------------------

}
