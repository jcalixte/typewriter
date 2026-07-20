//! Modal text editor core: a vim-style buffer with Normal / Insert (edit) /
//! Visual (selection) / View (read-only) modes, rendered onto the e-paper
//! [`Frame`].
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
use embedded_graphics::mono_font::iso_8859_15::{FONT_9X15, FONT_10X20};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Baseline, Text};

use display::{blit_glyph, extra_glyph, Frame, HEIGHT, WIDTH};
use keymap::Key;


mod buffers;
mod editing;
mod fuzzy;
mod markdown;
mod motions;
mod palette;
mod prefs;
mod render;
mod search;
mod snippets;
mod undo;
mod visual;

pub(crate) use buffers::*;
pub(crate) use editing::*;
pub(crate) use fuzzy::*;
pub(crate) use markdown::*;
pub(crate) use palette::*;
pub(crate) use prefs::*;
pub(crate) use render::*;
pub(crate) use snippets::*;

pub use buffers::{LOCAL_DIR, REPO_DIR};
pub use prefs::{Prefs, PREFS_PATH};
pub use render::{CH, CW};
pub use snippets::{Snippet, Snippets, SNIPPETS_PATH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Navigation and commands (hjkl, w/b/e, dd, x, …).
    Normal,
    /// Text entry — keys insert at the caret.
    Insert,
    /// Charwise selection: an anchor is dropped at the caret (`visual_anchor`)
    /// and motions extend the span; `y`/`d`/`c` act on it, `Esc`/`v` leave.
    Visual,
    /// Linewise selection (`V`): the span always covers whole logical lines
    /// from the anchor's line to the caret's, whatever the columns.
    VisualLine,
    /// Read-only reading (entered with `gr`): keys scroll the viewport, edits
    /// are locked out.
    View,
    /// `:` command line — keys accumulate a command shown in the status strip;
    /// Enter runs it, Esc cancels. Handles `:fmt` (in-core) plus `:w`/`:gp`
    /// (which ask the host to persist/push via an [`Effect`]).
    Command,
    /// File palette (`Cmd-P`, reachable from every mode) — a modal transient
    /// panel over the writing column. Typing fuzzy-filters the file list
    /// ([`Editor::set_file_list`]); `Ctrl-n`/`Ctrl-p` move the selection, Enter
    /// opens it, Esc (or `Cmd-P` again) cancels back to Normal. See
    /// [`Editor::palette_key`].
    Palette,
    /// Focus-mode break (Pomodoro rest): a full-screen card masks the editor
    /// and every key is swallowed but `c` (start the next focus block) and
    /// `q`/`Esc` (end the session). Entered by the host on the silent 25-min
    /// block timer via [`Editor::enter_rest`] — there is no way to *type* into
    /// Rest, so it never touches the hidden buffer.
    Rest,
    /// The `:about` splash: a full-screen card (like [`Rest`](Mode::Rest)) with
    /// the product name and running firmware version. Read-only — every key is
    /// swallowed but `Enter`/`q`/`Esc`, which return to Normal. See
    /// [`Editor::about_key`].
    About,
    /// A destructive command is waiting for a `y`/`n` answer (`:delete`,
    /// `:reboot`, `:setup`, or `:gl`'s commit-unsynced-and-pull). Modal like
    /// [`Rest`](Mode::Rest): every key is
    /// swallowed but `y`/`Y` (proceed) — anything else cancels. Which command
    /// is pending rides in [`Editor::pending_confirm`]; the prompt shows on the
    /// snackbar. See [`Editor::confirm_key`].
    Confirm,
}

/// Which destructive command a [`Mode::Confirm`] prompt is guarding. Stored
/// alongside the mode (as [`rest_stats`](Editor::rest_stats) rides with
/// [`Mode::Rest`]) so [`confirm_key`](Editor::confirm_key) knows what a `y`
/// should run and what a cancel should say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Confirm {
    /// `:delete` / `:d` — unlink the current file from the card.
    Delete,
    /// `:reboot` — restart the device (drops in-RAM state).
    Reboot,
    /// `:setup` — reboot into the onboarding wizard.
    Setup,
    /// `:update` — download and install a firmware update over the air (then
    /// reboot into it). Gated behind a confirm because it moves the device to new
    /// firmware and restarts it. On `y` the editor queues [`Effect::Update`].
    Update,
    /// `:gl` with unpushed saves — commit the dirty journal locally, then
    /// pull (fetch + fast-forward/rebase). Guards the commit `:gl` would
    /// otherwise make on the user's behalf. On `y` the editor queues
    /// [`Effect::Pull`] `{ commit_dirty: true }`.
    PullCommit,
}

/// Which of the two file scopes ([`CONTEXT.md`]) a buffer belongs to. Fixed at
/// creation — there is no move-between-scopes operation. **Tracked** files live
/// under [`REPO_DIR`] and can be pushed (`:gp`); **Local** files live under
/// [`LOCAL_DIR`] and never leave the device, so `:gp` is refused in-core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Tracked,
    Local,
}

/// A calendar day, fed by the host from its real-time clock
/// ([`set_today`](Editor::set_today)) — the pure core has no clock of its own.
/// `:inbox` uses it to name and title today's fleeting note. The host passes
/// `None` while it has **no trustworthy date**: the editor boot path never runs
/// SNTP, so the wall clock is at the epoch until a `:gl`/`:gp` sync sets it this
/// power cycle (there is no battery-backed RTC). `:inbox` then refuses rather than
/// dating a note `1970-01-01` (see [`open_inbox_today`](Editor::open_inbox_today)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    /// 1–12.
    pub month: u32,
    /// 1–31.
    pub day: u32,
}

impl Date {
    /// `YYYY-MM-DD` — the fleeting note's filename stem. ISO field order means a
    /// plain path sort is chronological, which is what [`open_oldest_inbox`]
    /// (`:oldest`) leans on to find the oldest note for free.
    ///
    /// [`open_oldest_inbox`]: Editor::open_oldest_inbox
    pub(crate) fn iso(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// `DD/MM/YYYY` — the note's `# ` heading, matching the writer's existing
    /// `_inbox` day-first convention (e.g. `# 18/07/2026`).
    pub(crate) fn title(&self) -> String {
        format!("{:02}/{:02}/{:04}", self.day, self.month, self.year)
    }
}

/// A side effect the host (firmware) must carry out. The editor core is pure and
/// does no IO, so persistence, pushing, and file reads can't happen here —
/// they are queued and drained by [`Editor::take_effects`] after a key batch,
/// then actioned by the main loop. `:fmt` is pure text work and stays in-core,
/// so it queues nothing.
///
/// A single key can queue more than one effect: opening a file that isn't
/// resident queues a [`Save`](Effect::Save) of the outgoing dirty buffer *and* a
/// [`Load`](Effect::Load) of the target. Effects are serviced in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Persist `contents` to `path` (an atomic save on the host). Queued by `:w`
    /// (and the `:wq`/`:x` aliases), by save-before-switch, and by
    /// save-before-evict. The contents ride along because the buffer being saved
    /// is not always the active one — an evicted buffer's text is no longer
    /// reachable through [`Editor::text`]. On success the host calls
    /// [`Editor::mark_saved`].
    Save { path: String, scope: Scope, contents: String },
    /// Read `path` from disk; on success the host installs it with
    /// [`Editor::install_loaded`]. Queued when switching to a file that is not
    /// resident in memory (`:e`, palette pick).
    Load { path: String, scope: Scope },
    /// `:gp` — push the Tracked working copy to the remote. Preceded by a
    /// [`Save`](Effect::Save) of the current buffer in the same batch. Never
    /// queued from a Local buffer (blocked in-core).
    Push,
    /// `:gl` — pull from the remote: fetch, then fast-forward (or rebase local
    /// work onto origin — never a content merge). When `commit_dirty` is set the
    /// host first folds any saved-but-unpushed work into a local commit, so
    /// the fetch can replant it onto origin instead of refusing. A bare `:gl`
    /// sends `commit_dirty: false`; if the dirty journal is non-empty the host
    /// asks to confirm (the commit is user-visible — see
    /// [`confirm_pull_commit`](Editor::confirm_pull_commit)) and the confirmed
    /// retry sets `commit_dirty: true`. Complements `:gp` (push) as the download half.
    Pull { commit_dirty: bool },
    /// `:delete` — unlink `path` from the card. For a **Tracked** file the removal
    /// lands in the git working copy, so the next [`Push`](Effect::Push)'s
    /// `add --all` stages the deletion (no eager `git rm` needed); a **Local** file
    /// is just unlinked. The editor has already dropped the file from its model and
    /// switched away by the time this drains, so `scope` is informational; the host
    /// reports the outcome on the snackbar (mirrors [`Save`](Effect::Save)).
    Delete { path: String, scope: Scope },
    /// `:pub`/`:publish` — publish the active file by renaming it from
    /// `<name>.md` to `<name>.pub.md`. The host writes `contents` to `to`
    /// (recording it dirty), then unlinks `from` (recording *that* dirty too), so
    /// the next `:gp` reconstructs the tree with `from` spliced out and `to` added
    /// — git sees a rename. Carries `contents` because the in-RAM buffer, not the
    /// on-disk `from`, is the source of truth (it may hold unsaved edits). Always
    /// Tracked (Local is refused in-core), so it needs no `scope`; it is
    /// [`Save`](Effect::Save) + [`Delete`](Effect::Delete) done as one step so the
    /// snackbar reads as a single publish.
    Rename { from: String, to: String, contents: String },
    /// Persist the preferences file ([`PREFS_PATH`]) after a palette `>` command
    /// changed a pref. Carries the already-serialized TOML ([`Prefs::to_toml`]),
    /// so the host only does the atomic write — no re-serialization or buffer
    /// bookkeeping. Separate from [`Save`](Effect::Save): prefs are not a text
    /// buffer and live at a fixed path outside the multi-buffer model.
    SavePrefs { contents: String },
    /// `:setup` (or `> setup`) — reopen the onboarding wizard to change Wi-Fi,
    /// re-sign-in, or switch repos. The running editor can't reclaim the radio
    /// from the git thread, so the host reboots into the boot-time wizard
    /// (prefilled from the card conf). Only queued when no buffer has unsaved
    /// edits ([`any_dirty`](Editor::any_dirty)) — the reboot would lose them.
    Setup,
    /// `:reboot` (or a software reboot button) — cleanly restart the device.
    /// Unlike [`Setup`](Effect::Setup) this needs no marker and no radio hand-off:
    /// the host paints the branded splash and calls `esp_restart()`. The editor
    /// auto-saves every named dirty buffer in the same batch first (queued ahead
    /// of this, so the host flushes them before the reset); a dirty *unnamed*
    /// scratch buffer has nowhere to save and blocks the reboot instead.
    Reboot,
    /// `:update` (or `> update`) — check for a newer firmware release and, if one
    /// exists, download it over the air into the inactive OTA slot and reboot into
    /// it. Runs on the same radio-owning background thread as [`Push`](Effect::Push)
    /// (the editor can't reclaim the modem), so it is fire-and-forget: dispatch
    /// shows `updating...`, and the terminal outcome (installed → reboot, already
    /// current, or failed) returns later like a sync outcome. Only queued when no
    /// buffer is dirty ([`any_dirty`](Editor::any_dirty)) — the post-install reboot
    /// would lose unsaved edits — so it is gated exactly like [`Setup`](Effect::Setup).
    Update,
    /// Focus mode (Pomodoro): begin — or, after a break, restart — a focus
    /// block. The host starts its silent monotonic block timer and snapshots the
    /// word count for the session stats. Queued by `:focus` (turning the session
    /// on) and by `c` in [`Mode::Rest`] (continuing to the next block).
    FocusStart,
    /// Focus mode: end the session. The host stops the block timer. Queued by
    /// `:focus` (turning it off) and by `q`/`Esc` in [`Mode::Rest`].
    FocusStop,
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
    /// First `g` of a `gg`/`gr` awaiting the second.
    pending_g: bool,
    /// The fixed end of a Visual selection (byte offset), dropped when `v`/`V`
    /// enters Visual and cleared on leaving. The selection spans from here to
    /// the caret; `None` outside Visual/VisualLine.
    visual_anchor: Option<usize>,
    /// The `:` command line being typed (valid only in `Mode::Command`).
    cmdline: String,
    /// Which prompt opened the command line — `':'` (ex command) or `'/'`
    /// (search). Both share `Mode::Command`'s line editing (vim models them as
    /// one command-line mode); Enter dispatches on this.
    cmd_prompt: char,
    /// The last `/` pattern, kept for `n`/`N` and a bare `/`+Enter repeat.
    /// Editor-global (not per-buffer), like vim's search register.
    last_search: String,
    /// Word count as of the last stats refresh. The panel shows this snapshot,
    /// not a live count, so ordinary typing never repaints the panel row — it is
    /// refreshed on a typing pause / non-Insert action via `refresh_stats`.
    shown_words: usize,
    /// Whether a USB keyboard is attached; drives the panel disconnect flag.
    /// Fed from `usb_kbd::keyboard_present()` by the main loop.
    keyboard_present: bool,
    /// Transient side-panel message ("snackbar") — the last host event
    /// (save/push result). Shown until the next keystroke dismisses it
    /// (cleared in [`Editor::handle`]); `None` means nothing to show.
    notice: Option<String>,
    /// Editor preferences (mirrors [`PREFS_PATH`]). Held here so the palette `>`
    /// command mode can toggle them live; the host reads the file at boot and
    /// applies it via [`set_prefs`](Self::set_prefs), and reads it back for the
    /// keys it honours (`save_on_idle`). `format_on_save` and `line_numbers` are
    /// consulted in-core (`:w`/`:gp` and the gutter).
    prefs: Prefs,
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
    /// Absolute path of the active buffer on the SD card (e.g.
    /// `/sd/repo/notes.md`). Empty for an unnamed scratch buffer (the boot-message
    /// layout use); `:w` on an empty path posts "no file name" rather than saving.
    path: String,
    /// The active buffer's scope. Gates Push — `:gp` is refused in Local.
    scope: Scope,
    /// Whether the active buffer has unsaved edits. Set at each change-group
    /// ([`checkpoint`](Self::checkpoint)) and cleared when the host confirms a
    /// save ([`mark_saved`](Self::mark_saved)). Decides whether a switch/evict
    /// persists the buffer first. Deliberately over-eager: entering Insert and
    /// leaving without typing marks it dirty, costing at most one redundant
    /// (idempotent) save — cheaper than tracking every mutation site.
    dirty: bool,
    /// Inactive-but-resident buffers, least-recently-used first. The active
    /// buffer plus these is capped at [`MAX_RESIDENT`]; switching away parks the
    /// active buffer here (with its caret, scroll, and undo), switching back
    /// restores it without touching the disk. A parked buffer pushed over the cap
    /// is evicted — saved first (via an [`Effect::Save`]) if it is dirty.
    parked: Vec<Buffer>,
    /// Host-effect queue, drained by [`take_effects`](Self::take_effects) after a
    /// key batch. See [`Effect`].
    requests: Vec<Effect>,
    /// Every openable file, as absolute paths, fed by the host at boot (a
    /// recursive walk of `/sd/repo` and `/sd/local`). The palette fuzzy-filters
    /// this once the query reaches [`PALETTE_MIN_QUERY`] chars; empty until the
    /// host feeds it.
    ///
    /// **Interned**: one newline-joined blob plus byte spans, not a
    /// `Vec<String>`. On the device 1099 paths as individual `String`s cost
    /// 182 KB of *internal* DRAM (small allocs stay under the 16 KB
    /// PSRAM-malloc threshold, and per-alloc overhead dwarfs the ~50-byte
    /// payloads) — which starved the SD DMA pool during the first on-device
    /// pull (2026-07-14). A single large blob lands in PSRAM; the spans are
    /// one small `Vec`. Access via [`file_at`](Self::file_at).
    file_blob: String,
    /// Byte ranges of each path in [`file_blob`](Self::file_blob), sorted by
    /// the paths they point at (the palette's stable base order).
    file_spans: Vec<(u32, u32)>,
    /// Recently-opened files, most-recent-first (an MRU), deduped and bounded to
    /// [`MRU_MAX`]. Every `:e`/palette open pushes to the front
    /// ([`note_recent`](Self::note_recent)); it orders the palette when the query
    /// is empty, so the file you were just in is one keystroke away.
    recent: Vec<String>,
    /// The palette's fuzzy query (valid only in [`Mode::Palette`]).
    palette_query: String,
    /// The selected row in the palette's *filtered* result list (index into
    /// [`palette_matches`](Self::palette_matches), not into [`files`](Self::files)).
    /// Reset to 0 whenever the query changes.
    palette_sel: usize,
    /// Which step the palette is in ([`List`](PaletteStep::List) filter vs the
    /// `New file` filename input). `List` whenever the palette is closed.
    palette_step: PaletteStep,
    /// Tab-completion cycle state for the `> new file` step: the stem the user
    /// had typed before the first Tab, plus the position last shown (see
    /// [`new_file_complete`](Self::new_file_complete)). `None` until the first
    /// Tab, and reset by any edit to the name so a later Tab re-seeds from the
    /// text then present. Only meaningful in [`PaletteStep::NewFile`].
    new_file_completion: Option<(String, usize)>,
    /// The snippet library, fed by the host at boot via
    /// [`set_snippets`](Self::set_snippets) from `.typoena.snippets.json`. Empty
    /// until fed (and after a missing/malformed file). Drives inline
    /// Tab-expansion and the `$` palette.
    snippets: Vec<Snippet>,
    /// Active snippet tab-stop session: the byte offsets of the **remaining**
    /// stops to visit, in order, with the caret sitting on the current one. Empty
    /// when no session is running. On each Insert-mode edit the pending offsets
    /// shift by the edit's length delta (they are all at/after the caret), so they
    /// track the text; Tab pops the next one, and leaving Insert clears them.
    snippet_stops: Vec<usize>,
    /// Snapshot of the snippet name inline Tab would expand right now (the word
    /// before the caret is a prefix), or `None`. Refreshed by
    /// [`refresh_stats`](Self::refresh_stats) on the typing pause — the same
    /// throttle as the word count — so the panel hint never repaints per keystroke.
    snippet_hint: Option<String>,
    /// Focus mode (Pomodoro) session active. Orthogonal to [`mode`](Self::mode):
    /// during a focus block you keep editing in Normal/Insert/Visual, and this
    /// only drives the side-panel `focus` marker and gates the `:focus` toggle.
    /// The break itself is [`Mode::Rest`]. RAM-only — a session never survives a
    /// reboot. The block *timer* lives host-side (it needs a monotonic clock the
    /// pure core can't read); the host drives it via [`Effect::FocusStart`] /
    /// [`Effect::FocusStop`].
    pomodoro_on: bool,
    /// Words and minutes of the just-finished block, shown on the [`Mode::Rest`]
    /// card. Set by [`enter_rest`](Self::enter_rest) (the host computes them),
    /// cleared on leaving Rest. `None` outside Rest.
    rest_stats: Option<(usize, u32)>,
    /// Debug time-base: the host runs the focus block on a 25-**second** clock
    /// instead of 25 minutes, so the whole cycle is testable in seconds. Toggled
    /// by `:focusdebug`; the panel marker shows `(s)` while it is on.
    focus_debug: bool,
    /// Which destructive command a [`Mode::Confirm`] prompt is guarding, if any.
    /// Set with the mode by the `request_*` commands, taken by
    /// [`confirm_key`](Self::confirm_key). `None` outside Confirm.
    pending_confirm: Option<Confirm>,
    /// Today's date, fed by the host each key batch ([`set_today`](Self::set_today))
    /// from its real-time clock. `None` until the host has a trustworthy date (see
    /// [`Date`]); `:inbox` needs it to name/date the note and refuses while it is
    /// `None`.
    today: Option<Date>,
    /// The running firmware version, fed by the host at boot via
    /// [`set_version`](Self::set_version) — the pure core has no build metadata.
    /// Shown by `:about`; empty until fed (host tests leave it so).
    version: String,
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
            visual_anchor: None,
            cmdline: String::new(),
            cmd_prompt: ':',
            last_search: String::new(),
            shown_words: 0,
            keyboard_present: false,
            notice: None,
            prefs: Prefs::default(),
            register: String::new(),
            register_linewise: false,
            undo: Vec::new(),
            redo: Vec::new(),
            dot: Vec::new(),
            dot_recording: None,
            replaying: false,
            path: String::new(),
            scope: Scope::Tracked,
            dirty: false,
            parked: Vec::new(),
            requests: Vec::new(),
            file_blob: String::new(),
            file_spans: Vec::new(),
            recent: Vec::new(),
            palette_query: String::new(),
            palette_sel: 0,
            palette_step: PaletteStep::List,
            new_file_completion: None,
            snippets: Vec::new(),
            snippet_stops: Vec::new(),
            snippet_hint: None,
            pomodoro_on: false,
            rest_stats: None,
            focus_debug: false,
            pending_confirm: None,
            today: None,
            version: String::new(),
        }
    }

    /// Seed a fresh editor from previously saved text — the boot-load path
    /// (`storage.load()` → `Editor`). Boots in **Normal** mode (vim opens a file
    /// in Normal, not Insert) with the caret on the *last* character — the
    /// resume point — matching the Esc→Normal convention rather than sitting one
    /// cell past the end. The first [`Editor::draw`] scrolls it into view. An
    /// empty string is equivalent to [`Editor::new`].
    pub fn with_text(text: String) -> Self {
        Self::with_file(String::new(), Scope::Tracked, text)
    }

    /// Seed a fresh editor from a named file's saved text — the boot-load and
    /// file-open path. Same boot posture as [`with_text`](Self::with_text)
    /// (Normal mode, caret on the last character) but records the file's `path`
    /// and `scope` so `:w` knows where to persist and `:gp` knows whether
    /// Push is offered.
    pub fn with_file(path: String, scope: Scope, text: String) -> Self {
        let mut ed = Editor { text, path, scope, ..Editor::new() };
        ed.caret = ed.text.len();
        if ed.caret > ed.line_start(ed.caret) {
            ed.caret = ed.prev_char(ed.caret);
        }
        ed
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The full buffer contents, for the host to persist on `:w`/`:gp`.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Absolute path of the active buffer (empty for an unnamed scratch buffer).
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The active buffer's [`Scope`]. The host hides/greys `Ctrl-G` in Local.
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Whether the active buffer has unsaved edits.
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Whether *any* resident buffer (active or parked) has unsaved edits. Used
    /// by [`request_setup`](Self::request_setup): a `:setup` reboot would lose
    /// every unsaved buffer, not just the active one.
    pub fn any_dirty(&self) -> bool {
        self.dirty || self.parked.iter().any(|b| b.dirty)
    }

    /// Drain the queued host effects (save/load/push/pull). The main loop
    /// calls this after applying a key batch and services them in order.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        core::mem::take(&mut self.requests)
    }

    pub fn scroll_top(&self) -> usize {
        self.scroll_top
    }

    /// Recompute the throttled panel snapshots from the buffer: the word count and
    /// the inline-snippet hint. The main loop calls this on a typing pause and on
    /// non-Insert actions, so the panel stays current without repainting on every
    /// keystroke.
    pub fn refresh_stats(&mut self) {
        self.shown_words = self.word_count();
        self.snippet_hint = self.current_snippet_hint();
    }

    /// The snippet name inline Tab would expand at the caret right now, or `None`.
    /// Only in Insert mode outside a live tab-stop session (mid-session Tab
    /// advances stops, it doesn't expand), and only when the word immediately
    /// before the caret is exactly a snippet prefix — the same test
    /// [`try_expand_snippet`](Self::try_expand_snippet) uses. Snapshotted into
    /// [`snippet_hint`](Self::snippet_hint) by [`refresh_stats`](Self::refresh_stats).
    fn current_snippet_hint(&self) -> Option<String> {
        if self.mode != Mode::Insert || !self.snippet_stops.is_empty() {
            return None;
        }
        let (_, word) = self.word_before_caret()?;
        self.snippets
            .iter()
            .find(|s| s.prefix == word)
            .map(|s| s.name.clone())
    }

    /// Tell the editor whether a keyboard is attached (for the panel flag).
    pub fn set_keyboard_present(&mut self, present: bool) {
        self.keyboard_present = present;
    }

    /// Feed the editor today's date from the host clock — the pure core has none.
    /// Passed each key batch so a session crossing midnight (or one whose clock is
    /// only set mid-session by the first sync) always sees the current day. `None`
    /// means the host has no trustworthy date yet (unset clock); `:inbox` refuses
    /// in that case. See [`Date`].
    pub fn set_today(&mut self, date: Option<Date>) {
        self.today = date;
    }

    /// Post a transient side-panel notice ("snackbar") — e.g. the result of a
    /// save or push. Shown from the next [`Editor::draw`] until the next
    /// keystroke dismisses it (see [`Editor::handle`]). The host calls this from
    /// its `:` command effect handlers.
    pub fn set_notice(&mut self, msg: impl Into<String>) {
        self.notice = Some(msg.into());
    }

    /// The current preferences. The host reads this for the keys it honours
    /// (`save_on_idle` in the idle loop); `format_on_save` and `line_numbers`
    /// are consulted in-core.
    pub fn prefs(&self) -> &Prefs {
        &self.prefs
    }

    /// Apply the preferences the host read from [`PREFS_PATH`] at boot. Called
    /// before the first render so `line_numbers` shapes the first frame. A live
    /// change later comes from the palette `>` commands, not this.
    pub fn set_prefs(&mut self, prefs: Prefs) {
        self.prefs = prefs;
    }

    /// Install the snippet library the host parsed from [`SNIPPETS_PATH`] at boot
    /// (via [`Snippets::parse`]). Mirrors [`set_prefs`](Self::set_prefs): a
    /// missing or malformed file simply yields an empty library and no snippets.
    pub fn set_snippets(&mut self, snippets: Snippets) {
        self.snippets = snippets.0;
    }

    /// Record the running firmware version for `:about`. The pure core has no
    /// build metadata, so the firmware feeds it its `CARGO_PKG_VERSION` at boot
    /// (mirrors [`set_prefs`](Self::set_prefs)); host tests leave it empty.
    pub fn set_version(&mut self, version: impl Into<String>) {
        self.version = version.into();
    }

    /// Whitespace-delimited word count of the whole buffer. Public so the host
    /// can snapshot it at a focus block's start and diff it for the rest card.
    pub fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }

    /// Dispatch one decoded key event according to the current mode. Any host
    /// effect a `:` command (or a buffer switch) triggers is pushed to the queue
    /// drained by [`take_effects`](Self::take_effects); ordinary keys queue
    /// nothing.
    pub fn handle(&mut self, key: Key) {
        // Rest (the focus-mode break) is a full-screen modal: swallow every key
        // but `c`/`q`/`Esc`, resolved before the Cmd+S, notice-clear, and `.`
        // machinery so a break key can't save, dismiss a snackbar, or record a
        // repeatable change. The only ways out are back through `rest_key`.
        if self.mode == Mode::Rest {
            self.rest_key(key);
            return;
        }

        // A pending confirmation (`:delete`) is modal like Rest: resolve it
        // before the Cmd+S / notice-clear / `.` machinery so a stray key can't
        // slip a save or a repeat past the guard. `y`/`Y` proceeds; anything
        // else cancels — see `confirm_key`.
        if self.mode == Mode::Confirm {
            self.confirm_key(key);
            return;
        }

        // The `:about` splash is a full-screen modal too: resolve it before the
        // Cmd+S / notice-clear / `.` machinery so a leave key can't slip a save
        // or a repeat past the card. `Enter`/`q`/`Esc` leave — see `about_key`.
        if self.mode == Mode::About {
            self.about_key(key);
            return;
        }

        // Cmd+S — an explicit save from any mode, mirroring `:w`, resolved
        // before mode dispatch so it never changes mode nor gets recorded for
        // `.` (returns early). It is guarded by the dirty flag: a clean buffer
        // is already on the card, so a habitual repeat tap re-confirms "saved"
        // with no SD write (the write is `atomic_write`: tmp + fsync + rename,
        // real I/O and flash wear). `:w` stays unconditional for the rare
        // force-write; the idle auto-save (`save_on_idle`) already covers the
        // common case, so Cmd+S is mostly a reassuring confirmation gesture.
        if key == Key::Save {
            if self.dirty {
                self.write_active();
            } else {
                self.set_notice(if self.path.is_empty() { "no file name" } else { "saved" });
            }
            return;
        }

        // Any keystroke dismisses the transient notice ("snackbar"). The host
        // sets a fresh one *after* the key batch (on a `:` command's effect), so
        // a save/push message survives to the next draw, then clears the
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
            return;
        }

        // State before dispatch, so `record_dot` can read the transition a key
        // caused (entered Insert, started an operator, …).
        let before_mode = self.mode;
        let before_pending = self.pending_op.is_some() || self.pending_obj.is_some();

        match self.mode {
            Mode::Insert => self.insert_key(key),
            Mode::Normal => self.normal_key(key),
            Mode::Visual | Mode::VisualLine => self.visual_key(key),
            Mode::View => self.view_key(key),
            Mode::Command => self.command_key(key),
            Mode::Palette => self.palette_key(key),
            // All resolved before dispatch (above); listed for exhaustiveness.
            Mode::Rest => self.rest_key(key),
            Mode::About => self.about_key(key),
            Mode::Confirm => self.confirm_key(key),
        }

        // A snippet tab-stop session lives only in Insert. Leaving Insert — Esc,
        // or any mode change — ends it (the buffer is then just text, so Tab
        // inserts a tab again). The natural end (Tab past the last stop) already
        // empties `snippet_stops` while still in Insert.
        if !self.snippet_stops.is_empty() && self.mode != Mode::Insert {
            self.snippet_stops.clear();
        }

        // Cmd-p mid-change (e.g. during an insert session) aborts the `.`
        // recording — a palette round-trip (file switch, `>` command) is not a
        // repeatable edit, and replaying it from `.` would reopen the palette.
        if key == Key::Palette {
            self.dot_recording = None;
        }

        if !self.replaying {
            self.record_dot(key, before_mode, before_pending);
        }
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
        // A live snippet session (non-empty `snippet_stops`) makes Tab advance to
        // the next stop instead of inserting a tab, and needs its pending offsets
        // kept in step with any edit at the caret (below).
        let session = !self.snippet_stops.is_empty();
        let len_before = self.text.len();
        match key {
            Key::Char('\t') if session => self.snippet_advance(),
            // Tab expands a snippet if the word before the caret is a prefix;
            // otherwise it inserts spaces as before.
            Key::Char('\t') => {
                if !self.try_expand_snippet() {
                    self.insert_str(TAB);
                }
            }
            Key::Char(c) => self.insert_char(c),
            Key::Enter => self.insert_newline(),
            Key::Backspace => self.backspace(),
            Key::DeleteWord => self.delete_word_before(),
            Key::DeleteLine => self.delete_to_line_start(),
            // Half-page scroll and the Ctrl-n/Ctrl-p line motions are navigation
            // gestures — Normal/View only. In Insert they're a no-op rather than
            // yanking the caret off the text you're typing. Redo (Ctrl-r) is
            // likewise ignored here.
            Key::HalfPageDown | Key::HalfPageUp | Key::Redo | Key::Down | Key::Up
            | Key::FocusContinue | Key::FocusQuit => {}
            // Cmd-S is resolved in `handle` before mode dispatch (so it saves
            // without leaving Insert); unreachable here, but the match is
            // exhaustive.
            Key::Save => {}
            // Cmd-p works from every mode: acts like Esc (ending the insert
            // session, caret onto the last inserted char), then opens the
            // palette. Closing it lands in Normal, as Esc would have.
            Key::Palette => {
                self.mode = Mode::Normal;
                if self.caret > self.line_start(self.caret) {
                    self.caret = self.prev_char(self.caret);
                }
                self.open_palette();
            }
            Key::Escape => {
                self.mode = Mode::Normal;
                // vim drops the caret onto the last inserted char.
                if self.caret > self.line_start(self.caret) {
                    self.caret = self.prev_char(self.caret);
                }
            }
        }
        // Every pending stop sits at/after the caret, so an edit at the caret
        // shifts them all by its signed length delta — keeping `$2 … $0` correct
        // while you type at `$1`. (Tab-advance and Esc don't change the length.)
        if session && !self.snippet_stops.is_empty() {
            let delta = self.text.len() as isize - len_before as isize;
            if delta != 0 {
                for s in &mut self.snippet_stops {
                    *s = s.saturating_add_signed(delta);
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
            // Ctrl-n/Ctrl-p: move down/up a line (vim CTRL-N ≡ j, CTRL-P ≡ k),
            // honouring a leading count (`3<C-n>`) then abandoning the rest of any
            // pending command like a plain motion.
            Key::Down => {
                let n = self.count.max(1);
                self.reset_pending();
                self.move_by('j', n);
                return;
            }
            Key::Up => {
                let n = self.count.max(1);
                self.reset_pending();
                self.move_by('k', n);
                return;
            }
            // Ctrl-r redo: like any non-motion key it abandons a pending command.
            Key::Redo => {
                self.reset_pending();
                self.redo();
                return;
            }
            // Cmd-p: open the file palette (abandoning any pending command).
            Key::Palette => {
                self.reset_pending();
                self.open_palette();
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
                    let t = (0..n).fold(self.caret, |t, _| self.word_forward_pos(t));
                    self.apply_op(op, self.caret, t);
                }
                'b' => {
                    let t = (0..n).fold(self.caret, |t, _| self.word_back_pos(t));
                    self.apply_op(op, self.caret, t);
                }
                'e' => {
                    let t = (0..n).fold(self.caret, |t, _| self.word_end_pos(t));
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
            match c {
                'g' => self.caret = 0,
                // `gr` (go-read): enter the read-only View/scroll mode. `v`/`V`
                // used to trigger it but now belong to Visual selection.
                'r' => self.mode = Mode::View,
                _ => {}
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
            // Drop an anchor at the caret and enter Visual (charwise `v`) /
            // VisualLine (`V`); motions then extend the selection.
            'v' => {
                self.visual_anchor = Some(self.caret);
                self.mode = Mode::Visual;
            }
            'V' => {
                self.visual_anchor = Some(self.caret);
                self.mode = Mode::VisualLine;
            }
            ':' => {
                self.reset_pending();
                self.cmdline.clear();
                self.cmd_prompt = ':';
                self.mode = Mode::Command;
                return;
            }
            // `/` opens the same command line with a search prompt. The jump
            // happens on Enter only — no incremental caret-chasing; the e-ink
            // refresh cost rules that out (same call as the no-completion-popup
            // snippet decision).
            '/' => {
                self.reset_pending();
                self.cmdline.clear();
                self.cmd_prompt = '/';
                self.mode = Mode::Command;
                return;
            }
            // Any remaining char is either a shared motion (h/l/j/k/w/b/e/0/$/G)
            // or unknown; `move_by` applies the former and ignores the latter.
            _ => {
                self.move_by(c, n);
            }
        }
        self.count = 0;
    }

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
                if self.cmd_prompt == '/' {
                    self.execute_search();
                } else {
                    self.execute_command();
                }
                self.cmdline.clear();
                // Most commands return to Normal; one that opened another mode
                // (`:settings` → the palette) set it during `execute_command`, so
                // only fall back to Normal if we're still in Command.
                if self.mode == Mode::Command {
                    self.mode = Mode::Normal;
                }
            }
            Key::Escape => {
                self.cmdline.clear();
                self.mode = Mode::Normal;
            }
            // Cmd-p works from every mode: abandon the half-typed `:`/`/` line
            // (as Esc would) and open the palette.
            Key::Palette => {
                self.cmdline.clear();
                self.open_palette();
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
    }

    /// Run the typed `:` command, queuing any [`Effect`] the host must carry out.
    /// Unknown commands are silently ignored. The `:q` quit family is deliberately
    /// absent — an always-on writing appliance has nothing to quit to; `:wq`/`:x`
    /// therefore just save (the "quit" half is dropped).
    fn execute_command(&mut self) {
        let cmd = self.cmdline.trim().to_string();
        // `:enew <path>` — create a new file. (`:e` was retired in v0.6: bare
        // `Cmd-P` opens files, and `> new file` creates them.)
        if let Some(arg) = cmd.strip_prefix("enew ") {
            self.new_file(arg);
            return;
        }
        match cmd.as_str() {
            "enew" => self.set_notice("usage: :enew <file>"),
            "inbox" | "in" => self.open_inbox_today(),
            "oldest" | "old" => self.open_oldest_inbox(),
            "delete" | "d" => self.request_delete(),
            "settings" => self.open_settings(),
            "fmt" => self.format_buffer(),
            "pub" | "publish" => self.publish_active(),
            "w" | "wq" | "x" => self.write_active(),
            // fmt → save → push, shared with the `>` push command.
            "gp" => self.run_push(),
            "gl" => self.requests.push(Effect::Pull { commit_dirty: false }),
            "setup" => self.request_setup(),
            "reboot" => self.request_reboot(),
            "update" => self.request_update(),
            "about" => self.show_about(),
            "focus" => self.toggle_focus(),
            "focusdebug" => self.toggle_focus_debug(),
            _ => {}
        }
    }

    /// `:setup` / `> setup` — reboot into the onboarding wizard, behind a y/n
    /// prompt (the reboot drops in-RAM state, and the wizard's reset menu gates
    /// the card-wiping paths, so it earns a deliberate confirm). Refuses up
    /// front while anything is unsaved — the reboot would lose it — so we never
    /// prompt for a restart we'd then have to block. Save with `:w` first.
    pub(crate) fn request_setup(&mut self) {
        if self.any_dirty() {
            self.set_notice("unsaved changes - :w first");
            return;
        }
        self.enter_confirm(Confirm::Setup, "reopen setup? y/n");
    }

    /// `:reboot` (or a software reboot button) — restart the device, behind a
    /// y/n prompt. The restart drops the in-RAM buffers, so an *unnamed* dirty
    /// scratch (nowhere to save to) blocks it up front with a notice rather than
    /// prompting for a reboot that would lose the text. Named dirty buffers are
    /// saved on confirm, not here — see [`do_reboot`](Self::do_reboot).
    pub(crate) fn request_reboot(&mut self) {
        if self.has_unnamed_dirty() {
            self.set_notice("unnamed buffer - name it first");
            return;
        }
        self.enter_confirm(Confirm::Reboot, "reboot? y/n");
    }

    /// `:update` / `> update` — check for and install a firmware update over the
    /// air, behind a y/n prompt. Refuses up front while anything is unsaved: the
    /// install ends in a reboot that drops the in-RAM buffers, so — like
    /// [`request_setup`](Self::request_setup) — we never prompt for an update we'd
    /// then have to block. Save with `:w` first.
    pub(crate) fn request_update(&mut self) {
        if self.any_dirty() {
            self.set_notice("unsaved changes - :w first");
            return;
        }
        self.enter_confirm(Confirm::Update, "check for firmware update? y/n");
    }

    /// `:about` — raise the full-screen splash ([`Mode::About`]) with the product
    /// name and running firmware version (injected by the host via
    /// [`set_version`](Self::set_version)). Read-only; `Enter`/`q`/`Esc` leave.
    fn show_about(&mut self) {
        self.mode = Mode::About;
    }

    /// Dispatch a key on the `:about` splash ([`Mode::About`]). The card masks the
    /// whole screen, so only `Enter`/`q`/`Esc` do anything — they return to
    /// Normal — and every other key is swallowed (no editing behind the card).
    fn about_key(&mut self, key: Key) {
        if matches!(key, Key::Enter | Key::Escape | Key::Char('q')) {
            self.mode = Mode::Normal;
        }
    }

    /// The confirmed `:reboot`: auto-save every *named* dirty buffer
    /// ([`try_save_all_dirty`](Self::try_save_all_dirty)) so those
    /// [`Save`](Effect::Save)s are queued ahead of [`Reboot`](Effect::Reboot)
    /// and the host flushes them before it resets. The unnamed-dirty case was
    /// already refused at the prompt, so the guard here is belt-and-braces.
    fn do_reboot(&mut self) {
        if !self.try_save_all_dirty() {
            self.set_notice("unnamed buffer - name it first");
            return;
        }
        self.requests.push(Effect::Reboot);
    }

    /// The host calls this when a bare `:gl` found saved-but-unpushed work in
    /// the dirty journal: open a y/n prompt before committing it. A pull now
    /// folds that work into a local commit so the fetch can rebase it onto origin
    /// (rather than refuse), and that commit is user-visible — so it earns a
    /// confirm. On `y`, [`confirm_key`](Self::confirm_key) queues
    /// [`Effect::Pull`] `{ commit_dirty: true }`; any other key cancels.
    pub fn confirm_pull_commit(&mut self) {
        self.enter_confirm(Confirm::PullCommit, "unsynced saves - commit & pull? y/n");
    }

    /// Enter the [`Mode::Confirm`] y/n prompt for `what`, showing `prompt` on
    /// the snackbar. Shared by every destructive command that needs a confirm.
    fn enter_confirm(&mut self, what: Confirm, prompt: impl Into<String>) {
        self.pending_confirm = Some(what);
        self.mode = Mode::Confirm;
        self.set_notice(prompt);
    }

    fn reset_pending(&mut self) {
        self.count = 0;
        self.pending_op = None;
        self.pending_obj = None;
        self.pending_g = false;
    }

    // --- Undo / redo -------------------------------------------------------

    fn view_key(&mut self, key: Key) {
        match key {
            // j/k and Ctrl-n/Ctrl-p both step one row (View is a pure viewport).
            Key::Char('j') | Key::Down => self.scroll_top += 1, // clamped in draw()
            Key::Char('k') | Key::Up => self.scroll_top = self.scroll_top.saturating_sub(1),
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
            // Cmd-p works from every mode; leaving View for the palette (and
            // then Normal on close), exactly as Esc-then-Cmd-p would.
            Key::Palette => {
                self.pending_g = false;
                self.open_palette();
            }
            _ => {}
        }
    }

    // --- Focus mode (Pomodoro) ---------------------------------------------

    /// Whether a focus session is running (drives the panel `focus` marker).
    pub fn pomodoro_on(&self) -> bool {
        self.pomodoro_on
    }

    /// Whether the debug (seconds-not-minutes) time-base is on. The host reads
    /// this to scale the block length and the displayed figure.
    pub fn focus_debug(&self) -> bool {
        self.focus_debug
    }

    /// `:focus` — toggle the focus session. On starts a block
    /// ([`Effect::FocusStart`]); off ends it ([`Effect::FocusStop`]). Only
    /// reachable outside Rest (Rest swallows `:`), so it never fights the break.
    fn toggle_focus(&mut self) {
        self.pomodoro_on = !self.pomodoro_on;
        if self.pomodoro_on {
            self.requests.push(Effect::FocusStart);
            self.set_notice("focus on");
        } else {
            self.requests.push(Effect::FocusStop);
            self.set_notice("focus off");
        }
    }

    /// `:focusdebug` — flip the debug time-base (25 **seconds** per block, so the
    /// whole cycle is testable in seconds). Independent of whether a session is
    /// running; the host picks up the new base on its next timer check. Queues no
    /// effect — the host reads [`focus_debug`](Self::focus_debug) directly.
    fn toggle_focus_debug(&mut self) {
        self.focus_debug = !self.focus_debug;
        self.set_notice(if self.focus_debug { "focus: debug (s)" } else { "focus: normal" });
    }

    /// Drop the Rest curtain: the host calls this when a running block reaches
    /// its length at a typing pause. `words`/`mins` are the finished block's
    /// stats (computed host-side from the monotonic clock and the word-count
    /// delta) and are shown on the card. The session stays on — `c` starts the
    /// next block, `q`/`Esc` ends it (see [`rest_key`](Self::rest_key)). Ends any
    /// in-progress change recording / snippet session, like leaving Insert would.
    pub fn enter_rest(&mut self, words: usize, mins: u32) {
        self.rest_stats = Some((words, mins));
        self.mode = Mode::Rest;
        self.dot_recording = None;
        self.snippet_stops.clear();
    }

    /// Dispatch a key in [`Mode::Rest`]. The break masks the whole screen, so
    /// only the two chords do anything: `Ctrl-C` continues to the next block and
    /// `Ctrl-Q` quits the session. Every other key — including a bare `c`/`q`
    /// and `Esc` — is swallowed, so no idle keypress can end a break or a
    /// session, and the reader can't edit the hidden buffer. Both exits are
    /// chords on purpose (a stray single key is too easy behind the curtain).
    fn rest_key(&mut self, key: Key) {
        match key {
            Key::FocusContinue => {
                self.mode = Mode::Normal;
                self.rest_stats = None;
                self.requests.push(Effect::FocusStart); // next block
            }
            Key::FocusQuit => {
                self.mode = Mode::Normal;
                self.rest_stats = None;
                self.pomodoro_on = false;
                self.requests.push(Effect::FocusStop);
            }
            _ => {} // swallowed — no editing behind the curtain
        }
    }

    /// Dispatch a key while a destructive command waits for confirmation
    /// ([`Mode::Confirm`] — `:delete`, `:reboot`, `:setup`, or `:gl`'s
    /// commit-unsynced-and-pull). Leaves Confirm
    /// either way: a deliberate `y`/`Y` runs the [`pending_confirm`](Self::pending_confirm)
    /// action, and every other key — `n`, `Esc`, a stray character — cancels
    /// with a notice. Cancel is the default so a fat-fingered command never
    /// deletes a file or restarts the device on its own.
    fn confirm_key(&mut self, key: Key) {
        self.mode = Mode::Normal;
        let what = self.pending_confirm.take();
        if matches!(key, Key::Char('y') | Key::Char('Y')) {
            self.notice = None; // the resulting effect's outcome replaces the prompt
            match what {
                Some(Confirm::Delete) => self.delete_current(),
                Some(Confirm::Reboot) => self.do_reboot(),
                Some(Confirm::Setup) => self.requests.push(Effect::Setup),
                Some(Confirm::Update) => self.requests.push(Effect::Update),
                Some(Confirm::PullCommit) => self.requests.push(Effect::Pull { commit_dirty: true }),
                None => {}
            }
        } else {
            self.set_notice(match what {
                Some(Confirm::Reboot) => "reboot cancelled",
                Some(Confirm::Setup) => "setup cancelled",
                Some(Confirm::Update) => "update cancelled",
                Some(Confirm::PullCommit) => "pull cancelled",
                _ => "delete cancelled",
            });
        }
    }

    // --- Motions (all on the logical buffer) -------------------------------

}

#[cfg(test)]
mod tests;
