//! Keyboard smoke test — USB host boot keyboard, nothing else.
//!
//! The lightest possible "does the keyboard work on this board?" check: it
//! brings up the shared [`firmware::usb_kbd`] host stack and echoes every
//! decoded key to the serial console. No SD card, no e-paper, no Wi-Fi — so it
//! runs on a bare ESP32-S3 with only a USB keyboard plugged into the OTG port
//! (the editor firmware `boot_halt`s on a missing card long before the keyboard
//! ever starts, which is exactly what this bin sidesteps).
//!
//! It links no libgit2 and uses the default single-app partition table, so
//! `just flash-kbd` builds and flashes fast. Type on the attached keyboard and
//! watch the `keyboard: …` lines on the monitor; the running `line` shows
//! printable characters accumulating so you can confirm the full US-QWERTY +
//! dead-key accent path, not just that reports arrive.

use esp_idf_svc::hal::delay::FreeRtos;

use firmware::usb_kbd::{self, Key};

/// Injected by build.rs so serial output identifies the exact build.
const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

fn main() -> anyhow::Result<()> {
    // Required once before any esp-idf-svc call; some runtime patches only link
    // if this symbol is referenced. See esp-idf-template#71.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Typoena — keyboard smoke test, {BUILD_TAG}");

    // Install the USB host stack and spawn its pumps; keys arrive via next_key().
    usb_kbd::start()?;
    log::info!("plug a USB keyboard into the OTG port and start typing…");

    // Drain decoded keys and echo them. `usb_kbd` already logs each raw key on
    // decode; here we also keep a running line of printable text so you can see
    // words form (and dead-key accents compose) rather than just event spam.
    let mut line = String::new();
    loop {
        while let Some(key) = usb_kbd::next_key() {
            match key {
                Key::Char(c) => line.push(c),
                Key::Enter => line.clear(),
                Key::Backspace => {
                    line.pop();
                }
                _ => {}
            }
            log::info!("keyboard: {key:?}   line so far: {line:?}");
        }
        FreeRtos::delay_ms(20);
    }
}
