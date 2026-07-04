//! Thin SSD1683 driver for the GDEY0579T93 (792×272) e-paper panel.
//!
//! This panel is a *dual-controller* device: 792×272 exceeds one SSD1683's
//! 400×300 limit, so it is driven as a **master** (command offset `0x00`) +
//! **slave** (`0x80`) pair, with the framebuffer split between them. The
//! command sequences and RAM-window math are ported faithfully from GxEPD2's
//! `GxEPD2_579_GDEY0579T93` (Jean-Marc Zingg), itself based on the Good
//! Display factory demo. See `docs/v0.1-mvp-technical.md` (Spike 2) and
//! ADR-003.
//!
//! **Spike 2a scope:** hardware reset, init, uniform full-screen fill, full
//! refresh — enough to prove wiring + SPI + both controllers + refresh.
//! Text (an `embedded-graphics` `DrawTarget` + the per-quadrant blit from
//! GxEPD2's `_writeFromImage`) is Spike 2b.

use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{Input, Output, PinDriver};
use esp_idf_svc::hal::spi::{SpiBusDriver, SpiDriver};
use esp_idf_svc::sys::EspError;

pub const WIDTH: u16 = 792;
pub const HEIGHT: u16 = 272;

/// Each controller drives one half. SSD1683 X is byte-addressed; 396 px
/// rounds up to 50 bytes (400 px) of RAM width, full panel height (272 rows).
const CTRL_BYTES_W: usize = 50;
const CTRL_BYTES: usize = CTRL_BYTES_W * HEIGHT as usize; // 50 * 272 = 13600

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

/// Max bytes per SPI transfer; matches the DMA size configured in `main`.
const SPI_CHUNK: usize = 4096;

pub struct Epd<'d> {
    spi: SpiBusDriver<'d, SpiDriver<'d>>,
    dc: PinDriver<'d, Output>,
    rst: PinDriver<'d, Output>,
    cs: PinDriver<'d, Output>,
    busy: PinDriver<'d, Input>,
}

impl<'d> Epd<'d> {
    pub fn new(
        spi: SpiBusDriver<'d, SpiDriver<'d>>,
        dc: PinDriver<'d, Output>,
        rst: PinDriver<'d, Output>,
        cs: PinDriver<'d, Output>,
        busy: PinDriver<'d, Input>,
    ) -> Self {
        Self { spi, dc, rst, cs, busy }
    }

    // ---- low-level SPI framing (DC low = command, DC high = data) ----

    fn cmd(&mut self, c: u8) -> Result<(), EspError> {
        self.dc.set_low()?;
        self.cs.set_low()?;
        self.spi.write(&[c])?;
        self.cs.set_high()?;
        Ok(())
    }

    fn data(&mut self, bytes: &[u8]) -> Result<(), EspError> {
        self.dc.set_high()?;
        self.cs.set_low()?;
        for chunk in bytes.chunks(SPI_CHUNK) {
            self.spi.write(chunk)?;
        }
        self.cs.set_high()?;
        Ok(())
    }

    /// BUSY is active-HIGH on this panel (GxEPD2 constructs with `HIGH`).
    fn wait_while_busy(&mut self, timeout_ms: u32) -> Result<(), EspError> {
        let mut waited = 0;
        while self.busy.is_high() {
            FreeRtos::delay_ms(1);
            waited += 1;
            if waited >= timeout_ms {
                log::warn!("EPD BUSY still high after {timeout_ms} ms — continuing");
                break;
            }
        }
        Ok(())
    }

    // ---- panel bring-up ----

    /// Hardware reset (RST is active-low). ~20 ms pulses per GxEPD2 default.
    pub fn reset(&mut self) -> Result<(), EspError> {
        self.rst.set_high()?;
        FreeRtos::delay_ms(20);
        self.rst.set_low()?;
        FreeRtos::delay_ms(20);
        self.rst.set_high()?;
        FreeRtos::delay_ms(20);
        self.wait_while_busy(100)?;
        Ok(())
    }

    /// Port of GxEPD2 `_InitDisplay` (B/W mode). The `0x20` master
    /// activations load the temperature value and LUT.
    pub fn init(&mut self) -> Result<(), EspError> {
        self.cmd(0x12)?; // SWRESET
        FreeRtos::delay_ms(10);
        self.wait_while_busy(100)?;
        self.cmd(0x18)?; // temperature sensor control
        self.data(&[0x80])?; // internal sensor
        self.cmd(0x22)?; // display update control 2
        self.data(&[0xB1])?; // enable clock, load temp, load LUT (B/W), disable clock
        self.cmd(0x20)?; // master activation
        FreeRtos::delay_ms(10);
        self.wait_while_busy(100)?;
        self.cmd(0x1A)?; // write to temperature register
        self.data(&[0x64, 0x00])?;
        self.cmd(0x22)?;
        self.data(&[0x91])?; // load temp, load LUT (B/W), disable clock
        self.cmd(0x20)?;
        FreeRtos::delay_ms(10);
        self.wait_while_busy(100)?;
        Ok(())
    }

    /// Port of GxEPD2 `_setPartialRamArea`. `target` is `0x00` (master) or
    /// `0x80` (slave); `mode` selects X/Y increment/decrement (0x00–0x03).
    fn set_ram_area(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        mode: u8,
        target: u8,
    ) -> Result<(), EspError> {
        self.cmd(0x11 | target)?; // data entry mode
        self.data(&[mode])?;
        let xl = (x / 8) as u8;
        let xh = ((x + w - 1) / 8) as u8;
        let ys = [(y % 256) as u8, (y / 256) as u8];
        let ye = [((y + h - 1) % 256) as u8, ((y + h - 1) / 256) as u8];
        match mode {
            0x03 => {
                // X increment, Y increment
                self.cmd(0x44 | target)?;
                self.data(&[xl, xh])?;
                self.cmd(0x45 | target)?;
                self.data(&[ys[0], ys[1], ye[0], ye[1]])?;
                self.cmd(0x4E | target)?;
                self.data(&[xl])?;
                self.cmd(0x4F | target)?;
                self.data(&[ys[0], ys[1]])?;
            }
            0x02 => {
                // X decrement, Y increment
                self.cmd(0x44 | target)?;
                self.data(&[xh, xl])?;
                self.cmd(0x45 | target)?;
                self.data(&[ys[0], ys[1], ye[0], ye[1]])?;
                self.cmd(0x4E | target)?;
                self.data(&[xh])?;
                self.cmd(0x4F | target)?;
                self.data(&[ys[0], ys[1]])?;
            }
            0x01 => {
                // X increment, Y decrement
                self.cmd(0x44 | target)?;
                self.data(&[xl, xh])?;
                self.cmd(0x45 | target)?;
                self.data(&[ye[0], ye[1], ys[0], ys[1]])?;
                self.cmd(0x4E | target)?;
                self.data(&[xl])?;
                self.cmd(0x4F | target)?;
                self.data(&[ye[0], ye[1]])?;
            }
            _ => {
                // 0x00: X decrement, Y decrement
                self.cmd(0x44 | target)?;
                self.data(&[xh, xl])?;
                self.cmd(0x45 | target)?;
                self.data(&[ye[0], ye[1], ys[0], ys[1]])?;
                self.cmd(0x4E | target)?;
                self.data(&[xh])?;
                self.cmd(0x4F | target)?;
                self.data(&[ye[0], ye[1]])?;
            }
        }
        FreeRtos::delay_ms(2);
        Ok(())
    }

    /// Fill one RAM bank (`0x24` current or `0x26` previous) on both
    /// controllers with a constant byte. One clean full-coverage window per
    /// controller (slave = left half `0x80`, master = right half `0x00`) —
    /// simpler and more complete than GxEPD2's overlapping-window fill, which
    /// only matters for a constant value anyway.
    fn write_buffer(&mut self, command: u8, value: u8) -> Result<(), EspError> {
        let buf = vec![value; CTRL_BYTES];
        for target in [0x80u8, 0x00u8] {
            self.set_ram_area(0, 0, 400, HEIGHT, 0x03, target)?;
            self.cmd(command | target)?;
            self.data(&buf)?;
        }
        Ok(())
    }

    /// Port of GxEPD2 `refresh(false)` → `_Update_Full` (fast full update).
    fn update_full(&mut self) -> Result<(), EspError> {
        self.set_ram_area(0, 0, WIDTH / 2, HEIGHT, 0x03, 0x80)?; // slave
        self.set_ram_area(0, 0, WIDTH / 2, HEIGHT, 0x03, 0x00)?; // master
        self.cmd(0x21)?; // display update control 1
        self.data(&[0x40, 0x10])?; // bypass RED as 0, cascade
        self.cmd(0x1A)?; // temperature register (fast full update)
        self.data(&[0x64, 0x00])?;
        self.cmd(0x22)?;
        self.data(&[0xD7])?; // fast full update
        self.cmd(0x20)?; // master activation
        self.wait_while_busy(2500)?; // full_refresh_time ≈ 2200 ms
        Ok(())
    }

    /// Fill the whole panel with one value and full-refresh.
    /// `0xFF` = white, `0x00` = black. Port of GxEPD2 `clearScreen`.
    pub fn clear_screen(&mut self, value: u8) -> Result<(), EspError> {
        self.write_buffer(0x26, value)?; // previous
        self.write_buffer(0x24, value)?; // current
        self.update_full()?;
        Ok(())
    }

    /// Blit a full 792×272 framebuffer into one RAM bank on both
    /// controllers. Port of the full-frame case of GxEPD2 `_writeFromImage`:
    /// slave gets panel bytes 0..=49 of each row in X-increment mode; the
    /// master's sources are wired mirrored, so it gets bytes 49..=98 in
    /// bitmap order while the address counter walks RAM 49..=0 (mode 0x02).
    /// The seam byte 49 (px 392..399) lands on both; the 4 columns past each
    /// controller's 396 sources aren't wired.
    fn write_frame_bank(&mut self, command: u8, fb: &[u8]) -> Result<(), EspError> {
        let mut buf = Vec::with_capacity(CTRL_BYTES);
        for y in 0..HEIGHT as usize {
            let row = &fb[y * FB_BYTES_W..(y + 1) * FB_BYTES_W];
            buf.extend_from_slice(&row[..CTRL_BYTES_W]);
        }
        self.set_ram_area(0, 0, WIDTH / 2, HEIGHT, 0x03, 0x80)?; // slave
        self.cmd(command | 0x80)?;
        self.data(&buf)?;

        buf.clear();
        for y in 0..HEIGHT as usize {
            let row = &fb[y * FB_BYTES_W..(y + 1) * FB_BYTES_W];
            buf.extend_from_slice(&row[FB_BYTES_W - CTRL_BYTES_W..]);
        }
        self.set_ram_area(0, 0, WIDTH / 2, HEIGHT, 0x02, 0x00)?; // master
        self.cmd(command)?;
        self.data(&buf)?;
        Ok(())
    }

    /// Show a full 792×272 framebuffer (`FB_BYTES` long) with a full
    /// refresh. Writes both RAM banks so the next differential update has a
    /// consistent "previous" image.
    pub fn display_frame(&mut self, fb: &[u8]) -> Result<(), EspError> {
        assert_eq!(fb.len(), FB_BYTES, "framebuffer must be 99 x 272 bytes");
        self.write_frame_bank(0x26, fb)?; // previous
        self.write_frame_bank(0x24, fb)?; // current
        self.update_full()?;
        Ok(())
    }
}
