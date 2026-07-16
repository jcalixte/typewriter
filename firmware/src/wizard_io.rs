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

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use embedded_svc::http::Method;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::modem::{Modem, WifiModem};
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::{EspSntp, SyncStatus};
use esp_idf_svc::wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi};

use display::{Frame, HEIGHT};
use firmware::epd::Epd;
use firmware::net::connect_wifi;
use firmware::persistence::Storage;
use wizard::github;
use wizard::{Effect, Event, RepoChoice, Wizard};

use crate::usb_kbd;

/// SNTP first-sync budget (mirrors git_sync's): required before any TLS.
const SNTP_TIMEOUT: Duration = Duration::from_secs(20);

/// Progress from the background clone thread (see `Effect::Clone`), drained in
/// the main loop's idle branch and turned into wizard `Event`s.
enum CloneMsg {
    Progress(String),
    Done,
    Failed(String),
}

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
    // Set while a clone thread is running; drained in the idle branch.
    let mut clone_rx: Option<Receiver<CloneMsg>> = None;

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
                Effect::ScanWifi => {
                    let ev = match scan_wifi(&mut wifi, &mut wifi_modem, sys_loop, nvs) {
                        Ok(nets) => {
                            log::info!("wizard: scan found {} network(s)", nets.len());
                            Event::WifiScan(nets)
                        }
                        Err(e) => {
                            log::warn!("wizard: scan failed: {e:#}");
                            Event::WifiScanFailed(format!("{e:#}"))
                        }
                    };
                    queue.extend(wiz.event(ev));
                    dirty = true;
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
                Effect::FetchRepos => {
                    let token = wiz.conf().token.clone();
                    let ev = match fetch_repos(&token) {
                        Ok(repos) => {
                            log::info!("wizard: {} repo(s) available", repos.len());
                            Event::Repos(repos)
                        }
                        Err(e) => {
                            log::warn!("wizard: repo list failed: {e:#}");
                            Event::ReposFailed(format!("{e:#}"))
                        }
                    };
                    queue.extend(wiz.event(ev));
                    dirty = true;
                }
                Effect::Clone { full_name } => {
                    // Shallow-clone on a dedicated GIT_STACK thread (libgit2's
                    // path nesting overflows the main task stack) using the
                    // network the wizard already brought up. Creds come from the
                    // in-progress conf — CARD_CONF isn't set until the wizard
                    // returns. Progress lands on `clone_rx`, drained below.
                    let remote_url = wiz.conf().remote_url.clone();
                    let gh_user = wiz.conf().gh_user.clone();
                    let token = wiz.conf().token.clone();
                    log::info!("wizard: cloning {full_name} from {remote_url}");
                    let (ctx, crx) = std::sync::mpsc::channel::<CloneMsg>();
                    clone_rx = Some(crx);
                    std::thread::Builder::new()
                        .name("wizclone".into())
                        .stack_size(firmware::git_sync::GIT_STACK)
                        .spawn(move || {
                            let ptx = ctx.clone();
                            let progress =
                                move |s: &str| { let _ = ptx.send(CloneMsg::Progress(s.to_string())); };
                            let msg = match firmware::git_sync::clone_repo(
                                &remote_url,
                                &gh_user,
                                &token,
                                &progress,
                            ) {
                                Ok(n) => {
                                    log::info!("wizard: clone wrote {n} file(s)");
                                    CloneMsg::Done
                                }
                                Err(e) => {
                                    log::warn!("wizard: clone failed: {e:#}");
                                    CloneMsg::Failed(format!("{e:#}"))
                                }
                            };
                            let _ = ctx.send(msg);
                        })
                        .context("spawning the clone thread")?;
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
                // Idle: drain a running clone thread's progress. Read out one
                // message with the borrow scoped tight, then act (so we can
                // clear `clone_rx` on the terminal ones).
                let cmsg = clone_rx.as_ref().and_then(|rx| match rx.try_recv() {
                    Ok(m) => Some(Ok(m)),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => Some(Err(())),
                });
                if let Some(res) = cmsg {
                    match res {
                        Ok(CloneMsg::Progress(s)) => {
                            queue.extend(wiz.event(Event::CloneProgress(s)));
                        }
                        Ok(CloneMsg::Done) => {
                            clone_rx = None;
                            queue.extend(wiz.event(Event::CloneDone));
                        }
                        Ok(CloneMsg::Failed(e)) => {
                            clone_rx = None;
                            queue.extend(wiz.event(Event::CloneFailed(e)));
                        }
                        Err(()) => {
                            clone_rx = None;
                            queue.extend(wiz.event(Event::CloneFailed(
                                "clone thread ended unexpectedly".into(),
                            )));
                        }
                    }
                    dirty = true;
                    continue;
                }

                // Advance an in-flight sign-in at GitHub's pace.
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

/// Build the persistent `EspWifi` from the reborrowed modem on first use.
/// Idempotent: later calls (scan, join, re-join) reuse the same radio.
fn ensure_radio<'d>(
    wifi: &mut Option<BlockingWifi<EspWifi<'d>>>,
    wifi_modem: &mut Option<WifiModem<'d>>,
    sys_loop: &EspSystemEventLoop,
    nvs: &EspDefaultNvsPartition,
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
    Ok(())
}

/// Scan for nearby networks so the SSID can be picked, not typed. Leaves the
/// radio built-but-stopped so the join path's `start()` runs from a clean
/// state. Returns SSIDs deduped and strongest-first, hidden (blank) ones
/// dropped.
fn scan_wifi<'d>(
    wifi: &mut Option<BlockingWifi<EspWifi<'d>>>,
    wifi_modem: &mut Option<WifiModem<'d>>,
    sys_loop: &EspSystemEventLoop,
    nvs: &EspDefaultNvsPartition,
) -> Result<Vec<String>> {
    ensure_radio(wifi, wifi_modem, sys_loop, nvs)?;
    let w = wifi.as_mut().expect("radio built");
    // Scanning needs the radio started in station mode; a default client
    // config is enough — we read beacons, never associate here.
    w.set_configuration(&Configuration::Client(ClientConfiguration::default()))?;
    w.start()?;
    let aps = w.scan().context("Wi-Fi scan failed")?;
    let _ = w.stop();

    // Dedup by SSID (mesh APs repeat the name), keep the strongest signal,
    // drop hidden networks (blank SSID), then sort strongest-first.
    let mut best: Vec<(String, i8)> = Vec::new();
    for ap in aps {
        let ssid = ap.ssid.as_str().to_string();
        if ssid.is_empty() {
            continue;
        }
        match best.iter_mut().find(|(s, _)| *s == ssid) {
            Some((_, rssi)) => *rssi = (*rssi).max(ap.signal_strength),
            None => best.push((ssid, ap.signal_strength)),
        }
    }
    best.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(best.into_iter().map(|(s, _)| s).collect())
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
    ensure_radio(wifi, wifi_modem, sys_loop, nvs)?;
    let w = wifi.as_mut().expect("radio built");
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

/// One authenticated GitHub API GET; returns the body, erroring on non-2xx.
/// The `api.github.com` replies (a repo list especially) dwarf the OAuth
/// bodies, hence the generous cap — the buffer lands on PSRAM.
fn github_get(token: &str, url: &str) -> Result<String> {
    let mut conn = https_conn()?;
    let auth = format!("Bearer {token}");
    let headers = [
        ("User-Agent", "typoena-device"),
        ("Accept", "application/vnd.github+json"),
        ("X-GitHub-Api-Version", "2022-11-28"),
        ("Authorization", auth.as_str()),
    ];
    conn.initiate_request(Method::Get, url, &headers)
        .with_context(|| format!("GET {url}"))?;
    conn.initiate_response()?;
    let status = conn.status();
    let body = read_body(&mut conn, 2 * 1024 * 1024)?;
    if !(200..300).contains(&status) {
        bail!("GitHub {url} answered {status}");
    }
    Ok(body)
}

/// `GET /user` → (login, name, email) for the commit identity.
fn fetch_identity(token: &str) -> Result<(String, String, String)> {
    let body = github_get(token, github::USER_URL).context("GET /user")?;
    github::parse_user(&body).map_err(|e| anyhow!(e))
}

/// The repos the app installation can reach, with sizes for the wizard's gate:
/// list the user's installations, then each one's repositories. Deduped across
/// installations, sorted by name. Empty list → the app isn't installed
/// anywhere yet (signing in proves identity, not repo access), so point at the
/// install page rather than showing a blank list.
fn fetch_repos(token: &str) -> Result<Vec<RepoChoice>> {
    let body = github_get(token, github::INSTALLATIONS_URL).context("listing installations")?;
    let ids = github::parse_installation_ids(&body).map_err(|e| anyhow!(e))?;
    if ids.is_empty() {
        bail!(
            "no GitHub App installation - install Typoena on the repo you want first: {}",
            github::APP_INSTALL_URL
        );
    }
    let mut out: Vec<RepoChoice> = Vec::new();
    for id in ids {
        let body = github_get(token, &github::installation_repos_url(id))
            .with_context(|| format!("listing repos for installation {id}"))?;
        for (full_name, size_kb) in github::parse_repos(&body).map_err(|e| anyhow!(e))? {
            if !out.iter().any(|r| r.full_name == full_name) {
                out.push(RepoChoice { full_name, size_kb });
            }
        }
    }
    out.sort_by(|a, b| a.full_name.to_lowercase().cmp(&b.full_name.to_lowercase()));
    Ok(out)
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
    let reply = read_body(&mut conn, 16 * 1024)?;
    log::info!("wizard: POST {url} -> {status} ({} B)", reply.len());
    Ok(reply)
}

/// Drain a response into a String; `max` guards against reading something
/// unexpected forever (OAuth replies are a few hundred bytes, an API repo
/// list can be a few hundred KB).
fn read_body(conn: &mut EspHttpConnection, max: usize) -> Result<String> {
    let mut out = Vec::new();
    let mut buf = [0u8; 2048];
    loop {
        let n = conn.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        if out.len() > max {
            bail!("reply unexpectedly large (> {} KB)", max / 1024);
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
