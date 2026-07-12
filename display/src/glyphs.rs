//! Extra glyphs the ISO-8859-15 render font lacks.
//!
//! The buffer is UTF-8, so it can hold codepoints outside Latin-9. The
//! `embedded-graphics` `iso_8859_15` font draws those as a fallback box. This
//! module carries hand-authored 10×20 bitmaps for a curated set of common
//! single-width characters — typographic punctuation that arrives in imported
//! prose (curly quotes, en/em dashes, ellipsis, bullet) and the math symbols the
//! user's Markdown snippets insert (→ ≠ Σ). [`Editor::draw`] overlays these over
//! the fallback box after the font has laid out the line.
//!
//! Every glyph is exactly one 10×20 cell (matching the body `FONT_10X20`, i.e.
//! `editor::CW`×`editor::CH`), so the editor's 1-char = 1-cell layout invariant
//! is untouched. A row is a 10-bit pattern: the leftmost column (x = 0) is the
//! high bit, so a binary literal reads left-to-right as the pixels of that row.
//! Rows are top (y = 0) to bottom (y = 19); 1 = ink.

use crate::Frame;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;

const GLYPH_W: usize = 10;
const GLYPH_H: usize = 20;

/// A 10×20 1-bit glyph: one `u16` per row, low `GLYPH_W` bits used, high bit =
/// leftmost pixel.
pub type Glyph = [u16; GLYPH_H];

// → U+2192 RIGHTWARDS ARROW: a horizontal shaft with a `>` head at the right.
#[rustfmt::skip]
const ARROW_RIGHT: Glyph = [
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000010000,
    0b0000001000,
    0b0000000100,
    0b0000000010,
    0b0111111111,
    0b0111111111,
    0b0000000010,
    0b0000000100,
    0b0000001000,
    0b0000010000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
];

// ≠ U+2260 NOT EQUAL TO: two equals bars with a slash through them.
#[rustfmt::skip]
const NOT_EQUAL: Glyph = [
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000010,
    0b0000000100,
    0b0000000100,
    0b0000001000,
    0b0111111110,
    0b0000010000,
    0b0000010000,
    0b0000100000,
    0b0111111110,
    0b0001000000,
    0b0001000000,
    0b0010000000,
    0b0010000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
];

// Σ U+03A3 GREEK CAPITAL LETTER SIGMA: top & bottom bars with the diagonals
// meeting at a vertex on the centre-right.
#[rustfmt::skip]
const SIGMA: Glyph = [
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0111111110,
    0b0100000000,
    0b0010000000,
    0b0001000000,
    0b0000100000,
    0b0000010000,
    0b0000100000,
    0b0001000000,
    0b0010000000,
    0b0100000000,
    0b0100000000,
    0b0111111110,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
];

// • U+2022 BULLET: a filled dot at mid-height.
#[rustfmt::skip]
const BULLET: Glyph = [
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0001111000,
    0b0011111100,
    0b0011111100,
    0b0011111100,
    0b0001111000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
];

// … U+2026 HORIZONTAL ELLIPSIS: three dots on the baseline.
#[rustfmt::skip]
const ELLIPSIS: Glyph = [
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0110110110,
    0b0110110110,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
];

// – U+2013 EN DASH: a short mid-height bar.
#[rustfmt::skip]
const EN_DASH: Glyph = [
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0011111100,
    0b0011111100,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
];

// — U+2014 EM DASH: a full-width mid-height bar.
#[rustfmt::skip]
const EM_DASH: Glyph = [
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b1111111111,
    0b1111111111,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
];

// ' U+2018 LEFT SINGLE QUOTATION MARK: raised, tail up (opening).
#[rustfmt::skip]
const LEFT_SINGLE_QUOTE: Glyph = [
    0b0000000000,
    0b0000000000,
    0b0000100000,
    0b0001100000,
    0b0001100000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
];

// ' U+2019 RIGHT SINGLE QUOTATION MARK / apostrophe: raised, tail down (closing).
#[rustfmt::skip]
const RIGHT_SINGLE_QUOTE: Glyph = [
    0b0000000000,
    0b0000000000,
    0b0001100000,
    0b0001100000,
    0b0000100000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
];

// " U+201C LEFT DOUBLE QUOTATION MARK: two opening quotes.
#[rustfmt::skip]
const LEFT_DOUBLE_QUOTE: Glyph = [
    0b0000000000,
    0b0000000000,
    0b0010010000,
    0b0011011000,
    0b0011011000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
];

// " U+201D RIGHT DOUBLE QUOTATION MARK: two closing quotes.
#[rustfmt::skip]
const RIGHT_DOUBLE_QUOTE: Glyph = [
    0b0000000000,
    0b0000000000,
    0b0011011000,
    0b0011011000,
    0b0010010000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
    0b0000000000,
];

/// The 10×20 bitmap for a character the ISO-8859-15 font can't draw, or `None`
/// if the base font already covers it (all ASCII and Latin-9, including `œ €`).
pub fn extra_glyph(c: char) -> Option<&'static Glyph> {
    Some(match c {
        '\u{2192}' => &ARROW_RIGHT,        // →
        '\u{2260}' => &NOT_EQUAL,          // ≠
        '\u{03A3}' => &SIGMA,              // Σ
        '\u{2022}' => &BULLET,             // •
        '\u{2026}' => &ELLIPSIS,           // …
        '\u{2013}' => &EN_DASH,            // –
        '\u{2014}' => &EM_DASH,            // —
        '\u{2018}' => &LEFT_SINGLE_QUOTE,  // '
        '\u{2019}' => &RIGHT_SINGLE_QUOTE, // '
        '\u{201C}' => &LEFT_DOUBLE_QUOTE,  // "
        '\u{201D}' => &RIGHT_DOUBLE_QUOTE, // "
        _ => return None,
    })
}

/// Paint a glyph over one 10×20 cell at `(x, y)`. Fills the whole cell — set
/// bits in `ink`, unset bits in `ink.invert()` — so it fully overwrites whatever
/// the base font drew there (e.g. the fallback box). Pass `ink = On` for a
/// normal black-on-white cell, `ink = Off` for a reverse-video cell (a white
/// glyph over the black selection/caret fill).
pub fn blit_glyph(f: &mut Frame, x: i32, y: i32, g: &Glyph, ink: BinaryColor) {
    let bg = ink.invert();
    let pixels = g.iter().enumerate().flat_map(move |(row, &bits)| {
        (0..GLYPH_W).map(move |c| {
            let on = (bits >> (GLYPH_W - 1 - c)) & 1 == 1;
            Pixel(
                Point::new(x + c as i32, y + row as i32),
                if on { ink } else { bg },
            )
        })
    });
    // Frame's DrawTarget error is Infallible.
    let _ = f.draw_iter(pixels);
}
