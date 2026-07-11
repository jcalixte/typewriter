//! Spike 9 — boot splash.
//!
//! Paints the Typoena wordmark inside a circle, centred on the 792×272 panel,
//! with one clean full refresh — the image the appliance shows at boot before
//! the editor opens (v0.1's "e-ink shows Typoena splash").
//!
//! The frame itself is [`display::Frame::splash`], a pure `embedded-graphics`
//! drawing shared with `main.rs`'s boot path (so this bench binary and the real
//! boot show the identical mark). It draws **vectors** — a stroked circle + a
//! centred `FONT_10X20` string — rather than the embedded 1-bit *bitmap* asset
//! sketched in `docs/spikes.md`: a deliberate trade. Spike 2 already proved
//! vector + font rendering, so the splash carries no new stack risk and needs
//! no asset-embed step, but it also does NOT retire the image-blit pipeline the
//! doc named as Spike 9's only risk. A raster logo is deferred to v1.0 polish.
//!
//! EPD bring-up mirrors `main.rs` (SPI2, SCK 12 · DIN/MOSI 11 · CS 7 · DC 6 ·
//! RST 5 · BUSY 4), driving the shared [`firmware::epd`] driver. Flash with
//! `just flash-splash`. Needs no `.env`.

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{AnyIOPin, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::spi::config::{Config, DriverConfig};
use esp_idf_svc::hal::spi::{Dma, SpiBusDriver, SpiDriver};
use esp_idf_svc::hal::units::FromValueType;

use display::Frame;
use firmware::epd::Epd;

/// Injected by build.rs so serial output identifies the exact build.
const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

fn main() -> anyhow::Result<()> {
    // Required once before any esp-idf-svc call; some runtime patches only link
    // if this symbol is referenced. See esp-idf-template#71.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Typoena — Spike 9 (boot splash), {BUILD_TAG}");

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;

    // GDEY0579T93 on the same S3-safe GPIOs main.rs uses (Spike 2 wiring):
    //   SCK 12 · DIN/MOSI 11 · CS 7 · DC 6 · RST 5 · BUSY 4
    let spi = SpiDriver::new(
        peripherals.spi2,
        pins.gpio12,
        pins.gpio11,
        None::<AnyIOPin>,
        &DriverConfig::new().dma(Dma::Auto(4096)),
    )?;
    let bus = SpiBusDriver::new(spi, &Config::new().baudrate(4.MHz().into()))?;
    let cs = PinDriver::output(pins.gpio7)?;
    let dc = PinDriver::output(pins.gpio6)?;
    let rst = PinDriver::output(pins.gpio5)?;
    let busy = PinDriver::input(pins.gpio4, Pull::Down)?;
    let mut epd = Epd::new(bus, dc, rst, cs, busy);

    log::info!("EPD reset + init…");
    epd.reset()?;
    epd.init()?;

    log::info!("painting splash…");
    epd.display_frame(Frame::splash().bytes())?;
    log::info!("✅ Spike 9 complete — splash on panel");

    // Idle so the splash stays up and the result stays on the monitor.
    loop {
        FreeRtos::delay_ms(1000);
    }
}
