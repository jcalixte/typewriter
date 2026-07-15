//! Firmware driver for the onboarding wizard (v0.9 slice 2).
//!
//! The wizard crate is pure logic; this module is its I/O: keys from
//! `usb_kbd`, frames to the panel, and the effects executed against real
//! hardware. Runs *before* the git thread spawns, so it may borrow the modem
//! for the Wi-Fi join test and release it (dropping the `EspWifi`) for the
//! normal boot path to own afterwards.
//!
//! Slice status: `TestWifi` / `WriteConf` / `Finish` are real. `StartAuth`,
//! `FetchRepos` and `Clone` (slices 3–4) surface as a hard stop with a
//! pointer at the installer — the honest intermediate state, reachable only
//! on an unconfigured card.

use anyhow::{bail, Result};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};

use display::{Frame, HEIGHT};
use firmware::epd::Epd;
use firmware::net::connect_wifi;
use firmware::persistence::Storage;
use wizard::{Effect, Event, Wizard};

use crate::usb_kbd;

/// Run the wizard to completion and return the final conf for the normal
/// boot path to install (`set_card_conf`). Blocks the boot; the editor only
/// exists after this returns. An `Err` is terminal (main `boot_halt`s with
/// it) — today that includes reaching the not-yet-built sign-in step.
pub fn run(
    epd: &mut Epd,
    storage: &Storage,
    start: conf::Conf,
    sys_loop: &EspSystemEventLoop,
    nvs: &EspDefaultNvsPartition,
    modem: &mut Modem,
) -> Result<conf::Conf> {
    let mut wiz = Wizard::resume(start);
    let mut frame = Frame::new_white();
    let mut queue: Vec<Effect> = wiz.pending().into_iter().collect();
    let mut first_paint = true;
    let mut dirty = true;

    loop {
        // Paint before executing: waiting screens ("Joining Wi-Fi…") must be
        // visible while their effect blocks below. First paint is a full
        // refresh (clears the splash cleanly), the rest ride the ~630 ms
        // full-area partial like live typing does.
        if dirty {
            wiz.draw_into(&mut frame);
            if first_paint {
                epd.display_frame(frame.bytes())?;
                first_paint = false;
            } else {
                epd.display_frame_partial_window(frame.bytes(), 0, HEIGHT)?;
            }
            dirty = false;
        }

        if !queue.is_empty() {
            let fx = queue.remove(0);
            match fx {
                Effect::WriteConf(c) => {
                    storage.write_conf(&c.render())?;
                    log::info!("wizard: conf persisted");
                }
                Effect::TestWifi { ssid, pass } => {
                    let ev = test_wifi(sys_loop, nvs, modem, &ssid, &pass);
                    queue.extend(wiz.event(ev));
                    dirty = true;
                }
                Effect::StartAuth | Effect::FetchRepos | Effect::Clone { .. } => {
                    // Slices 3–4. Stop rather than loop: feeding AuthFailed
                    // back would re-request StartAuth forever.
                    bail!(
                        "Wi-Fi is saved. On-device GitHub sign-in and repo setup \
                         land in the next firmware - until then, finish this card \
                         with the installer (typoena.dev)."
                    );
                }
                Effect::Finish => return Ok(wiz.conf().clone()),
            }
            continue;
        }

        match usb_kbd::next_key() {
            Some(k) => {
                queue.extend(wiz.key(k));
                // Coalesce a typing burst into one repaint (and preserve any
                // effects each key produced, in order).
                while let Some(k) = usb_kbd::next_key() {
                    queue.extend(wiz.key(k));
                }
                dirty = true;
            }
            None => FreeRtos::delay_ms(10),
        }
    }
}

/// One bounded join attempt. The `EspWifi` is dropped on the way out — pass
/// or fail, the radio and modem go back to the boot path (the git thread
/// re-associates on the first `:gp`; a session's second join is fast).
fn test_wifi(
    sys_loop: &EspSystemEventLoop,
    nvs: &EspDefaultNvsPartition,
    modem: &mut Modem,
    ssid: &str,
    pass: &str,
) -> Event {
    let attempt = (|| -> Result<()> {
        // Reborrow the Wi-Fi half for the duration of the test; the owned
        // Modem stays with the boot path for the git thread.
        let (wifi_modem, _) = modem.split_reborrow();
        let mut w = BlockingWifi::wrap(
            EspWifi::new(wifi_modem, sys_loop.clone(), Some(nvs.clone()))?,
            sys_loop.clone(),
        )?;
        connect_wifi(&mut w, ssid, pass)?;
        let ip = w.wifi().sta_netif().get_ip_info()?;
        log::info!("wizard: joined {ssid}, ip {}", ip.ip);
        Ok(())
    })();
    match attempt {
        Ok(()) => Event::WifiOk,
        Err(e) => {
            log::warn!("wizard: join failed: {e:#}");
            Event::WifiFailed(format!("{e:#}"))
        }
    }
}
