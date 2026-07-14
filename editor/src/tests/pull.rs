//! `:gl` pull support.

use super::*;

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
