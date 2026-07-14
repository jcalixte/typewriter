//! Charwise and linewise Visual mode.

use super::*;

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
