//! `/` search and n/N repeat.

use super::*;

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
