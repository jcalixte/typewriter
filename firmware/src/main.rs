use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::PinDriver;
use esp_idf_svc::hal::peripherals::Peripherals;

fn main() -> anyhow::Result<()> {
    // Required once before any esp-idf-svc call; some runtime patches
    // only link if this symbol is referenced. See esp-idf-template#71.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let mut led = PinDriver::output(peripherals.pins.gpio2)?;

    log::info!("Typoena Spike 1 — Blink on GPIO 2");

    let mut n: u32 = 0;
    loop {
        led.set_high()?;
        log::info!("blink {n}");
        n = n.wrapping_add(1);
        FreeRtos::delay_ms(500);

        led.set_low()?;
        FreeRtos::delay_ms(500);
    }
}
