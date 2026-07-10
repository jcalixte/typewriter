//! The e-paper panel's geometry and an in-memory drawable frame.
//!
//! Split out of the hardware driver (`firmware/src/epd.rs`) so the driver and
//! the host-testable `editor` crate share one framebuffer definition. `Frame`
//! is a pure `embedded-graphics` [`DrawTarget`]; the `Epd` driver in firmware
//! consumes its raw bytes via [`Frame::bytes`] and never names the type, so
//! nothing here depends on esp-idf and the whole crate builds on the host.

use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;

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

    pub fn new_black() -> Self {
        Self { buf: vec![0x00; FB_BYTES] }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.buf
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
