//! Shared networking helpers.
//!
//! Extracted from the near-identical `connect_wifi` copies that lived in the
//! wifi_tls / git_push / git_sync spikes. The single copy adds the resilience
//! the spikes lacked: a bounded retry with exponential backoff around
//! association + DHCP.

use anyhow::{Context, Result};
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

/// Association + DHCP attempts before giving up. The first attempt after a
/// reset frequently fails: the AP may still hold the pre-reset association and
/// reject the re-join until it ages out, the radio may not be settled, and DHCP
/// can drop the first DISCOVER on a cold interface. Retrying a handful of times
/// turns those transient misses into a clean connect on attempt 2–3 instead of
/// a failed boot.
const MAX_ATTEMPTS: u32 = 5;

/// Backoff before the first retry; doubles each attempt up to [`MAX_BACKOFF_MS`].
const INITIAL_BACKOFF_MS: u32 = 500;
const MAX_BACKOFF_MS: u32 = 4000;

/// Configure the station, start the radio, and associate with `ssid`, retrying
/// association + DHCP with exponential backoff.
///
/// `set_configuration` and `start` run once; only the association + netif-up
/// wait is retried — restarting the radio each attempt is wasteful and can wedge
/// the driver. Between attempts the station is disconnected to clear any
/// half-open state before the next `connect`.
///
/// An empty `pass` selects an open network; otherwise WPA2-Personal.
pub fn connect_wifi(
    wifi: &mut BlockingWifi<EspWifi<'static>>,
    ssid: &str,
    pass: &str,
) -> Result<()> {
    let auth_method = if pass.is_empty() {
        AuthMethod::None
    } else {
        AuthMethod::WPA2Personal
    };
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: ssid.try_into().ok().context("SSID > 32 bytes")?,
        password: pass.try_into().ok().context("password > 64 bytes")?,
        auth_method,
        ..Default::default()
    }))?;
    wifi.start()?;

    let mut backoff_ms = INITIAL_BACKOFF_MS;
    for attempt in 1..=MAX_ATTEMPTS {
        log::info!("associating with \"{ssid}\" (attempt {attempt}/{MAX_ATTEMPTS})…");
        match associate_once(wifi) {
            Ok(()) => return Ok(()),
            Err(e) if attempt < MAX_ATTEMPTS => {
                log::warn!("Wi-Fi attempt {attempt} failed: {e:#}; retrying in {backoff_ms} ms");
                // Clear any half-open association before the next connect. Ignore
                // the result — it errors harmlessly when we never associated.
                let _ = wifi.disconnect();
                FreeRtos::delay_ms(backoff_ms);
                backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
            }
            Err(e) => {
                return Err(e).with_context(|| format!("Wi-Fi failed after {MAX_ATTEMPTS} attempts"));
            }
        }
    }
    unreachable!("loop returns on success or on the final-attempt error")
}

/// One association + DHCP wait. Split out so the retry loop reads cleanly and to
/// keep the two `wifi` borrows in separate statements.
fn associate_once(wifi: &mut BlockingWifi<EspWifi<'static>>) -> Result<()> {
    wifi.connect().context("Wi-Fi association failed")?;
    wifi.wait_netif_up().context("DHCP / netif never came up")?;
    Ok(())
}
