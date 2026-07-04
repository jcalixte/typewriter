mod epd;

use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle};
use embedded_graphics::text::{Alignment, Text};
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{AnyIOPin, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::spi::config::{Config, DriverConfig};
use esp_idf_svc::hal::spi::{Dma, SpiBusDriver, SpiDriver};
use esp_idf_svc::hal::units::FromValueType;

use epd::Epd;

/// Injected by build.rs so serial output and the panel itself identify the
/// exact build being diagnosed.
const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

fn main() -> anyhow::Result<()> {
    // Required once before any esp-idf-svc call; some runtime patches
    // only link if this symbol is referenced. See esp-idf-template#71.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;

    // GDEY0579T93 wiring on S3-safe GPIOs (clear of flash 26–32, octal PSRAM
    // 33–37, strapping 0/3/45/46, USB 19/20, RGB LED 38/48). See
    // docs/v0.1-mvp-technical.md (Spike 2):
    //   SCK 12 · DIN/MOSI 11 · CS 7 · DC 6 · RST 5 · BUSY 4
    let spi = SpiDriver::new(
        peripherals.spi2,
        pins.gpio12,              // SCK
        pins.gpio11,              // SDO / MOSI (DIN)
        None::<AnyIOPin>,         // SDI / MISO — unused (write-only panel)
        &DriverConfig::new().dma(Dma::Auto(4096)),
    )?;
    // 4 MHz — GxEPD2's default for this controller. Verified clean on the
    // breadboard rig; a loose CS jumper (not clock speed) was behind the
    // early bring-up noise.
    let bus = SpiBusDriver::new(spi, &Config::new().baudrate(4.MHz().into()))?;

    let cs = PinDriver::output(pins.gpio7)?;
    let dc = PinDriver::output(pins.gpio6)?;
    let rst = PinDriver::output(pins.gpio5)?;
    let busy = PinDriver::input(pins.gpio4, Pull::Down)?;

    let mut epd = Epd::new(bus, dc, rst, cs, busy);

    log::info!("Typoena Spike 2b — GDEY0579T93 text test, {BUILD_TAG}");
    log::info!("hardware reset…");
    epd.reset()?;
    log::info!("init…");
    epd.init()?;
    epd.clear_screen(0xFF)?; // initial clean slate, per GxEPD2

    // Alternate normal and inverted frames: circle on the master/slave seam
    // (proves the split-and-mirror blit), "Typoena" inside it, and the build
    // tag at the bottom so the panel identifies the running build.
    let frames = [make_frame(false), make_frame(true)];
    let mut i = 0;
    loop {
        log::info!("frame → {}", if i == 0 { "black on WHITE" } else { "white on BLACK" });
        epd.display_frame(frames[i].bytes())?;
        log::info!("refresh done; holding 3 s");
        FreeRtos::delay_ms(3000);
        i = 1 - i;
    }
}

/// Circle centered on the controller seam, "Typoena" inside, build tag at
/// the bottom edge. `inverted` swaps ink and paper.
fn make_frame(inverted: bool) -> epd::Frame {
    let (mut frame, ink) = if inverted {
        (epd::Frame::new_black(), BinaryColor::Off)
    } else {
        (epd::Frame::new_white(), BinaryColor::On)
    };
    let center = Point::new(epd::WIDTH as i32 / 2, epd::HEIGHT as i32 / 2);
    let style = MonoTextStyle::new(&FONT_10X20, ink);
    Circle::with_center(center, 200)
        .into_styled(PrimitiveStyle::with_stroke(ink, 6))
        .draw(&mut frame)
        .unwrap();
    Text::with_alignment("Typoena", center + Point::new(0, 7), style, Alignment::Center)
        .draw(&mut frame)
        .unwrap();
    Text::with_alignment(
        BUILD_TAG,
        Point::new(center.x, epd::HEIGHT as i32 - 10),
        style,
        Alignment::Center,
    )
    .draw(&mut frame)
    .unwrap();
    frame
}
