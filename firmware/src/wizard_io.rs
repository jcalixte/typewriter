//! Firmware driver for the onboarding wizard (v0.9 slices 2–3).
//!
//! The wizard crate is pure logic; this module is its I/O: keys from
//! `usb_kbd`, frames to the panel, and the effects executed against real
//! hardware. Runs *before* the git thread spawns.
//!
//! Radio ownership: the Wi-Fi half of the modem is reborrowed once
//! (`split_reborrow`) and the `EspWifi` built from it is **kept up for the
//! whole wizard** — sign-in, repo listing and clone are all network steps.
//! Dropping it when `run` returns releases the modem for the git thread,
//! which re-associates on the first `:gp` (a session's second join is fast).
//!
//! HTTPS: `EspHttpConnection` over the esp-idf certificate bundle (the
//! Spike 6 stack — `CONFIG_MBEDTLS_CERTIFICATE_BUNDLE=y`), after an SNTP
//! sync (cert validity needs a sane wall clock). The GitHub device flow's
//! pure half (bodies, parsers) lives in `wizard::github`, host-tested.
//!
//! Slice status: Wi-Fi + device-flow sign-in (QR on panel, poll-to-token,
//! `GET /user` identity) are real. `FetchRepos` / `Clone` (slice 4) surface
//! as a hard stop with a pointer at the installer.

use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use embedded_svc::http::Method;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::modem::{Modem, WifiModem};
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::{EspSntp, SyncStatus};
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};

use display::{Frame, HEIGHT};
use firmware::epd::Epd;
use firmware::net::connect_wifi;
use firmware::persistence::Storage;
use wizard::github;
use wizard::{Effect, Event, Wizard};

use crate::usb_kbd;

/// SNTP first-sync budget (mirrors git_sync's): required before any TLS.
const SNTP_TIMEOUT: Duration = Duration::from_secs(20);

/// An in-flight device-flow grant the main loop polls between keystrokes.
struct PendingAuth {
    device_code: String,
    interval: Duration,
    next_poll: Instant,
    deadline: Instant,
}

/// Run the wizard to completion and return the final conf for the normal
/// boot path to install (`set_card_conf`). Blocks the boot; the editor only
/// exists after this returns. An `Err` is terminal (main `boot_halt`s with
/// it) — today that includes reaching the not-yet-built repo step.
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

    // Radio state for the whole run (see module docs).
    let (wifi_modem, _) = modem.split_reborrow();
    let mut wifi_modem = Some(wifi_modem);
    let mut wifi: Option<BlockingWifi<EspWifi<'_>>> = None;
    let mut clock_synced = false;
    let mut pending_auth: Option<PendingAuth> = None;

    loop {
        // Paint before executing: waiting screens ("Joining Wi-Fi…",
        // "contacting github.com…") must be visible while their effect blocks
        // below. First paint is a full refresh (clears the splash cleanly),
        // the rest ride the ~630 ms full-area partial like live typing does.
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
                    let ev = match join_wifi(
                        &mut wifi, &mut wifi_modem, sys_loop, nvs, &ssid, &pass,
                    ) {
                        Ok(()) => Event::WifiOk,
                        Err(e) => {
                            log::warn!("wizard: join failed: {e:#}");
                            Event::WifiFailed(format!("{e:#}"))
                        }
                    };
                    queue.extend(wiz.event(ev));
                    dirty = true;
                }
                Effect::StartAuth => {
                    pending_auth = None;
                    let ev = match start_auth(
                        &mut wifi,
                        &mut wifi_modem,
                        sys_loop,
                        nvs,
                        &mut clock_synced,
                        wiz.conf(),
                    ) {
                        Ok(dc) => {
                            log::info!(
                                "wizard: device flow started — code {} (expires in {}s)",
                                dc.user_code,
                                dc.expires_in_secs
                            );
                            let now = Instant::now();
                            let interval = Duration::from_secs(dc.interval_secs);
                            pending_auth = Some(PendingAuth {
                                device_code: dc.device_code,
                                interval,
                                next_poll: now + interval,
                                deadline: now + Duration::from_secs(dc.expires_in_secs),
                            });
                            Event::AuthCode {
                                verification_uri: dc.verification_uri,
                                user_code: dc.user_code,
                            }
                        }
                        Err(e) => {
                            log::warn!("wizard: device flow start failed: {e:#}");
                            Event::AuthFailed(format!("{e:#}"))
                        }
                    };
                    queue.extend(wiz.event(ev));
                    dirty = true;
                }
                Effect::FetchRepos | Effect::Clone { .. } => {
                    // Slice 4. Stop rather than loop.
                    bail!(
                        "You are signed in. The on-device repo setup lands in the \
                         next firmware - until then, finish this card with the \
                         installer (typoena.dev)."
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
            None => {
                // Idle: advance an in-flight sign-in at GitHub's pace.
                if let Some(pa) = pending_auth.as_mut() {
                    let now = Instant::now();
                    if now >= pa.deadline {
                        pending_auth = None;
                        queue.extend(wiz.event(Event::AuthFailed(
                            "the code expired - retry for a fresh one".into(),
                        )));
                        dirty = true;
                        continue;
                    }
                    if now >= pa.next_poll {
                        match poll_token(&pa.device_code) {
                            Ok(github::Poll::Token(token)) => {
                                pending_auth = None;
                                let ev = match fetch_identity(&token) {
                                    Ok((login, name, email)) => Event::AuthDone {
                                        token,
                                        login,
                                        name,
                                        email,
                                    },
                                    Err(e) => {
                                        log::warn!("wizard: /user failed: {e:#}");
                                        Event::AuthFailed(format!("{e:#}"))
                                    }
                                };
                                queue.extend(wiz.event(ev));
                                dirty = true;
                            }
                            Ok(github::Poll::Pending) => pa.next_poll = now + pa.interval,
                            Ok(github::Poll::SlowDown(secs)) => {
                                pa.interval = Duration::from_secs(secs);
                                pa.next_poll = now + pa.interval;
                            }
                            Ok(github::Poll::Failed(reason)) => {
                                pending_auth = None;
                                queue.extend(wiz.event(Event::AuthFailed(reason)));
                                dirty = true;
                            }
                            Err(e) => {
                                // Transport hiccup — keep polling until the
                                // code's own deadline says otherwise.
                                log::warn!("wizard: poll failed (will retry): {e:#}");
                                pa.next_poll = now + pa.interval;
                            }
                        }
                        continue;
                    }
                }
                FreeRtos::delay_ms(10);
            }
        }
    }
}

/// Build the radio if needed, then (re)associate with the given credentials.
/// The `EspWifi` persists across calls; only the association changes.
fn join_wifi<'d>(
    wifi: &mut Option<BlockingWifi<EspWifi<'d>>>,
    wifi_modem: &mut Option<WifiModem<'d>>,
    sys_loop: &EspSystemEventLoop,
    nvs: &EspDefaultNvsPartition,
    ssid: &str,
    pass: &str,
) -> Result<()> {
    if wifi.is_none() {
        let m = wifi_modem
            .take()
            .context("radio unavailable (a previous Wi-Fi init failed)")?;
        *wifi = Some(BlockingWifi::wrap(
            EspWifi::new(m, sys_loop.clone(), Some(nvs.clone()))?,
            sys_loop.clone(),
        )?);
    }
    let w = wifi.as_mut().expect("just built");
    if w.is_connected().unwrap_or(false) {
        // Re-testing (new creds after a failure, or :setup later): start clean.
        let _ = w.disconnect();
    }
    connect_wifi(w, ssid, pass)?;
    let ip = w.wifi().sta_netif().get_ip_info()?;
    log::info!("wizard: joined {ssid}, ip {}", ip.ip);
    Ok(())
}

/// Network preamble + `POST login/device/code`. The Wi-Fi step normally ran
/// just before, but a resume can land here directly — ensure the radio with
/// the conf's credentials, sync the clock once, then start the flow.
fn start_auth<'d>(
    wifi: &mut Option<BlockingWifi<EspWifi<'d>>>,
    wifi_modem: &mut Option<WifiModem<'d>>,
    sys_loop: &EspSystemEventLoop,
    nvs: &EspDefaultNvsPartition,
    clock_synced: &mut bool,
    conf: &conf::Conf,
) -> Result<github::DeviceCode> {
    let connected = wifi
        .as_mut()
        .map(|w| w.is_connected().unwrap_or(false))
        .unwrap_or(false);
    if !connected {
        join_wifi(
            wifi,
            wifi_modem,
            sys_loop,
            nvs,
            &conf.wifi_ssid,
            &conf.wifi_pass,
        )
        .context("joining Wi-Fi")?;
    }
    if !*clock_synced {
        sync_clock().context("syncing the clock (TLS needs it)")?;
        *clock_synced = true;
    }
    let body = post_form(github::DEVICE_CODE_URL, &github::device_code_body())
        .context("asking GitHub for a device code")?;
    github::parse_device_code(&body).map_err(|e| anyhow!(e))
}

/// One `POST login/oauth/access_token` poll.
fn poll_token(device_code: &str) -> Result<github::Poll> {
    let body = post_form(github::TOKEN_URL, &github::poll_body(device_code))?;
    Ok(github::parse_poll(&body))
}

/// `GET /user` → (login, name, email) for the commit identity.
fn fetch_identity(token: &str) -> Result<(String, String, String)> {
    let mut conn = https_conn()?;
    let auth = format!("Bearer {token}");
    let headers = [
        ("User-Agent", "typoena-device"),
        ("Accept", "application/vnd.github+json"),
        ("X-GitHub-Api-Version", "2022-11-28"),
        ("Authorization", auth.as_str()),
    ];
    conn.initiate_request(Method::Get, github::USER_URL, &headers)
        .context("GET /user")?;
    conn.initiate_response()?;
    let status = conn.status();
    let body = read_body(&mut conn)?;
    if !(200..300).contains(&status) {
        bail!("GitHub /user answered {status}");
    }
    github::parse_user(&body).map_err(|e| anyhow!(e))
}

/// One TLS connection against the esp-idf certificate bundle (Spike 6).
fn https_conn() -> Result<EspHttpConnection> {
    EspHttpConnection::new(&HttpConfig {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        ..Default::default()
    })
    .context("creating the HTTPS connection (TLS init)")
}

/// Form-encoded POST; returns the body regardless of status — GitHub's OAuth
/// endpoints put errors in parseable `error=` fields, sometimes on a 4xx.
fn post_form(url: &str, body: &str) -> Result<String> {
    let mut conn = https_conn()?;
    let len = body.len().to_string();
    let headers = [
        ("User-Agent", "typoena-device"),
        ("Content-Type", "application/x-www-form-urlencoded"),
        ("Content-Length", len.as_str()),
    ];
    conn.initiate_request(Method::Post, url, &headers)
        .with_context(|| format!("POST {url}"))?;
    conn.write_all(body.as_bytes()).context("sending the form")?;
    conn.initiate_response()?;
    let status = conn.status();
    let reply = read_body(&mut conn)?;
    log::info!("wizard: POST {url} -> {status} ({} B)", reply.len());
    Ok(reply)
}

/// Drain a response into a String (these replies are a few hundred bytes;
/// the cap is a guard against reading something unexpected forever).
fn read_body(conn: &mut EspHttpConnection) -> Result<String> {
    let mut out = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = conn.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        if out.len() > 16 * 1024 {
            bail!("reply unexpectedly large (> 16 KB)");
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// SNTP once before the first TLS (mirrors git_sync's `sync_clock`).
fn sync_clock() -> Result<()> {
    let sntp = EspSntp::new_default()?;
    log::info!("wizard: SNTP started, waiting for first sync…");
    let start = Instant::now();
    while sntp.get_sync_status() != SyncStatus::Completed {
        if start.elapsed() >= SNTP_TIMEOUT {
            bail!("SNTP did not sync within {SNTP_TIMEOUT:?}");
        }
        FreeRtos::delay_ms(100);
    }
    Ok(())
}
