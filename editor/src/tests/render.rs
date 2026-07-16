//! Viewport rendering: half-page scroll and the absolute line-number gutter.

use super::*;

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

// ---- scrolloff / vertical scroll margin ----

/// In `vec!["a"; N]` each display row `r` starts at byte `2*r` (`"a\n"`).
/// A caret parked in the middle keeps `scroll_margin` rows of context on both
/// sides.
#[test]
fn scroll_margin_keeps_context_above_and_below_the_caret() {
    let mut e = Editor::with_text(vec!["a"; 40].join("\n"));
    assert_eq!(e.prefs.scroll_margin, 2); // the shipped default
    e.caret = 2 * 20; // display row 20, mid-document
    e.draw(true); // runs adjust_scroll
    let top = e.scroll_top();
    let (row, _) = e.caret_rc(&e.layout());
    assert_eq!(row, 20);
    assert!(row >= top + 2, "want >=2 rows above; top {top}, row {row}");
    assert!(
        row + 2 < top + ROWS,
        "want >=2 rows below; top {top}, row {row}"
    );
}

/// The margin collapses at the top of the buffer — no blank rows above line 0.
#[test]
fn scroll_margin_collapses_at_the_top() {
    let mut e = Editor::with_text(vec!["a"; 40].join("\n"));
    e.caret = 2 * 20; // scroll away from the top first
    e.draw(true);
    assert!(e.scroll_top() > 0);
    e.caret = 0; // back to line 0
    e.draw(true);
    assert_eq!(e.scroll_top(), 0, "no blank rows should sit above line 0");
}

/// The margin collapses at the end — the last line pins to the bottom row
/// rather than floating `scroll_margin` rows up with blank space beneath it.
#[test]
fn scroll_margin_collapses_at_the_end() {
    let mut e = Editor::with_text(vec!["a"; 40].join("\n"));
    e.caret = 2 * 39; // last display row
    e.draw(true);
    let lay = e.layout();
    let (row, _) = e.caret_rc(&lay);
    assert_eq!(row, e.scroll_top() + ROWS - 1, "last line pinned to the bottom");
    assert_eq!(e.scroll_top(), lay.len() - ROWS);
}

/// `scroll_margin = 0` reproduces the old edge-triggered behaviour: a caret
/// pushed below the window lands flush on the bottom row.
#[test]
fn scroll_margin_zero_is_edge_triggered() {
    let mut e = Editor::with_text(vec!["a"; 40].join("\n"));
    e.prefs.scroll_margin = 0;
    e.caret = 2 * 20;
    e.draw(true);
    let (row, _) = e.caret_rc(&e.layout());
    assert_eq!(row, e.scroll_top() + ROWS - 1, "caret flush on the bottom edge");
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
