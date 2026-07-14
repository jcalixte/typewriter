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
const WRITE_COLS: usize = 63;
/// Minimum digit columns in the line-number gutter (before the 1-col separator).
/// Files up to 99 lines still get a 2-wide gutter so short notes don't jitter.
const GUTTER_MIN_DIGITS: usize = 2;
/// Visible writing rows. 13 × 20 px = 260 px. The transient `:` command line is
/// drawn at body size over the **bottom** writing row (see [`Editor::draw_cmdline`]),
/// so no rows are permanently reserved for it.
const ROWS: usize = (HEIGHT / 20) as usize; // 13
/// Half-page scroll distance for `Ctrl-d`/`Ctrl-u`, in **display rows** — vim's
/// `'scroll'` default (half the visible window). Fixed, not configurable: a
/// resizable `'scroll'` is meaningless on a fixed 13-row panel.
const HALF_PAGE: usize = ROWS / 2; // 6
/// x of the 1 px rule dividing writing column from side panel, and the left edge
/// of panel text (a small gutter past the rule).
const DIVIDER_X: i32 = WRITE_COLS as i32 * CW; // 630
const PANEL_X: i32 = DIVIDER_X + 8; // 638
/// Side-panel font cell: **FONT_9X15** — a middle size between the old squint-y
/// 6×10 and the body 10×20. Legible metadata without eating as many columns as
/// the body font would (the `:` command line, being text you type, stays at the
/// body 10×20 — see [`Editor::draw_cmdline`]). Kept as its own pair (not reusing
/// `CW`/`CH`) so the panel font tunes independently of the writing font; change
/// these **and** the `MonoTextStyle` font in `draw_panel` together.
const PANEL_CW: i32 = 9;
const PANEL_CH: i32 = 15;
/// Side-panel text width in [`PANEL_CW`]-px columns, for clamping panel strings —
/// the snackbar notice, word count — so they never draw past the right edge of
/// the panel.
const PANEL_COLS: usize = (WIDTH as usize - PANEL_X as usize) / PANEL_CW as usize; // 15
/// Max wrapped lines the snackbar draws under the word count, so a long notice
/// can't run down into the bottom mode strip. Four PANEL_CH rows ≈ 60 chars,
/// enough for any current message.
const NOTICE_MAX_LINES: usize = 4;
/// Tab stop, in spaces. Tabs never enter the buffer — they expand on insert so
/// the buffer stays 1 char = 1 column.
const TAB: &str = "    ";

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
    /// (which ask the host to persist/publish via an [`Effect`]).
    Command,
    /// File palette (`Cmd-P`) — a modal transient panel over the writing column.
    /// Typing fuzzy-filters the file list ([`Editor::set_file_list`]); `Ctrl-n`/
    /// `Ctrl-p` move the selection, Enter opens it, Esc (or `Cmd-P` again)
    /// cancels. See [`Editor::palette_key`].
    Palette,
}

/// Which of the two file scopes ([`CONTEXT.md`]) a buffer belongs to. Fixed at
/// creation — there is no move-between-scopes operation. **Tracked** files live
/// under [`REPO_DIR`] and can be Published (`:gp`); **Local** files live under
/// [`LOCAL_DIR`] and never leave the device, so `:gp` is refused in-core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Tracked,
    Local,
}

/// A side effect the host (firmware) must carry out. The editor core is pure and
/// does no IO, so persistence, publishing, and file reads can't happen here —
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
    /// `:gp` — publish the Tracked working copy (git push). Preceded by a
    /// [`Save`](Effect::Save) of the current buffer in the same batch. Never
    /// queued from a Local buffer (blocked in-core).
    Publish,
    /// `:gl` — pull from the remote: fetch, then **fast-forward only**. The host
    /// refuses (and surfaces) a divergence rather than merging, and never
    /// touches local commits. Complements `:gp` (push) as the download half.
    Pull,
    /// `:delete` — unlink `path` from the card. For a **Tracked** file the removal
    /// lands in the git working copy, so the next [`Publish`](Effect::Publish)'s
    /// `add --all` stages the deletion (no eager `git rm` needed); a **Local** file
    /// is just unlinked. The editor has already dropped the file from its model and
    /// switched away by the time this drains, so `scope` is informational; the host
    /// reports the outcome on the snackbar (mirrors [`Save`](Effect::Save)).
    Delete { path: String, scope: Scope },
    /// Persist the preferences file ([`PREFS_PATH`]) after a palette `>` command
    /// changed a pref. Carries the already-serialized TOML ([`Prefs::to_toml`]),
    /// so the host only does the atomic write — no re-serialization or buffer
    /// bookkeeping. Separate from [`Save`](Effect::Save): prefs are not a text
    /// buffer and live at a fixed path outside the multi-buffer model.
    SavePrefs { contents: String },
}

/// Tracked files live here (the git working copy).
pub const REPO_DIR: &str = "/sd/repo";
/// Local files live here (never published).
pub const LOCAL_DIR: &str = "/sd/local";
/// The git-tracked preferences file. Read at boot and rewritten when a palette
/// `>` command changes a pref, so the setting survives a reboot and rides the
/// next `:gp` to every device that clones the repo. Deliberately **distinct**
/// from the gitignored `/sd/typoena.conf` device secrets (Wi-Fi / PAT / remote /
/// author, never committed — see v0.1): behaviour is shared, secrets are not.
pub const PREFS_PATH: &str = "/sd/repo/.typoena.toml";

/// Editor preferences, mirroring the git-tracked [`PREFS_PATH`] TOML. The host
/// reads the file at boot and applies it with [`Editor::set_prefs`]; the palette
/// `>` command mode toggles a pref live and queues an [`Effect::SavePrefs`] to
/// write the change back. Every key falls back to the [`Default`] below, so a
/// missing, empty, or partial file still yields a full, usable `Prefs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefs {
    /// Auto-save the active buffer on the idle typing-pause, so `:w` becomes
    /// optional. The idle save is **unformatted** — a safety net against power
    /// loss, not a formatting pass; `:fmt` only runs on an explicit `:w`/`:gp`
    /// (see [`format_on_save`](Prefs::format_on_save)) so text is never reflowed
    /// mid-session. Honoured by the host loop, not the core.
    pub save_on_idle: bool,
    /// Run `:fmt` (table alignment, blank-line collapse, trailing-whitespace
    /// strip) on the buffer before an explicit `:w`/`:gp` persist.
    pub format_on_save: bool,
    /// Show the absolute line-number gutter (built always-on in v0.2). Off
    /// reclaims the gutter's columns for text — applied live by [`gutter_cols`].
    pub line_numbers: bool,
    /// Panel colour polarity: `"light"` (native black-ink-on-white-paper) or
    /// `"dark"` (white-on-black). On the 1-bit panel this is a whole-frame invert
    /// applied at the end of [`draw`](Editor::draw) via [`Frame::invert`], so any
    /// value other than `"dark"` reads as light. The palette rotates it through
    /// [`THEME_OPTIONS`]; a hand-typed value still round-trips.
    pub theme: String,
    /// Max-staleness cap for opportunistic auto-publish, as a duration string.
    /// The palette rotates it through [`AUTO_SYNC_OPTIONS`] (`"2m"`..`"30m"`);
    /// hand-editing can still set any string. **Persisted-but-inert in v0.5** —
    /// the periodic push that reads it rides v0.7/v0.8, so cycling it changes the
    /// stored/displayed value but triggers nothing yet.
    pub auto_sync: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            save_on_idle: true,
            format_on_save: true,
            line_numbers: true,
            theme: "light".into(),
            auto_sync: "10m".into(),
        }
    }
}

impl Prefs {
    /// Parse a [`PREFS_PATH`] file, falling back to [`Default`] for any missing or
    /// unrecognized key (so a partial or empty file still yields a full `Prefs`).
    /// A deliberately tiny line-based reader: these are flat `key = value` pairs
    /// (bool, or a quoted string) with `#` comments — not a general TOML parser,
    /// so it pulls no crate onto the xtensa build and stays host-testable here. An
    /// unparseable value for a key leaves that key at its default.
    pub fn parse(src: &str) -> Self {
        let mut p = Self::default();
        for line in src.lines() {
            // Strip a trailing/whole-line `#` comment, then split `key = value`.
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((key, val)) = line.split_once('=') else {
                continue;
            };
            let (key, val) = (key.trim(), val.trim());
            match key {
                "save_on_idle" => {
                    if let Some(b) = parse_bool(val) {
                        p.save_on_idle = b;
                    }
                }
                "format_on_save" => {
                    if let Some(b) = parse_bool(val) {
                        p.format_on_save = b;
                    }
                }
                "line_numbers" => {
                    if let Some(b) = parse_bool(val) {
                        p.line_numbers = b;
                    }
                }
                "theme" => p.theme = val.trim_matches('"').to_string(),
                "auto_sync" => p.auto_sync = val.trim_matches('"').to_string(),
                _ => {}
            }
        }
        p
    }

    /// Serialize back to the [`PREFS_PATH`] form, with a header comment pointing at
    /// both edit paths. Round-trips with [`parse`](Prefs::parse). Ends in a newline
    /// like a normal text file; `save_path`'s guarded final-newline write leaves it
    /// as exactly one.
    pub fn to_toml(&self) -> String {
        format!(
            "# Typoena editor preferences — hand-editable, git-tracked.\n\
             # Edit here, or change live from the Cmd-P palette (type `>`).\n\
             save_on_idle = {}\n\
             format_on_save = {}\n\
             line_numbers = {}\n\
             theme = \"{}\"\n\
             auto_sync = \"{}\"\n",
            self.save_on_idle,
            self.format_on_save,
            self.line_numbers,
            self.theme,
            self.auto_sync,
        )
    }
}

/// Parse a TOML boolean literal, or `None` for anything else (so a typo leaves
/// the key at its default rather than silently reading as `false`).
fn parse_bool(v: &str) -> Option<bool> {
    match v {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// The panel-polarity presets the palette rotates [`Prefs::theme`] through.
const THEME_OPTIONS: [&str; 2] = ["light", "dark"];

/// The auto-publish intervals the palette rotates [`Prefs::auto_sync`] through.
/// Hand-editing the TOML can still set any duration string; these are just the
/// values the `>` palette cycles.
const AUTO_SYNC_OPTIONS: [&str; 5] = ["2m", "5m", "10m", "15m", "30m"];

/// The option after `current` in `options`, wrapping past the end — the
/// rotate-on-Enter for a preset string pref. A `current` that isn't in the list
/// (e.g. hand-typed into the TOML) snaps to the first option, so one Enter
/// always lands on a known value.
fn next_option<'a>(current: &str, options: &[&'a str]) -> &'a str {
    match options.iter().position(|&o| o == current) {
        Some(i) => options[(i + 1) % options.len()],
        None => options[0],
    }
}

/// The git-tracked snippet library, read at boot like [`PREFS_PATH`]. The host
/// reads this file and hands the parsed list to [`Editor::set_snippets`]; a
/// missing or malformed file is non-fatal (no snippets, editor runs). It lives in
/// the Tracked repo so the library syncs across devices. Full format reference:
/// `docs/typoena-snippets.md`.
pub const SNIPPETS_PATH: &str = "/sd/repo/.typoena.snippets.json";

/// One snippet: an inline trigger [`prefix`](Snippet::prefix), a
/// [`body`](Snippet::body) carrying `$1..$n`/`$0` tab stops, and a
/// [`name`](Snippet::name)/[`description`](Snippet::description) the `$` palette
/// shows. Parsed from the Zed-compatible JSON by [`Snippets::parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    /// Display name — the top-level JSON key. What the `$` palette lists.
    pub name: String,
    /// The word that triggers inline Tab-expansion.
    pub prefix: String,
    /// Literal body text with `$1..$n`/`$0` stops. `${n:label}` placeholders are
    /// stripped to bare `$n` at parse time (no completion popup to show a label,
    /// no overtype model to fill it) — see [`strip_stop_labels`].
    pub body: String,
    /// Human description; the `$` palette fuzzy-matches and shows it. Empty if the
    /// JSON entry omits it.
    pub description: String,
}

/// A parsed snippet library. [`parse`](Snippets::parse) reads the Zed JSON shape;
/// [`Editor::set_snippets`] installs it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snippets(pub Vec<Snippet>);

/// The Zed JSON value for one snippet: `body` is a string or an array of lines.
#[derive(serde::Deserialize)]
struct RawSnippet {
    prefix: String,
    body: RawBody,
    #[serde(default)]
    description: String,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum RawBody {
    /// `"body": ["line", "line"]` — Zed's multi-line form (joined with `\n`).
    Lines(Vec<String>),
    /// `"body": "one line"`.
    Text(String),
}

impl Snippets {
    /// Parse the Zed-compatible `.typoena.snippets.json`: an object keyed by
    /// display name, each value `{ prefix, body, description? }` where `body` is a
    /// string or an array of lines. Labels in `${n:label}` stops are stripped to
    /// `$n`. Entries come back sorted by name (a `BTreeMap` parse — deterministic,
    /// and the empty-`$` palette order; a query re-ranks by fuzzy score anyway). An
    /// empty (or whitespace-only) file means "no snippets", same as a missing one —
    /// only a malformed file is an `Err` the host logs before booting with none.
    pub fn parse(src: &str) -> Result<Self, serde_json::Error> {
        if src.trim().is_empty() {
            return Ok(Self::default());
        }
        let raw: std::collections::BTreeMap<String, RawSnippet> = serde_json::from_str(src)?;
        let snippets = raw
            .into_iter()
            .map(|(name, r)| {
                let body = match r.body {
                    RawBody::Text(s) => s,
                    RawBody::Lines(v) => v.join("\n"),
                };
                Snippet {
                    name,
                    prefix: r.prefix,
                    body: strip_stop_labels(&body),
                    description: r.description,
                }
            })
            .collect();
        Ok(Self(snippets))
    }
}

/// Rewrite `${n:label}` (and `${n}`) tab stops to a bare `$n`, leaving plain
/// `$n`/`$0` and every other `$` untouched. The editor has no completion popup to
/// surface a label and no selection/overtype model to fill one, so the label is
/// only noise to delete — dropping it is what lets a Zed snippet file with
/// `${1:Titre}` load unchanged. Byte-indexed but UTF-8-safe: it only ever indexes
/// the ASCII `$ { } :` and digits; any multi-byte char is copied whole.
fn strip_stop_labels(body: &str) -> String {
    let b = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    while i < body.len() {
        if b[i] == b'$' && i + 1 < body.len() && b[i + 1] == b'{' {
            // Read the digits right after "${".
            let mut j = i + 2;
            while j < body.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            // A numbered placeholder `${<digits>…}` → `$<digits>`, dropping the rest.
            if j > i + 2 {
                if let Some(close) = body[j..].find('}') {
                    out.push('$');
                    out.push_str(&body[i + 2..j]);
                    i = j + close + 1;
                    continue;
                }
            }
            // Not a numbered placeholder (or unclosed) — emit the `$` literally.
            out.push('$');
            i += 1;
            continue;
        }
        let c = body[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// Split a snippet body into its literal text (tab-stop markers removed) and the
/// caret **visit order** of those stops as byte offsets into that literal: `$1 …
/// $n` ascending, then `$0` last. If the body has numbered stops but no explicit
/// `$0`, a final stop at the body end is appended (so the last Tab lands past the
/// text). A body with no stops returns an empty stop list (the caret just lands at
/// the end — no session). `$` not followed by a digit is literal.
fn parse_snippet_body(body: &str) -> (String, Vec<usize>) {
    let b = body.as_bytes();
    let mut literal = String::with_capacity(body.len());
    let mut stops: Vec<(u32, usize)> = Vec::new(); // (stop number, offset in `literal`)
    let mut i = 0;
    while i < body.len() {
        if b[i] == b'$' {
            let mut j = i + 1;
            while j < body.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 {
                let num: u32 = body[i + 1..j].parse().unwrap_or(0);
                stops.push((num, literal.len()));
                i = j;
                continue;
            }
            literal.push('$'); // a lone `$` (e.g. a price) is literal text
            i += 1;
            continue;
        }
        let c = body[i..].chars().next().unwrap();
        literal.push(c);
        i += c.len_utf8();
    }
    // Ensure a final resting stop: `$0` if present, else an implicit one at the end.
    if !stops.is_empty() && !stops.iter().any(|&(n, _)| n == 0) {
        stops.push((0, literal.len()));
    }
    // Visit order: $1..$n ascending, $0 (the final rest) last.
    stops.sort_by_key(|&(n, _)| if n == 0 { u32::MAX } else { n });
    (literal, stops.into_iter().map(|(_, off)| off).collect())
}

/// Resolve a `:e`/`:enew` argument (or palette pick) to an absolute path +
/// [`Scope`]. Everything the writer can reach lives on the card under `/sd`, so
/// the `/sd` prefix is **optional**: `/sd/repo/x`, `/repo/x`, and `repo/x` all
/// name the same file, and nothing resolves outside `/sd`. The arg is normalized
/// to a scope-relative form (peel an optional `/sd`, then an optional leading
/// `/`), then:
/// - a leading `local/` or `repo/` segment **selects the scope** and names the
///   file in it — the same labels the palette shows (`local/journal.md`,
///   `repo/notes.md`), so a name read off the palette is typeable verbatim. Safe
///   because scopes are flat: there are no real `local/`/`repo/` subdirectories;
/// - otherwise a bare name joins the **current** buffer's scope directory, so
///   `:e draft.md` opens a sibling of the file you're in.
fn resolve_path(arg: &str, current: Scope) -> (String, Scope) {
    // Peel the optional `/sd` prefix, then an optional leading `/`, leaving a
    // scope-relative remainder (`repo/…`, `local/…`, or a bare name).
    let rel = arg
        .strip_prefix("/sd/")
        .or_else(|| arg.strip_prefix('/'))
        .unwrap_or(arg);
    if let Some(name) = rel.strip_prefix("local/") {
        (format!("{LOCAL_DIR}/{name}"), Scope::Local)
    } else if let Some(name) = rel.strip_prefix("repo/") {
        (format!("{REPO_DIR}/{name}"), Scope::Tracked)
    } else {
        let dir = match current {
            Scope::Tracked => REPO_DIR,
            Scope::Local => LOCAL_DIR,
        };
        (format!("{dir}/{rel}"), current)
    }
}

/// Word-wrap `text` to lines of at most `width` characters, for the side-panel
/// snackbar. Packs whole words greedily; a word longer than `width` is hard-split
/// across lines (so a long path or oid still shows in full rather than being
/// truncated). Empty input yields no lines.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
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

/// Fuzzy-match `query` against `text` (the file palette's matcher). Returns a
/// relevance score if every `query` character appears in `text` in order
/// (a subsequence match, case-insensitive over ASCII), else `None`. Higher is
/// better; a higher score is a "tighter" match.
///
/// Scoring rewards two things prose filenames make meaningful: a match at a
/// **word boundary** (start of string, or just after `/ _ - . space`) scores far
/// above one mid-word, and a **run** of consecutive matches scores extra per
/// char. So typing `notes` ranks `repo/notes.md` above `promo-tests.md` even
/// though both contain the letters. There are no penalties, so a score is always
/// ≥ the query length; ties are broken by the caller (recency, then list order).
/// An empty query matches everything with score 0.
fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query.chars().collect();
    if q.is_empty() {
        return Some(0);
    }
    let mut qi = 0;
    let mut score = 0i32;
    let mut prev_matched = false;
    let mut prev: Option<char> = None;
    for (i, tc) in text.chars().enumerate() {
        if qi < q.len() && tc.eq_ignore_ascii_case(&q[qi]) {
            score += 1;
            let boundary =
                i == 0 || matches!(prev, Some('/') | Some('_') | Some('-') | Some('.') | Some(' '));
            if boundary {
                score += 10;
            }
            if prev_matched {
                score += 5;
            }
            qi += 1;
            prev_matched = true;
        } else {
            prev_matched = false;
        }
        prev = Some(tc);
    }
    (qi == q.len()).then_some(score)
}

/// The palette's display label for an absolute path: `/sd/` stripped, so
/// `/sd/repo/notes.md` shows as `repo/notes.md` and `/sd/local/journal.md` as
/// `local/journal.md`. The scope dir (`repo`/`local`) stays, which both
/// disambiguates same-named files across scopes and reads as a scope tag. A path
/// not under `/sd/` is shown verbatim. Matching (`fuzzy_score`) runs on this
/// label, so you can filter by scope (`local`) or subpath, not just basename.
fn palette_label(path: &str) -> &str {
    path.strip_prefix("/sd/").unwrap_or(path)
}

/// A `>` palette command — a real action registry, not a settings box (v0.6).
/// Three dispatch shapes, distinguished by [`PaletteCmd::kind`]:
/// - a **[one-shot](CmdKind::OneShot)** ([`Format`](PaletteCmd::Format),
///   [`Publish`](PaletteCmd::Publish)) runs and closes the palette;
/// - a **[parameterised](CmdKind::Param)** command ([`NewFile`](PaletteCmd::NewFile))
///   morphs the palette into a filename input step;
/// - a **[toggle](CmdKind::Toggle)** — the boolean prefs and the
///   [`Theme`](PaletteCmd::Theme)/[`AutoSync`](PaletteCmd::AutoSync) rotations —
///   applies live and keeps the list open, so several settings flip in a row. Each
///   toggle's *label* carries the pref's current state ([`Editor::command_label`]),
///   so the list still doubles as a settings readout. `auto_sync` has no behaviour
///   yet (v0.7); cycling it only changes the stored/displayed value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaletteCmd {
    NewFile,
    Format,
    Publish,
    SaveOnIdle,
    FormatOnSave,
    LineNumbers,
    Theme,
    AutoSync,
}

/// How a [`PaletteCmd`] behaves on Enter — see [`PaletteCmd::kind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CmdKind {
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
            PaletteCmd::Format | PaletteCmd::Publish => CmdKind::OneShot,
            _ => CmdKind::Toggle,
        }
    }
}

/// The palette command list, in display order (empty `>` query shows them all):
/// the actions first, the settings after.
const PALETTE_CMDS: [PaletteCmd; 8] = [
    PaletteCmd::NewFile,
    PaletteCmd::Format,
    PaletteCmd::Publish,
    PaletteCmd::SaveOnIdle,
    PaletteCmd::FormatOnSave,
    PaletteCmd::LineNumbers,
    PaletteCmd::Theme,
    PaletteCmd::AutoSync,
];

/// Which step the palette is showing. Most of its life it is a
/// [`List`](PaletteStep::List) — files, `>` commands, or `$` snippets, chosen by
/// the query's leading sigil. Selecting a [parameterised](CmdKind::Param) `>`
/// command switches it to an input step ([`NewFile`](PaletteStep::NewFile)), where
/// the query is a value (a filename) rather than a filter, and Enter commits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaletteStep {
    List,
    NewFile,
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
    /// (save/publish result). Shown until the next keystroke dismisses it
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
    /// The active buffer's scope. Gates Publish — `:gp` is refused in Local.
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
}

/// A resident-but-inactive buffer: everything needed to restore a file's editing
/// state when the user switches back, without re-reading the disk. The active
/// buffer holds these same fields inline on [`Editor`]; parking marshals them
/// out to here, activation marshals them back.
struct Buffer {
    path: String,
    scope: Scope,
    text: String,
    caret: usize,
    scroll_top: usize,
    dirty: bool,
    undo: Vec<(String, usize)>,
    redo: Vec<(String, usize)>,
}

/// Buffers kept resident at once — the active one plus [`MAX_RESIDENT`] − 1
/// parked (v0.5 keeps ≤ 3). Beyond this the least-recently-used parked buffer is
/// evicted; it is saved first if dirty, so an evicted buffer is never lost.
const MAX_RESIDENT: usize = 3;

/// Recent-files (MRU) list length — how many opens the palette remembers; they
/// are the whole result list below [`PALETTE_MIN_QUERY`] chars and float to the
/// top above it. Far more than [`MAX_RESIDENT`] (recency
/// outlives residency: a file evicted from memory is still recently *used*), but
/// bounded so the list can't grow without limit over a long session.
const MRU_MAX: usize = 16;

/// Query length (chars) at which the file palette searches the full file list.
/// Shorter queries show only the recents ([`MRU_MAX`]) — the list is a
/// recursive walk of the card, and one char can't rank hundreds of paths
/// usefully. `>` commands and `$` snippets are short curated lists, so the
/// threshold does not apply to them.
const PALETTE_MIN_QUERY: usize = 2;

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
            snippets: Vec::new(),
            snippet_stops: Vec::new(),
            snippet_hint: None,
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
    /// Publish is offered.
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

    /// Drain the queued host effects (save/load/publish/pull). The main loop
    /// calls this after applying a key batch and services them in order.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        core::mem::take(&mut self.requests)
    }

    /// The host confirms `path` was persisted; clear its dirty flag wherever that
    /// buffer is resident (active or parked). A no-op for a path that is no longer
    /// in memory (already-evicted buffers were saved on the way out).
    pub fn mark_saved(&mut self, path: &str) {
        if self.path == path {
            self.dirty = false;
        }
        if let Some(b) = self.parked.iter_mut().find(|b| b.path == path) {
            b.dirty = false;
        }
    }

    /// Install a file the host read from disk in response to an [`Effect::Load`]:
    /// park the current buffer and make the loaded one active. If the target
    /// turned resident in the meantime, switch to that copy instead (its in-memory
    /// edits win over a stale disk read).
    pub fn install_loaded(&mut self, path: String, scope: Scope, contents: String) {
        if path == self.path {
            return;
        }
        if self.parked.iter().any(|b| b.path == path) {
            self.open_path(path, scope);
            return;
        }
        self.park_active();
        self.set_active(path, scope, contents);
    }

    /// Replace the active buffer's contents after the file changed on disk
    /// underneath us — a `:gl` pull fast-forwarded the working copy. Same boot
    /// posture as a fresh load (Normal, caret on the last char, clean, no undo
    /// history — the old snapshots reference the replaced text). The host only
    /// calls this when the buffer is clean; a dirty buffer's RAM edits win
    /// (last-writer-wins, like the reconcile path).
    pub fn refresh_active(&mut self, contents: String) {
        let (path, scope) = (self.path.clone(), self.scope);
        self.set_active(path, scope, contents);
    }

    /// Drop every *clean* parked buffer, so the next switch to one re-reads the
    /// disk ([`Effect::Load`]) instead of resurrecting a stale resident copy —
    /// a `:gl` pull may have rewritten any tracked file. Dirty parked buffers
    /// are kept: their unsaved edits win over the pulled state, exactly like
    /// the active buffer's.
    pub fn drop_clean_parked(&mut self) {
        self.parked.retain(|b| b.dirty);
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

    /// Post a transient side-panel notice ("snackbar") — e.g. the result of a
    /// save or publish. Shown from the next [`Editor::draw`] until the next
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

    /// Whitespace-delimited word count of the whole buffer.
    fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }

    /// Dispatch one decoded key event according to the current mode. Any host
    /// effect a `:` command (or a buffer switch) triggers is pushed to the queue
    /// drained by [`take_effects`](Self::take_effects); ordinary keys queue
    /// nothing.
    pub fn handle(&mut self, key: Key) {
        // Any keystroke dismisses the transient notice ("snackbar"). The host
        // sets a fresh one *after* the key batch (on a `:` command's effect), so
        // a save/publish message survives to the next draw, then clears the
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
        }

        // A snippet tab-stop session lives only in Insert. Leaving Insert — Esc,
        // or any mode change — ends it (the buffer is then just text, so Tab
        // inserts a tab again). The natural end (Tab past the last stop) already
        // empties `snippet_stops` while still in Insert.
        if !self.snippet_stops.is_empty() && self.mode != Mode::Insert {
            self.snippet_stops.clear();
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
            // yanking the caret off the text you're typing. Redo (Ctrl-r) and the
            // palette (Cmd-p) are likewise ignored here.
            Key::HalfPageDown | Key::HalfPageUp | Key::Redo | Key::Palette | Key::Down
            | Key::Up => {}
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

    /// Apply a plain caret motion shared by Normal and Visual — `h l j k`,
    /// `w b e`, `0 $`, `G` — `n` times, returning whether `c` was a motion (and
    /// so consumed). `gg`/`gr` are handled by their callers' pending-`g` state,
    /// not here.
    fn move_by(&mut self, c: char, n: usize) -> bool {
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
            // Repeat the last `/` search; a motion here so Visual extends over
            // it for free. Deliberately not an operator target (`dn` is not in
            // scope) — operators resolve their own motion table in `normal_key`.
            'n' => self.search_repeat(n, true),
            'N' => self.search_repeat(n, false),
            _ => return false,
        }
        true
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
            "delete" => self.delete_current(),
            "settings" => self.open_settings(),
            "fmt" => self.format_buffer(),
            "w" | "wq" | "x" => {
                if self.prefs.format_on_save {
                    self.format_buffer();
                }
                self.request_save_active();
            }
            // fmt → save → push, shared with the `>` publish command.
            "gp" => self.run_publish(),
            "gl" => self.requests.push(Effect::Pull),
            _ => {}
        }
    }

    /// Run the typed `/` search: remember the pattern (a bare `/`+Enter repeats
    /// the last one, like vim) and jump forward once. Literal substring — no
    /// regex on a writing appliance — matched **smartcase** (an all-lowercase
    /// pattern is case-insensitive; one capital makes it exact, vim-style) and
    /// always **accent-folded** (`/ete` finds `été`; see [`fold`]).
    fn execute_search(&mut self) {
        if !self.cmdline.is_empty() {
            self.last_search = self.cmdline.clone();
        }
        self.search_repeat(1, true);
    }

    /// Jump `n` matches of [`last_search`](Self::last_search) forward
    /// (`fwd`, the `/`-Enter and `n` step) or backward (`N`), wrapping around
    /// the buffer with a "wrapped" notice. A missing pattern or an absent
    /// match posts a notice and leaves the caret alone.
    fn search_repeat(&mut self, n: usize, fwd: bool) {
        if self.last_search.is_empty() {
            self.set_notice("no previous search");
            return;
        }
        let pat = self.last_search.clone();
        // Smartcase: any capital in the pattern makes the search exact
        // (`n`/`N` recompute from the remembered pattern, so a repeat behaves
        // like the original search).
        let ci = !pat.chars().any(char::is_uppercase);
        for _ in 0..n {
            // Start strictly past (or before) the caret so a caret already on a
            // match moves to the next one, per vim.
            let hit = if fwd {
                let start = if self.caret >= self.text.len() {
                    self.text.len()
                } else {
                    self.next_char(self.caret)
                };
                match find_fold(&self.text[start..], &pat, ci) {
                    Some(i) => Some((start + i, false)),
                    None => find_fold(&self.text, &pat, ci).map(|i| (i, true)),
                }
            } else {
                match rfind_fold(&self.text[..self.caret], &pat, ci) {
                    Some(i) => Some((i, false)),
                    None => rfind_fold(&self.text, &pat, ci).map(|i| (i, true)),
                }
            };
            match hit {
                Some((pos, wrapped)) => {
                    self.caret = pos;
                    if wrapped {
                        self.set_notice("wrapped");
                    }
                }
                None => {
                    self.set_notice(format!("not found: {pat}"));
                    return; // absent now, absent on every repeat
                }
            }
        }
    }

    /// Queue an [`Effect::Save`] of the active buffer. Posts "no file name" for an
    /// unnamed scratch buffer (nothing to save to) rather than writing to `""`.
    fn request_save_active(&mut self) {
        if self.path.is_empty() {
            self.set_notice("no file name");
            return;
        }
        self.requests.push(Effect::Save {
            path: self.path.clone(),
            scope: self.scope,
            contents: self.text.clone(),
        });
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
        // A change-group is about to begin, so the buffer is (or is about to be)
        // modified relative to the last save. See the `dirty` field note on why
        // this is deliberately slightly over-eager.
        self.dirty = true;
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

    // --- Buffers (multi-file) ----------------------------------------------

    /// Switch the active buffer to `path`. If it is already resident (parked),
    /// restore that copy with its caret/scroll/undo intact — no disk read. If it
    /// is not resident, queue an [`Effect::Load`]; the host reads the file and
    /// calls [`install_loaded`](Self::install_loaded), which does the park + swap.
    /// A dirty outgoing buffer is preserved in RAM (parked) and persisted only
    /// when it is later evicted, so switching itself never blocks on IO.
    fn open_path(&mut self, path: String, scope: Scope) {
        if path == self.path {
            return; // already the active buffer
        }
        self.note_recent(&path); // float it to the top of the palette's MRU
        match self.parked.iter().position(|b| b.path == path) {
            Some(i) => {
                let target = self.parked.remove(i);
                self.park_active();
                self.activate(target);
            }
            None => self.requests.push(Effect::Load { path, scope }),
        }
    }

    /// Move the active buffer's editing state into a parked [`Buffer`], leaving
    /// the active fields empty for a subsequent [`activate`](Self::activate) or
    /// [`set_active`](Self::set_active). Evicts the least-recently-used parked
    /// buffer if that pushes residency over [`MAX_RESIDENT`]; an evicted dirty
    /// buffer queues a [`Effect::Save`] so no unsaved work leaves memory.
    fn park_active(&mut self) {
        let buf = Buffer {
            path: core::mem::take(&mut self.path),
            scope: self.scope,
            text: core::mem::take(&mut self.text),
            caret: self.caret,
            scroll_top: self.scroll_top,
            dirty: self.dirty,
            undo: core::mem::take(&mut self.undo),
            redo: core::mem::take(&mut self.redo),
        };
        self.parked.push(buf);
        // Active is currently empty, so residency == parked.len(); keep it under
        // MAX_RESIDENT so the buffer about to become active fits.
        while self.parked.len() >= MAX_RESIDENT {
            let evicted = self.parked.remove(0);
            if evicted.dirty {
                self.requests.push(Effect::Save {
                    path: evicted.path,
                    scope: evicted.scope,
                    contents: evicted.text,
                });
            }
        }
    }

    /// Restore a parked buffer into the active fields (its caret, scroll, undo,
    /// and dirty flag come back with it). Lands in Normal with input state reset.
    fn activate(&mut self, b: Buffer) {
        self.path = b.path;
        self.scope = b.scope;
        self.text = b.text;
        self.caret = b.caret;
        self.scroll_top = b.scroll_top;
        self.dirty = b.dirty;
        self.undo = b.undo;
        self.redo = b.redo;
        self.reset_active_input();
    }

    /// Make a freshly-loaded file the active buffer: same boot posture as
    /// [`with_file`](Self::with_file) (Normal, caret on the last char) with empty
    /// undo history and a clean dirty flag.
    fn set_active(&mut self, path: String, scope: Scope, text: String) {
        self.path = path;
        self.scope = scope;
        self.text = text;
        self.caret = self.text.len();
        if self.caret > self.line_start(self.caret) {
            self.caret = self.prev_char(self.caret);
        }
        self.scroll_top = 0;
        self.dirty = false;
        self.undo.clear();
        self.redo.clear();
        self.reset_active_input();
    }

    /// Reset the transient per-keystroke input state (mode, pending operator,
    /// visual anchor, command line) on a buffer swap, so nothing leaks across.
    /// The register and `.` history are deliberately left alone — they are global
    /// (vim-like), so a yank in one file pastes in another.
    fn reset_active_input(&mut self) {
        self.mode = Mode::Normal;
        self.visual_anchor = None;
        self.cmdline.clear();
        self.reset_pending();
    }


    /// `:enew <arg>` — create a new file and make it the active buffer. Scope is
    /// read from the path exactly like `:e` (`local/…` → Local, else Tracked;
    /// a bare name lands in the current buffer's scope), so no scope prompt is
    /// needed — the resolved scope is echoed in the snackbar instead. If the name
    /// already resolves to the active or a parked buffer, this just switches to it
    /// (no clobber); otherwise the buffer starts empty and **dirty**, so it is
    /// durable (a later eviction or `:w` persists it) and shows in the palette at
    /// once. The file is not written to disk until then — `:enew` alone allocates
    /// no card IO.
    fn new_file(&mut self, arg: &str) {
        let arg = arg.trim();
        if arg.is_empty() {
            self.set_notice("usage: :enew <file>");
            return;
        }
        let (path, scope) = resolve_path(arg, self.scope);
        // Already open (active or parked) — treat `:enew` of an existing name as a
        // switch rather than replacing its contents with an empty buffer.
        if path == self.path || self.parked.iter().any(|b| b.path == path) {
            self.open_path(path, scope);
            return;
        }
        self.note_recent(&path);
        self.add_to_file_list(&path);
        self.park_active();
        self.set_active(path.clone(), scope, String::new());
        // A fresh file is unsaved: mark it dirty so eviction/`:w` persists it and
        // it never silently vanishes (unlike an `:e` of a missing name).
        self.dirty = true;
        self.set_notice(format!("new {}", palette_label(&path)));
    }

    /// `:delete` — unlink the **current** file from the card and leave it. Queues
    /// an [`Effect::Delete`] (the host does the removal + reports the outcome) and
    /// updates the in-core model now: the path is dropped from the file list and
    /// MRU, and the active buffer switches to the most-recently-parked buffer, or
    /// an empty unnamed scratch if none is resident. An unnamed scratch buffer has
    /// nothing on disk, so it is a no-op with a notice. Deleting an arbitrary
    /// (non-current) file is deferred — this is the file you are looking at.
    fn delete_current(&mut self) {
        if self.path.is_empty() {
            self.set_notice("no file to delete");
            return;
        }
        let path = core::mem::take(&mut self.path);
        let scope = self.scope;
        self.requests.push(Effect::Delete { path: path.clone(), scope });
        self.remove_from_file_list(&path);
        self.recent.retain(|p| p != &path);
        // The current buffer is being discarded, not parked: restore the most
        // recently parked buffer if one is resident, else fall back to scratch.
        match self.parked.pop() {
            Some(b) => {
                self.note_recent(&b.path);
                self.activate(b);
            }
            None => self.set_active(String::new(), Scope::Tracked, String::new()),
        }
    }

    /// The `i`-th file path in the palette's sorted base order (a slice into
    /// [`file_blob`](Self::file_blob)).
    fn file_at(&self, i: usize) -> &str {
        let (s, e) = self.file_spans[i];
        &self.file_blob[s as usize..e as usize]
    }

    /// How many files the palette knows about.
    fn file_count(&self) -> usize {
        self.file_spans.len()
    }

    /// Insert `path` into the palette's file list, keeping the spans sorted and
    /// unique (matches [`set_file_list_joined`](Self::set_file_list_joined)'s
    /// invariant). Used by `:enew` so a just-created file is findable without a
    /// disk re-enumeration. Appends to the blob; a `String` realloc only moves
    /// bytes, the spans are indices and stay valid.
    fn add_to_file_list(&mut self, path: &str) {
        match self
            .file_spans
            .binary_search_by(|&(s, e)| self.file_blob[s as usize..e as usize].cmp(path))
        {
            Ok(_) => {}
            Err(i) => {
                let start = self.file_blob.len() as u32;
                self.file_blob.push_str(path);
                self.file_spans.insert(i, (start, start + path.len() as u32));
            }
        }
    }

    /// Drop `path` from the palette's file list (used by `:delete`). Only the
    /// span goes; its bytes stay in the blob as dead weight until the next
    /// host re-walk replaces the whole thing — a few dozen bytes at most.
    fn remove_from_file_list(&mut self, path: &str) {
        let blob = &self.file_blob;
        self.file_spans
            .retain(|&(s, e)| &blob[s as usize..e as usize] != path);
    }

    // --- File palette (Ctrl-P) ---------------------------------------------

    /// Feed the palette its file list as **one newline-joined blob** of
    /// absolute paths, enumerated by the host from `/sd/repo` and `/sd/local`.
    /// This is the device's entry point: a single large `String` lands in
    /// PSRAM (allocations ≥ 16 KB cross the SPIRAM-malloc threshold), where
    /// the same list as 1099 individual `String`s measured 182 KB of internal
    /// DRAM. Spans are sorted + deduped for a stable base order; the MRU
    /// floats recents above it. The palette is a pure view over this — nothing
    /// is read from disk until a file is actually opened.
    pub fn set_file_list_joined(&mut self, blob: String) {
        let mut spans: Vec<(u32, u32)> = Vec::new();
        let mut start = 0u32;
        for line in blob.split('\n') {
            let end = start + line.len() as u32;
            if !line.is_empty() {
                spans.push((start, end));
            }
            start = end + 1; // past the '\n'
        }
        spans.sort_by(|&(a, b), &(c, d)| blob[a as usize..b as usize].cmp(&blob[c as usize..d as usize]));
        spans.dedup_by(|&mut (a, b), &mut (c, d)| blob[a as usize..b as usize] == blob[c as usize..d as usize]);
        self.file_blob = blob;
        self.file_spans = spans;
    }

    /// [`set_file_list_joined`](Self::set_file_list_joined) from a `Vec` —
    /// convenience for hosts/tests that already hold separate strings.
    pub fn set_file_list(&mut self, files: Vec<String>) {
        self.set_file_list_joined(files.join("\n"));
    }

    /// Push `path` to the front of the recent-files MRU (dropping any earlier
    /// occurrence), bounded to [`MRU_MAX`]. Drives the palette's empty-query
    /// order, so the file you were just in sits at the top.
    fn note_recent(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(MRU_MAX);
    }

    /// `Ctrl-P` — open the file palette: empty query (full list, recents first),
    /// selection on the first row.
    fn open_palette(&mut self) {
        self.mode = Mode::Palette;
        self.palette_query.clear();
        self.palette_sel = 0;
        self.palette_step = PaletteStep::List;
    }

    /// `:settings` — open the palette straight into `>` command mode (the
    /// settings list), so the prefs are reachable in one command instead of
    /// `Cmd-P` then `>`. Same surface, same stay-open toggle behaviour.
    fn open_settings(&mut self) {
        self.mode = Mode::Palette;
        self.palette_query = ">".to_string();
        self.palette_sel = 0;
        self.palette_step = PaletteStep::List;
    }

    /// Leave the palette back to Normal, clearing its query, selection, and step.
    fn close_palette(&mut self) {
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
    fn palette_key(&mut self, key: Key) {
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
            Key::Redo => {}
        }
    }

    /// Keys in the `> new file` input step: the query is a filename, not a filter.
    /// Enter creates it (scope resolved from a `repo/`/`local/` prefix, exactly as
    /// `:enew` did) and closes; an empty name is a no-op that stays in the step.
    /// Backspacing past the start steps **back** to the `>` command list rather
    /// than closing, so the step is escapable without losing the palette. Esc or
    /// `Cmd-P` closes outright.
    fn new_file_step_key(&mut self, key: Key) {
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
            // No list to move over in this step.
            Key::Up | Key::Down | Key::HalfPageUp | Key::HalfPageDown | Key::Redo => {}
        }
    }

    /// Open the palette's selected file (Enter). A no-op on an empty result set.
    /// Closes the palette first, then routes through [`open_path`](Self::open_path)
    /// exactly like `:e`, so the switch/park/evict/MRU path is shared.
    fn palette_open_selected(&mut self) {
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
    fn palette_matches(&self) -> Vec<usize> {
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
    fn palette_command_mode(&self) -> bool {
        self.palette_query.starts_with('>')
    }

    /// The command filter: everything after the leading `>`, trimmed. `>` alone
    /// (or with only spaces) is an empty filter, which matches every command.
    fn command_filter(&self) -> &str {
        self.palette_query.strip_prefix('>').unwrap_or("").trim()
    }

    /// A command's display label. An action's label is a plain verb (with a
    /// trailing `...` on the parameterised `new file`, VS-Code-style, to flag the
    /// second step — ASCII dots, since Latin-9 has no `…` glyph); a toggle's label
    /// carries its pref's current state, so the list reads as a live settings panel
    /// and the effect is legible before and after. This is also the text
    /// [`fuzzy_score`] matches against.
    fn command_label(&self, cmd: PaletteCmd) -> String {
        let on = |b| if b { "on" } else { "off" };
        match cmd {
            PaletteCmd::NewFile => "new file...".to_string(),
            PaletteCmd::Format => "format".to_string(),
            PaletteCmd::Publish => "publish".to_string(),
            PaletteCmd::SaveOnIdle => format!("save on idle: {}", on(self.prefs.save_on_idle)),
            PaletteCmd::FormatOnSave => format!("format on save: {}", on(self.prefs.format_on_save)),
            PaletteCmd::LineNumbers => format!("line numbers: {}", on(self.prefs.line_numbers)),
            PaletteCmd::Theme => format!("theme: {}", self.prefs.theme),
            PaletteCmd::AutoSync => format!("auto sync: {}", self.prefs.auto_sync),
        }
    }

    /// Filtered, ranked command indices into [`PALETTE_CMDS`]. An empty filter
    /// keeps registry order; a non-empty one fuzzy-ranks by label, same matcher
    /// and stable-sort as the file list.
    fn palette_command_matches(&self) -> Vec<usize> {
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
    fn palette_run_command(&mut self) {
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
    fn begin_new_file_step(&mut self) {
        self.palette_step = PaletteStep::NewFile;
        self.palette_query.clear();
        self.palette_sel = 0;
    }

    /// The publish path shared by `:gp` and the `>` `publish` command: format on
    /// save (if enabled), queue the buffer save, then the git push — the host
    /// services them in order. Tracked-only: a Local buffer never reaches the
    /// remote, so it is a no-op with a notice.
    fn run_publish(&mut self) {
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
    fn cycle_pref(&mut self, cmd: PaletteCmd) {
        match cmd {
            PaletteCmd::SaveOnIdle => self.prefs.save_on_idle = !self.prefs.save_on_idle,
            PaletteCmd::FormatOnSave => self.prefs.format_on_save = !self.prefs.format_on_save,
            PaletteCmd::LineNumbers => self.prefs.line_numbers = !self.prefs.line_numbers,
            PaletteCmd::Theme => {
                self.prefs.theme = next_option(&self.prefs.theme, &THEME_OPTIONS).to_string()
            }
            PaletteCmd::AutoSync => {
                self.prefs.auto_sync = next_option(&self.prefs.auto_sync, &AUTO_SYNC_OPTIONS).to_string()
            }
            // Actions, not prefs: palette_run_command routes them away, so we never
            // arrive here. Return before the SavePrefs/notice below rather than
            // panicking the firmware on a would-be routing bug.
            PaletteCmd::NewFile | PaletteCmd::Format | PaletteCmd::Publish => {
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
    fn palette_snippet_mode(&self) -> bool {
        self.palette_query.starts_with('$')
    }

    /// The snippet filter: everything after the leading `$`, trimmed. `$` alone is
    /// an empty filter, which lists every snippet.
    fn snippet_filter(&self) -> &str {
        self.palette_query.strip_prefix('$').unwrap_or("").trim()
    }

    /// The text a snippet is fuzzy-matched against: name, prefix, and description
    /// together, so you find a snippet by whichever you remember. Matching runs on
    /// this joined haystack (see [`fuzzy_score`]); [`snippet_label`] is the shorter
    /// string actually drawn.
    fn snippet_haystack(s: &Snippet) -> String {
        format!("{} {} {}", s.name, s.prefix, s.description)
    }

    /// A snippet's palette row: the display name with its inline trigger in
    /// brackets (`Markdown link [link]`), so browsing also teaches the prefix you'd
    /// type for the fast inline path. Truncated to the column width by the caller.
    fn snippet_label(s: &Snippet) -> String {
        format!("{} [{}]", s.name, s.prefix)
    }

    /// Filtered, ranked snippet indices into [`snippets`](Self::snippets). An empty
    /// filter keeps the parsed order (sorted by name); a non-empty one fuzzy-ranks
    /// over [`snippet_haystack`], same matcher and stable-sort as the file list.
    fn palette_snippet_matches(&self) -> Vec<usize> {
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
    fn palette_insert_selected(&mut self) {
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
    fn palette_len(&self) -> usize {
        if self.palette_snippet_mode() {
            self.palette_snippet_matches().len()
        } else if self.palette_command_mode() {
            self.palette_command_matches().len()
        } else {
            self.palette_matches().len()
        }
    }

    // --- Visual mode -------------------------------------------------------

    /// True while a Visual selection is active (charwise or linewise).
    fn in_visual(&self) -> bool {
        matches!(self.mode, Mode::Visual | Mode::VisualLine)
    }

    /// Dispatch a key in Visual/VisualLine. Motions extend the selection (the
    /// anchor stays put, the caret moves); `y`/`d`/`c` act on the span and
    /// leave Visual; `v`/`V` switch submode or toggle back to Normal; `Esc`
    /// cancels. Counts and `gg`/`G` work as in Normal.
    fn visual_key(&mut self, key: Key) {
        let c = match key {
            Key::Char(c) => c,
            Key::HalfPageDown => {
                self.count = 0;
                self.move_display_rows(HALF_PAGE as isize);
                return;
            }
            Key::HalfPageUp => {
                self.count = 0;
                self.move_display_rows(-(HALF_PAGE as isize));
                return;
            }
            Key::Escape => {
                self.exit_visual();
                return;
            }
            // Enter/Backspace/etc. carry no Visual meaning; drop any count.
            _ => {
                self.count = 0;
                self.pending_g = false;
                return;
            }
        };

        // `gg` — jump to the top, extending the selection.
        if self.pending_g {
            self.pending_g = false;
            if c == 'g' {
                self.caret = 0;
            }
            self.count = 0;
            return;
        }

        // Count prefix, exactly as in Normal (a leading `0` is the motion).
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
            // `v` toggles charwise off (or switches VisualLine → charwise);
            // `V` toggles linewise off (or switches charwise → linewise).
            'v' => {
                if self.mode == Mode::Visual {
                    self.exit_visual();
                } else {
                    self.mode = Mode::Visual;
                }
            }
            'V' => {
                if self.mode == Mode::VisualLine {
                    self.exit_visual();
                } else {
                    self.mode = Mode::VisualLine;
                }
            }
            'y' => self.visual_yank(),
            'd' => self.visual_delete(),
            'c' => self.visual_change(),
            // Any other char: a shared motion extends the selection, or is a
            // no-op. Unlike Normal, edit keys (`x`, `p`, …) aren't bound here.
            _ => {
                self.move_by(c, n);
            }
        }
        self.count = 0;
    }

    /// The current selection as `(start, end, linewise)` byte offsets, `start <
    /// end` (or equal on an empty buffer). Charwise is vim-inclusive of the char
    /// under the further caret; linewise always spans whole logical lines from
    /// the anchor's line through the caret's.
    fn visual_span(&self) -> (usize, usize, bool) {
        let anchor = self.visual_anchor.unwrap_or(self.caret);
        let lo = anchor.min(self.caret);
        let hi = anchor.max(self.caret);
        if self.mode == Mode::VisualLine {
            (self.line_start(lo), self.line_end(hi), true)
        } else {
            (lo, self.next_char(hi).min(self.text.len()), false)
        }
    }

    /// Copy the selection into the unnamed register (linewise from `V`, charwise
    /// otherwise), leave the caret at the selection start, and return to Normal.
    fn visual_yank(&mut self) {
        let (s, e, line) = self.visual_span();
        self.register = self.selection_text(s, e, line);
        self.register_linewise = line;
        self.caret = s;
        self.exit_visual();
    }

    /// Delete the selection (filling the register like `visual_yank`), leaving
    /// the caret at the span start, and return to Normal. Linewise removes whole
    /// lines including a bounding newline, mirroring `dd`.
    fn visual_delete(&mut self) {
        let (s, e, line) = self.visual_span();
        self.register = self.selection_text(s, e, line);
        self.register_linewise = line;
        self.checkpoint();
        let (ds, de) = self.delete_bounds(s, e, line);
        self.text.replace_range(ds..de, "");
        self.caret = if line {
            self.line_start(ds.min(self.text.len()))
        } else {
            ds.min(self.text.len())
        };
        self.exit_visual();
    }

    /// Change the selection: delete it (filling the register) and drop into
    /// Insert at the start. A linewise change clears the lines' text but leaves
    /// one empty line to type on (like `cc`), rather than removing the line.
    fn visual_change(&mut self) {
        let (s, e, line) = self.visual_span();
        self.register = self.selection_text(s, e, line);
        self.register_linewise = line;
        self.checkpoint();
        // Linewise: replace only `s..e` (the text, keeping the final newline) so
        // one blank line remains. Charwise: remove the exact span.
        self.text.replace_range(s..e, "");
        self.caret = s.min(self.text.len());
        self.visual_anchor = None;
        self.pending_g = false;
        self.count = 0;
        self.mode = Mode::Insert;
    }

    /// The register contents for a selection span: charwise is the raw slice;
    /// linewise gets a synthesised trailing newline (as `yy`/`dd` store lines).
    fn selection_text(&self, s: usize, e: usize, line: bool) -> String {
        let mut block = self.text[s..e].to_string();
        if line && !block.ends_with('\n') {
            block.push('\n');
        }
        block
    }

    /// Byte range to actually remove for a delete. Charwise is the span as-is;
    /// linewise also eats the trailing newline (or, on the last line, the
    /// preceding one) so no blank line is left behind — matching `dd`.
    fn delete_bounds(&self, s: usize, e: usize, line: bool) -> (usize, usize) {
        if !line {
            return (s, e);
        }
        if e < self.text.len() {
            (s, self.next_char(e)) // eat the trailing '\n' at `e`
        } else if s > 0 {
            (self.prev_char(s), e) // last line: eat the preceding '\n' instead
        } else {
            (s, e) // whole buffer
        }
    }

    /// Leave Visual for Normal, clearing the anchor and any pending state.
    fn exit_visual(&mut self) {
        self.mode = Mode::Normal;
        self.visual_anchor = None;
        self.pending_g = false;
        self.count = 0;
    }

    // --- View mode ---------------------------------------------------------

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

    // --- Snippets ----------------------------------------------------------

    /// Expand a snippet `body` at the caret and enter its tab-stop session. Splits
    /// the body into literal text (stops removed) and their [`parse_snippet_body`]
    /// visit order, inserts the literal, lands the caret on `$1` (or the body end
    /// if there are no stops), and enters Insert. The remaining stops queue in
    /// [`snippet_stops`](Self::snippet_stops) for Tab. Shared by inline
    /// Tab-expansion and the `$` palette. **The caller owns the undo boundary** —
    /// it must [`checkpoint`](Self::checkpoint) *before* any related mutation
    /// (the inline path deletes the trigger word first), so one undo group covers
    /// the whole expansion; `insert_snippet` deliberately does not checkpoint.
    fn insert_snippet(&mut self, body: &str) {
        let (literal, stops) = parse_snippet_body(body);
        let base = self.caret;
        self.text.insert_str(base, &literal);
        let mut abs = stops.into_iter().map(|o| base + o);
        match abs.next() {
            Some(first) => {
                self.caret = first;
                self.snippet_stops = abs.collect();
            }
            None => {
                self.caret = base + literal.len();
                self.snippet_stops.clear();
            }
        }
        self.mode = Mode::Insert;
    }

    /// Tab inside a live session: jump the caret to the next pending stop. The
    /// final stop (`$0` / the body end) empties the queue, ending the session with
    /// the caret resting there. Offsets were kept current by the edit-shift in
    /// [`insert_key`](Self::insert_key), so this is a plain move.
    fn snippet_advance(&mut self) {
        if self.snippet_stops.is_empty() {
            return;
        }
        let next = self.snippet_stops.remove(0);
        self.caret = next.min(self.text.len());
    }

    /// The maximal run of non-whitespace ending exactly at the caret — the
    /// candidate inline snippet trigger — or `None` if the caret is at a line/word
    /// start (the char before it is whitespace). Whitespace is ASCII, so scanning
    /// bytes is UTF-8-safe: `start` lands just past an ASCII space/newline (a char
    /// boundary) or at 0.
    fn word_before_caret(&self) -> Option<(usize, &str)> {
        let b = self.text.as_bytes();
        if self.caret == 0 || b[self.caret - 1].is_ascii_whitespace() {
            return None;
        }
        let mut start = self.caret;
        while start > 0 && !b[start - 1].is_ascii_whitespace() {
            start -= 1;
        }
        Some((start, &self.text[start..self.caret]))
    }

    /// Inline Tab-expansion: if the word immediately before the caret is exactly a
    /// snippet prefix, replace it with the expansion (as one undo group) and start
    /// the tab-stop session, returning `true`. Otherwise leave the buffer untouched
    /// and return `false`, so Tab falls back to inserting spaces.
    fn try_expand_snippet(&mut self) -> bool {
        let Some((start, word)) = self.word_before_caret() else {
            return false;
        };
        let Some(body) = self.snippets.iter().find(|s| s.prefix == word).map(|s| s.body.clone())
        else {
            return false;
        };
        self.checkpoint(); // baseline includes the trigger word — undo restores it
        self.text.replace_range(start..self.caret, "");
        self.caret = start;
        self.insert_snippet(&body);
        true
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
        // `end`: byte offset of the last pasted char, so the viewport can reveal
        // the whole block even when the caret stays on its first line.
        let end = if self.register_linewise {
            let le = self.line_end(self.caret);
            if le < self.text.len() {
                let at = le + 1; // start of the following line
                self.text.insert_str(at, &content);
                self.caret = at;
                at + content.len() - 1
            } else {
                // Last line has no trailing newline: prefix one, drop the
                // block's trailing newline so we don't leave a blank line.
                let block = content.strip_suffix('\n').unwrap_or(&content);
                let inserted = format!("\n{block}");
                let end = le + inserted.len() - 1;
                self.text.insert_str(le, &inserted);
                self.caret = le + 1;
                end
            }
        } else {
            let at = if self.text.is_empty() { 0 } else { self.next_char(self.caret) };
            self.text.insert_str(at, &content);
            self.caret = self.prev_char(at + content.len()); // onto the last char
            self.caret
        };
        self.reveal(end);
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
        let end = if self.register_linewise {
            let ls = self.line_start(self.caret);
            self.text.insert_str(ls, &content);
            self.caret = ls;
            ls + content.len() - 1
        } else {
            let at = self.caret;
            self.text.insert_str(at, &content);
            self.caret = self.prev_char(at + content.len()); // onto the last char
            self.caret
        };
        self.reveal(end);
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
        if !self.prefs.line_numbers {
            return 0; // gutter off: text reclaims the full writing width
        }
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

    /// Scroll so the display row holding byte offset `pos` is visible,
    /// bottom-aligning it when it sits below the viewport (never scrolls up).
    /// Called after an insert that can run past the fold — chiefly a multi-line
    /// paste — so the *whole* pasted block is revealed, not just the caret's
    /// first line. The caret is left where the edit put it; `draw`'s
    /// `adjust_scroll` won't override this as long as the caret stays within the
    /// resulting window (true for any block up to a screen tall).
    fn reveal(&mut self, pos: usize) {
        let lay = self.layout();
        if lay.is_empty() {
            return;
        }
        let pos = pos.min(self.text.len());
        let mut row = 0;
        for (i, l) in lay.iter().enumerate() {
            if l.start <= pos {
                row = i;
            } else {
                break;
            }
        }
        if row >= self.scroll_top + ROWS {
            self.scroll_top = row + 1 - ROWS;
        }
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

    /// Paint the non-Latin-9 glyphs (`→ ≠ Σ … – — ' ' " " •`) that the base font
    /// can't draw, overlaying them onto the cells `embedded-graphics` just filled
    /// with fallback boxes. `line` is a laid-out display line; `x0` is its
    /// x-origin (`gx`, or `gx + 1` for the heading double-strike). Only the
    /// half-open display-column range `[col_start, col_end)` is painted, so the
    /// visual-selection pass can invert just the selected span. `ink = On` draws
    /// black-on-white; `ink = Off` draws white-on-black for reverse-video cells.
    /// Because every extra glyph is one cell wide (like the font), cell x is
    /// `x0 + col * CW` — the same grid the layout and caret math already use.
    fn overlay_extras(
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
    fn draw_panel(&self, f: &mut Frame) {
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

    /// The transient `:` command line, drawn at body size (FONT_10X20) along the
    /// bottom of the writing column (vim-style). Shown only while composing a
    /// command. At this size it no longer fits the old 12 px sliver, so it
    /// overlays the bottom writing row: blank that row to white first, then draw
    /// `:cmd` over it. The row's text reappears on the next render once the
    /// command finishes (Enter/Esc) — you never read the last line while typing a
    /// command. Blanks only the writing column (left of the divider), so the
    /// panel is untouched (and in Command mode its mode line isn't drawn anyway).
    fn draw_cmdline(&self, f: &mut Frame) {
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
    fn draw_palette(&self, f: &mut Frame) {
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

    // 3. Collapse 2+ consecutive blank lines to one. A trailing blank run
    //    collapses the same way, so at most one trailing blank line survives — and
    //    we deliberately keep that one rather than dropping it. A writer often
    //    presses Enter to open the next line before pausing; yanking that line
    //    (and the caret) out from under them on every format-on-save is jarring.
    //    The file's POSIX terminator is `save_path`'s job, not this pass's, so
    //    keeping the blank line here is purely about not disturbing the buffer.
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

/// Byte offset of the first folded match of `pat` in `hay`, or `None`.
/// Char-by-char comparison through [`fold`] at every char boundary — no
/// folded copy of the buffer (folding can change byte lengths, which would
/// break the returned offsets). `ci` is the smartcase verdict, computed once
/// per search from the pattern. O(n·m), fine at note sizes for an
/// Enter-triggered jump.
fn find_fold(hay: &str, pat: &str, ci: bool) -> Option<usize> {
    hay.char_indices()
        .map(|(i, _)| i)
        .find(|&i| starts_with_fold(&hay[i..], pat, ci))
}

/// [`find_fold`], but the *last* match — the backward (`N`) direction.
fn rfind_fold(hay: &str, pat: &str, ci: bool) -> Option<usize> {
    hay.char_indices()
        .map(|(i, _)| i)
        .rev()
        .find(|&i| starts_with_fold(&hay[i..], pat, ci))
}

/// Whether `s` begins with `pat` under [`fold`].
fn starts_with_fold(s: &str, pat: &str, ci: bool) -> bool {
    let mut sc = s.chars();
    pat.chars()
        .all(|p| sc.next().is_some_and(|c| fold(c, ci) == fold(p, ci)))
}

/// A char's search identity: diacritics are always stripped (`é` = `e`, so
/// `/ete` finds `été` and vice versa — accents are how the word is *spelled*,
/// not what you're *searching for*), and case is dropped only when `ci`
/// (the smartcase rule: an all-lowercase pattern searches insensitively; one
/// capital in it makes the search exact).
fn fold(c: char, ci: bool) -> char {
    let c = if ci {
        // First char of the lowercase expansion — 1:1 for all of Latin,
        // which is what this appliance types.
        c.to_lowercase().next().unwrap_or(c)
    } else {
        c
    };
    strip_diacritic(c)
}

/// Map accented Latin letters to their base letter, both cases (the Latin-1
/// Supplement set — the French/Western repertoire the keymap can produce).
/// Ligatures (`œ`, `æ`) fold to more than one char and are left alone.
fn strip_diacritic(c: char) -> char {
    match c {
        'à'..='å' => 'a',
        'ç' => 'c',
        'è'..='ë' => 'e',
        'ì'..='ï' => 'i',
        'ñ' => 'n',
        'ò'..='ö' => 'o',
        'ù'..='ü' => 'u',
        'ý' | 'ÿ' => 'y',
        'À'..='Å' => 'A',
        'Ç' => 'C',
        'È'..='Ë' => 'E',
        'Ì'..='Ï' => 'I',
        'Ñ' => 'N',
        'Ò'..='Ö' => 'O',
        'Ù'..='Ü' => 'U',
        'Ý' => 'Y',
        _ => c,
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

    /// From a fresh editor over a named Tracked file, run `:{cmd}<Enter>`,
    /// returning the editor and the drained [`Effect`]s the command queued.
    fn command(cmd: &str) -> (Editor, Vec<Effect>) {
        let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, String::new());
        e.handle(Key::Char(':')); // Normal -> Command
        for c in cmd.chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
        let effects = e.take_effects();
        (e, effects)
    }

    /// Coarse kind of an [`Effect`], ignoring `Save`/`Load` payloads, so the
    /// command tests can assert intent without pinning path/scope/contents.
    #[derive(Debug, PartialEq)]
    enum Kind {
        Save,
        Load,
        Publish,
        Pull,
        Delete,
        SavePrefs,
    }

    fn kinds(effects: &[Effect]) -> Vec<Kind> {
        effects
            .iter()
            .map(|e| match e {
                Effect::Save { .. } => Kind::Save,
                Effect::Load { .. } => Kind::Load,
                Effect::Publish => Kind::Publish,
                Effect::Pull => Kind::Pull,
                Effect::Delete { .. } => Kind::Delete,
                Effect::SavePrefs { .. } => Kind::SavePrefs,
            })
            .collect()
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
    fn extra_glyph_covers_snippet_and_prose_symbols() {
        // The curated non-Latin-9 set the overlay draws (→ ≠ Σ … – — ' ' " " •).
        let targets = [
            '\u{2192}', '\u{2260}', '\u{03A3}', '\u{2026}', '\u{2013}', '\u{2014}', '\u{2018}',
            '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}',
        ];
        for c in targets {
            assert!(extra_glyph(c).is_some(), "missing glyph for U+{:04X}", c as u32);
        }
        // The base font already draws these — the overlay must defer to it,
        // including œ/€ (which *are* in ISO-8859-15, at 0xBD/0xA4).
        for c in ['a', 'é', 'œ', '€', ' ', '#', '-'] {
            assert!(extra_glyph(c).is_none(), "should defer to base font: {c}");
        }
    }

    #[test]
    fn draw_runs_for_symbol_buffer() {
        // Insert the whole extra set and render with a caret — no panic, right size.
        let mut e = typed("\u{2192} \u{2260} \u{03A3} \u{2026} \u{2013} \u{2014} \u{2018}x\u{2019} \u{201C}y\u{201D} \u{2022}");
        let frame = e.draw(true);
        assert_eq!(frame.bytes().len(), display::FB_BYTES);
        assert!(e.text.is_char_boundary(e.caret));
    }

    /// 1 = white paper, 0 = black ink (SSD16xx convention). Reads one pixel.
    fn ink_at(frame: &Frame, x: usize, y: usize) -> bool {
        frame.bytes()[y * display::FB_BYTES_W + x / 8] & (0x80 >> (x % 8)) == 0
    }

    #[test]
    fn overlay_paints_extra_glyph_over_fallback_box() {
        // The em dash is two solid mid-height bars and nothing else; a fallback
        // box would ink the cell's top row. Gutter off so it lands in column 0.
        let mut e = Editor::with_text("\u{2014}".into()); // —
        e.prefs.line_numbers = false;
        let f = e.draw(false); // no caret
        assert!((0..10).all(|x| !ink_at(&f, x, 0)), "cell top row must be blank");
        assert!((0..10).all(|x| ink_at(&f, x, 9)), "row 9 must be solid ink");
        assert!((0..10).all(|x| ink_at(&f, x, 10)), "row 10 must be solid ink");
    }

    #[test]
    fn overlay_inverts_extra_glyph_under_selection() {
        // Same em dash, but selected: reverse-video flips the cell — the fill
        // goes black and the dash bars punch back to white paper.
        let mut e = Editor::with_text("\u{2014}x".into());
        e.prefs.line_numbers = false;
        e.handle(Key::Char('0')); // to column 0 (the em dash)
        e.handle(Key::Char('v')); // charwise Visual selects the char under the caret
        let f = e.draw(false); // no active-end caret punch, just the selection
        assert!((0..10).all(|x| ink_at(&f, x, 0)), "selected cell top row must be inked");
        assert!((0..10).all(|x| !ink_at(&f, x, 9)), "row 9 dash must punch to white");
        assert!((0..10).all(|x| !ink_at(&f, x, 10)), "row 10 dash must punch to white");
    }

    #[test]
    fn w_command_signals_save_and_returns_to_normal() {
        let (e, effs) = command("w");
        assert_eq!(
            effs,
            vec![Effect::Save {
                path: "/sd/repo/notes.md".into(),
                scope: Scope::Tracked,
                contents: String::new(),
            }]
        );
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn gp_command_saves_then_publishes() {
        // `:gp` queues a save of the current buffer, then the git publish.
        assert_eq!(kinds(&command("gp").1), vec![Kind::Save, Kind::Publish]);
    }

    #[test]
    fn gl_command_signals_pull() {
        assert_eq!(kinds(&command("gl").1), vec![Kind::Pull]);
    }

    #[test]
    fn gp_formats_the_buffer_before_publishing() {
        // fmt → save → commit → push: `:gp` runs :fmt in-core first (default on).
        let mut e = Editor::with_file(
            "/sd/repo/notes.md".into(),
            Scope::Tracked,
            "hello   \nworld".to_string(), // trailing spaces
        );
        e.handle(Key::Char(':'));
        for c in "gp".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
        assert_eq!(kinds(&e.take_effects()), vec![Kind::Save, Kind::Publish]);
        assert_eq!(e.text(), "hello\nworld"); // :fmt stripped the trailing whitespace
    }

    #[test]
    fn gp_is_refused_in_a_local_buffer() {
        // Publish is Tracked-only; `:gp` in Local queues nothing and warns.
        let mut e = Editor::with_file(
            "/sd/local/journal.md".into(),
            Scope::Local,
            "dear diary".to_string(),
        );
        e.handle(Key::Char(':'));
        for c in "gp".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
        assert!(e.take_effects().is_empty());
    }

    #[test]
    fn format_on_save_off_leaves_the_buffer_untouched() {
        let mut e = Editor::with_file(
            "/sd/repo/notes.md".into(),
            Scope::Tracked,
            "hello   \nworld".to_string(),
        );
        e.prefs.format_on_save = false;
        e.handle(Key::Char(':'));
        e.handle(Key::Char('w'));
        e.handle(Key::Enter);
        assert_eq!(kinds(&e.take_effects()), vec![Kind::Save]);
        assert_eq!(e.text(), "hello   \nworld"); // unchanged when the pref is off
    }

    #[test]
    fn format_keeps_at_most_one_trailing_blank_line() {
        // The writer's trailing blank line (pressed Enter to open the next line) is
        // kept; a run of them collapses to one; a note with none gains none.
        assert_eq!(format_markdown("hello\n"), "hello\n"); // one blank kept
        assert_eq!(format_markdown("hello\n\n\n"), "hello\n"); // extras collapsed to one
        assert_eq!(format_markdown("hello"), "hello"); // none added
    }

    #[test]
    fn format_on_save_keeps_the_caret_on_a_trailing_blank_line() {
        // Regression: `:w` used to drop the trailing blank line and yank the caret
        // up onto the last non-empty line. The blank line — and the caret — stay.
        let mut e = Editor::with_file(
            "/sd/repo/notes.md".into(),
            Scope::Tracked,
            "hello\n".to_string(), // row 0 "hello", row 1 "" (a fresh empty line)
        );
        e.caret = e.text().len(); // caret at the very end = on the trailing blank row
        let lay = e.layout();
        assert_eq!(e.caret_rc(&lay).0, 1, "precondition: caret on the blank row");

        e.handle(Key::Char(':'));
        e.handle(Key::Char('w'));
        e.handle(Key::Enter);

        assert_eq!(e.text(), "hello\n", "trailing blank line survived format-on-save");
        let lay = e.layout();
        assert_eq!(e.caret_rc(&lay).0, 1, "caret stayed on the blank row");
    }

    #[test]
    fn wq_and_x_alias_save_dropping_the_quit() {
        assert_eq!(kinds(&command("wq").1), vec![Kind::Save]);
        assert_eq!(kinds(&command("x").1), vec![Kind::Save]);
    }

    #[test]
    fn fmt_stays_in_core_and_asks_the_host_for_nothing() {
        assert!(command("fmt").1.is_empty());
    }

    #[test]
    fn unknown_command_is_ignored() {
        let (e, effs) = command("q"); // quit is deliberately unimplemented
        assert!(effs.is_empty());
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn w_on_an_unnamed_buffer_posts_no_file_name() {
        // A scratch buffer (empty path) has nowhere to save to.
        let mut e = Editor::new();
        e.handle(Key::Char(':'));
        e.handle(Key::Char('w'));
        e.handle(Key::Enter);
        assert!(e.take_effects().is_empty());
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
        e.handle(Key::Char('g')); // `gr` -> View (v/V are now Visual)
        e.handle(Key::Char('r'));
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

    #[test]
    fn multiline_paste_at_the_bottom_reveals_the_whole_block() {
        // A screenful+ of lines, caret on the last line; paste two lines after
        // it. Both pasted lines must be visible without a manual scroll — the
        // caret stays on the first pasted line, but the viewport reveals the end.
        let mut e = Editor::with_text(vec!["x"; 20].join("\n")); // 20 display rows
        e.handle(Key::Char('g'));
        e.handle(Key::Char('g'));
        e.handle(Key::Char('2'));
        e.handle(Key::Char('y'));
        e.handle(Key::Char('y')); // yank two lines
        e.handle(Key::Char('G')); // to the last line
        e.handle(Key::Char('p')); // paste two lines below it (22 rows total)
        e.draw(true); // adjust_scroll runs; reveal already applied by paste
        let last_row = e.layout().len() - 1; // the second pasted line
        assert!(
            last_row >= e.scroll_top() && last_row < e.scroll_top() + ROWS,
            "pasted block end (row {last_row}) off-screen at scroll_top {}",
            e.scroll_top()
        );
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

    // ---- Visual mode (v0.4) ----

    /// Feed a run of characters as Normal-mode keys.
    fn send(e: &mut Editor, s: &str) {
        for c in s.chars() {
            e.handle(Key::Char(c));
        }
    }

    #[test]
    fn v_enters_charwise_visual_and_anchors_at_the_caret() {
        let mut e = Editor::with_text("hello".into());
        e.caret = 2;
        e.handle(Key::Char('v'));
        assert_eq!(e.mode(), Mode::Visual);
        assert_eq!(e.visual_anchor, Some(2));
    }

    #[test]
    fn capital_v_enters_linewise_visual() {
        let mut e = Editor::with_text("hello".into());
        e.handle(Key::Char('V'));
        assert_eq!(e.mode(), Mode::VisualLine);
    }

    #[test]
    fn charwise_yank_is_inclusive_and_lands_the_caret_at_the_start() {
        let mut e = Editor::with_text("hello world".into());
        e.caret = 0;
        send(&mut e, "vey"); // select "hello" (e -> last char of the word), yank
        assert_eq!(e.mode(), Mode::Normal);
        assert_eq!(e.caret, 0);
        assert_eq!(e.register, "hello");
        assert!(!e.register_linewise);
    }

    #[test]
    fn vy_yanks_the_single_char_under_the_caret() {
        let mut e = Editor::with_text("hello".into());
        e.caret = 1;
        send(&mut e, "vy");
        assert_eq!(e.register, "e");
    }

    #[test]
    fn charwise_delete_removes_the_span_and_fills_the_register() {
        let mut e = Editor::with_text("hello world".into());
        e.caret = 0;
        send(&mut e, "ved"); // select "hello", delete
        assert_eq!(e.text(), " world");
        assert_eq!(e.caret, 0);
        assert_eq!(e.register, "hello");
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn charwise_change_deletes_the_span_and_enters_insert() {
        let mut e = Editor::with_text("hello".into());
        e.caret = 0;
        send(&mut e, "v$c"); // select the whole line, change
        assert_eq!(e.mode(), Mode::Insert);
        assert_eq!(e.text(), "");
        send(&mut e, "bye");
        assert_eq!(e.text(), "bye");
    }

    #[test]
    fn count_in_visual_extends_the_selection() {
        let mut e = Editor::with_text("abcdef".into());
        e.caret = 0;
        send(&mut e, "v2ld"); // select a,b,c (2l from a), delete
        assert_eq!(e.text(), "def");
    }

    #[test]
    fn linewise_delete_removes_the_whole_line_like_dd() {
        let mut e = Editor::with_text("one\ntwo\nthree".into());
        e.caret = e.text().find("two").unwrap();
        send(&mut e, "Vd");
        assert_eq!(e.text(), "one\nthree");
        assert!(e.register_linewise);
        assert_eq!(e.register, "two\n");
    }

    #[test]
    fn linewise_selection_spans_multiple_lines_with_j() {
        let mut e = Editor::with_text("a\nb\nc\nd".into());
        e.caret = 0;
        send(&mut e, "Vjd"); // select lines a and b, delete both
        assert_eq!(e.text(), "c\nd");
    }

    #[test]
    fn linewise_yank_then_paste_copies_the_line_below() {
        let mut e = Editor::with_text("one\ntwo".into());
        e.caret = 0;
        send(&mut e, "Vy"); // yank line "one" linewise
        assert_eq!(e.register, "one\n");
        send(&mut e, "p");
        assert_eq!(e.text(), "one\none\ntwo");
    }

    #[test]
    fn linewise_change_clears_the_line_but_keeps_one_to_type_on() {
        let mut e = Editor::with_text("one\ntwo\nthree".into());
        e.caret = e.text().find("two").unwrap();
        send(&mut e, "Vc");
        assert_eq!(e.mode(), Mode::Insert);
        assert_eq!(e.text(), "one\n\nthree"); // the line's text is gone, the row remains
        send(&mut e, "X");
        assert_eq!(e.text(), "one\nX\nthree");
    }

    #[test]
    fn esc_leaves_visual_without_touching_the_buffer() {
        let mut e = Editor::with_text("hello".into());
        e.caret = 2;
        send(&mut e, "vll");
        e.handle(Key::Escape);
        assert_eq!(e.mode(), Mode::Normal);
        assert_eq!(e.text(), "hello");
        assert_eq!(e.visual_anchor, None);
    }

    #[test]
    fn v_toggles_charwise_visual_off() {
        let mut e = Editor::with_text("hello".into());
        send(&mut e, "vv");
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn capital_v_then_v_switches_to_charwise() {
        let mut e = Editor::with_text("hello".into());
        send(&mut e, "Vv");
        assert_eq!(e.mode(), Mode::Visual);
    }

    #[test]
    fn gr_enters_view_and_v_no_longer_does() {
        let mut e = Editor::with_text("hello".into());
        send(&mut e, "gr");
        assert_eq!(e.mode(), Mode::View);
        e.handle(Key::Escape);
        e.handle(Key::Char('v'));
        assert_eq!(e.mode(), Mode::Visual); // v is Visual now, not View
    }

    #[test]
    fn visual_ops_do_not_clobber_the_dot_register() {
        let mut e = Editor::with_text("abcdef".into());
        e.caret = 0;
        e.handle(Key::Char('x')); // dot = x ; "bcdef"
        send(&mut e, "vld"); // a visual delete must not become the new dot
        e.handle(Key::Char('.')); // repeats the x
        // buffer after x -> "bcdef"; vld deletes "bc" -> "def"; . deletes 'd' -> "ef"
        assert_eq!(e.text(), "ef");
    }

    #[test]
    fn draw_inverts_the_selected_cells() {
        let mut e = Editor::with_text("hello world".into());
        e.caret = 0;
        let normal = e.draw(true).bytes().to_vec();
        send(&mut e, "ve"); // select "hello"
        let visual = e.draw(true).bytes().to_vec();
        assert_ne!(normal, visual); // the selection changed pixels
    }

    #[test]
    fn draw_runs_for_a_linewise_selection_over_a_blank_line() {
        let mut e = Editor::with_text("a\n\nb".into());
        e.caret = 0;
        send(&mut e, "Vjj"); // select all three rows, including the blank one
        let _ = e.draw(true); // must not panic on the empty-row highlight path
        assert_eq!(e.mode(), Mode::VisualLine);
    }

    // ---- Multi-file buffers (v0.5) ----

    /// Open `arg` the way a user now does — via the file palette. (`:e` was
    /// retired in v0.6; bare `Cmd-P` opens files.) Lists the target, opens the
    /// palette, and types its exact label so the fuzzy matcher ranks it first,
    /// then Enter selects it — routing through the same `open_path` `:e` used.
    fn edit(e: &mut Editor, arg: &str) {
        let (path, _) = resolve_path(arg, e.scope);
        e.add_to_file_list(&path);
        e.open_palette();
        for c in palette_label(&path).chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
    }

    /// Drive an arbitrary `:{cmd}<Enter>` from Normal.
    fn ex(e: &mut Editor, cmd: &str) {
        e.handle(Key::Char(':'));
        for c in cmd.chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
    }

    #[test]
    fn wrap_text_packs_words_and_splits_overlong_tokens() {
        // Short message: one line.
        assert_eq!(wrap_text("saved", 15), vec!["saved"]);
        // Word-wraps on the space, keeping the actionable tail.
        assert_eq!(
            wrap_text("save FAILED - retry :w", 15),
            vec!["save FAILED -", "retry :w"]
        );
        // A token longer than the width is hard-split rather than truncated.
        assert_eq!(
            wrap_text("supercalifragilistic", 8),
            vec!["supercal", "ifragili", "stic"]
        );
        assert!(wrap_text("", 15).is_empty());
    }

    #[test]
    fn resolve_path_maps_prefixes_and_bare_names() {
        assert_eq!(
            resolve_path("/sd/local/j.md", Scope::Tracked),
            ("/sd/local/j.md".to_string(), Scope::Local)
        );
        assert_eq!(
            resolve_path("/sd/repo/n.md", Scope::Local),
            ("/sd/repo/n.md".to_string(), Scope::Tracked)
        );
        // A leading `local/` or `repo/` segment selects scope (the palette label
        // form), independent of the current buffer's scope.
        assert_eq!(
            resolve_path("local/j.md", Scope::Tracked),
            ("/sd/local/j.md".to_string(), Scope::Local)
        );
        assert_eq!(
            resolve_path("repo/n.md", Scope::Local),
            ("/sd/repo/n.md".to_string(), Scope::Tracked)
        );
        // The `/sd` prefix is optional: `/repo/x` and `/local/x` (leading slash,
        // no `/sd`) resolve into the same scopes as their `/sd/…` spellings.
        assert_eq!(
            resolve_path("/repo/n.md", Scope::Local),
            ("/sd/repo/n.md".to_string(), Scope::Tracked)
        );
        assert_eq!(
            resolve_path("/local/j.md", Scope::Tracked),
            ("/sd/local/j.md".to_string(), Scope::Local)
        );
        // A bare name lands in the current buffer's scope directory.
        assert_eq!(
            resolve_path("draft.md", Scope::Local),
            ("/sd/local/draft.md".to_string(), Scope::Local)
        );
        assert_eq!(
            resolve_path("draft.md", Scope::Tracked),
            ("/sd/repo/draft.md".to_string(), Scope::Tracked)
        );
    }

    #[test]
    fn an_edit_marks_dirty_and_mark_saved_clears_it() {
        let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "hi".into());
        assert!(!e.dirty()); // a freshly loaded buffer is clean
        e.handle(Key::Char('x')); // delete a char
        assert!(e.dirty());
        e.mark_saved("/sd/repo/a.md");
        assert!(!e.dirty());
    }

    #[test]
    fn opening_a_nonresident_file_queues_a_load() {
        let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "A".into());
        edit(&mut e, "/sd/local/j.md");
        assert_eq!(
            e.take_effects(),
            vec![Effect::Load {
                path: "/sd/local/j.md".into(),
                scope: Scope::Local,
            }]
        );
        // The active buffer does not change until the host loads and installs it.
        assert_eq!(e.path(), "/sd/repo/a.md");
    }

    #[test]
    fn install_loaded_parks_current_and_activates_the_target() {
        let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "A".into());
        e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, "hello B".into());
        assert_eq!(e.path(), "/sd/repo/b.md");
        assert_eq!(e.text(), "hello B");
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn switching_back_to_a_resident_buffer_needs_no_load() {
        let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "AAA".into());
        assert_eq!(e.caret, 2); // caret on A's last char
        e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, "BBBBB".into());
        // A is parked (resident) — switching back reads memory, not disk.
        edit(&mut e, "/sd/repo/a.md");
        assert!(e.take_effects().is_empty());
        assert_eq!(e.path(), "/sd/repo/a.md");
        assert_eq!(e.text(), "AAA");
        assert_eq!(e.caret, 2); // its caret came back with it
    }

    #[test]
    fn the_register_is_global_across_buffers() {
        let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "word".into());
        e.handle(Key::Char('y')); // yy — yank the line
        e.handle(Key::Char('y'));
        e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, String::new());
        e.handle(Key::Char('p')); // paste it into the other buffer
        assert!(e.text().contains("word"));
    }

    #[test]
    fn a_dirty_parked_buffer_is_saved_when_evicted() {
        let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "A".into());
        // Dirty the active buffer, then push it out of the ≤3 resident window.
        e.handle(Key::Char('i'));
        e.handle(Key::Char('!'));
        e.handle(Key::Escape);
        assert!(e.dirty());
        e.take_effects(); // discard anything queued so far
        e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, "B".into()); // parks A(dirty)
        e.install_loaded("/sd/repo/c.md".into(), Scope::Tracked, "C".into()); // parked: [A,B]
        assert!(e.take_effects().is_empty()); // nothing evicted yet
        e.install_loaded("/sd/repo/d.md".into(), Scope::Tracked, "D".into()); // evicts A
        let effs = e.take_effects();
        assert_eq!(effs.len(), 1, "the evicted dirty buffer must be saved");
        match &effs[0] {
            Effect::Save { path, .. } => assert_eq!(path, "/sd/repo/a.md"),
            other => panic!("expected a Save of A, got {other:?}"),
        }
    }

    #[test]
    fn a_clean_parked_buffer_is_dropped_silently_on_eviction() {
        let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "A".into());
        // A is never edited (clean); filling past ≤3 must evict it without a Save.
        e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, "B".into());
        e.install_loaded("/sd/repo/c.md".into(), Scope::Tracked, "C".into());
        e.take_effects();
        e.install_loaded("/sd/repo/d.md".into(), Scope::Tracked, "D".into());
        assert!(e.take_effects().is_empty()); // clean buffer: no save on evict
    }

    // ---- :enew / :delete (v0.5 slice 3) ----

    #[test]
    fn enew_creates_a_dirty_empty_buffer_and_asks_the_host_for_nothing() {
        let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "A".into());
        ex(&mut e, "enew draft.md");
        assert_eq!(e.path(), "/sd/repo/draft.md"); // bare name → current (Tracked) scope
        assert_eq!(e.scope(), Scope::Tracked);
        assert_eq!(e.text(), "");
        assert!(e.dirty()); // fresh + unsaved, so eviction/`:w` will persist it
        assert_eq!(e.mode(), Mode::Normal);
        // `:enew` allocates no card IO — it neither loads nor saves.
        assert!(e.take_effects().is_empty());
    }

    #[test]
    fn enew_derives_local_scope_from_the_path() {
        let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "A".into());
        ex(&mut e, "enew local/journal.md");
        assert_eq!(e.path(), "/sd/local/journal.md");
        assert_eq!(e.scope(), Scope::Local);
    }

    #[test]
    fn enew_adds_the_new_file_to_the_palette_list() {
        let mut e = palette_editor(&["/sd/repo/notes.md", "/sd/repo/todo.md"]);
        ex(&mut e, "enew draft.md");
        assert!(files_vec(&e).contains(&"/sd/repo/draft.md".to_string()));
        // and it is findable in the palette without a disk re-enumeration
        e.handle(Key::Palette);
        for c in "draft".chars() {
            e.handle(Key::Char(c));
        }
        assert_eq!(palette_labels(&e), vec!["repo/draft.md"]);
    }

    #[test]
    fn enew_of_an_already_open_file_switches_without_clobbering() {
        let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "AAA".into());
        e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, "BBB".into()); // parks A
        e.take_effects();
        ex(&mut e, "enew /sd/repo/a.md"); // A is parked (resident) — switch, don't empty it
        assert_eq!(e.path(), "/sd/repo/a.md");
        assert_eq!(e.text(), "AAA"); // contents preserved, not clobbered to empty
        assert!(e.take_effects().is_empty()); // resident: no Load
    }

    #[test]
    fn enew_without_a_name_is_a_usage_noop() {
        let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "A".into());
        ex(&mut e, "enew");
        assert_eq!(e.path(), "/sd/repo/notes.md"); // unchanged
        assert!(e.take_effects().is_empty());
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn delete_queues_a_delete_of_the_current_file() {
        let (mut e, effs) = command("delete");
        assert_eq!(
            effs,
            vec![Effect::Delete {
                path: "/sd/repo/notes.md".into(),
                scope: Scope::Tracked,
            }]
        );
        // No file remains active (nothing else was resident): a scratch buffer.
        assert_eq!(e.path(), "");
        assert_eq!(e.text(), "");
        assert_eq!(e.mode(), Mode::Normal);
        assert!(e.take_effects().is_empty());
    }

    #[test]
    fn delete_never_saves_the_discarded_buffer_even_when_dirty() {
        let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "A".into());
        e.handle(Key::Char('x')); // dirty it
        assert!(e.dirty());
        ex(&mut e, "delete");
        // The buffer is being deleted, so it is discarded, not saved: Delete only.
        assert_eq!(kinds(&e.take_effects()), vec![Kind::Delete]);
    }

    #[test]
    fn delete_switches_to_the_most_recently_parked_buffer() {
        let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "AAA".into());
        e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, "BBB".into()); // active B, A parked
        e.take_effects();
        ex(&mut e, "delete"); // deletes B, restores A
        assert_eq!(e.path(), "/sd/repo/a.md");
        assert_eq!(e.text(), "AAA"); // A came back from RAM, caret/undo with it
        match &e.take_effects()[..] {
            [Effect::Delete { path, .. }] => assert_eq!(path, "/sd/repo/b.md"),
            other => panic!("expected a single Delete of B, got {other:?}"),
        }
    }

    #[test]
    fn delete_drops_the_file_from_the_palette_list() {
        let mut e = palette_editor(&["/sd/repo/notes.md", "/sd/repo/todo.md"]);
        ex(&mut e, "delete"); // notes.md is active
        e.take_effects();
        assert!(!files_vec(&e).contains(&"/sd/repo/notes.md".to_string()));
        e.handle(Key::Palette);
        for c in "md".chars() {
            e.handle(Key::Char(c)); // reach the search threshold
        }
        assert_eq!(palette_labels(&e), vec!["repo/todo.md"]); // only the survivor
    }

    #[test]
    fn delete_of_a_local_file_carries_local_scope() {
        let mut e = Editor::with_file("/sd/local/j.md".into(), Scope::Local, "diary".into());
        ex(&mut e, "delete");
        match &e.take_effects()[..] {
            [Effect::Delete { path, scope }] => {
                assert_eq!(path, "/sd/local/j.md");
                assert_eq!(*scope, Scope::Local);
            }
            other => panic!("expected a Local Delete, got {other:?}"),
        }
    }

    #[test]
    fn delete_on_an_unnamed_buffer_is_a_noop() {
        let mut e = Editor::new(); // scratch, empty path — nothing on disk to delete
        ex(&mut e, "delete");
        assert!(e.take_effects().is_empty());
        assert_eq!(e.mode(), Mode::Normal);
    }

    // ---- File palette (Ctrl-P) ----

    /// A fresh editor over `/sd/repo/notes.md` with a palette file list.
    fn palette_editor(files: &[&str]) -> Editor {
        let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, String::new());
        e.set_file_list(files.iter().map(|s| s.to_string()).collect());
        e
    }

    /// The palette's current result as display labels, in ranked order.
    fn palette_labels(e: &Editor) -> Vec<&str> {
        e.palette_matches().iter().map(|&i| palette_label(e.file_at(i))).collect()
    }

    /// The interned file list back as owned strings, in base (sorted) order.
    fn files_vec(e: &Editor) -> Vec<String> {
        (0..e.file_count()).map(|i| e.file_at(i).to_string()).collect()
    }

    #[test]
    fn fuzzy_score_matches_subsequence_case_insensitively() {
        assert!(fuzzy_score("notes", "repo/notes.md").is_some());
        assert!(fuzzy_score("NOTES", "repo/notes.md").is_some()); // case-insensitive
        assert!(fuzzy_score("rpnm", "repo/notes.md").is_some()); // scattered subsequence
        assert!(fuzzy_score("xyz", "repo/notes.md").is_none()); // not a subsequence
        assert_eq!(fuzzy_score("", "anything"), Some(0)); // empty query matches all
    }

    #[test]
    fn fuzzy_score_ranks_word_boundaries_above_midword() {
        // "no" after the "/" boundary in repo/notes beats a mid-word hit.
        let boundary = fuzzy_score("no", "repo/notes.md").unwrap();
        let midword = fuzzy_score("no", "cannotes.md").unwrap();
        assert!(boundary > midword, "{boundary} !> {midword}");
    }

    #[test]
    fn set_file_list_sorts_and_dedups() {
        let mut e = Editor::new();
        e.set_file_list(vec![
            "/sd/repo/b.md".into(),
            "/sd/repo/a.md".into(),
            "/sd/repo/b.md".into(),
        ]);
        assert_eq!(files_vec(&e), vec!["/sd/repo/a.md", "/sd/repo/b.md"]);
    }

    #[test]
    fn cmd_p_opens_the_palette_and_esc_closes_it() {
        let mut e = palette_editor(&["/sd/repo/notes.md", "/sd/repo/todo.md"]);
        e.handle(Key::Palette);
        assert_eq!(e.mode(), Mode::Palette);
        e.handle(Key::Escape);
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn cmd_p_toggles_the_palette_closed() {
        let mut e = palette_editor(&["/sd/repo/notes.md"]);
        e.handle(Key::Palette);
        e.handle(Key::Palette); // same chord closes
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn cmd_p_is_ignored_in_insert_mode() {
        let mut e = typed("hi");
        e.handle(Key::Palette);
        assert_eq!(e.mode(), Mode::Insert); // a Normal gesture only; no-op here
    }

    #[test]
    fn typing_filters_and_enter_opens_the_picked_file() {
        let mut e = palette_editor(&[
            "/sd/repo/notes.md",
            "/sd/repo/todo.md",
            "/sd/local/journal.md",
        ]);
        e.handle(Key::Palette);
        for c in "todo".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
        assert_eq!(e.mode(), Mode::Normal); // palette closed
        let effs = e.take_effects();
        assert_eq!(kinds(&effs), vec![Kind::Load]);
        match &effs[0] {
            Effect::Load { path, scope } => {
                assert_eq!(path, "/sd/repo/todo.md");
                assert_eq!(*scope, Scope::Tracked);
            }
            other => panic!("expected a Load, got {other:?}"),
        }
    }

    #[test]
    fn opening_a_local_file_from_the_palette_carries_local_scope() {
        let mut e = palette_editor(&["/sd/repo/notes.md", "/sd/local/journal.md"]);
        e.handle(Key::Palette);
        for c in "journal".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
        match &e.take_effects()[0] {
            Effect::Load { path, scope } => {
                assert_eq!(path, "/sd/local/journal.md");
                assert_eq!(*scope, Scope::Local); // scope derived from the /sd/local path
            }
            other => panic!("expected a Local Load, got {other:?}"),
        }
    }

    #[test]
    fn opening_the_active_file_from_the_palette_is_a_noop() {
        let mut e = palette_editor(&["/sd/repo/notes.md", "/sd/repo/todo.md"]);
        e.handle(Key::Palette);
        for c in "notes".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
        // notes.md is already active: no Load, just a closed palette.
        assert!(e.take_effects().is_empty());
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn half_page_keys_move_the_selection_wrapping() {
        let mut e = palette_editor(&["/sd/repo/a.md", "/sd/repo/b.md", "/sd/repo/c.md"]);
        e.handle(Key::Palette);
        for ch in "md".chars() {
            e.handle(Key::Char(ch)); // reach the search threshold: all three match
        }
        assert_eq!(e.palette_sel, 0);
        e.handle(Key::HalfPageDown);
        assert_eq!(e.palette_sel, 1);
        e.handle(Key::HalfPageDown);
        e.handle(Key::HalfPageDown); // wraps past the last row to the top
        assert_eq!(e.palette_sel, 0);
        e.handle(Key::HalfPageUp); // wraps back to the last row
        assert_eq!(e.palette_sel, 2);
    }

    #[test]
    fn ctrl_n_p_navigate_the_palette() {
        let mut e = palette_editor(&["/sd/repo/a.md", "/sd/repo/b.md", "/sd/repo/c.md"]);
        e.handle(Key::Palette);
        for ch in "md".chars() {
            e.handle(Key::Char(ch)); // reach the search threshold: all three match
        }
        e.handle(Key::Down); // Ctrl-n
        assert_eq!(e.palette_sel, 1);
        e.handle(Key::Down);
        e.handle(Key::Down); // wraps past the last row to the top
        assert_eq!(e.palette_sel, 0);
        e.handle(Key::Up); // Ctrl-p wraps back to the last row
        assert_eq!(e.palette_sel, 2);
    }

    #[test]
    fn ctrl_n_p_move_by_a_line_in_normal_mode() {
        // Three lines; caret starts on the last char of line 3 (with_file posture).
        let mut e = Editor::with_file("/sd/repo/n.md".into(), Scope::Tracked, "aa\nbb\ncc".into());
        e.handle(Key::Char('g')); // gg → top (line 1)
        e.handle(Key::Char('g'));
        assert_eq!(e.caret, 0);
        e.handle(Key::Down); // Ctrl-n → line 2
        assert_eq!(e.caret, 3); // start of "bb"
        e.handle(Key::Down); // → line 3
        assert_eq!(e.caret, 6); // start of "cc"
        e.handle(Key::Up); // Ctrl-p → line 2
        assert_eq!(e.caret, 3);
    }

    #[test]
    fn ctrl_n_takes_a_count_in_normal_mode() {
        let mut e = Editor::with_file("/sd/repo/n.md".into(), Scope::Tracked, "aa\nbb\ncc".into());
        e.handle(Key::Char('g'));
        e.handle(Key::Char('g')); // top
        e.handle(Key::Char('2'));
        e.handle(Key::Down); // 2<C-n> → down two lines
        assert_eq!(e.caret, 6); // start of "cc"
    }

    #[test]
    fn ctrl_n_p_scroll_in_view_mode() {
        let mut e = Editor::with_file("/sd/repo/n.md".into(), Scope::Tracked, "a\nb\nc\nd".into());
        e.handle(Key::Char('g'));
        e.handle(Key::Char('r')); // gr → View
        assert_eq!(e.mode(), Mode::View);
        let top = e.scroll_top();
        e.handle(Key::Down); // Ctrl-n scrolls like j
        assert_eq!(e.scroll_top(), top + 1);
        e.handle(Key::Up); // Ctrl-p scrolls like k
        assert_eq!(e.scroll_top(), top);
    }

    #[test]
    fn editing_the_query_resets_the_selection_to_the_top() {
        let mut e = palette_editor(&["/sd/repo/a.md", "/sd/repo/b.md"]);
        e.handle(Key::Palette);
        for ch in "md".chars() {
            e.handle(Key::Char(ch)); // reach the search threshold: both match
        }
        e.handle(Key::HalfPageDown);
        assert_eq!(e.palette_sel, 1);
        e.handle(Key::Char('a')); // a query edit resets the selection
        assert_eq!(e.palette_sel, 0);
    }

    #[test]
    fn backspace_on_an_empty_query_closes_the_palette() {
        let mut e = palette_editor(&["/sd/repo/a.md"]);
        e.handle(Key::Palette);
        e.handle(Key::Backspace);
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn short_query_lists_recents_only() {
        let mut e = palette_editor(&["/sd/repo/b.md", "/sd/repo/a.md", "/sd/repo/c.md"]);
        // No opens yet: below the search threshold there is nothing to show.
        assert!(palette_labels(&e).is_empty());
        // Open c.md through the palette; it becomes the recents-only result.
        e.handle(Key::Palette);
        for ch in "c.md".chars() {
            e.handle(Key::Char(ch));
        }
        e.handle(Key::Enter);
        e.take_effects(); // drop the queued Load; we only care about the MRU
        assert_eq!(palette_labels(&e), vec!["repo/c.md"]);
    }

    #[test]
    fn two_char_query_reveals_the_full_file_list() {
        let mut e = palette_editor(&["/sd/repo/b.md", "/sd/repo/a.md", "/sd/repo/c.md"]);
        e.handle(Key::Palette);
        e.handle(Key::Char('m')); // one char: still recents-only (none yet)
        assert!(e.palette_matches().is_empty());
        e.handle(Key::Char('d')); // "md": the full list, fuzzy-ranked
        assert_eq!(palette_labels(&e), vec!["repo/a.md", "repo/b.md", "repo/c.md"]);
    }

    #[test]
    fn recents_float_above_the_full_list_on_a_matching_query() {
        let mut e = palette_editor(&["/sd/repo/b.md", "/sd/repo/a.md", "/sd/repo/c.md"]);
        // Open c.md so it is the MRU head.
        e.handle(Key::Palette);
        for ch in "c.md".chars() {
            e.handle(Key::Char(ch));
        }
        e.handle(Key::Enter);
        e.take_effects();
        // "md" scores the three labels equally; the stable sort keeps the
        // recently-opened c.md in front of the sorted rest.
        e.handle(Key::Palette);
        for ch in "md".chars() {
            e.handle(Key::Char(ch));
        }
        assert_eq!(palette_labels(&e), vec!["repo/c.md", "repo/a.md", "repo/b.md"]);
    }

    #[test]
    fn draw_in_palette_mode_does_not_panic() {
        let mut e = palette_editor(&["/sd/repo/a.md", "/sd/local/j.md"]);
        e.handle(Key::Palette);
        let _ = e.draw(true); // empty query, no recents: "(type to search)"
        e.handle(Key::Char('j')); // one char, still below the threshold
        let _ = e.draw(true);
        e.handle(Key::Char('m')); // at the threshold: the ranked list
        let _ = e.draw(true);
        // Empty file list: the "(no files on card)" path must also be safe.
        let mut empty = Editor::new();
        empty.handle(Key::Palette);
        let _ = empty.draw(true);
    }

    // ---- Preferences (.typoena.toml) ----

    #[test]
    fn prefs_default_matches_the_documented_defaults() {
        let p = Prefs::default();
        assert!(p.save_on_idle);
        assert!(p.format_on_save);
        assert!(p.line_numbers);
        assert_eq!(p.theme, "light");
        assert_eq!(p.auto_sync, "10m");
    }

    #[test]
    fn prefs_parse_falls_back_to_defaults_for_missing_keys() {
        // Only one key present; the rest stay at their defaults.
        let p = Prefs::parse("line_numbers = false\n");
        assert!(!p.line_numbers);
        assert!(p.save_on_idle); // untouched -> default
        assert!(p.format_on_save);
        assert_eq!(p.auto_sync, "10m");
    }

    #[test]
    fn prefs_parse_reads_all_keys_and_ignores_comments_and_junk() {
        let src = "\
            # a header comment\n\
            save_on_idle = false   # trailing comment\n\
            format_on_save = false\n\
            line_numbers = false\n\
            auto_sync = \"2m\"\n\
            bogus_key = whatever\n\
            not a pair\n";
        let p = Prefs::parse(src);
        assert!(!p.save_on_idle);
        assert!(!p.format_on_save);
        assert!(!p.line_numbers);
        assert_eq!(p.auto_sync, "2m");
    }

    #[test]
    fn prefs_parse_keeps_default_on_an_unparseable_bool() {
        // A typo in a bool value leaves that key at its default, not `false`.
        let p = Prefs::parse("save_on_idle = yes\n");
        assert!(p.save_on_idle); // "yes" isn't a TOML bool -> default (true)
    }

    #[test]
    fn prefs_to_toml_round_trips_through_parse() {
        let p = Prefs {
            save_on_idle: false,
            format_on_save: true,
            line_numbers: false,
            theme: "dark".into(),
            auto_sync: "5m".into(),
        };
        assert_eq!(Prefs::parse(&p.to_toml()), p);
    }

    #[test]
    fn prefs_parse_reads_theme_and_auto_sync_strings() {
        let p = Prefs::parse("theme = \"dark\"\nauto_sync = \"15m\"\n");
        assert_eq!(p.theme, "dark");
        assert_eq!(p.auto_sync, "15m");
    }

    #[test]
    fn empty_prefs_file_yields_defaults() {
        assert_eq!(Prefs::parse(""), Prefs::default());
    }

    // ---- line_numbers pref (live gutter toggle) ----

    #[test]
    fn line_numbers_off_reclaims_the_gutter_columns() {
        let mut e = Editor::with_text("one\ntwo\nthree".into());
        assert!(e.text_cols() < WRITE_COLS); // gutter present by default
        e.prefs.line_numbers = false;
        assert_eq!(e.gutter_cols(), 0);
        assert_eq!(e.text_cols(), WRITE_COLS); // full width reclaimed
    }

    #[test]
    fn draw_with_line_numbers_off_does_not_panic() {
        // The `gutter - 1` field width would underflow if unguarded.
        let mut e = Editor::with_text("alpha\nbeta\ngamma".into());
        e.prefs.line_numbers = false;
        let _ = e.draw(true);
    }

    // ---- Palette command mode (`>`) ----

    /// Open the palette and type `query` (so `>...` enters command mode).
    fn palette_type(files: &[&str], query: &str) -> Editor {
        let mut e = palette_editor(files);
        e.handle(Key::Palette);
        for c in query.chars() {
            e.handle(Key::Char(c));
        }
        e
    }

    #[test]
    fn leading_gt_switches_the_palette_to_command_mode() {
        let e = palette_type(&["/sd/repo/notes.md"], ">");
        assert!(e.palette_command_mode());
        // Every prefs command is offered on a bare `>`.
        assert_eq!(e.palette_command_matches().len(), PALETTE_CMDS.len());
    }

    #[test]
    fn backspacing_the_gt_returns_to_file_mode() {
        let mut e = palette_type(&["/sd/repo/notes.md"], ">");
        assert!(e.palette_command_mode());
        e.handle(Key::Backspace);
        assert!(!e.palette_command_mode());
        assert_eq!(e.mode(), Mode::Palette); // still open, just file mode again
    }

    #[test]
    fn command_filter_fuzzy_matches_the_label() {
        let e = palette_type(&["/sd/repo/notes.md"], ">line");
        let matches = e.palette_command_matches();
        assert_eq!(matches.len(), 1);
        assert_eq!(PALETTE_CMDS[matches[0]], PaletteCmd::LineNumbers);
    }

    #[test]
    fn command_label_reflects_current_pref_state() {
        let e = palette_editor(&["/sd/repo/notes.md"]);
        assert_eq!(e.command_label(PaletteCmd::LineNumbers), "line numbers: on");
    }

    #[test]
    fn running_a_command_toggles_the_pref_live_and_queues_a_save_prefs() {
        // >line<Enter> flips line_numbers off, in-core, and asks the host to persist.
        let mut e = palette_type(&["/sd/repo/notes.md"], ">line");
        assert!(e.prefs().line_numbers);
        e.handle(Key::Enter);
        assert!(!e.prefs().line_numbers); // applied live
        assert_eq!(e.mode(), Mode::Palette); // stays open for more toggles
        assert_eq!(kinds(&e.take_effects()), vec![Kind::SavePrefs]);
    }

    #[test]
    fn command_mode_stays_open_across_multiple_toggles() {
        // Flip line numbers, then retype the query for save-on-idle and flip that,
        // without the palette closing between them; each toggle persists. Navigate
        // by fuzzy filter, not registry position.
        let mut e = palette_type(&["/sd/repo/notes.md"], ">line");
        let before = e.prefs().line_numbers;
        e.handle(Key::Enter);
        assert_eq!(e.prefs().line_numbers, !before); // applied live
        assert_eq!(e.mode(), Mode::Palette); // stays open
        assert_eq!(kinds(&e.take_effects()), vec![Kind::SavePrefs]);
        // Still open: retype the query for a different pref (">idle" is unique to
        // "save on idle").
        for _ in 0.."line".len() {
            e.handle(Key::Backspace);
        }
        for c in "idle".chars() {
            e.handle(Key::Char(c));
        }
        let before = e.prefs().save_on_idle;
        e.handle(Key::Enter);
        assert_eq!(e.prefs().save_on_idle, !before);
        assert_eq!(e.mode(), Mode::Palette);
        assert_eq!(kinds(&e.take_effects()), vec![Kind::SavePrefs]);
    }

    #[test]
    fn save_prefs_carries_the_serialized_prefs() {
        let mut e = palette_type(&["/sd/repo/notes.md"], ">line");
        e.handle(Key::Enter);
        let effects = e.take_effects();
        let Effect::SavePrefs { contents } = &effects[0] else {
            panic!("expected SavePrefs, got {effects:?}");
        };
        // The written TOML reflects the toggled state and round-trips.
        assert!(!Prefs::parse(contents).line_numbers);
    }

    #[test]
    fn running_a_command_confirms_the_new_state_on_the_snackbar() {
        let mut e = palette_type(&["/sd/repo/notes.md"], ">save");
        e.handle(Key::Enter);
        // save_on_idle default true -> off; the notice names the new state.
        assert_eq!(e.notice.as_deref(), Some("save on idle: off - saved"));
    }

    // ---- Preset (non-boolean) prefs: rotate through options on Enter ----

    #[test]
    fn next_option_rotates_and_wraps() {
        assert_eq!(next_option("light", &THEME_OPTIONS), "dark");
        assert_eq!(next_option("dark", &THEME_OPTIONS), "light"); // wraps
        assert_eq!(next_option("10m", &AUTO_SYNC_OPTIONS), "15m");
        assert_eq!(next_option("30m", &AUTO_SYNC_OPTIONS), "2m"); // wraps
    }

    #[test]
    fn next_option_snaps_an_unknown_value_to_the_head() {
        // A hand-typed value outside the preset list lands on the first option.
        assert_eq!(next_option("sepia", &THEME_OPTIONS), "light");
        assert_eq!(next_option("7m", &AUTO_SYNC_OPTIONS), "2m");
    }

    #[test]
    fn theme_command_label_reflects_the_current_value() {
        let e = palette_editor(&["/sd/repo/notes.md"]);
        assert_eq!(e.command_label(PaletteCmd::Theme), "theme: light");
        assert_eq!(e.command_label(PaletteCmd::AutoSync), "auto sync: 10m");
    }

    #[test]
    fn running_the_theme_command_rotates_the_preset_and_queues_a_save_prefs() {
        // >theme<Enter> flips light -> dark, in-core, and asks the host to persist.
        let mut e = palette_type(&["/sd/repo/notes.md"], ">theme");
        assert_eq!(e.prefs().theme, "light");
        e.handle(Key::Enter);
        assert_eq!(e.prefs().theme, "dark"); // applied live
        assert_eq!(e.mode(), Mode::Palette); // stays open for more changes
        assert_eq!(kinds(&e.take_effects()), vec![Kind::SavePrefs]);
        e.handle(Key::Enter);
        assert_eq!(e.prefs().theme, "light"); // rotates back, wrapping
    }

    #[test]
    fn running_the_auto_sync_command_walks_the_interval_presets() {
        // Default 10m; Enter rotates 10m -> 15m -> 30m -> 2m (wrap).
        let mut e = palette_type(&["/sd/repo/notes.md"], ">auto");
        assert_eq!(e.prefs().auto_sync, "10m");
        for expected in ["15m", "30m", "2m"] {
            e.handle(Key::Enter);
            assert_eq!(e.prefs().auto_sync, expected);
        }
    }

    #[test]
    fn dark_theme_inverts_the_whole_frame() {
        // The dark frame is the exact bitwise inverse of the light one.
        let mut e = Editor::with_text("hello world".into());
        e.caret = 0;
        let light = e.draw(true).bytes().to_vec();
        e.prefs.theme = "dark".into();
        let dark = e.draw(true).bytes().to_vec();
        assert_eq!(light.len(), dark.len());
        assert!(light.iter().zip(&dark).all(|(l, d)| *l == !*d));
    }

    #[test]
    fn a_no_match_command_query_runs_nothing() {
        let mut e = palette_type(&["/sd/repo/notes.md"], ">zzzzz");
        assert!(e.palette_command_matches().is_empty());
        let before = e.prefs().clone();
        e.handle(Key::Enter);
        assert_eq!(e.prefs(), &before); // nothing toggled
        assert!(e.take_effects().is_empty()); // nothing queued
        assert_eq!(e.mode(), Mode::Palette); // stays open so the query can be fixed
    }

    #[test]
    fn ctrl_n_moves_the_command_selection_wrapping() {
        let mut e = palette_type(&["/sd/repo/notes.md"], ">");
        for _ in 0..PALETTE_CMDS.len() - 1 {
            e.handle(Key::Down); // Ctrl-N down to the last command
        }
        assert_eq!(e.palette_sel, PALETTE_CMDS.len() - 1);
        e.handle(Key::Down); // wraps back to the top
        assert_eq!(e.palette_sel, 0);
        e.handle(Key::Up); // and Ctrl-P wraps back to the bottom
        assert_eq!(e.palette_sel, PALETTE_CMDS.len() - 1);
    }

    #[test]
    fn settings_command_opens_the_palette_in_command_mode() {
        let (e, _) = command("settings");
        assert_eq!(e.mode(), Mode::Palette);
        assert!(e.palette_command_mode()); // dropped straight into `>` mode
        assert_eq!(e.palette_command_matches().len(), PALETTE_CMDS.len());
    }

    #[test]
    fn draw_in_command_mode_does_not_panic() {
        let mut e = palette_type(&["/sd/repo/notes.md"], ">");
        let _ = e.draw(true);
        let mut none = palette_type(&["/sd/repo/notes.md"], ">zzzzz"); // "(no command)"
        let _ = none.draw(true);
    }

    // ---- `>` command palette generalisation (v0.6) ----

    #[test]
    fn command_labels_for_the_new_actions() {
        let e = Editor::new();
        assert_eq!(e.command_label(PaletteCmd::NewFile), "new file...");
        assert_eq!(e.command_label(PaletteCmd::Format), "format");
        assert_eq!(e.command_label(PaletteCmd::Publish), "publish");
    }

    #[test]
    fn format_command_runs_and_closes() {
        // A one-shot: it formats the buffer (extra trailing blanks collapse) and
        // closes the palette — and, being an action not a toggle, queues no
        // SavePrefs. `>format` ranks the action above `format on save` (registry
        // order breaks the tie).
        let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "a\n\n\n".into());
        e.handle(Key::Palette);
        for c in ">format".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
        assert_eq!(e.text(), "a\n"); // formatted → not the FormatOnSave toggle
        assert_eq!(e.mode(), Mode::Normal); // one-shot closes
        assert!(!kinds(&e.take_effects()).contains(&Kind::SavePrefs));
    }

    #[test]
    fn publish_command_saves_and_pushes_then_closes() {
        let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "hi".into());
        e.handle(Key::Palette);
        for c in ">publish".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
        assert_eq!(e.mode(), Mode::Normal);
        assert_eq!(kinds(&e.take_effects()), vec![Kind::Save, Kind::Publish]);
    }

    #[test]
    fn publish_command_is_unavailable_in_a_local_buffer() {
        let mut e = Editor::with_file("/sd/local/j.md".into(), Scope::Local, "hi".into());
        e.handle(Key::Palette);
        for c in ">publish".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
        assert_eq!(e.mode(), Mode::Normal);
        assert!(e.take_effects().is_empty()); // Local never reaches the remote
    }

    #[test]
    fn new_file_command_opens_the_filename_input_step() {
        let mut e = palette_type(&["/sd/repo/notes.md"], ">new");
        e.handle(Key::Enter);
        assert_eq!(e.mode(), Mode::Palette); // still open — now the input step
        assert_eq!(e.palette_step, PaletteStep::NewFile);
        assert!(e.palette_query.is_empty()); // the list filter gave way to the name
    }

    #[test]
    fn new_file_step_creates_the_typed_file_and_switches() {
        let mut e = palette_type(&["/sd/repo/notes.md"], ">new");
        e.handle(Key::Enter); // → input step
        for c in "draft.md".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
        assert_eq!(e.mode(), Mode::Normal); // committed and closed
        assert_eq!(e.path(), "/sd/repo/draft.md"); // scope inherited from notes.md
        assert!(e.dirty()); // a fresh, unsaved file
    }

    #[test]
    fn new_file_step_backspace_returns_to_the_command_list() {
        let mut e = palette_type(&["/sd/repo/notes.md"], ">new");
        e.handle(Key::Enter); // input step, empty name
        e.handle(Key::Backspace); // nothing to erase → step back to the `>` list
        assert_eq!(e.palette_step, PaletteStep::List);
        assert!(e.palette_command_mode()); // query restored to ">"
        assert_eq!(e.mode(), Mode::Palette);
    }

    #[test]
    fn new_file_step_empty_enter_stays_in_the_step() {
        let mut e = palette_type(&["/sd/repo/notes.md"], ">new");
        e.handle(Key::Enter); // input step
        e.handle(Key::Enter); // empty name → no-op, still awaiting one
        assert_eq!(e.palette_step, PaletteStep::NewFile);
        assert_eq!(e.mode(), Mode::Palette);
        assert_eq!(e.path(), "/sd/repo/notes.md"); // unchanged
    }

    #[test]
    fn draw_in_new_file_step_does_not_panic() {
        let mut e = palette_type(&["/sd/repo/notes.md"], ">new");
        e.handle(Key::Enter);
        let _ = e.draw(true); // empty-name placeholder path
        for c in "draft.md".chars() {
            e.handle(Key::Char(c));
        }
        let _ = e.draw(true); // with a typed name
    }

    #[test]
    fn e_command_is_retired() {
        // `:e` no longer opens files — bare Cmd-P does. `:e foo` is now a no-op.
        let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "A".into());
        ex(&mut e, "e /sd/repo/b.md");
        assert_eq!(e.path(), "/sd/repo/a.md"); // unchanged
        assert!(e.take_effects().is_empty()); // nothing queued
    }

    // ---- Snippets (v0.6) ----

    fn with_snippets(json: &str) -> Editor {
        let mut e = Editor::new();
        e.set_snippets(Snippets::parse(json).unwrap());
        e
    }

    #[test]
    fn strip_labels_reduces_placeholders_to_bare_stops() {
        assert_eq!(strip_stop_labels("# ${1:Titre}"), "# $1");
        assert_eq!(strip_stop_labels("${2}"), "$2");
        assert_eq!(strip_stop_labels("[$1]($2)$0"), "[$1]($2)$0"); // plain stops untouched
        assert_eq!(strip_stop_labels("price: $ and ${3:x}"), "price: $ and $3"); // lone $ kept
    }

    #[test]
    fn parse_body_extracts_literal_and_visit_order() {
        let (lit, stops) = parse_snippet_body("[$1]($2)$0");
        assert_eq!(lit, "[]()");
        assert_eq!(stops, vec![1, 3, 4]); // $1, $2, then $0 (end) last
    }

    #[test]
    fn parse_body_appends_implicit_final_stop_when_no_zero() {
        let (lit, stops) = parse_snippet_body("# $1\n## $2");
        assert_eq!(lit, "# \n## ");
        assert_eq!(stops, vec![2, 6, 6]); // $1, $2, implicit rest at the end
    }

    #[test]
    fn parse_body_no_stops_has_empty_visit_list() {
        let (lit, stops) = parse_snippet_body("- [ ] ");
        assert_eq!(lit, "- [ ] ");
        assert!(stops.is_empty());
    }

    #[test]
    fn parse_snippets_reads_zed_json_string_and_array_bodies() {
        // r###"…"### so the `"#`/`"##` in the heading bodies don't close the string.
        let json = r###"{
            "Link": { "prefix": "link", "body": "[$1]($2)$0", "description": "Inline link" },
            "Book notes": { "prefix": "booknotes", "body": ["# ${1:Titre}", "## $2"] }
        }"###;
        let s = Snippets::parse(json).unwrap().0;
        assert_eq!(s.len(), 2);
        // BTreeMap parse → sorted by display name ("Book notes" < "Link").
        assert_eq!(s[0].name, "Book notes");
        assert_eq!(s[0].prefix, "booknotes");
        assert_eq!(s[0].body, "# $1\n## $2"); // array joined with \n, label stripped
        assert_eq!(s[0].description, ""); // omitted → empty
        assert_eq!(s[1].name, "Link");
        assert_eq!(s[1].body, "[$1]($2)$0");
        assert_eq!(s[1].description, "Inline link");
    }

    #[test]
    fn parse_snippets_empty_and_malformed() {
        assert!(Snippets::parse("{}").unwrap().0.is_empty());
        assert!(Snippets::parse("").unwrap().0.is_empty()); // empty file = no snippets
        assert!(Snippets::parse(" \n\t").unwrap().0.is_empty()); // whitespace-only too
        assert!(Snippets::parse("{ not json").is_err()); // host logs, boots with none
    }

    #[test]
    fn tab_expands_prefix_and_lands_on_first_stop() {
        let mut e = with_snippets(r#"{ "Link": { "prefix": "link", "body": "[$1]($2)$0" } }"#);
        e.handle(Key::Char('i'));
        for c in "link".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Char('\t')); // expand
        assert_eq!(e.text, "[]()"); // trigger word replaced by the expansion
        assert_eq!(e.caret, 1); // caret on $1
        assert_eq!(e.mode(), Mode::Insert);
        assert_eq!(e.snippet_stops, vec![3, 4]); // $2 then $0 pending
    }

    #[test]
    fn tab_advances_stops_and_typing_shifts_pending_ones() {
        let mut e = with_snippets(r#"{ "Link": { "prefix": "link", "body": "[$1]($2)$0" } }"#);
        e.handle(Key::Char('i'));
        for c in "link".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Char('\t'));
        for c in "url".chars() {
            e.handle(Key::Char(c)); // type at $1
        }
        assert_eq!(e.text, "[url]()");
        assert_eq!(e.snippet_stops, vec![6, 7]); // pending shifted by +3
        e.handle(Key::Char('\t')); // → $2
        assert_eq!(e.caret, 6);
        assert_eq!(e.snippet_stops, vec![7]);
        for c in "http".chars() {
            e.handle(Key::Char(c));
        }
        assert_eq!(e.text, "[url](http)");
        e.handle(Key::Char('\t')); // → $0 (end); session ends
        assert_eq!(e.caret, 11);
        assert!(e.snippet_stops.is_empty());
    }

    #[test]
    fn esc_ends_the_snippet_session() {
        let mut e = with_snippets(r#"{ "Link": { "prefix": "link", "body": "[$1]($2)$0" } }"#);
        e.handle(Key::Char('i'));
        for c in "link".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Char('\t'));
        assert!(!e.snippet_stops.is_empty());
        e.handle(Key::Escape);
        assert_eq!(e.mode(), Mode::Normal);
        assert!(e.snippet_stops.is_empty(), "leaving Insert ends the session");
    }

    #[test]
    fn tab_without_matching_prefix_inserts_spaces() {
        let mut e = with_snippets(r#"{ "Link": { "prefix": "link", "body": "[$1]($2)$0" } }"#);
        e.handle(Key::Char('i'));
        for c in "zzz".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Char('\t'));
        assert!(e.text.starts_with("zzz"));
        assert!(e.text.len() > 3, "tab inserted whitespace, not an expansion");
        assert!(e.snippet_stops.is_empty());
    }

    #[test]
    fn no_stop_snippet_expands_without_a_session() {
        let mut e = with_snippets(r#"{ "Todo": { "prefix": "todo", "body": "- [ ] " } }"#);
        e.handle(Key::Char('i'));
        for c in "todo".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Char('\t'));
        assert_eq!(e.text, "- [ ] ");
        assert_eq!(e.caret, 6); // caret at the end, no session
        assert!(e.snippet_stops.is_empty());
    }

    #[test]
    fn undo_after_expansion_restores_the_trigger_word() {
        let mut e = with_snippets(r#"{ "Link": { "prefix": "link", "body": "[$1]($2)$0" } }"#);
        e.handle(Key::Char('i'));
        for c in "link".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Char('\t'));
        assert_eq!(e.text, "[]()");
        e.handle(Key::Escape);
        e.handle(Key::Char('u')); // undo the whole expansion
        assert_eq!(e.text, "link");
    }

    // ---- `$` snippet palette ----

    // r##"…"## so the `"#` in the `"# $1"` heading body doesn't close the string.
    const TWO_SNIPPETS: &str = r##"{
        "Markdown link": { "prefix": "link", "body": "[$1]($2)$0", "description": "Inline link" },
        "Book notes": { "prefix": "booknotes", "body": "# $1", "description": "Reading fiche" }
    }"##;

    /// Open the palette on an editor with `json`'s snippets loaded, then type `q`.
    fn snippet_palette(json: &str, q: &str) -> Editor {
        let mut e = with_snippets(json);
        e.handle(Key::Palette);
        for c in q.chars() {
            e.handle(Key::Char(c));
        }
        e
    }

    #[test]
    fn dollar_switches_the_palette_to_snippet_mode() {
        let e = snippet_palette(TWO_SNIPPETS, "$");
        assert!(e.palette_snippet_mode());
        assert!(!e.palette_command_mode());
        // A bare `$` lists every snippet.
        assert_eq!(e.palette_snippet_matches().len(), 2);
    }

    #[test]
    fn backspacing_the_dollar_returns_to_file_mode() {
        let mut e = snippet_palette(TWO_SNIPPETS, "$");
        assert!(e.palette_snippet_mode());
        e.handle(Key::Backspace);
        assert!(!e.palette_snippet_mode());
        assert_eq!(e.mode(), Mode::Palette); // still open, file mode again
    }

    #[test]
    fn snippet_filter_fuzzy_matches_name_prefix_and_description() {
        // Query the prefix.
        let e = snippet_palette(TWO_SNIPPETS, "$link");
        let m = e.palette_snippet_matches();
        assert_eq!(m.len(), 1);
        assert_eq!(e.snippets[m[0]].name, "Markdown link");
        // Query a word only in the *description* ("fiche") finds the other one.
        let e = snippet_palette(TWO_SNIPPETS, "$fiche");
        let m = e.palette_snippet_matches();
        assert_eq!(m.len(), 1);
        assert_eq!(e.snippets[m[0]].name, "Book notes");
    }

    #[test]
    fn enter_in_snippet_mode_inserts_and_starts_the_session() {
        let mut e = snippet_palette(TWO_SNIPPETS, "$link");
        e.handle(Key::Enter);
        assert_eq!(e.mode(), Mode::Insert); // dropped into the buffer at $1
        assert_eq!(e.text, "[]()");
        assert_eq!(e.caret, 1); // on $1
        assert_eq!(e.snippet_stops, vec![3, 4]); // $2 then $0 pending
        // Inserting content closes the palette (unlike a `>` toggle, which stays).
        // A follow-up Esc must leave Insert, not reopen anything.
        e.handle(Key::Escape);
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn snippet_palette_insertion_is_one_undo_group() {
        let mut e = snippet_palette(TWO_SNIPPETS, "$link");
        e.handle(Key::Enter);
        e.handle(Key::Escape); // end the session, back to Normal
        e.handle(Key::Char('u')); // undo the whole insertion
        assert_eq!(e.text, ""); // buffer restored to its pre-insert state
    }

    #[test]
    fn ctrl_n_wraps_around_the_snippet_result_list() {
        let mut e = snippet_palette(TWO_SNIPPETS, "$");
        e.handle(Key::Down);
        assert_eq!(e.palette_sel, 1); // two snippets → last index is 1
        e.handle(Key::Down); // past the end — wraps to the top
        assert_eq!(e.palette_sel, 0);
        e.handle(Key::Up); // before the top — wraps to the bottom
        assert_eq!(e.palette_sel, 1);
    }

    #[test]
    fn empty_snippet_library_matches_nothing() {
        let mut e = Editor::new(); // no snippets set
        e.handle(Key::Palette);
        e.handle(Key::Char('$'));
        assert!(e.palette_snippet_mode());
        assert!(e.palette_snippet_matches().is_empty());
        e.handle(Key::Enter); // no-op, stays open
        assert_eq!(e.mode(), Mode::Palette);
        let _ = e.draw(true); // "(no snippets)" path must not panic
    }

    #[test]
    fn draw_in_snippet_mode_does_not_panic() {
        let mut e = snippet_palette(TWO_SNIPPETS, "$");
        let _ = e.draw(true);
        let mut filtered = snippet_palette(TWO_SNIPPETS, "$link");
        let _ = filtered.draw(true);
    }

    // ---- hint-on-pause ----

    /// Type `word` into a fresh Insert-mode buffer with the two snippets loaded.
    fn typed_in_insert(word: &str) -> Editor {
        let mut e = with_snippets(TWO_SNIPPETS);
        e.handle(Key::Char('i'));
        for c in word.chars() {
            e.handle(Key::Char(c));
        }
        e
    }

    #[test]
    fn pause_hint_names_the_snippet_a_prefix_would_expand() {
        let mut e = typed_in_insert("link");
        assert_eq!(e.snippet_hint, None); // not computed per keystroke
        e.refresh_stats(); // the typing-pause throttle
        assert_eq!(e.snippet_hint.as_deref(), Some("Markdown link"));
    }

    #[test]
    fn pause_hint_is_absent_without_a_matching_prefix() {
        let mut e = typed_in_insert("zz");
        e.refresh_stats();
        assert_eq!(e.snippet_hint, None);
    }

    #[test]
    fn pause_hint_clears_when_leaving_insert() {
        let mut e = typed_in_insert("link");
        e.refresh_stats();
        assert!(e.snippet_hint.is_some());
        e.handle(Key::Escape); // → Normal
        e.refresh_stats(); // the main loop refreshes on non-Insert actions
        assert_eq!(e.snippet_hint, None);
    }

    #[test]
    fn pause_hint_is_absent_during_a_live_session() {
        let mut e = typed_in_insert("link");
        e.handle(Key::Char('\t')); // expand → session live, caret on $1
        assert!(!e.snippet_stops.is_empty());
        e.refresh_stats();
        assert_eq!(e.snippet_hint, None, "mid-session Tab advances, not expands");
    }

    #[test]
    fn draw_with_a_pause_hint_does_not_panic() {
        let mut e = typed_in_insert("link");
        e.refresh_stats();
        assert!(e.snippet_hint.is_some());
        let _ = e.draw(true); // the `» name` panel row must render cleanly
    }

    // --- `/` search (v0.7) --------------------------------------------------

    /// A fresh Normal-mode editor over `text`, caret normalized to 0 with `gg`
    /// (a loaded file resumes at its end).
    fn over(text: &str) -> Editor {
        let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, text.into());
        e.handle(Key::Char('g'));
        e.handle(Key::Char('g'));
        e
    }

    /// Run `/{pat}<Enter>` on `e`.
    fn search(e: &mut Editor, pat: &str) {
        e.handle(Key::Char('/'));
        for c in pat.chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
    }

    #[test]
    fn slash_opens_the_search_prompt_and_esc_cancels() {
        let mut e = over("alpha beta");
        e.handle(Key::Char('/'));
        assert_eq!(e.mode(), Mode::Command); // command-line mode, `/` prompt
        e.handle(Key::Char('b'));
        e.handle(Key::Escape);
        assert_eq!(e.mode(), Mode::Normal);
        assert_eq!(e.caret, 0); // cancelled search never moves the caret
    }

    #[test]
    fn search_jumps_past_the_caret_to_the_next_match() {
        let mut e = over("alpha beta alpha");
        search(&mut e, "alpha");
        // The caret sits on the first "alpha"; search starts *after* it.
        assert_eq!(e.caret, 11);
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn search_wraps_to_the_top_with_a_notice() {
        let mut e = over("alpha beta");
        search(&mut e, "beta"); // caret → 6
        search(&mut e, "alpha"); // no match after 6 → wraps to 0
        assert_eq!(e.caret, 0);
        assert_eq!(e.notice.as_deref(), Some("wrapped"));
    }

    #[test]
    fn search_not_found_keeps_the_caret_and_says_so() {
        let mut e = over("alpha beta");
        search(&mut e, "gamma");
        assert_eq!(e.caret, 0);
        assert_eq!(e.notice.as_deref(), Some("not found: gamma"));
    }

    #[test]
    fn n_repeats_forward_and_wraps() {
        let mut e = over("ab x ab x ab");
        search(&mut e, "ab"); // → 5
        assert_eq!(e.caret, 5);
        e.handle(Key::Char('n')); // → 10
        assert_eq!(e.caret, 10);
        e.handle(Key::Char('n')); // wraps → 0
        assert_eq!(e.caret, 0);
        assert_eq!(e.notice.as_deref(), Some("wrapped"));
    }

    #[test]
    fn capital_n_repeats_backward_and_wraps() {
        let mut e = over("ab x ab x ab");
        search(&mut e, "ab"); // → 5
        e.handle(Key::Char('N')); // back → 0
        assert_eq!(e.caret, 0);
        e.handle(Key::Char('N')); // wraps to the last match → 10
        assert_eq!(e.caret, 10);
        assert_eq!(e.notice.as_deref(), Some("wrapped"));
    }

    #[test]
    fn count_applies_to_n() {
        let mut e = over("ab ab ab ab");
        search(&mut e, "ab"); // → 3
        e.handle(Key::Char('2'));
        e.handle(Key::Char('n')); // 2 matches forward → 9
        assert_eq!(e.caret, 9);
    }

    #[test]
    fn n_without_a_previous_search_says_so() {
        let mut e = over("alpha");
        e.handle(Key::Char('n'));
        assert_eq!(e.caret, 0);
        assert_eq!(e.notice.as_deref(), Some("no previous search"));
    }

    #[test]
    fn empty_slash_repeats_the_last_search() {
        let mut e = over("ab x ab x ab");
        search(&mut e, "ab"); // → 5
        search(&mut e, ""); // bare `/` Enter reuses "ab" → 10
        assert_eq!(e.caret, 10);
    }

    #[test]
    fn lowercase_search_is_case_insensitive() {
        let mut e = over("x Alpha alpha");
        search(&mut e, "alpha");
        assert_eq!(e.caret, 2); // "Alpha" matches "alpha"
    }

    #[test]
    fn smartcase_a_capital_makes_the_search_exact() {
        let mut e = over("x paris Paris");
        search(&mut e, "Paris"); // capital → case-sensitive
        assert_eq!(e.caret, 8); // skips the lowercase "paris"
        e.handle(Key::Char('g'));
        e.handle(Key::Char('g'));
        search(&mut e, "paris"); // all-lowercase → insensitive again
        assert_eq!(e.caret, 2);
    }

    #[test]
    fn search_folds_accents_both_ways() {
        let mut e = over("x Été bien"); // 'É' (2 bytes) folds to 'e'
        search(&mut e, "été");
        assert_eq!(e.caret, 2);
        assert_eq!(&e.text[e.caret..e.caret + 5], "Été");
        e.handle(Key::Char('g'));
        e.handle(Key::Char('g'));
        search(&mut e, "ete"); // bare ascii finds the accented word too
        assert_eq!(e.caret, 2);
    }

    #[test]
    fn smartcase_still_folds_accents() {
        let mut e = over("x ete Ete");
        search(&mut e, "Été"); // capital É → case-sensitive, but é still = e
        assert_eq!(e.caret, 6); // matches "Ete", not "ete"
    }

    #[test]
    fn backward_search_is_case_insensitive() {
        let mut e = over("Alpha x alpha");
        search(&mut e, "alpha"); // → 8 (past the caret on 'A')
        assert_eq!(e.caret, 8);
        e.handle(Key::Char('N')); // back → the capitalized one at 0
        assert_eq!(e.caret, 0);
    }

    #[test]
    fn search_lands_on_char_boundaries_in_multibyte_text() {
        let mut e = over("héé ém"); // 'é' is 2 bytes
        search(&mut e, "ém");
        assert_eq!(e.caret, 6); // byte offset of the standalone "ém"
        assert_eq!(&e.text[e.caret..e.caret + 3], "ém");
    }

    #[test]
    fn n_extends_a_visual_selection() {
        let mut e = over("ab x ab");
        search(&mut e, "ab"); // → 5
        e.handle(Key::Char('N')); // back → 0
        e.handle(Key::Char('v')); // Visual, anchor at 0
        e.handle(Key::Char('n')); // extend to the next match
        assert_eq!(e.mode(), Mode::Visual);
        assert_eq!(e.caret, 5);
        e.handle(Key::Char('y')); // yank the span (inclusive of the caret char)
        assert_eq!(e.register, "ab x a");
    }

    #[test]
    fn last_search_survives_a_buffer_switch() {
        let mut e = over("ab x ab");
        search(&mut e, "ab"); // → 5
        e.handle(Key::Char(':'));
        for c in "enew /sd/repo/other.md".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
        e.handle(Key::Char('i'));
        for c in "ab cd ab".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Escape);
        e.handle(Key::Char('0'));
        e.handle(Key::Char('n')); // the pattern is editor-global, like vim
        assert_eq!(e.caret, 6);
    }

    #[test]
    fn slash_prompt_draws_without_panic() {
        let mut e = over("alpha");
        e.handle(Key::Char('/'));
        e.handle(Key::Char('a'));
        let _ = e.draw(true);
    }

    // --- `:gl` pull support (v0.7) ------------------------------------------

    #[test]
    fn refresh_active_replaces_text_and_resets_state() {
        let mut e = over("old text");
        e.handle(Key::Char('v')); // some transient state to reset
        e.refresh_active("pulled text".into());
        assert_eq!(e.text, "pulled text");
        assert_eq!(e.mode(), Mode::Normal);
        assert!(!e.dirty());
        assert!(e.undo.is_empty()); // old snapshots reference the old text
        assert_eq!(e.path(), "/sd/repo/notes.md"); // same file, new contents
        assert_eq!(e.caret, 10); // boot posture: caret on the last char
    }

    #[test]
    fn joined_file_list_sorts_dedups_and_survives_blank_lines() {
        let mut e = Editor::new();
        e.set_file_list_joined("/sd/repo/b.md\n\n/sd/repo/a.md\n/sd/repo/b.md\n".into());
        assert_eq!(files_vec(&e), vec!["/sd/repo/a.md", "/sd/repo/b.md"]);
        e.add_to_file_list("/sd/repo/ab.md"); // lands between, sorted
        e.add_to_file_list("/sd/repo/a.md"); // already known — no dup
        assert_eq!(
            files_vec(&e),
            vec!["/sd/repo/a.md", "/sd/repo/ab.md", "/sd/repo/b.md"]
        );
        e.remove_from_file_list("/sd/repo/b.md");
        assert_eq!(files_vec(&e), vec!["/sd/repo/a.md", "/sd/repo/ab.md"]);
    }

    #[test]
    fn drop_clean_parked_keeps_only_dirty_buffers() {
        let mut e = over("one"); // active: notes.md, clean
        e.handle(Key::Char(':'));
        for c in "enew /sd/repo/b.md".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter); // notes.md parked (clean); b.md active (dirty by design)
        e.handle(Key::Char(':'));
        for c in "enew /sd/repo/c.md".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter); // b.md parked (dirty); c.md active
        assert_eq!(e.parked.len(), 2);
        e.drop_clean_parked();
        let kept: Vec<&str> = e.parked.iter().map(|b| b.path.as_str()).collect();
        assert_eq!(kept, ["/sd/repo/b.md"]); // clean notes.md dropped, dirty b.md kept
    }
}
