use std::time::Duration;

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::PinDriver;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::rmt::config::{TransmitConfig, TxChannelConfig};
use esp_idf_svc::hal::rmt::encoder::CopyEncoder;
use esp_idf_svc::hal::rmt::{PinState, Symbol, TxChannelDriver};
use esp_idf_svc::hal::units::Hertz;

const WS2812_RESOLUTION: Hertz = Hertz(10_000_000);

fn main() -> anyhow::Result<()> {
    // Required once before any esp-idf-svc call; some runtime patches
    // only link if this symbol is referenced. See esp-idf-template#71.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let mut led = PinDriver::output(peripherals.pins.gpio2)?;

    // On-board addressable LED (WS2812) — GPIO 48 on the DevKitC-1 v1.0;
    // v1.1 boards moved it to GPIO 38.
    let mut rgb = TxChannelDriver::new(
        peripherals.pins.gpio48,
        &TxChannelConfig {
            resolution: WS2812_RESOLUTION,
            ..Default::default()
        },
    )?;

    log::info!("Typoena Spike 1 — Blink on GPIO 2 + on-board WS2812 (GPIO 48)");

    let mut n: u32 = 0;
    loop {
        led.set_high()?;
        ws2812_set(&mut rgb, 4, 0, 24)?;
        log::info!("blink {n}");
        n = n.wrapping_add(1);
        FreeRtos::delay_ms(500);

        led.set_low()?;
        ws2812_set(&mut rgb, 0, 0, 0)?;
        FreeRtos::delay_ms(500);
    }
}

/// Send one WS2812 GRB frame. The ≥50 µs low reset the LED needs between
/// frames is covered by the 500 ms idle gap between calls.
fn ws2812_set(tx: &mut TxChannelDriver, r: u8, g: u8, b: u8) -> anyhow::Result<()> {
    // Bit timings from the WS2812 datasheet.
    let zero = Symbol::new_with(
        WS2812_RESOLUTION,
        PinState::High,
        Duration::from_nanos(350),
        PinState::Low,
        Duration::from_nanos(800),
    )?;
    let one = Symbol::new_with(
        WS2812_RESOLUTION,
        PinState::High,
        Duration::from_nanos(700),
        PinState::Low,
        Duration::from_nanos(600),
    )?;

    let mut symbols = [zero; 24];
    for (i, byte) in [g, r, b].into_iter().enumerate() {
        for bit in 0..8 {
            if byte & (0x80 >> bit) != 0 {
                symbols[i * 8 + bit] = one;
            }
        }
    }
    tx.send_and_wait(CopyEncoder::new()?, &symbols, &TransmitConfig::default())?;
    Ok(())
}
