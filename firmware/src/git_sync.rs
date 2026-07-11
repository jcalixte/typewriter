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
use std::path::Path;
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

    // Phases are timed so a cold :sync reports where the seconds go. Wi-Fi, clock
    // and TLS run only on the first sync of a session; a warm sync skips them, so
    // they read 0 ms and the total collapses to just publish(fetch+commit+push).
    let t_total = Instant::now();

    // Bring Wi-Fi up once (on-demand: the radio stays off until the first :sync).
    let mut wifi_ms = 0u128;
    if wifi.is_none() {
        let t = Instant::now();
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
        wifi_ms = t.elapsed().as_millis();
    }
    let mut clock_ms = 0u128;
    if !*clock_synced {
        let t = Instant::now();
        sync_clock()?;
        *clock_synced = true;
        clock_ms = t.elapsed().as_millis();
    }
    let mut tls_ms = 0u128;
    if !*tls_ready {
        let t = Instant::now();
        install_tls_trust_store()?;
        *tls_ready = true;
        tls_ms = t.elapsed().as_millis();
    }

    let t_publish = Instant::now();
    let outcome = publish_once()?;
    log::info!(
        ":sync timing — wifi {wifi_ms}ms, clock {clock_ms}ms, tls {tls_ms}ms, publish(commit+push) {}ms, total {}ms",
        t_publish.elapsed().as_millis(),
        t_total.elapsed().as_millis(),
    );
    Ok(outcome)
}

/// Open `/sd/repo`, commit the working tree on the current branch, and push.
///
/// Optimistic: it pushes onto the current tip *without* a pre-fetch, so the
/// common case (nothing else touched the remote) costs a single TLS handshake.
/// If the remote has moved under us — a foreign push, e.g. maintenance — the push
/// is rejected non-fast-forward; we then reconcile onto origin, replay our note on
/// the new tip, and retry once.
///
/// Never clones or wipes: a `/sd/repo` that isn't a valid repo is a provisioning
/// error, surfaced as such.
fn publish_once() -> Result<PublishOutcome> {
    log::info!("publish started — free heap {}", free_heap());
    let repo = Repository::open(REPO_DIR).with_context(|| {
        format!("opening git repo at {REPO_DIR} — provision the card with a clone (just init) whose origin is your remote")
    })?;

    let Some(mut oid) = stage_and_commit(&repo)? else {
        return Ok(PublishOutcome::UpToDate);
    };
    let branch = repo
        .head()?
        .shorthand()
        .context("HEAD has no branch shorthand")?
        .to_string();
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

    // Optimistic push. A non-fast-forward rejection means the remote moved under
    // us: reconcile onto origin and replay the note on the new tip, then retry
    // once. reconcile_onto_origin mixed-resets, so the just-saved note survives in
    // the working tree and stage_and_commit lands it on top of origin.
    if let Err(first) = try_push(&repo, &refspec) {
        log::warn!("push rejected ({first}); reconciling onto origin and replaying the note");
        reconcile_onto_origin(&repo, &branch).context("reconciling after a rejected push")?;
        match stage_and_commit(&repo)? {
            Some(replayed) => {
                oid = replayed;
                try_push(&repo, &refspec).context("push after reconcile")?;
            }
            // The note was already on origin (nothing to replay) — treat as done.
            None => {
                log::info!("nothing to replay after reconcile — already up to date");
                return Ok(PublishOutcome::UpToDate);
            }
        }
    }

    log::info!(
        "push done — free heap {}, min-ever {}",
        free_heap(),
        min_free_heap()
    );
    Ok(PublishOutcome::Pushed(short(oid)))
}

/// Stage the working tree and commit it on top of the current branch tip.
/// Returns the new commit id, or `None` when the tree already matches the parent
/// (nothing to publish). Called on the first attempt and again to replay the note
/// after a reconcile.
///
/// Staging is `add --all` **plus** `add -u`, which together equal `git add -A`.
/// `add_all` stages new + modified files; `update_all` re-syncs already-tracked
/// entries to the working tree, which is what actually removes an index entry
/// whose file was deleted. Spike 14 found `add_all` alone did **not** stage a
/// `:delete`d file's removal on this libgit2 build (the tree came back unchanged,
/// so the publish was a silent no-op), so the `update_all` pass is load-bearing,
/// not belt-and-braces — do not drop it.
///
/// Both run a per-path filter that drops macOS AppleDouble sidecars (`._name`)
/// and `.DS_Store` that Finder/Spotlight sprinkle onto the FAT card whenever it's
/// mounted on a Mac — without it, a blind add --all sweeps them into the commit
/// (it did once: 07d87772 shipped `._.git`, `._README.md`, `._notes.md`).
/// Filtering here fixes it for *every* repo at the device level, so no per-repo
/// `.gitignore` is needed.
fn stage_and_commit(repo: &Repository) -> Result<Option<git2::Oid>> {
    let mut index = repo.index().context("opening index")?;
    let mut skip_macos_cruft = |path: &Path, _matched: &[u8]| -> i32 {
        match path.file_name().and_then(|n| n.to_str()) {
            Some(name) if name.starts_with("._") || name == ".DS_Store" => 1, // skip
            _ => 0,                                                            // add
        }
    };
    index
        .add_all(["*"], IndexAddOption::DEFAULT, Some(&mut skip_macos_cruft))
        .context("staging new/modified (add --all)")?;
    // Stage deletions: update_all removes index entries whose working-tree file is
    // gone. add_all does not do this reliably here (Spike 14), so this is required.
    index
        .update_all(["*"], Some(&mut skip_macos_cruft))
        .context("staging deletions (add -u)")?;
    index.write().context("writing index")?;
    let tree = repo.find_tree(index.write_tree().context("writing tree")?)?;

    // Commit on top of the current branch tip (None on an empty/unborn remote).
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    if let Some(p) = &parent {
        if p.tree_id() == tree.id() {
            log::info!("nothing to publish — tree unchanged @ {}", short(p.id()));
            return Ok(None);
        }
    }

    let sig = Signature::now(AUTHOR_NAME, AUTHOR_EMAIL).context("building signature")?;
    let message = format!("Typoena publish — unix {}", now_unix());
    let parents: Vec<&Commit> = parent.iter().collect();
    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, &message, &tree, &parents)
        .context("creating commit")?;
    log::info!("committed {} — free heap {}", short(oid), free_heap());
    Ok(Some(oid))
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

/// Fetch origin and mixed-reset the local branch onto it, so our just-made commit
/// can be replayed on the current tip. Only runs after a non-fast-forward push
/// rejection — i.e. the remote moved under us.
///
/// **MIXED**, deliberately not a force checkout: the note we're publishing lives
/// in the working tree, and a force checkout would clobber it. Mixed moves the
/// branch ref + index onto origin but leaves the working tree, so the note
/// survives and `stage_and_commit` replays it on top. For a single-writer
/// appliance this resolves last-writer-wins — a concurrent remote *edit* to the
/// same note loses to ours, and a remote-only *added* file the card doesn't have
/// is dropped by the replay's add --all. Both need a real merge (increment B) and
/// don't arise from this device's own use.
fn reconcile_onto_origin(repo: &Repository, branch: &str) -> Result<()> {
    let mut remote = repo.find_remote("origin")?;
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(auth_callbacks());
    remote
        .fetch(&[branch], Some(&mut fo), None)
        .context("fetch origin")?;

    let fetch_head = repo
        .find_reference("FETCH_HEAD")
        .context("no FETCH_HEAD after fetch")?;
    let theirs = repo.reference_to_annotated_commit(&fetch_head)?;
    log::info!(
        "reconcile: resetting local {branch} onto origin @ {} (mixed, keeps the note)",
        short(theirs.id())
    );
    let their_obj = repo.find_object(theirs.id(), None)?;
    repo.reset(&their_obj, git2::ResetType::Mixed, None)
        .context("mixed reset onto origin")?;
    Ok(())
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
