//! The e-paper panel's geometry and an in-memory drawable frame.
//!
//! Split out of the hardware driver (`firmware/src/epd.rs`) so the driver and
//! the host-testable `editor` crate share one framebuffer definition. `Frame`
//! is a pure `embedded-graphics` [`DrawTarget`]; the `Epd` driver in firmware
//! consumes its raw bytes via [`Frame::bytes`] and never names the type, so
//! nothing here depends on esp-idf and the whole crate builds on the host.

use embedded_graphics::mono_font::iso_8859_15::FONT_10X20;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyleBuilder};
use embedded_graphics::text::{Alignment, Baseline, Text, TextStyleBuilder};

mod glyphs;
pub use glyphs::{blit_glyph, extra_glyph, Glyph};

pub const WIDTH: u16 = 792;
pub const HEIGHT: u16 = 272;

/// Full-frame 1-bit framebuffer: 792 px = 99 bytes per row, MSB-first,
/// 1 = white, 0 = black (SSD16xx convention).
pub const FB_BYTES_W: usize = (WIDTH / 8) as usize; // 99
pub const FB_BYTES: usize = FB_BYTES_W * HEIGHT as usize; // 26928

/// In-memory 792×272 1-bit frame, drawable via `embedded-graphics`.
/// `BinaryColor::On` = black ink, `Off` = white paper.
pub struct Frame {
    buf: Vec<u8>,
}

impl Frame {
    pub fn new_white() -> Self {
        Self { buf: vec![0xFF; FB_BYTES] }
    }

    /// A zero-capacity placeholder, for the buffer-swap pattern:
    /// `Editor::draw_into` takes the caller's buffer out through one of these
    /// and puts it back when done. Not drawable until [`clear_white`]
    /// (or a `draw_into`) gives it its buffer.
    ///
    /// [`clear_white`]: Frame::clear_white
    pub fn empty() -> Self {
        Self { buf: Vec::new() }
    }

    /// Reset to all-white paper, reusing the existing allocation when the
    /// buffer is already full-size. This is what lets firmware repaint without
    /// allocating: a background `:gp` push can take the heap to the floor,
    /// and a failed framebuffer alloc aborts the whole app (2026-07-13).
    pub fn clear_white(&mut self) {
        self.buf.clear();
        self.buf.resize(FB_BYTES, 0xFF);
    }

    pub fn new_black() -> Self {
        Self { buf: vec![0x00; FB_BYTES] }
    }

    /// The Typoena boot splash (Spike 9): the wordmark centred inside a stroked
    /// circle on a white frame. Pure `embedded-graphics`, so it renders the same
    /// on the host (the preview) as it does through the `Epd` driver at boot.
    /// `main.rs` shows this once at startup, before the editor opens.
    pub fn splash() -> Self {
        // Badge sized to leave a comfortable margin inside the 272 px panel
        // height (diameter 200 → 36 px clear top and bottom).
        const WORDMARK: &str = "typoena";
        const CIRCLE_DIAMETER: u32 = 200;
        const STROKE_WIDTH: u32 = 4;

        let mut f = Self::new_white();
        let center = Point::new(WIDTH as i32 / 2, HEIGHT as i32 / 2);

        let stroke = PrimitiveStyleBuilder::new()
            .stroke_color(BinaryColor::On) // black ink
            .stroke_width(STROKE_WIDTH)
            .build();
        Circle::with_center(center, CIRCLE_DIAMETER)
            .into_styled(stroke)
            .draw(&mut f)
            .unwrap(); // Frame's DrawTarget error is Infallible

        // Centre the wordmark on the panel centre in both axes, so it sits in
        // the middle of the circle regardless of string length.
        let char_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
        let text_style = TextStyleBuilder::new()
            .alignment(Alignment::Center)
            .baseline(Baseline::Middle)
            .build();
        Text::with_text_style(WORDMARK, center, char_style, text_style)
            .draw(&mut f)
            .unwrap();

        f
    }

    pub fn bytes(&self) -> &[u8] {
        &self.buf
    }

    /// Flip every pixel black↔white across the whole framebuffer. The editor
    /// draws its native black-ink-on-white-paper frame, then calls this once at
    /// the end for the dark theme — so text, selection, caret, panel and palette
    /// all invert together and each stays legible against the flipped ground.
    pub fn invert(&mut self) {
        for b in &mut self.buf {
            *b = !*b;
        }
    }
}

impl OriginDimensions for Frame {
    fn size(&self) -> Size {
        Size::new(WIDTH as u32, HEIGHT as u32)
    }
}

impl DrawTarget for Frame {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, color) in pixels {
            if (0..WIDTH as i32).contains(&p.x) && (0..HEIGHT as i32).contains(&p.y) {
                let idx = p.y as usize * FB_BYTES_W + p.x as usize / 8;
                let bit = 0x80u8 >> (p.x % 8);
                match color {
                    BinaryColor::On => self.buf[idx] &= !bit, // black ink
                    BinaryColor::Off => self.buf[idx] |= bit, // white paper
                }
            }
        }
        Ok(())
    }
}
