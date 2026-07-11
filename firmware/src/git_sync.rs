//! On-device git publish — the transport behind the editor's `:sync`.
//!
//! Graduated from the `src/bin/git_sync.rs` spike (milestone #2A, hardware-
//! verified 2026-07-07). The spike proved `open` + fast-forward `push` over
//! mbedTLS HTTPS+PAT against a persistent clone; this module lifts that logic
//! into a service the editor drives, with three changes for the product:
//!
//! 1. **Storage is the SD card `/sd/repo`** (the same working copy the editor
//!    saves `notes.md` into via [`crate::persistence`]), not the spike's 4 MB
//!    flash-FAT `/spiflash/repo`. The real notes repo can't fit in flash, so the
//!    card is the only viable home — and there's a single source of truth: git
//!    commits the exact file the editor just wrote. The git thread reaches the
//!    card through plain `std::fs`; FatFS's per-volume reentrancy lock serialises
//!    it against the UI task's saves (see [`crate::persistence::Storage`]).
//! 2. **`open` only — never clone-and-wipe.** The spike re-cloned into a
//!    throwaway flash dir; doing that to the user's card would delete their
//!    notes. A `/sd/repo` that isn't a valid repo is a provisioning error
//!    (`just init`), surfaced as such, not papered over.
//! 3. **No synthetic content.** The spike appended a marker line; here the
//!    editor has already saved the user's `notes.md` before `:sync` signals us,
//!    so we just stage + commit + push what's on disk.
//!
//! Runs on a dedicated 96 KB thread (libgit2's init→push chain nests ~67 KB of
//! `GIT_PATH_MAX` stack buffers — see git_push.rs / postmortem #3). Config is
//! baked at build time (`TW_*`, ADR-007: v0.1 device config is compiled in).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::{EspSntp, SyncStatus};
use esp_idf_svc::sys;
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use git2::{
    CertificateCheckStatus, Commit, Cred, CredentialType, FetchOptions, IndexAddOption,
    PushOptions, RemoteCallbacks, Repository, Signature,
};

use crate::net::connect_wifi;
use crate::persistence::REPO_DIR;

// Baked in at build time from firmware/.env (see build.rs). Empty when unset;
// checked at runtime before the first publish so a misconfigured build fails
// with a clear message rather than a cryptic git error.
const WIFI_SSID: &str = env!("TW_WIFI_SSID");
const WIFI_PASS: &str = env!("TW_WIFI_PASS");
const REMOTE_URL: &str = env!("TW_REMOTE_URL");
const GH_USER: &str = env!("TW_GH_USER");
const PAT: &str = env!("TW_PAT");
const AUTHOR_NAME: &str = env!("TW_AUTHOR_NAME");
const AUTHOR_EMAIL: &str = env!("TW_AUTHOR_EMAIL");

/// GitHub's root CAs, embedded so the push can verify the server's TLS chain.
/// Shared with the spikes. Written to the card and handed to libgit2 via
/// `GIT_OPT_SET_SSL_CERT_LOCATIONS`.
const GITHUB_ROOTS_PEM: &str = include_str!("bin/github_roots.pem");
/// CA bundle on the card root — outside `/sd/repo`, so it's never staged.
const CA_BUNDLE_PATH: &str = "/sd/ca.pem";

/// SNTP first-sync budget (same as Spike 6): required before TLS (cert validity)
/// and before committing (signature timestamp).
const SNTP_TIMEOUT: Duration = Duration::from_secs(20);

/// Stack for the dedicated git thread. The init→push chain measured ~67 KB;
/// keep the proven 96 KB (see git_push.rs / postmortem #3). Wi-Fi association
/// now also runs here, but it's shallow next to libgit2's path-buffer nesting.
pub const GIT_STACK: usize = 96 * 1024;

/// A request to publish. The note is already saved to `/sd/repo/notes.md` by the
/// UI task before this is sent, so the request carries no payload (a future
/// multi-file publish can grow one).
pub struct PublishRequest;

/// Result of a publish attempt, sent back to the UI task for the snackbar. The
/// detailed error always goes to the serial log; the panel gets a short line.
pub enum PublishOutcome {
    /// Committed and pushed. Carries the short commit id for the panel.
    Pushed(String),
    /// The working tree matched HEAD — nothing new to push.
    UpToDate,
    /// Something failed; the string is a short reason for the panel (full error
    /// is logged).
    Failed(String),
}

/// The git service loop, run on the dedicated git thread. Owns the Wi-Fi stack,
/// bringing it up lazily on the first request and keeping it up afterwards.
/// Blocks on `rx`; for each request it ensures connectivity + clock + trust
/// store, runs one publish cycle, and reports the outcome on `tx`. Returns when
/// the request channel closes (UI task gone). Errors are reported, never
/// panicked — a failed push must not take the thread (and its Wi-Fi) down.
pub fn run_git_service(
    modem: Modem<'static>,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    rx: Receiver<PublishRequest>,
    tx: Sender<PublishOutcome>,
) {
    // Lazily initialised on the first request, then reused across publishes.
    let mut wifi: Option<BlockingWifi<EspWifi<'static>>> = None;
    let mut modem = Some(modem);
    let mut nvs = Some(nvs);
    let mut clock_synced = false;
    let mut tls_ready = false;

    while rx.recv().is_ok() {
        let outcome = publish_cycle(
            &sys_loop,
            &mut wifi,
            &mut modem,
            &mut nvs,
            &mut clock_synced,
            &mut tls_ready,
        );
        let msg = match outcome {
            Ok(o) => o,
            Err(e) => {
                log::error!("❌ :sync failed: {e:?}");
                PublishOutcome::Failed(short_reason(&e))
            }
        };
        // If the UI task has gone away there's nothing to report to; exit.
        if tx.send(msg).is_err() {
            break;
        }
    }
    log::info!("git service: request channel closed — exiting");
}

/// One full publish: ensure Wi-Fi + clock + trust store (each done once), then
/// open the repo, stage, commit, and fast-forward push.
fn publish_cycle(
    sys_loop: &EspSystemEventLoop,
    wifi: &mut Option<BlockingWifi<EspWifi<'static>>>,
    modem: &mut Option<Modem<'static>>,
    nvs: &mut Option<EspDefaultNvsPartition>,
    clock_synced: &mut bool,
    tls_ready: &mut bool,
) -> Result<PublishOutcome> {
    if REMOTE_URL.is_empty() || GH_USER.is_empty() || PAT.is_empty() || WIFI_SSID.is_empty() {
        bail!("git config missing — set TW_WIFI_SSID / TW_REMOTE_URL / TW_GH_USER / TW_PAT in firmware/.env and rebuild");
    }

    // Bring Wi-Fi up once (on-demand: the radio stays off until the first :sync).
    if wifi.is_none() {
        log::info!("first :sync — bringing Wi-Fi up; free heap {}", free_heap());
        let m = modem.take().expect("modem taken once");
        let n = nvs.take().expect("nvs taken once");
        let mut w = BlockingWifi::wrap(
            EspWifi::new(m, sys_loop.clone(), Some(n))?,
            sys_loop.clone(),
        )?;
        connect_wifi(&mut w, WIFI_SSID, WIFI_PASS).context("connecting Wi-Fi")?;
        let ip = w.wifi().sta_netif().get_ip_info()?;
        log::info!("Wi-Fi up — IP {}", ip.ip);
        *wifi = Some(w);
    }
    if !*clock_synced {
        sync_clock()?;
        *clock_synced = true;
    }
    if !*tls_ready {
        install_tls_trust_store()?;
        *tls_ready = true;
    }

    publish_once()
}

/// Open `/sd/repo`, stage the working tree, commit on top of the current branch,
/// and fast-forward push. Never clones or wipes: a `/sd/repo` that isn't a valid
/// repo is a provisioning error, surfaced as such.
fn publish_once() -> Result<PublishOutcome> {
    log::info!("publish started — free heap {}", free_heap());
    let repo = Repository::open(REPO_DIR).with_context(|| {
        format!("opening git repo at {REPO_DIR} — provision the card with a clone (just init) whose origin is your remote")
    })?;

    // Stage everything (add --all also stages deletions, for a future note-delete)
    // and build the tree from what the editor saved.
    let mut index = repo.index().context("opening index")?;
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .context("staging (add --all)")?;
    index.write().context("writing index")?;
    let tree = repo.find_tree(index.write_tree().context("writing tree")?)?;

    // Commit on top of the current branch tip (None on an empty/unborn remote).
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());

    // Nothing-to-publish short-circuit: staged tree identical to the parent's.
    if let Some(p) = &parent {
        if p.tree_id() == tree.id() {
            log::info!("nothing to publish — tree unchanged @ {}", short(p.id()));
            return Ok(PublishOutcome::UpToDate);
        }
    }

    let sig = Signature::now(AUTHOR_NAME, AUTHOR_EMAIL).context("building signature")?;
    let message = format!("Typoena publish — unix {}", now_unix());
    let parents: Vec<&Commit> = parent.iter().collect();
    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, &message, &tree, &parents)
        .context("creating commit")?;
    let branch = repo
        .head()?
        .shorthand()
        .context("HEAD has no branch shorthand")?
        .to_string();
    log::info!("committed {} to {branch} — free heap {}", short(oid), free_heap());

    push_with_retry(&repo, &branch)?;

    log::info!(
        "push done — free heap {}, min-ever {}",
        free_heap(),
        min_free_heap()
    );
    Ok(PublishOutcome::Pushed(short(oid)))
}

/// Push `branch` to origin; on a rejected push, fetch origin and reconcile, then
/// retry once. A true divergence (two writers) needs a merge commit and is
/// deferred to increment B — `fetch_and_integrate` bails there. A single-writer
/// appliance always fast-forwards, so the happy path never hits it.
fn push_with_retry(repo: &Repository, branch: &str) -> Result<()> {
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    match try_push(repo, &refspec) {
        Ok(()) => Ok(()),
        Err(first) => {
            log::warn!("push rejected ({first}); fetching origin to reconcile");
            fetch_and_integrate(repo, branch).context("reconciling after a rejected push")?;
            log::info!("reconciled with origin; retrying push");
            try_push(repo, &refspec).context("push after reconcile")
        }
    }
}

/// One push attempt over HTTPS. Binds the PAT credential + the cert-verify
/// callback, and surfaces a server-side ref rejection (e.g. non-fast-forward) as
/// an error (it arrives via `push_update_reference`, not as a `push()` error).
fn try_push(repo: &Repository, refspec: &str) -> Result<()> {
    let mut remote = repo.find_remote("origin")?;
    let rejection: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    let mut cbs = auth_callbacks();
    {
        let rejection = rejection.clone();
        cbs.push_update_reference(move |refname, status| {
            if let Some(msg) = status {
                *rejection.borrow_mut() = Some(format!("{refname}: {msg}"));
            }
            Ok(())
        });
    }

    let mut opts = PushOptions::new();
    opts.remote_callbacks(cbs);
    remote
        .push(&[refspec], Some(&mut opts))
        .context("push transport")?;

    if let Some(msg) = rejection.borrow().clone() {
        bail!("remote rejected ref: {msg}");
    }
    log::info!("push accepted by remote");
    Ok(())
}

/// Fetch origin and integrate `branch` into the local branch. Handles up-to-date
/// and fast-forward (the common single-writer cases). A real divergence needs a
/// merge commit written to FATFS — deferred to increment B — so it bails.
fn fetch_and_integrate(repo: &Repository, branch: &str) -> Result<()> {
    let mut remote = repo.find_remote("origin")?;
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(auth_callbacks());
    remote
        .fetch(&[branch], Some(&mut fo), None)
        .context("fetch origin")?;

    let fetch_head = repo.find_reference("FETCH_HEAD")?;
    let theirs = repo.reference_to_annotated_commit(&fetch_head)?;
    let (analysis, _) = repo.merge_analysis(&[&theirs])?;

    if analysis.is_up_to_date() {
        log::info!("already up to date with origin/{branch}");
        return Ok(());
    }
    if analysis.is_fast_forward() {
        log::info!("fast-forwarding local {branch} to origin");
        let refname = format!("refs/heads/{branch}");
        repo.find_reference(&refname)?
            .set_target(theirs.id(), "fast-forward to origin")?;
        repo.set_head(&refname)?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        return Ok(());
    }
    bail!("origin/{branch} diverged from local — a real merge commit is needed (increment B, deferred)")
}

/// Auth + cert callbacks shared by fetch and push. Captures only the baked
/// consts, so a fresh set can be built per operation. The PAT is handed to
/// libgit2 here and never logged.
fn auth_callbacks<'a>() -> RemoteCallbacks<'a> {
    let mut cbs = RemoteCallbacks::new();
    cbs.credentials(|_url, _user_from_url, allowed| {
        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
            return Cred::userpass_plaintext(GH_USER, PAT);
        }
        Err(git2::Error::from_str(
            "server did not offer USER_PASS_PLAINTEXT — cannot authenticate with a PAT",
        ))
    });
    cbs.certificate_check(|_cert, host| {
        log::info!("verifying {host} TLS chain against embedded GitHub CA bundle");
        Ok(CertificateCheckStatus::CertificatePassthrough)
    });
    cbs
}

/// Kick off SNTP and block until first sync. Required before TLS (cert validity)
/// and before committing (signature timestamp). Mirrors Spike 6 / the spike.
fn sync_clock() -> Result<()> {
    let sntp = EspSntp::new_default()?;
    log::info!("SNTP started, waiting for first sync…");
    let start = Instant::now();
    while sntp.get_sync_status() != SyncStatus::Completed {
        if start.elapsed() >= SNTP_TIMEOUT {
            bail!("SNTP did not sync within {SNTP_TIMEOUT:?} — TLS + commit time would be wrong");
        }
        FreeRtos::delay_ms(100);
    }
    let unix = now_unix();
    if unix < 1_700_000_000 {
        bail!("clock still at {unix} after SNTP — refusing TLS/commit with a bad wall clock");
    }
    log::info!("clock synced — unix {unix}");
    Ok(())
}

/// Write the embedded GitHub root CAs to the card and point libgit2's mbedTLS
/// stream at them. Must run before any TLS. Mirrors the spike, but writes to the
/// card root (`/sd/ca.pem`) instead of flash-FAT.
fn install_tls_trust_store() -> Result<()> {
    std::fs::write(CA_BUNDLE_PATH, GITHUB_ROOTS_PEM)
        .with_context(|| format!("writing CA bundle to {CA_BUNDLE_PATH}"))?;
    // SAFETY: sets a process-global libgit2 option once, before any TLS work.
    unsafe { git2::opts::set_ssl_cert_file(CA_BUNDLE_PATH) }
        .context("git2::opts::set_ssl_cert_file")?;
    log::info!(
        "TLS trust store installed — {} B of GitHub roots at {CA_BUNDLE_PATH}",
        GITHUB_ROOTS_PEM.len()
    );
    Ok(())
}

/// A short, panel-friendly reason from an error chain (first line, clamped). The
/// full chain is logged separately; the editor clamps this to the panel width.
fn short_reason(e: &anyhow::Error) -> String {
    let full = format!("{e}");
    let first = full.lines().next().unwrap_or("sync failed");
    format!("sync: {}", first.chars().take(24).collect::<String>())
}

/// First 8 hex chars of an OID, for readable logs and the panel.
fn short(oid: git2::Oid) -> String {
    oid.to_string()[..8].to_string()
}

/// Current wall-clock seconds since the Unix epoch (valid after SNTP).
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn free_heap() -> u32 {
    unsafe { sys::esp_get_free_heap_size() }
}

fn min_free_heap() -> u32 {
    unsafe { sys::esp_get_minimum_free_heap_size() }
}
