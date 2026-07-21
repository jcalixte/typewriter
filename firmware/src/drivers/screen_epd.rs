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
//! Capabilities: hardware reset, init, uniform fill, full-frame blit via an
//! `embedded-graphics` `DrawTarget` (`Frame`), full refresh (`display_frame`),
//! and partial refresh (`display_frame_partial`) — Spikes 2 and 5.

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{Input, Output, PinDriver};
use esp_idf_svc::hal::spi::{SpiBusDriver, SpiDriver};
use esp_idf_svc::sys::EspError;

// Panel geometry and the drawable `Frame` now live in the `display` crate, so
// the editor can render onto them off the xtensa target. Re-exported here so
// `epd::HEIGHT`, `epd::FB_BYTES`, etc. keep resolving for main.rs and the driver
// code below, and so the driver need not know they were relocated.
pub use display::{FB_BYTES, FB_BYTES_W, HEIGHT, WIDTH};

/// Each controller drives one half. SSD1683 X is byte-addressed; 396 px
/// rounds up to 50 bytes (400 px) of RAM width, full panel height (272 rows).
const CTRL_BYTES_W: usize = 50;
const CTRL_BYTES: usize = CTRL_BYTES_W * HEIGHT as usize; // 50 * 272 = 13600

/// Max bytes per SPI transfer; matches the DMA size configured in `main`.
const SPI_CHUNK: usize = 4096;

/// EXPERIMENT (2026-07-17): temperature value written to the `0x1A` register
/// before each *partial* update, to test whether a hotter OTP LUT shortens the
/// ~543 ms partial-waveform floor. The partial's `0x22 ← 0xFF` reloads temp+LUT
/// from this register every refresh; `init()` already leaves it at `[0x64,0x00]`
/// (~100), which is the shipping baseline — so `Some([0x64,0x00])` is the
/// control and higher values sweep for a faster-indexed LUT.
///
/// This is NOT a custom/authored waveform (still a factory OTP LUT, just a
/// different temperature index), so the DC-balance/longevity risks of the
/// `0x32` path don't apply — the only cost is ghosting if a hot LUT under-drives
/// at room temperature. Fully reversible: set to `None` to restore the honest
/// behaviour (register left at init's `[0x64,0x00]`, no per-partial rewrite).
///
/// Results log: docs/tradeoff-curves/epd-refresh-latency.md.
///
/// CLOSED 2026-07-17: swept hot `[0x7F,0x00]` and cold `[0x19,0x00]` against the
/// `[0x64,0x00]` default — windowed stayed ~565 ms and full-area ~690 ms at
/// *every* value. The partial waveform's BUSY time is temperature-independent
/// here (the `0x18`←`0x80` internal sensor overrides the register, or the OTP
/// partial LUT simply has one fixed schedule). Not a lever. Left as `None`
/// (honest baseline, no per-partial register write); the scaffolding stays only
/// so the closed result is self-documenting next to the driver.
const PARTIAL_TEMP: Option<[u8; 2]> = None;

/// Settle delay after each RAM-window set in `set_ram_area`. A partial refresh
/// issues 8 of these (both windowed and full-area paths). The original port slept
/// `delay_ms(2)` here — but at `CONFIG_FREERTOS_HZ = 100` that rounds *up* to
/// `vTaskDelay(1)`, one 10 ms tick, blocking to the next tick boundary: 0–10 ms
/// each, not 2 ms. Eight per refresh cost ~40 ms average. Shipped at 0 on
/// 2026-07-17 (verified clean, −70 ms windowed / −44 ms full-area): an e-ink
/// controller latches the RAM-window address when the SPI transaction completes,
/// so there is nothing to wait for. Raise to 1 (a full tick) only if band
/// corruption/ghosting ever appears. Log: docs/tradeoff-curves/epd-refresh-latency.md.
const RAM_SETTLE_MS: u32 = 0;

/// Fast partial-refresh waveform for the GDEY0579T93 (SSD1683) — the per-keystroke
/// typing-latency lever. Written to the LUT register (`0x32`) before each additive
/// partial repaint (via [`Epd::update_part_fast`]) *instead of* the factory OTP
/// partial waveform, whose ~540 ms BUSY time is the typing-latency floor and is not
/// reducible any other way (see `PARTIAL_TEMP` and `update_part`'s gate-scan note —
/// both closed). A shorter, custom LUT is the only remaining lever.
///
/// **Provenance:** `LUT_DATA_part` (the array tagged `5.79`) from Good Display's
/// official GDEY0579T93 reference driver `Display_EPD_W21.c`, archive
/// `S-GDEY0579T93-FP(LUT)-20250814` (received 2026-07-21). This is the panel's *own*
/// partial waveform, and it replaces the earlier Waveshare 1.54"/SSD1681 guess that
/// never darkened the ink (see below).
///
/// **Layout (233 bytes, all used).** Bytes `[0..227)` are the phase/timing table
/// written to `0x32` (this includes the FR/XON bytes at `[224..227)`). The trailing 6
/// are drive config, sent to their own registers by [`Epd::update_part_fast`]:
/// `[227]` EOPT → `0x3F`, `[228]` VGH → `0x03`, `[229..232)` VSH1/VSH2/VSL → `0x04`,
/// `[232]` VCOM → `0x2C`. Per-phase frame counts live inside each 7-byte group row
/// (the `0x18/0x58/0x98/0x41/0x81` TP fields); tune those, not a single knob, once
/// BUSY time actually needs cutting.
///
/// **Why the earlier attempt failed (2026-07-19 bench), now explained by the
/// reference:** the 153-byte Waveshare LUT was wrong on every axis for this panel —
/// wrong length (`0x32` expects 227 bytes here), wrong phase content, wrong drive
/// voltages (`EOPT`/`VSH2`/`VCOM`), and it omitted the `0x37` display-option write.
/// `0x32`+`0xCF` was genuinely live, but that waveform could not drive these pixels.
/// This array plus [`Epd::update_part_fast`]'s register sequence is a faithful port
/// of the vendor recipe (`EPD_Part_init_LUT` / `Epaper_Partial`), voltages included.
///
/// **NOT YET VALIDATED ON HARDWARE.** Before any longevity soak: confirm both panel
/// halves paint identically (a mismatch ⇒ the slave `0x32|0x80` write or the cascade
/// is wrong), read the actual BUSY time off the `windowed-fast` trace, and watch for
/// residual ghosting after a full refresh (⇒ back this out). Gated behind the
/// `fast_partial` pref (default off); a full refresh reloads the OTP waveform, so
/// nothing here persists past the next clean pass.
// Trimmed from Good Display's `LUT_DATA_part` (preserved verbatim in
// `reference/gdey0579t93-fp-lut/Display_EPD_W21.c`) to cut per-keystroke latency.
// Each 7-byte row is one phase: byte[0] = frame count, byte[1..3] = packed 2-bit
// level codes, byte[5] = repeat. Waveform BUSY time ∝ the number of active
// (non-zero-frame) rows. Good Display's own 快刷/"fast" waveform (`LUT_DATA1`) speeds
// up by zeroing whole phase-rows (frame count 0x01 → 0x00), so we do the same here.
//
// Iteration 1 (2026-07-21): the weakest *tail* phase of each of the 4 groups — the
// near-noop `0x01,0x01,0x00…` single-sub-step follow-ups — is zeroed (marked TRIMMED),
// taking 12 active phases → 8 (~⅓ fewer frames) while keeping every real drive phase.
// If the bench shows this still darkens with no new ghosting, next step is zeroing the
// remaining `0x01,0x01` follow-ups (g1r1, g4r1) down to the 6 real drive phases.
const FAST_PARTIAL_LUT: [u8; 233] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x18, 0x01, 0x00, 0x00, 0x01, 0x00, // g1 main drive
    0x01, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, // g1 follow-up
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // g1 tail — TRIMMED (was 0x01,0x01,0x00…)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x58, 0x41, 0x00, 0x00, 0x01, 0x00, // g2 main drive
    0x01, 0x41, 0x00, 0x00, 0x00, 0x01, 0x00, // g2 follow-up drive
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // g2 tail — TRIMMED (was 0x01,0x01,0x00…)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x98, 0x81, 0x00, 0x00, 0x01, 0x00, // g3 main drive
    0x01, 0x81, 0x00, 0x00, 0x00, 0x01, 0x00, // g3 follow-up drive
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // g3 tail — TRIMMED (was 0x01,0x01,0x00…)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x18, 0x41, 0x00, 0x00, 0x01, 0x00, // g4 main drive
    0x01, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, // g4 follow-up
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // g4 tail — TRIMMED (was 0x01,0x01,0x00…)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // FR, XON
    0x04, 0x00, 0x00,
    // EOPT, VGH, VSH1, VSH2, VSL, VCOM
    0x06, 0x17, 0x41, 0xA8, 0x32, 0x00,
];

/// `0x22` (Display Update Control 2) value for the fast partial: enable clock +
/// analog, DISPLAY in Mode 2 using the LUT *already written* by `0x32`, then power
/// down — crucially **without** the load-temperature / load-LUT-from-OTP bits that
/// the factory partial's `0xFF` sets (which would overwrite `FAST_PARTIAL_LUT` with
/// the OTP waveform). Matches Waveshare's `TurnOnDisplayPart` for this family.
const FAST_PART_UPDATE: u8 = 0xCF;

pub struct Epd<'d> {
    spi: SpiBusDriver<'d, SpiDriver<'d>>,
    dc: PinDriver<'d, Output>,
    rst: PinDriver<'d, Output>,
    cs: PinDriver<'d, Output>,
    busy: PinDriver<'d, Input>,
    /// A refresh kicked off by `display_frame_async` whose waveform may still
    /// be running. Every public display call waits it out (`wait_ready`)
    /// before sending further controller traffic.
    refresh_pending: bool,
}

impl<'d> Epd<'d> {
    pub fn new(
        spi: SpiBusDriver<'d, SpiDriver<'d>>,
        dc: PinDriver<'d, Output>,
        rst: PinDriver<'d, Output>,
        cs: PinDriver<'d, Output>,
        busy: PinDriver<'d, Input>,
    ) -> Self {
        Self { spi, dc, rst, cs, busy, refresh_pending: false }
    }

    /// Wait out a refresh started by `display_frame_async`, if one is still
    /// running. Safe to call anytime; a no-op when nothing is pending.
    pub fn wait_ready(&mut self) -> Result<(), EspError> {
        if self.refresh_pending {
            self.wait_while_busy(2500)?; // full_refresh_time ≈ 2200 ms
            self.refresh_pending = false;
        }
        Ok(())
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
        if RAM_SETTLE_MS > 0 {
            FreeRtos::delay_ms(RAM_SETTLE_MS);
        }
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
        self.kick_update_full()?;
        self.wait_while_busy(2500)?; // full_refresh_time ≈ 2200 ms
        Ok(())
    }

    /// The command half of `update_full`: starts the full-refresh waveform
    /// (~2.2 s) and returns while it runs. The caller owns the eventual BUSY
    /// wait before any further controller traffic.
    fn kick_update_full(&mut self) -> Result<(), EspError> {
        self.set_ram_area(0, 0, WIDTH / 2, HEIGHT, 0x03, 0x80)?; // slave
        self.set_ram_area(0, 0, WIDTH / 2, HEIGHT, 0x03, 0x00)?; // master
        self.cmd(0x21)?; // display update control 1
        self.data(&[0x40, 0x10])?; // bypass RED as 0, cascade
        self.cmd(0x1A)?; // temperature register (fast full update)
        self.data(&[0x64, 0x00])?;
        self.cmd(0x22)?;
        self.data(&[0xD7])?; // fast full update
        self.cmd(0x20)?; // master activation
        Ok(())
    }

    /// Port of GxEPD2 `_Update_Part` — the partial-update waveform. No full
    /// flashing; only pixels that differ between the "previous" (`0x26`) and
    /// "current" (`0x24`) banks transition. Much faster than a full refresh
    /// but leaves faint ghosting that a periodic full refresh clears.
    /// `y0`/`h` restrict the RAM window (and thus the SPI transfer) to a
    /// horizontal band of rows; the *waveform* still drives the whole panel.
    ///
    /// Do NOT try to restrict the gate scan to the band (driver output
    /// control `0x01` MUX + gate scan start `0x0F`) — spiked and refuted on
    /// hardware 2026-07-16: a 20-gate scan still took 571 ms (vs 543 ms for
    /// the default full scan), so the waveform's BUSY time does not scale
    /// with MUX here, and writing `0x01` with the datasheet POR scan-order
    /// byte mirrored the panel vertically — the real gate config is loaded
    /// from panel OTP at reset and can't be read back, so any `0x01` write
    /// risks clobbering it for zero gain.
    fn update_part(&mut self, y0: u16, h: u16) -> Result<(), EspError> {
        self.set_ram_area(0, y0, WIDTH / 2, h, 0x03, 0x80)?; // slave
        self.set_ram_area(0, y0, WIDTH / 2, h, 0x03, 0x00)?; // master
        self.cmd(0x3C)?; // border waveform control
        self.data(&[0x80])?; // VCOM
        self.cmd(0x21)?; // display update control 1
        self.data(&[0x00, 0x10])?; // RED normal, cascade
        if let Some(temp) = PARTIAL_TEMP {
            // EXPERIMENT: override the LUT temperature index for this partial.
            // The 0x22←0xFF kick below includes load-temp + load-LUT, so this
            // takes effect on the very next activation. See PARTIAL_TEMP.
            self.cmd(0x1A)?; // write to temperature register
            self.data(&temp)?;
        }
        self.cmd(0x22)?; // display update control 2
        self.data(&[0xFF])?; // partial update (incl. load-temp + load-LUT)
        self.cmd(0x20)?; // master activation
        self.wait_while_busy(2000)?; // partial is well under the full ~2.2 s
        Ok(())
    }

    /// EXPERIMENTAL fast partial (see [`FAST_PARTIAL_LUT`]): identical to
    /// [`update_part`](Self::update_part) except it loads the panel's own custom
    /// partial waveform via `0x32` and triggers with [`FAST_PART_UPDATE`] (`0xCF`) so the panel
    /// displays with *that* LUT rather than reloading the ~540 ms OTP one. The LUT
    /// is written to **both** controllers (`0x32` master, `0x32|0x80` slave): each
    /// half has its own waveform SRAM, so writing only the master would leave the
    /// left half on its OTP waveform and the two halves would ghost differently.
    /// Reached only from the `fast_partial`-gated windowed-additive path.
    fn update_part_fast(&mut self, y0: u16, h: u16) -> Result<(), EspError> {
        self.set_ram_area(0, y0, WIDTH / 2, h, 0x03, 0x80)?; // slave
        self.set_ram_area(0, y0, WIDTH / 2, h, 0x03, 0x00)?; // master
        // FAST_PARTIAL_LUT bytes [0..LUT) are the 0x32 phase table (incl. FR/XON);
        // the trailing 6 are drive config fanned out to their own registers below.
        // Both controllers get the whole recipe — each half has its own waveform
        // SRAM *and* charge pump, so a master-only write leaves the left half on its
        // OTP waveform/voltages and the two halves ghost differently. This mirrors
        // Good Display's `Epaper_Partial` recipe (0x32, 0x3F, 0x03, 0x04, 0x2C, 0x37).
        const LUT: usize = 227;
        for target in [0x80u8, 0x00u8] {
            self.cmd(0x32 | target)?; // LUT register (waveform phases + FR/XON)
            self.data(&FAST_PARTIAL_LUT[..LUT])?;
            self.cmd(0x3F | target)?; // EOPT — LUT end option
            self.data(&[FAST_PARTIAL_LUT[LUT]])?;
            self.cmd(0x03 | target)?; // VGH — gate driving voltage
            self.data(&[FAST_PARTIAL_LUT[LUT + 1]])?;
            self.cmd(0x04 | target)?; // VSH1, VSH2, VSL — source driving voltage
            self.data(&FAST_PARTIAL_LUT[LUT + 2..LUT + 5])?;
            self.cmd(0x2C | target)?; // VCOM
            self.data(&[FAST_PARTIAL_LUT[LUT + 5]])?;
            // 0x37 display-option: omitting this was one of the 2026-07-19 defects.
            self.cmd(0x37 | target)?;
            self.data(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00])?;
        }
        // Border: left at the known-good 0x80 (the vendor custom-LUT recipe uses
        // 0xC0 — a cosmetic edge knob to try only if the border misbehaves).
        self.cmd(0x3C)?; // border waveform control
        self.data(&[0x80])?;
        self.cmd(0x21)?; // display update control 1
        self.data(&[0x00, 0x10])?; // RED normal, cascade
        self.cmd(0x22)?; // display update control 2
        self.data(&[FAST_PART_UPDATE])?; // 0xCF: display with the LUT just written
        self.cmd(0x20)?; // master activation
        self.wait_while_busy(2000)?;
        Ok(())
    }

    /// Fill the whole panel with one value and full-refresh.
    /// `0xFF` = white, `0x00` = black. Port of GxEPD2 `clearScreen`.
    pub fn clear_screen(&mut self, value: u8) -> Result<(), EspError> {
        self.wait_ready()?;
        self.write_buffer(0x26, value)?; // previous
        self.write_buffer(0x24, value)?; // current
        self.update_full()?;
        Ok(())
    }

    /// Blit rows `y0..y0+h` of a 792×272 framebuffer into one RAM bank on both
    /// controllers. Port of GxEPD2 `_writeFromImage`, windowed in Y: slave gets
    /// panel bytes 0..=49 of each row in X-increment mode; the master's sources
    /// are wired mirrored, so it gets bytes 49..=98 in bitmap order while the
    /// address counter walks RAM 49..=0 (mode 0x02). The seam byte 49
    /// (px 392..399) lands on both; the 4 columns past each controller's 396
    /// sources aren't wired. Pass `(0, HEIGHT)` for a full-frame blit.
    fn write_frame_bank(&mut self, command: u8, fb: &[u8], y0: u16, h: u16) -> Result<(), EspError> {
        let rows = y0 as usize..(y0 + h) as usize;

        let mut buf = Vec::with_capacity(CTRL_BYTES_W * h as usize);
        for y in rows.clone() {
            let row = &fb[y * FB_BYTES_W..(y + 1) * FB_BYTES_W];
            buf.extend_from_slice(&row[..CTRL_BYTES_W]);
        }
        self.set_ram_area(0, y0, WIDTH / 2, h, 0x03, 0x80)?; // slave
        self.cmd(command | 0x80)?;
        self.data(&buf)?;

        buf.clear();
        for y in rows {
            let row = &fb[y * FB_BYTES_W..(y + 1) * FB_BYTES_W];
            buf.extend_from_slice(&row[FB_BYTES_W - CTRL_BYTES_W..]);
        }
        self.set_ram_area(0, y0, WIDTH / 2, h, 0x02, 0x00)?; // master
        self.cmd(command)?;
        self.data(&buf)?;
        Ok(())
    }

    /// Show a full 792×272 framebuffer (`FB_BYTES` long) with a full
    /// refresh. Writes both RAM banks so the next differential update has a
    /// consistent "previous" image.
    pub fn display_frame(&mut self, fb: &[u8]) -> Result<(), EspError> {
        assert_eq!(fb.len(), FB_BYTES, "framebuffer must be 99 x 272 bytes");
        self.wait_ready()?;
        self.write_frame_bank(0x26, fb, 0, HEIGHT)?; // previous
        self.write_frame_bank(0x24, fb, 0, HEIGHT)?; // current
        self.update_full()?;
        Ok(())
    }

    /// `display_frame` minus the wait: writes both RAM banks, starts the
    /// full-refresh waveform (~2.2 s), and returns immediately so the caller
    /// can do other work (SD mount, note load) while the panel paints itself.
    /// Every public display call waits out the pending refresh (`wait_ready`)
    /// before its own controller traffic, so nothing can collide with it.
    pub fn display_frame_async(&mut self, fb: &[u8]) -> Result<(), EspError> {
        assert_eq!(fb.len(), FB_BYTES, "framebuffer must be 99 x 272 bytes");
        self.wait_ready()?;
        self.write_frame_bank(0x26, fb, 0, HEIGHT)?; // previous
        self.write_frame_bank(0x24, fb, 0, HEIGHT)?; // current
        self.kick_update_full()?;
        self.refresh_pending = true;
        Ok(())
    }

    /// Partial-refresh only rows `y0..y0+h` of the panel from a full
    /// framebuffer — the fast per-keystroke path (pass `(0, HEIGHT)` for the
    /// whole panel). Requires the banks to already hold the on-screen image
    /// for those rows — true after any `display_frame`, `clear_screen`, or a
    /// prior partial covering them. Writes the new rows to `0x24`, runs the
    /// partial waveform over just that band, then re-writes the band to BOTH
    /// banks. Both, not just `0x26`: the controller ping-pongs its two RAM
    /// buffers on a Mode-2 display, so post-refresh the bank addressed as
    /// `0x24` is the stale one. Syncing only `0x26` (this driver's original
    /// port, until 2026-07-16) left `0x24` two frames old outside each
    /// update's band, and the next partial drove the panel back toward it —
    /// on the panel, lines/chars from the previous batches flapped in and out
    /// while typing fast. GxEPD2's `writeImageAgain` (same panel) writes
    /// `0x26` then `0x24` after every partial refresh; this is that sequence.
    /// `fb` is always the full frame; only the given rows are used.
    pub fn display_frame_partial_window(
        &mut self,
        fb: &[u8],
        y0: u16,
        h: u16,
    ) -> Result<(), EspError> {
        self.partial_window(fb, y0, h, false)
    }

    /// The experimental fast-waveform twin of
    /// [`display_frame_partial_window`](Self::display_frame_partial_window): same
    /// windowed sequence, but the transition runs the short custom LUT
    /// ([`update_part_fast`](Self::update_part_fast)) instead of the OTP partial.
    /// The render engine calls this only for the per-keystroke additive repaint and
    /// only when the `fast_partial` pref is on. See [`FAST_PARTIAL_LUT`].
    pub fn display_frame_partial_window_fast(
        &mut self,
        fb: &[u8],
        y0: u16,
        h: u16,
    ) -> Result<(), EspError> {
        self.partial_window(fb, y0, h, true)
    }

    /// Shared body of the two windowed-partial methods above. `fast` selects the
    /// custom-LUT waveform; everything else — the RAM-bank writes and the
    /// post-refresh resync — is identical and kept here in one place.
    fn partial_window(&mut self, fb: &[u8], y0: u16, h: u16, fast: bool) -> Result<(), EspError> {
        assert_eq!(fb.len(), FB_BYTES, "framebuffer must be 99 x 272 bytes");
        assert!(h > 0 && y0 + h <= HEIGHT, "row window out of range");
        self.wait_ready()?;
        self.write_frame_bank(0x24, fb, y0, h)?; // current = new
        if fast {
            self.update_part_fast(y0, h)?; // transition previous -> current
        } else {
            self.update_part(y0, h)?;
        }
        self.write_frame_bank(0x26, fb, y0, h)?; // resync both banks…
        self.write_frame_bank(0x24, fb, y0, h)?; // …post ping-pong
        Ok(())
    }
}

/// The [`hal::Screen`] port: the render engine (`app::Panel`) drives the panel
/// through this contract rather than the concrete `Epd`, so it no longer names
/// esp-idf. Both methods forward to the inherent driver methods above; the
/// associated error is esp-idf's `EspError`, kept off the layers above by the
/// trait's associated type.
impl hal::Screen for Epd<'_> {
    type Error = EspError;

    fn display_frame(&mut self, fb: &[u8]) -> Result<(), Self::Error> {
        Epd::display_frame(self, fb)
    }

    fn display_frame_partial_window(
        &mut self,
        fb: &[u8],
        y0: u16,
        h: u16,
    ) -> Result<(), Self::Error> {
        Epd::display_frame_partial_window(self, fb, y0, h)
    }

    fn display_frame_partial_window_fast(
        &mut self,
        fb: &[u8],
        y0: u16,
        h: u16,
    ) -> Result<(), Self::Error> {
        Epd::display_frame_partial_window_fast(self, fb, y0, h)
    }
}
