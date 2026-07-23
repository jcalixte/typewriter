//! Test support: shared helpers plus the per-theme test module
//! declarations.
//!
//! The editor's tests are behavioural — each builds a whole [`Editor`],
//! drives it through [`Key`] events, and asserts on private state — so they
//! live in this `#[cfg(test)] mod tests` (declared in `lib.rs`) to keep
//! crate-private access. Every helper lives here; each theme's `#[test]`
//! fns live in the submodule named below.

mod editing;
mod utf8;
mod commands;
mod render;
mod undo;
mod dot_repeat;
mod visual;
mod buffers;
mod inbox;
mod palette;
mod prefs;
mod snippets;
mod search;
mod pull;
mod focus;
mod companion;

pub(crate) use crate::*;

const TWO_SNIPPETS: &str = r##"{
    "Markdown link": { "prefix": "link", "body": "[$1]($2)$0", "description": "Inline link" },
    "Book notes": { "prefix": "booknotes", "body": "# $1", "description": "Reading fiche" }
}"##;

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
    Push,
    Pull,
    Delete,
    Rename,
    SavePrefs,
    Setup,
    Reboot,
    Update,
    FocusStart,
    FocusStop,
}

fn kinds(effects: &[Effect]) -> Vec<Kind> {
    effects
        .iter()
        .map(|e| match e {
            Effect::Save { .. } => Kind::Save,
            Effect::Load { .. } => Kind::Load,
            Effect::Push => Kind::Push,
            Effect::Pull { .. } => Kind::Pull,
            Effect::Delete { .. } => Kind::Delete,
            Effect::Rename { .. } => Kind::Rename,
            Effect::SavePrefs { .. } => Kind::SavePrefs,
            Effect::Setup => Kind::Setup,
            Effect::Reboot => Kind::Reboot,
            Effect::Update => Kind::Update,
            Effect::FocusStart => Kind::FocusStart,
            Effect::FocusStop => Kind::FocusStop,
        })
        .collect()
}

/// 1 = white paper, 0 = black ink (SSD16xx convention). Reads one pixel.
fn ink_at(frame: &Frame, x: usize, y: usize) -> bool {
    frame.bytes()[y * display::FB_BYTES_W + x / 8] & (0x80 >> (x % 8)) == 0
}

/// Answer `y` at a destructive-command `Mode::Confirm` prompt. Doubles as an
/// assertion that a prompt is actually up (the guard was entered, not skipped).
fn confirm(e: &mut Editor) {
    assert_eq!(e.mode(), Mode::Confirm, "expected a confirm prompt");
    e.handle(Key::Char('y'));
}

/// Feed a run of characters as Normal-mode keys.
fn send(e: &mut Editor, s: &str) {
    for c in s.chars() {
        e.handle(Key::Char(c));
    }
}

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

/// Open the palette and type `query` (so `>...` enters command mode).
fn palette_type(files: &[&str], query: &str) -> Editor {
    let mut e = palette_editor(files);
    e.handle(Key::Palette);
    for c in query.chars() {
        e.handle(Key::Char(c));
    }
    e
}

fn with_snippets(json: &str) -> Editor {
    let mut e = Editor::new();
    e.set_snippets(Snippets::parse(json).unwrap());
    e
}

/// Open the palette on an editor with `json`'s snippets loaded, then type `q`.
fn snippet_palette(json: &str, q: &str) -> Editor {
    let mut e = with_snippets(json);
    e.handle(Key::Palette);
    for c in q.chars() {
        e.handle(Key::Char(c));
    }
    e
}

/// Type `word` into a fresh Insert-mode buffer with the two snippets loaded.
fn typed_in_insert(word: &str) -> Editor {
    let mut e = with_snippets(TWO_SNIPPETS);
    e.handle(Key::Char('i'));
    for c in word.chars() {
        e.handle(Key::Char(c));
    }
    e
}

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
