//! UTF-8 correctness for accented (Latin-9) input, plus extra-glyph overlay drawing.

use super::*;

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
