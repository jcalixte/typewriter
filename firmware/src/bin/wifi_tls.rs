//! Spike 6 — Wi-Fi + TLS.
//!
//! A small standalone bench program (separate binary from the editor firmware)
//! that proves the networking + TLS stack end to end:
//!
//!   1. Bring up the station and associate with the home AP.
//!   2. Sync the clock over SNTP — mbedtls checks the server cert's
//!      not-before/not-after against wall time, so without this the 1970 RTC
//!      makes every handshake fail with "certificate is not valid yet".
//!   3. HTTPS GET https://api.github.com/ with cert-chain validation against
//!      the esp-idf certificate bundle (esp_crt_bundle_attach), and read the
//!      response body.
//!
//! A validated GET is the whole point: it's the gate for Spike 7 (gitoxide
//! push over HTTPS + PAT). Free heap is logged around the handshake because
//! TLS heap pressure on this chip is a top-3 watched risk (see qfd.md §6).
//!
//! Credentials come from build-time env (build.rs → env!): set TW_WIFI_SSID /
//! TW_WIFI_PASS in firmware/.env and run `just flash-wifi`.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use embedded_svc::http::Method;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::{EspSntp, SyncStatus};
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

/// Injected by build.rs so serial output identifies the exact build.
const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

/// Wi-Fi credentials, baked in at build time from firmware/.env. Empty when
/// unset — checked at runtime so the editor build never depends on them.
const WIFI_SSID: &str = env!("TW_WIFI_SSID");
const WIFI_PASS: &str = env!("TW_WIFI_PASS");

/// The validated endpoint. Root of the GitHub REST API: returns JSON, needs a
/// User-Agent, and is served over a normal public CA chain — a faithful
/// stand-in for the api.github.com host Spike 7 will push through.
const TEST_URL: &str = "https://api.github.com/";

/// SNTP first-sync budget. Home networks resolve pool.ntp.org and answer well
/// within this; failing past it is a real problem worth surfacing.
const SNTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

fn main() -> Result<()> {
    // Required once before any esp-idf-svc call; some runtime patches only link
    // if this symbol is referenced. See esp-idf-template#71.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Typoena — Spike 6 (Wi-Fi + TLS), {BUILD_TAG}");

    match run() {
        Ok(()) => log::info!("✅ Spike 6 complete — Wi-Fi assoc + SNTP + validated HTTPS GET"),
        Err(e) => log::error!("❌ Spike 6 failed: {e:?}"),
    }

    // Idle instead of returning, so the result stays on screen and Wi-Fi/other
    // tasks keep running for inspection rather than the app winding down.
    loop {
        FreeRtos::delay_ms(1000);
    }
}

fn run() -> Result<()> {
    if WIFI_SSID.is_empty() {
        bail!("TW_WIFI_SSID is empty — set it in firmware/.env (see .env.example) and rebuild");
    }

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;

    connect_wifi(&mut wifi)?;
    let ip = wifi.wifi().sta_netif().get_ip_info()?;
    log::info!("Wi-Fi up — IP {}, GW {}", ip.ip, ip.subnet.gateway);

    sync_clock()?;

    https_get(TEST_URL)?;
    Ok(())
}

/// Associate with the configured AP and wait for the netif (DHCP) to come up.
fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> Result<()> {
    // Open network → no auth; otherwise WPA2-Personal (see .env.example for WPA3).
    let auth_method = if WIFI_PASS.is_empty() {
        AuthMethod::None
    } else {
        AuthMethod::WPA2Personal
    };

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID
            .try_into()
            .ok()
            .context("SSID longer than 32 bytes")?,
        password: WIFI_PASS
            .try_into()
            .ok()
            .context("password longer than 64 bytes")?,
        auth_method,
        ..Default::default()
    }))?;

    wifi.start()?;
    log::info!("associating with \"{WIFI_SSID}\"…");
    wifi.connect().context("Wi-Fi association failed")?;
    wifi.wait_netif_up().context("DHCP / netif never came up")?;
    Ok(())
}

/// Kick off SNTP and block until the first sync (or time out). Required before
/// TLS: cert validity is checked against wall time.
fn sync_clock() -> Result<()> {
    let sntp = EspSntp::new_default()?;
    log::info!("SNTP started, waiting for first sync…");

    let start = Instant::now();
    while sntp.get_sync_status() != SyncStatus::Completed {
        if start.elapsed() >= SNTP_TIMEOUT {
            bail!("SNTP did not sync within {SNTP_TIMEOUT:?} — TLS cert validity would fail");
        }
        FreeRtos::delay_ms(100);
    }

    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // A synced clock lands well past this (2023-11-14); anything below means the
    // RTC never actually advanced and TLS would reject on validity.
    if unix < 1_700_000_000 {
        bail!("clock still at {unix} after SNTP sync — refusing TLS with a bad wall clock");
    }
    log::info!("clock synced — unix {unix}");
    Ok(())
}

/// HTTPS GET with cert-chain validation against the esp-idf certificate bundle.
/// Logs status, the first chunk of the body, and free heap around the request.
fn https_get(url: &str) -> Result<()> {
    let heap_before = unsafe { esp_idf_svc::sys::esp_get_free_heap_size() };

    let mut conn = EspHttpConnection::new(&HttpConfig {
        // Validate the server chain against the bundled roots. If this is None,
        // the handshake skips verification — which would defeat the spike.
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        ..Default::default()
    })
    .context("creating the HTTPS connection (TLS init)")?;

    // GitHub rejects requests without a User-Agent.
    let headers = [
        ("User-Agent", "typoena-spike6"),
        ("Accept", "application/vnd.github+json"),
    ];
    conn.initiate_request(Method::Get, url, &headers)
        .context("TLS handshake / request send failed")?;
    conn.initiate_response()?;

    let status = conn.status();
    log::info!("HTTPS GET {url} → {status}");

    // Preview the first chunk, then drain the rest so we log the real byte count
    // (proves the encrypted stream reads back cleanly, not just the handshake).
    let mut buf = [0u8; 512];
    let first = conn.read(&mut buf)?;
    log::info!(
        "body[..{first}]: {}",
        String::from_utf8_lossy(&buf[..first]).replace('\n', " ")
    );
    let mut total = first;
    loop {
        let n = conn.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n;
    }

    let heap_after = unsafe { esp_idf_svc::sys::esp_get_free_heap_size() };
    log::info!(
        "read {total} bytes; free heap {heap_before} → {heap_after} (Δ {} B during TLS)",
        heap_before as i64 - heap_after as i64
    );

    if !(200..300).contains(&status) {
        bail!("unexpected HTTP status {status} (TLS validated, but the request was not OK)");
    }
    Ok(())
}
