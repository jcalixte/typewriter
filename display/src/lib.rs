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
use embedded_graphics::text::{Alignment, Baseline, Text, TextStyleBuilder};

mod glyphs;
pub use glyphs::{blit_glyph, extra_glyph, Glyph};

pub mod fonts;
pub use fonts::{body_font, FONT_OPTIONS};

pub mod typo;

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

    /// The Typoena boot splash: Typo (the tucano companion, [`typo::BODY`])
    /// centred over the lowercase "typoena" wordmark on a white frame. Pure
    /// `embedded-graphics`, so it renders the same on the host (the preview) as
    /// it does through the `Epd` driver at boot. `main.rs` shows this once at
    /// startup (async full refresh, overlapping the SD mount + note load); the
    /// first editor frame then partial-refreshes over it, and the one-shot
    /// boot-cleanup full refresh launders the residual ghost at the first typing
    /// pause (see `app::Panel`).
    pub fn splash() -> Self {
        let mut f = Self::new_white();
        f.draw_brand();
        f
    }

    /// The `:reboot` offboarding screen: the same "typoena" wordmark as
    /// [`splash`](Self::splash) with a "restarting..." subtitle. Painted with a
    /// blocking full refresh just before `esp_restart()`, so the bistable panel
    /// holds it across the whole reboot and the wordmark simply carries over into
    /// the boot splash — the restart reads as one continuous motion, not a freeze.
    pub fn reboot() -> Self {
        let mut f = Self::new_white();
        f.draw_brand();

        // A subtitle near the bottom edge, well clear of the centred wordmark
        // (baseline ≈ HEIGHT/2), with room for one FONT_10X20 line.
        let char_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
        let text_style = TextStyleBuilder::new()
            .alignment(Alignment::Center)
            .baseline(Baseline::Middle)
            .build();
        Text::with_text_style(
            "restarting...",
            Point::new(WIDTH as i32 / 2, HEIGHT as i32 - 18),
            char_style,
            text_style,
        )
        .draw(&mut f)
        .unwrap();

        f
    }

    /// Draw the brand lockup — Typo at 3× over the lowercase "typoena" wordmark,
    /// centred on the panel — onto this frame. Shared by [`splash`](Self::splash)
    /// and [`reboot`](Self::reboot) so the boot and restart screens are
    /// pixel-identical bar the subtitle, keeping a `:reboot` visually seamless
    /// into boot.
    ///
    /// Typo is line art (his belly is paper, not fill), so the splash still
    /// leaves little ink: the first editor frame partial-refreshes over it, and a
    /// partial can't fully drive ink back to paper — the residual sprite ghost is
    /// laundered by the one-shot boot-cleanup full refresh (see `app::Panel`).
    fn draw_brand(&mut self) {
        const WORDMARK: &str = "typoena";
        /// Boot-splash sprite scale: 48 px × 3 = 144 px, the size approved on
        /// the character sheet.
        const SPLASH_SCALE: i32 = 3;

        // Stack the sprite over the wordmark and centre the lockup vertically.
        let sprite_px = typo::BODY.h as i32 * SPLASH_SCALE;
        let gap = 14; // sprite baseline → wordmark centre
        let top = (HEIGHT as i32 - (sprite_px + gap + 20)) / 2;
        typo::blit_sprite(
            self,
            (WIDTH as i32 - typo::BODY.w as i32 * SPLASH_SCALE) / 2,
            top,
            typo::BODY,
            SPLASH_SCALE,
        );

        let char_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
        let text_style = TextStyleBuilder::new()
            .alignment(Alignment::Center)
            .baseline(Baseline::Middle)
            .build();
        Text::with_text_style(
            WORDMARK,
            Point::new(WIDTH as i32 / 2, top + sprite_px + gap + 10),
            char_style,
            text_style,
        )
        .draw(self)
        .unwrap();
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
