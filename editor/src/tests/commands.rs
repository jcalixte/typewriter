//! Ex commands (`:w`, `:gp`, `:gl`, format-on-save, aliases) and command-line editing.

use super::*;

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
