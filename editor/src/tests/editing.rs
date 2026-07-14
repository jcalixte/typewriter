//! Insert/Normal editing basics and the register + yank/paste commands.

use super::*;

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
fn enter_in_a_blockquote_continues_the_marker() {
    let mut e = typed("> quote");
    e.handle(Key::Enter);
    e.handle(Key::Char('m'));
    assert_eq!(e.text, "> quote\n> m");
    assert_eq!(e.caret, 11);
}

#[test]
fn enter_on_an_empty_blockquote_exits_the_quote() {
    let mut e = typed("> quote");
    e.handle(Key::Enter); // -> "> quote\n> "
    e.handle(Key::Enter); // empty quote: drop the "> ", leaving a blank line
    assert_eq!(e.text, "> quote\n");
    assert_eq!(e.caret, 8);
}

#[test]
fn enter_in_a_nested_blockquote_keeps_the_depth() {
    let mut e = typed("> > deep");
    e.handle(Key::Enter);
    e.handle(Key::Char('x'));
    assert_eq!(e.text, "> > deep\n> > x");
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
