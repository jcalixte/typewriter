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
//!    editor has already saved the user's buffers before `:sync` signals us,
//!    so we just commit + push what's on disk.
//! 4. **The commit is an O(depth) TreeBuilder splice, not an index pass.**
//!    The request carries the repo-relative paths saved/deleted since the last
//!    confirmed publish (`Storage`'s journaled dirty set); `stage_and_commit`
//!    patches exactly those onto HEAD's tree. The index pipeline it replaced
//!    (`add_all` → `index.write` → `write_tree`) is O(N_tree) and measured up
//!    to 611 s on the real 1179-file / 570 MB-pack clone — see
//!    docs/tradeoff-curves/sync-commit-staging.md for the whole trail.
//!
//! Runs on a dedicated 96 KB thread (libgit2's init→push chain nests ~67 KB of
//! `GIT_PATH_MAX` stack buffers — see git_push.rs / postmortem #3). Config is
//! baked at build time (`TW_*`, ADR-007: v0.1 device config is compiled in).

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
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
    CertificateCheckStatus, Commit, Cred, CredentialType, FetchOptions, ObjectType, Oid,
    PushOptions, RemoteCallbacks, Repository, Signature, Tree,
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

/// A request to publish. The UI task has already saved every dirty buffer to
/// the card before sending this; `paths` is `Storage::take_dirty`'s snapshot —
/// the repo-relative paths saved or `:delete`d since the last confirmed
/// publish. The working tree stays the source of truth: at commit time a path
/// that exists on the card is spliced into the tree from disk, a missing one
/// is spliced out. An unchanged path is a no-op, so over-reporting is safe.
pub struct PublishRequest {
    pub paths: BTreeSet<String>,
}

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
    // Process-global libgit2 tuning, once, before any repo work. The 32-bit
    // defaults (32 MB window / 256 MB mapped budget, mwindow.c) would
    // git__malloc past PSRAM on the first pack access of the real 570 MB-pack
    // clone; these are the bench-proven values (git_bench), and ~1.9 MB of
    // windows stays live during git ops — the p_mmap emulation (esp_map.c)
    // relies on this mapped limit being real.
    // SAFETY: set on the git thread before any Repository is opened.
    unsafe {
        if let Err(e) = git2::opts::set_mwindow_size(256 * 1024) {
            log::error!("set_mwindow_size failed ({e}); first pack access may OOM");
        }
        if let Err(e) = git2::opts::set_mwindow_mapped_limit(4 * 1024 * 1024) {
            log::error!("set_mwindow_mapped_limit failed ({e}); first pack access may OOM");
        }
    }

    // Lazily initialised on the first request, then reused across publishes.
    let mut wifi: Option<BlockingWifi<EspWifi<'static>>> = None;
    let mut modem = Some(modem);
    let mut nvs = Some(nvs);
    let mut clock_synced = false;
    let mut tls_ready = false;

    while let Ok(req) = rx.recv() {
        let outcome = publish_cycle(
            &sys_loop,
            &mut wifi,
            &mut modem,
            &mut nvs,
            &mut clock_synced,
            &mut tls_ready,
            &req.paths,
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
    paths: &BTreeSet<String>,
) -> Result<PublishOutcome> {
    if REMOTE_URL.is_empty() || GH_USER.is_empty() || PAT.is_empty() || WIFI_SSID.is_empty() {
        bail!("git config missing — set TW_WIFI_SSID / TW_REMOTE_URL / TW_GH_USER / TW_PAT in firmware/.env and rebuild");
    }

    // Nothing recorded dirty and origin's tracking ref already has HEAD: this
    // `:sync` has nothing to do — say so without touching the radio (~150 ms
    // instead of a Wi-Fi + TLS round). A stranded local commit (committed but
    // never pushed, e.g. a push that failed mid-air) makes the check false and
    // takes the full path below, where publish_once pushes it.
    if paths.is_empty() && remote_current().unwrap_or(false) {
        log::info!(":sync — no dirty paths and origin has HEAD; up to date, radio untouched");
        return Ok(PublishOutcome::UpToDate);
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
    let outcome = publish_once(paths)?;
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
fn publish_once(paths: &BTreeSet<String>) -> Result<PublishOutcome> {
    log::info!(
        "publish started — {} dirty path(s), free heap {} ({} internal)",
        paths.len(),
        free_heap(),
        internal_free_heap()
    );
    let repo = Repository::open(REPO_DIR).with_context(|| {
        format!("opening git repo at {REPO_DIR} — provision the card with a clone (just init) whose origin is your remote")
    })?;

    let branch = repo
        .head()?
        .shorthand()
        .context("HEAD has no branch shorthand")?
        .to_string();
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

    let mut oid = match stage_and_commit(&repo, paths)? {
        Some(oid) => oid,
        None => {
            // Nothing new to commit. Usually genuinely up to date — but a
            // previous cycle may have committed and then failed to push,
            // stranding a local-only commit (the old add_all path silently
            // never retried those). Push whenever origin's tracking ref
            // doesn't already have HEAD.
            let head = repo.head()?.peel_to_commit()?.id();
            if tracking_tip(&repo, &branch) == Some(head) {
                return Ok(PublishOutcome::UpToDate);
            }
            log::info!(
                "tree unchanged but origin/{branch} lacks HEAD {} — pushing the stranded commit",
                short(head)
            );
            head
        }
    };

    // Optimistic push. A non-fast-forward *rejection* means the remote moved
    // under us: reconcile onto origin and replay the dirty paths on the new
    // tip, then retry once (reconcile_onto_origin soft-resets — ref move only —
    // so the notes stay on the card and stage_and_commit splices them on top of
    // origin). A transport-level failure is surfaced as-is: its fetch would die
    // the same way, and the commit is safe locally — the stranded-commit check
    // above pushes it once the transport works again.
    if let Err(failure) = try_push(&repo, &refspec) {
        let rejection = match failure {
            PushFailure::Rejected(msg) => msg,
            PushFailure::Other(e) => return Err(e),
        };
        log::warn!("push rejected ({rejection}); reconciling onto origin and replaying the note");
        reconcile_onto_origin(&repo, &branch).context("reconciling after a rejected push")?;
        match stage_and_commit(&repo, paths)? {
            Some(replayed) => {
                oid = replayed;
                try_push(&repo, &refspec)
                    .map_err(PushFailure::into_error)
                    .context("push after reconcile")?;
            }
            // The note was already on origin (nothing to replay) — treat as done.
            None => {
                log::info!("nothing to replay after reconcile — already up to date");
                return Ok(PublishOutcome::UpToDate);
            }
        }
    }

    log::info!(
        "push done — free heap {} ({} internal), min-ever {}",
        free_heap(),
        internal_free_heap(),
        min_free_heap()
    );
    Ok(PublishOutcome::Pushed(short(oid)))
}

/// Build the commit for `paths` as an O(depth) TreeBuilder splice onto HEAD's
/// tree and return the new commit id — or `None` when the result matches the
/// parent (nothing to publish). Called on the first attempt and again to
/// replay the dirty paths after a reconcile.
///
/// This replaces the index pipeline (`add_all` → `index.write` → `write_tree`),
/// which is O(N_tree) and cannot run on the real 1179-file / 570 MB-pack clone:
/// `index.write`'s racy-clean pass re-hashes ~every entry on FAT's 2 s mtimes
/// (measured up to **611 s**), and even the index-free `read_tree` walk was
/// 77 s. The splice reads and writes only the dirty paths' ancestor chains —
/// O(depth × dirty), flat in repo size, **~2–2.8 s measured on the real
/// clone** — and carries every untouched entry (including the ~150 MB of
/// images) forward by OID without ever opening it. Trail + bench numbers:
/// docs/tradeoff-curves/sync-commit-staging.md.
///
/// The working tree is the source of truth: a recorded path that exists on the
/// card is spliced in from disk, a missing one is spliced out (a `:delete`).
/// Unrecorded paths are never visited — so Finder cruft (`._*`, `.DS_Store`)
/// on the FAT card can no longer ride into a commit the way it once did with
/// `add_all` (07d87772), and the old cruft filter is gone with the walk.
fn stage_and_commit(repo: &Repository, paths: &BTreeSet<String>) -> Result<Option<Oid>> {
    // Commit on top of the current branch tip (None on an empty/unborn remote,
    // where the splice starts from an empty base and makes a parentless commit).
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let base = match &parent {
        Some(c) => Some(c.tree().context("loading HEAD tree")?),
        None => None,
    };

    let t_splice = Instant::now();
    let mut tree = base;
    for path in paths {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            continue;
        }
        let blob = match fs::read(format!("{REPO_DIR}/{path}")) {
            Ok(bytes) => Some(
                repo.blob(&bytes)
                    .with_context(|| format!("writing blob for {path}"))?,
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None, // deleted → splice out
            Err(e) => return Err(e).with_context(|| format!("reading {path}")),
        };
        let spliced = splice(repo, tree.as_ref(), &parts, blob)
            .with_context(|| format!("splicing {path}"))?;
        tree = Some(repo.find_tree(spliced).context("loading spliced tree")?);
    }
    let Some(tree) = tree else {
        return Ok(None); // unborn branch and nothing dirty — nothing to commit
    };
    let splice_ms = t_splice.elapsed().as_millis();

    if let Some(p) = &parent {
        if p.tree_id() == tree.id() {
            log::info!("nothing to publish — tree unchanged @ {}", short(p.id()));
            return Ok(None);
        }
    }

    let sig = Signature::now(AUTHOR_NAME, AUTHOR_EMAIL).context("building signature")?;
    let message = format!("Typoena publish — unix {}", now_unix());
    let parents: Vec<&Commit> = parent.iter().collect();
    let t_commit = Instant::now();
    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, &message, &tree, &parents)
        .context("creating commit")?;
    log::info!(
        "commit split — splice {splice_ms}ms ({} path(s)), commit-obj {}ms; committed {} — free heap {} ({} internal)",
        paths.len(),
        t_commit.elapsed().as_millis(),
        short(oid),
        free_heap(),
        internal_free_heap()
    );
    Ok(Some(oid))
}

/// Return a new tree = `base` with `path` set to `blob` (`Some` inserts or
/// replaces, `None` removes). Recurses down the path's subtree chain: reads
/// ~depth tree objects and writes ~depth new ones, leaving every sibling entry
/// untouched (carried by OID — never opened). A missing intermediate directory
/// is synthesized on the way down; a directory emptied by a remove is pruned
/// on the way up rather than left behind as an empty tree entry.
fn splice(repo: &Repository, base: Option<&Tree>, path: &[&str], blob: Option<Oid>) -> Result<Oid> {
    let (head, rest) = path.split_first().context("splice: empty path")?;
    let mut tb = repo.treebuilder(base).context("treebuilder")?;
    if rest.is_empty() {
        match blob {
            Some(oid) => {
                tb.insert(*head, oid, 0o100644)
                    .context("inserting blob entry")?;
            }
            // Removing a never-committed path is a no-op, not an error (a note
            // created and deleted between two syncs).
            None => {
                let _ = tb.remove(*head);
            }
        }
    } else {
        let sub = match base.and_then(|b| b.get_name(head)) {
            Some(e) if e.kind() == Some(ObjectType::Tree) => {
                Some(repo.find_tree(e.id()).context("loading subtree")?)
            }
            // Absent (a new directory) or a non-tree shadowing the name —
            // build the subtree from scratch either way.
            _ => None,
        };
        let new_sub = splice(repo, sub.as_ref(), rest, blob)?;
        if repo.find_tree(new_sub)?.len() == 0 {
            let _ = tb.remove(*head); // the remove emptied this directory — prune it
        } else {
            tb.insert(*head, new_sub, 0o040000)
                .context("inserting subtree entry")?;
        }
    }
    tb.write().context("writing spliced tree")
}

/// Origin's remote-tracking tip for `branch`, if the ref exists. libgit2
/// updates it after a successful push/fetch, so it is "the newest commit we
/// know origin has" — without touching the network.
fn tracking_tip(repo: &Repository, branch: &str) -> Option<Oid> {
    repo.find_reference(&format!("refs/remotes/origin/{branch}"))
        .ok()?
        .peel_to_commit()
        .ok()
        .map(|c| c.id())
}

/// Whether origin is known to already have HEAD (local refs only, no network).
/// Errors read as "not current", so the caller falls through to the full
/// publish path where the real failure surfaces with context.
fn remote_current() -> Result<bool> {
    let repo = Repository::open(REPO_DIR)?;
    let head = repo.head()?.peel_to_commit()?.id();
    let branch = repo
        .head()?
        .shorthand()
        .context("HEAD has no branch shorthand")?
        .to_string();
    Ok(tracking_tip(&repo, &branch) == Some(head))
}

/// How a push attempt failed — this decides whether reconciling can help.
enum PushFailure {
    /// The server processed the push but refused the ref update (arrives via
    /// the `push_update_reference` callback — e.g. non-fast-forward): the
    /// remote moved under us, and reconcile + replay is the right response.
    Rejected(String),
    /// Transport / TLS / auth / URL — the push never reached a ref decision,
    /// so a reconcile (whose fetch needs the same transport) cannot help.
    /// Surfaced directly; the 2026-07-13 on-device run burned a doomed
    /// reconcile on an "unsupported URL protocol" because this wasn't split.
    Other(anyhow::Error),
}

impl PushFailure {
    fn into_error(self) -> anyhow::Error {
        match self {
            Self::Rejected(msg) => anyhow::anyhow!("remote rejected ref: {msg}"),
            Self::Other(e) => e,
        }
    }
}

/// One push attempt over HTTPS. Binds the PAT credential + the cert-verify
/// callback, and separates a server-side ref rejection (reconcilable) from a
/// transport-level failure (not).
fn try_push(repo: &Repository, refspec: &str) -> Result<(), PushFailure> {
    let mut remote = repo
        .find_remote("origin")
        .map_err(|e| PushFailure::Other(anyhow::Error::new(e).context("finding remote origin")))?;
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
        .map_err(|e| PushFailure::Other(anyhow::Error::new(e).context("push transport")))?;

    if let Some(msg) = rejection.borrow().clone() {
        return Err(PushFailure::Rejected(msg));
    }
    log::info!("push accepted by remote");
    Ok(())
}

/// Fetch origin and *soft*-reset the local branch onto it, so our changes can
/// be replayed on the current tip. Only runs after a non-fast-forward push
/// rejection — i.e. the remote moved under us.
///
/// **SOFT**, deliberately: it moves only the branch ref. The previous Mixed
/// reset also rewrote the index — pure waste now that the splice commit never
/// reads the index, and on the real repo an index write is exactly the
/// racy-clean wall the splice exists to avoid. Neither flavor touches the
/// working tree, so the notes being published survive on the card and the
/// replay splices them onto the new tip. For a single-writer appliance this
/// resolves last-writer-wins: a concurrent remote *edit* to a note we're
/// publishing loses to ours, while a remote-only added/changed file is simply
/// carried forward — origin's tree is now the splice base, so the replay
/// keeps it (an improvement over the old `add --all` replay, which dropped
/// files the card didn't have). A real merge stays increment-B work.
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
        "reconcile: resetting local {branch} onto origin @ {} (soft — ref move only, notes stay on the card)",
        short(theirs.id())
    );
    let their_obj = repo.find_object(theirs.id(), None)?;
    repo.reset(&their_obj, git2::ResetType::Soft, None)
        .context("soft reset onto origin")?;
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

/// Free INTERNAL RAM (DRAM), excluding PSRAM. `free_heap` is dominated by the
/// 8 MB PSRAM pool and masks internal exhaustion — which is what actually
/// killed the first real-repo push (mbedTLS's ssl_setup could not get its
/// ~33 KB while Wi-Fi + USB + editor + libgit2 were resident).
fn internal_free_heap() -> u32 {
    unsafe { sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL) as u32 }
}

fn min_free_heap() -> u32 {
    unsafe { sys::esp_get_minimum_free_heap_size() }
}
