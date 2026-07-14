//! Undo / redo history.

use super::*;

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
