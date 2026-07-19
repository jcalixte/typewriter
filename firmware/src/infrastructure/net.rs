//! The net thread — the sole owner of the Wi-Fi modem, and the transport behind
//! everything the editor does over the wire: git publish (`:gp`) and pull
//! (`:gl`), plus firmware update (`:update`, whose logic lives in the sibling
//! [`crate::infrastructure::ota`] module — this file only dispatches to it).
//! One thread because there is one radio the editor loop can never reclaim, so
//! all three multiplex over a single [`NetRequest`]/[`NetOutcome`] channel and
//! back the app's [`app::NetService`] port. Most of what follows is the git
//! machinery; the OTA hop is [`NetRequest::Update`] → [`update_cycle`].
//!
//! Graduated from the `src/bin/git_sync.rs` spike (milestone #2A, hardware-
//! verified 2026-07-07). The spike proved `open` + fast-forward `push` over
//! mbedTLS HTTPS+PAT against a persistent clone; this module lifts that logic
//! into a service the editor drives, with three changes for the product:
//!
//! 1. **Storage is the SD card `/sd/repo`** (the same working copy the editor
//!    saves `notes.md` into via [`crate::infrastructure::storage_sd`]), not the spike's 4 MB
//!    flash-FAT `/spiflash/repo`. The real notes repo can't fit in flash, so the
//!    card is the only viable home — and there's a single source of truth: git
//!    commits the exact file the editor just wrote. The net thread reaches the
//!    card through plain `std::fs`; FatFS's per-volume reentrancy lock serialises
//!    it against the UI task's saves (see [`crate::infrastructure::storage_sd::Storage`]).
//! 2. **`open` only — never clone-and-wipe.** The spike re-cloned into a
//!    throwaway flash dir; doing that to the user's card would delete their
//!    notes. A `/sd/repo` that isn't a valid repo is a provisioning error
//!    (`just init`), surfaced as such, not papered over.
//! 3. **No synthetic content.** The spike appended a marker line; here the
//!    editor has already saved the user's buffers before `:gp` signals us,
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
//! `GIT_PATH_MAX` stack buffers — see git_push.rs / postmortem #3). Config
//! comes from the card's `/sd/typoena.conf` (installer- or wizard-written,
//! parsed at boot — v0.9 onboarding slice 0), falling back per field to the
//! build-time `TW_*` values (ADR-007's dev path, `firmware/.env`).

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

use crate::drivers::wifi_esp::connect_wifi;
use crate::infrastructure::storage_sd::{LOCAL_DIR, REPO_DIR};

// Baked in at build time from firmware/.env (see build.rs). Empty when unset.
// Since the runtime conf (v0.9 onboarding slice 0) these are the per-field
// FALLBACK: the card's /sd/typoena.conf overrides them, so a provisioned card
// works on a firmware built with an empty .env. A field empty in both is
// caught by the publish/pull guards with a clear message.
const BAKED_WIFI_SSID: &str = env!("TW_WIFI_SSID");
const BAKED_WIFI_PASS: &str = env!("TW_WIFI_PASS");
const BAKED_REMOTE_URL: &str = env!("TW_REMOTE_URL");
const BAKED_GH_USER: &str = env!("TW_GH_USER");
const BAKED_TOKEN: &str = env!("TW_TOKEN");
const BAKED_AUTHOR_NAME: &str = env!("TW_AUTHOR_NAME");
const BAKED_AUTHOR_EMAIL: &str = env!("TW_AUTHOR_EMAIL");

/// The card's parsed `typoena.conf`, installed once by `main` after the SD
/// mount and before the net thread spawns. `OnceLock` because the net thread
/// reads it concurrently with the UI thread from then on.
static CARD_CONF: std::sync::OnceLock<conf::Conf> = std::sync::OnceLock::new();

/// Install the card config (once; later calls are ignored).
pub fn set_card_conf(c: conf::Conf) {
    let _ = CARD_CONF.set(c);
}

/// The per-field card-over-baked merge as a value, WITHOUT installing the
/// card conf: the wizard gate needs the effective view before deciding
/// whether to run (and what to prefill), and `set_card_conf` is called once
/// afterwards with the final result. The Wi-Fi password follows the SSID's
/// source, exactly like [`wifi_pass`].
pub fn effective_conf_from(card: &conf::Conf) -> conf::Conf {
    let pick = |v: &str, baked: &'static str| {
        if v.trim().is_empty() {
            baked.to_string()
        } else {
            v.to_string()
        }
    };
    conf::Conf {
        wifi_pass: if card.wifi_ssid.trim().is_empty() {
            BAKED_WIFI_PASS.to_string()
        } else {
            card.wifi_pass.clone()
        },
        wifi_ssid: pick(&card.wifi_ssid, BAKED_WIFI_SSID),
        remote_url: pick(&card.remote_url, BAKED_REMOTE_URL),
        gh_user: pick(&card.gh_user, BAKED_GH_USER),
        token: pick(&card.token, BAKED_TOKEN),
        author_name: pick(&card.author_name, BAKED_AUTHOR_NAME),
        author_email: pick(&card.author_email, BAKED_AUTHOR_EMAIL),
    }
}

/// Card value if present and non-blank, else the baked fallback. `&'static`
/// works because `CARD_CONF` is a static — the parsed strings live forever.
fn cfg(field: conf::Field, baked: &'static str) -> &'static str {
    match CARD_CONF.get() {
        Some(c) if !c.get(field).trim().is_empty() => c.get(field),
        _ => baked,
    }
}

fn wifi_ssid() -> &'static str {
    cfg(conf::Field::WifiSsid, BAKED_WIFI_SSID)
}
/// The pass follows whichever source supplied the SSID: a card SSID with a
/// blank pass is an OPEN NETWORK, not "fall back to the baked pass" — mixing
/// the card's SSID with the .env password of a different network must never
/// happen.
fn wifi_pass() -> &'static str {
    match CARD_CONF.get() {
        Some(c) if !c.wifi_ssid.trim().is_empty() => &c.wifi_pass,
        _ => BAKED_WIFI_PASS,
    }
}
fn remote_url() -> &'static str {
    cfg(conf::Field::RemoteUrl, BAKED_REMOTE_URL)
}
fn gh_user() -> &'static str {
    cfg(conf::Field::GhUser, BAKED_GH_USER)
}
fn token() -> &'static str {
    cfg(conf::Field::Token, BAKED_TOKEN)
}
fn author_name() -> &'static str {
    cfg(conf::Field::AuthorName, BAKED_AUTHOR_NAME)
}
fn author_email() -> &'static str {
    cfg(conf::Field::AuthorEmail, BAKED_AUTHOR_EMAIL)
}

/// GitHub's root CAs, embedded so the push can verify the server's TLS chain.
/// Shared with the spikes. Written to the card and handed to libgit2 via
/// `GIT_OPT_SET_SSL_CERT_LOCATIONS`. Path is relative to this source file —
/// `../bin/` reaches `src/bin/` now that this module lives under `infrastructure/`.
const GITHUB_ROOTS_PEM: &str = include_str!("../bin/github_roots.pem");
/// CA bundle on the card root — outside `/sd/repo`, so it's never staged.
const CA_BUNDLE_PATH: &str = "/sd/ca.pem";

/// SNTP first-sync budget (same as Spike 6): required before TLS (cert validity)
/// and before committing (signature timestamp).
const SNTP_TIMEOUT: Duration = Duration::from_secs(20);

/// Stack for the dedicated net thread. The init→push chain measured ~67 KB;
/// keep the proven 96 KB (see git_push.rs / postmortem #3). Wi-Fi association
/// now also runs here, but it's shallow next to libgit2's path-buffer nesting.
pub const GIT_STACK: usize = 96 * 1024;

/// Cap on libgit2's odb object cache (default max: 256 MB — unbounded on this
/// device). Run 4 (2026-07-13): the real-repo push ground through pack-building
/// for 66 s and PSRAM hit the floor — the UI aborted on a 27 KB framebuffer
/// alloc. Every tree/commit the push's walks read lands in this cache, and
/// nothing bounded it. 1 MB still holds the whole tree set the push's two
/// full-tree walks share (mark-uninteresting over origin's tip, then the insert
/// over ours — near-identical trees), so the second walk stays off the SD card.
const ODB_CACHE_MAX_BYTES: isize = 1024 * 1024;

/// Process-global libgit2 tuning, applied once before any repo work. The 32-bit
/// defaults (32 MB window / 256 MB mapped budget, mwindow.c) would git__malloc
/// past PSRAM on the first pack access — the p_mmap emulation (esp_map.c) makes
/// every window a real PSRAM malloc, so this budget decides whether a
/// push/clone survives. 64 KB / 1.5 MB leaves ~2 MB headroom even with the
/// 5-pack card and shrinks read amplification (a window miss costs a 64 KB SPI
/// read, ~65 ms, not 256 KB). Both the service thread and the onboarding
/// wizard's one-shot clone call this on their own thread before opening a repo;
/// re-applying the same values is harmless.
///
/// SAFETY: set before any Repository is opened on the calling thread.
pub fn tune_libgit2() {
    unsafe {
        if let Err(e) = git2::opts::set_mwindow_size(64 * 1024) {
            log::error!("set_mwindow_size failed ({e}); first pack access may OOM");
        }
        if let Err(e) = git2::opts::set_mwindow_mapped_limit(1536 * 1024) {
            log::error!("set_mwindow_mapped_limit failed ({e}); first pack access may OOM");
        }
        // Odb cache cap (see ODB_CACHE_MAX_BYTES). git2 0.20 wraps only the
        // per-object-type limit, not the total, so this one is a raw call.
        let rc = libgit2_sys::git_libgit2_opts(
            libgit2_sys::GIT_OPT_SET_CACHE_MAX_SIZE as i32,
            ODB_CACHE_MAX_BYTES,
        );
        if rc < 0 {
            log::error!("set cache_max_size failed (rc {rc}); a push/clone may exhaust the heap");
        }
    }
}

/// What the UI task asks the net thread to do.
pub enum NetRequest {
    /// `:gp` — commit the dirty paths and push (the upload half).
    Publish(PublishRequest),
    /// `:gl` — fetch, then fast-forward or rebase (the download half). Carries
    /// the dirty-journal snapshot (`Storage::take_dirty`): before fetching, the
    /// thread folds those saved-but-unpublished paths into a local commit so the
    /// fetch can replant them onto origin, then the working copy matches HEAD and
    /// the apply-diff belt can't fight a device-side save. Empty for a plain
    /// fetch (nothing was dirty).
    Pull(BTreeSet<String>),
    /// `:update` — check for a newer firmware release and, if one exists, stream
    /// it into the inactive OTA slot over HTTPS. Carries nothing: the running
    /// version and the manifest URL are known to the firmware. Rides this thread
    /// because it already owns the Wi-Fi modem, not because it touches git.
    Update,
}

/// A request to publish. The UI task has already saved every dirty buffer to
/// the card before sending this; `paths` is `Storage::take_dirty`'s snapshot —
/// the repo-relative paths saved or `:delete`d since the last confirmed
/// publish. The working tree stays the source of truth: at commit time a path
/// that exists on the card is spliced into the tree from disk, a missing one
/// is spliced out. An unchanged path is a no-op, so over-reporting is safe.
pub struct PublishRequest {
    pub paths: BTreeSet<String>,
}

/// What the net thread reports back, tagged by the request kind so the UI can
/// settle the dirty snapshot for a publish and refresh buffers for a pull.
pub enum NetOutcome {
    Publish(PublishOutcome),
    Pull(PullOutcome),
    Update(UpdateOutcome),
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

/// Result of a `:gl` pull attempt. The device never does a content merge; a
/// clean fast-forward applies origin directly, and a divergence is resolved by
/// rebasing our local commit(s) onto origin (last-writer-wins per note) rather
/// than left for a computer.
pub enum PullOutcome {
    /// Fast-forwarded onto origin's tip. Carries the short commit id; the UI
    /// must treat every tracked file as possibly rewritten (reload buffers,
    /// re-walk the palette list).
    Pulled(String),
    /// Histories diverged: origin's changes were integrated and our local
    /// commit(s) replanted on top (a rebase, not a merge). The working copy
    /// moved — same UI refresh as `Pulled` — and the device is now `LocalAhead`,
    /// so the user finishes with `:gp`. Carries the rebased commit's short id.
    Rebased(String),
    /// Origin's tip is our HEAD — nothing to pull.
    UpToDate,
    /// We are strictly ahead of origin (e.g. a stranded commit whose push
    /// failed) — nothing to pull; the next `:gp` publishes it.
    LocalAhead,
    /// Something failed; short reason for the panel (full error is logged).
    Failed(String),
}

/// Result of a `:update` firmware check, sent back to the UI task for the
/// snackbar. Not a git operation — it shares this channel only because the OTA
/// download runs on the same Wi-Fi-owning thread.
pub enum UpdateOutcome {
    /// A newer image was written to the inactive OTA slot, which is now the boot
    /// target. Carries the new version string; the UI reboots into it.
    Installed(String),
    /// The running firmware is already the newest release — nothing to install.
    /// Carries the running version, shown in the notice.
    UpToDate(String),
    /// Something failed (a missing newer release is *not* a failure — that is
    /// `UpToDate`); short reason for the panel, full error logged. The running
    /// slot is untouched, so the device keeps booting the current image.
    Failed(String),
}

/// The net service loop, run on the dedicated net thread. Owns the Wi-Fi stack,
/// bringing it up lazily on the first request and keeping it up afterwards.
/// Blocks on `rx`; for each request it ensures connectivity + clock + trust
/// store, runs one publish cycle, and reports the outcome on `tx`. Returns when
/// the request channel closes (UI task gone). Errors are reported, never
/// panicked — a failed push must not take the thread (and its Wi-Fi) down.
pub fn run_net_service(
    modem: Modem<'static>,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    rx: Receiver<NetRequest>,
    tx: Sender<NetOutcome>,
) {
    tune_libgit2();

    // Lazily initialised on the first request, then reused across publishes.
    let mut wifi: Option<BlockingWifi<EspWifi<'static>>> = None;
    let mut modem = Some(modem);
    let mut nvs = Some(nvs);
    let mut clock_synced = false;
    let mut tls_ready = false;

    while let Ok(req) = rx.recv() {
        let msg = match req {
            NetRequest::Publish(req) => NetOutcome::Publish(
                match publish_cycle(
                    &sys_loop,
                    &mut wifi,
                    &mut modem,
                    &mut nvs,
                    &mut clock_synced,
                    &mut tls_ready,
                    &req.paths,
                ) {
                    Ok(o) => o,
                    Err(e) => {
                        log::error!("❌ :gp failed: {e:?}");
                        PublishOutcome::Failed(short_reason("sync", &e))
                    }
                },
            ),
            NetRequest::Pull(paths) => NetOutcome::Pull(
                match pull_cycle(
                    &sys_loop,
                    &mut wifi,
                    &mut modem,
                    &mut nvs,
                    &mut clock_synced,
                    &mut tls_ready,
                    &paths,
                ) {
                    Ok(o) => o,
                    Err(e) => {
                        log::error!("❌ :gl failed: {e:?}");
                        PullOutcome::Failed(short_reason("pull", &e))
                    }
                },
            ),
            NetRequest::Update => NetOutcome::Update(
                match update_cycle(
                    &sys_loop,
                    &mut wifi,
                    &mut modem,
                    &mut nvs,
                    &mut clock_synced,
                    &mut tls_ready,
                ) {
                    Ok(o) => o,
                    Err(e) => {
                        log::error!("❌ :update failed: {e:?}");
                        UpdateOutcome::Failed(short_reason("update", &e))
                    }
                },
            ),
        };
        // If the UI task has gone away there's nothing to report to; exit.
        if tx.send(msg).is_err() {
            break;
        }
    }
    log::info!("git service: request channel closed — exiting");
}

/// Shallow-clone `remote_url` into `REPO_DIR` for the onboarding wizard
/// (v0.9 slice 4): init, learn the default branch, fetch it at depth 1, then
/// materialize the tip tree to the working copy (media skipped, like the pull).
/// Credentials are passed explicitly — the wizard runs before `set_card_conf`,
/// so the global accessors are still empty. Returns the number of working-tree
/// files written.
///
/// Runs on a `GIT_STACK` thread (libgit2's path-buffer nesting overflows the
/// default) and applies `tune_libgit2` + installs the TLS trust store itself,
/// since the service thread that normally does so (via `ensure_online`) hasn't
/// started yet — without the trust store libgit2 rejects github.com as
/// `NOT_TRUSTED`. Wi-Fi is already up and the clock already SNTP-synced by the
/// wizard's device-flow step, so only the CA bundle is missing here.
/// `progress` receives short status lines for the panel.
pub fn clone_repo(
    remote_url: &str,
    gh_user: &str,
    token: &str,
    progress: &dyn Fn(&str),
) -> Result<usize> {
    tune_libgit2();
    // libgit2's mbedTLS stream has no CA set until this runs; the service thread
    // that normally installs it hasn't started during the wizard.
    install_tls_trust_store()?;
    log::info!(
        "clone: init {REPO_DIR} <- {remote_url} (free heap {})",
        free_heap()
    );
    let repo = Repository::init(REPO_DIR).context("git init")?;
    // A previous failed attempt may have left `origin` behind (init is
    // idempotent, but re-adding a remote is not). Reuse or repoint it — this
    // also covers the user re-picking a different repo on retry.
    let mut remote = match repo.remote("origin", remote_url) {
        Ok(r) => r,
        Err(_) => {
            repo.remote_set_url("origin", remote_url)
                .context("repointing existing origin")?;
            repo.find_remote("origin").context("reopening origin")?
        }
    };

    // Learn the default branch (main/master/…) from the ref advertisement.
    progress("contacting origin");
    {
        let (u, t) = (gh_user.to_string(), token.to_string());
        let mut cbs = RemoteCallbacks::new();
        cbs.credentials(move |_url, _u, allowed| {
            if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
                Cred::userpass_plaintext(&u, &t)
            } else {
                Err(git2::Error::from_str("server did not offer USER_PASS_PLAINTEXT"))
            }
        });
        cbs.certificate_check(|_c, host| {
            log::info!("verifying {host} TLS chain against embedded GitHub CA bundle");
            Ok(CertificateCheckStatus::CertificatePassthrough)
        });
        remote
            .connect_auth(git2::Direction::Fetch, Some(cbs), None)
            .context("connecting to origin")?;
    }
    let default = remote
        .default_branch()
        .context("origin advertised no default branch")?;
    let refname = default.as_str().context("default branch not UTF-8")?;
    let branch = refname
        .strip_prefix("refs/heads/")
        .unwrap_or(refname)
        .to_string();
    let _ = remote.disconnect();

    // Shallow-fetch just that branch's tip.
    progress(&format!("downloading {branch}"));
    {
        let (u, t) = (gh_user.to_string(), token.to_string());
        let mut cbs = RemoteCallbacks::new();
        cbs.credentials(move |_url, _u, allowed| {
            if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
                Cred::userpass_plaintext(&u, &t)
            } else {
                Err(git2::Error::from_str("server did not offer USER_PASS_PLAINTEXT"))
            }
        });
        cbs.certificate_check(|_c, _host| Ok(CertificateCheckStatus::CertificatePassthrough));
        // Throttled transfer progress: a line every ~512 objects, not per-object
        // (each line repaints the panel).
        let mut last = 0usize;
        cbs.transfer_progress(|p| {
            let recv = p.received_objects();
            let total = p.total_objects();
            if recv >= last + 512 || (total > 0 && recv == total) {
                last = recv;
                progress(&format!("downloading {recv}/{total} objects"));
            }
            true
        });
        let mut fo = FetchOptions::new();
        fo.depth(1);
        fo.remote_callbacks(cbs);
        remote
            .fetch(&[branch.as_str()], Some(&mut fo), None)
            .context("shallow fetch")?;
    }

    let tip = repo
        .find_reference("FETCH_HEAD")
        .context("no FETCH_HEAD after fetch")?
        .peel_to_commit()
        .context("FETCH_HEAD is not a commit")?
        .id();

    // Establish branch + HEAD + tracking ref so the next boot sees a real repo,
    // and a power-pull between here and the working-copy write still resumes
    // (the wizard re-enters on missing/partial repo).
    repo.reference(&format!("refs/heads/{branch}"), tip, true, "typoena clone")
        .context("creating local branch")?;
    repo.set_head(&format!("refs/heads/{branch}"))
        .context("setting HEAD")?;
    repo.reference(
        &format!("refs/remotes/origin/{branch}"),
        tip,
        true,
        "typoena clone",
    )
    .context("creating tracking ref")?;

    // Materialize the tip tree (media skipped — never writes a big blob to RAM).
    progress("writing files");
    let tree = repo.find_commit(tip)?.tree().context("tip tree")?;
    let mut count = 0usize;
    materialize_tree(&repo, &tree, "", &mut count)?;

    // The file palette walks /sd/local too; make sure it exists.
    let _ = fs::create_dir_all(LOCAL_DIR);
    log::info!(
        "clone: wrote {count} file(s), branch {branch} @ {} (free heap {})",
        short(tip),
        free_heap()
    );
    Ok(count)
}

/// Recursively write a tree's blobs to the working copy under `REPO_DIR`,
/// skipping media (a pulled image would materialize its whole blob in RAM — the
/// OOM the pull avoids). Atomic writes (tmp + rename, FAT won't overwrite) like
/// the pull, so a power-pull mid-clone leaves partial files the next attempt
/// overwrites idempotently.
fn materialize_tree(
    repo: &Repository,
    tree: &git2::Tree,
    prefix: &str,
    count: &mut usize,
) -> Result<()> {
    for entry in tree.iter() {
        let Some(name) = entry.name() else { continue };
        let rel = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        match entry.kind() {
            Some(ObjectType::Tree) => {
                let obj = entry
                    .to_object(repo)
                    .with_context(|| format!("reading subtree {rel}"))?;
                if let Some(sub) = obj.as_tree() {
                    materialize_tree(repo, sub, &rel, count)?;
                }
            }
            Some(ObjectType::Blob) => {
                if is_media_path(&rel) {
                    continue;
                }
                let abs = format!("{REPO_DIR}/{rel}");
                if let Some(dir) = std::path::Path::new(&abs).parent() {
                    fs::create_dir_all(dir).with_context(|| format!("mkdir for {rel}"))?;
                }
                let blob = repo
                    .find_blob(entry.id())
                    .with_context(|| format!("reading blob for {rel}"))?;
                let tmp = format!("{abs}.gltmp");
                fs::write(&tmp, blob.content()).with_context(|| format!("writing {rel}"))?;
                let _ = fs::remove_file(&abs);
                fs::rename(&tmp, &abs).with_context(|| format!("landing {rel}"))?;
                *count += 1;
            }
            _ => {}
        }
    }
    Ok(())
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
    if remote_url().is_empty() || gh_user().is_empty() || token().is_empty() || wifi_ssid().is_empty() {
        bail!("git config missing — provision the card's typoena.conf (installer / wizard) or set TW_* in firmware/.env and rebuild");
    }

    // Nothing recorded dirty and origin's tracking ref already has HEAD: this
    // `:gp` has nothing to do — say so without touching the radio (~150 ms
    // instead of a Wi-Fi + TLS round). A stranded local commit (committed but
    // never pushed, e.g. a push that failed mid-air) makes the check false and
    // takes the full path below, where publish_once pushes it.
    if paths.is_empty() && remote_current().unwrap_or(false) {
        log::info!(":gp — no dirty paths and origin has HEAD; up to date, radio untouched");
        return Ok(PublishOutcome::UpToDate);
    }

    // Phases are timed so a cold :gp reports where the seconds go. Wi-Fi, clock
    // and TLS run only on the first sync of a session; a warm sync skips them, so
    // they read 0 ms and the total collapses to just publish(fetch+commit+push).
    let t_total = Instant::now();
    ensure_online(sys_loop, wifi, modem, nvs, clock_synced, tls_ready)?;

    let t_publish = Instant::now();
    let outcome = publish_once(paths)?;
    log::info!(
        ":gp timing — publish(commit+push) {}ms, total {}ms",
        t_publish.elapsed().as_millis(),
        t_total.elapsed().as_millis(),
    );
    Ok(outcome)
}

/// One full pull (`:gl`): ensure connectivity, then fetch + fast-forward/rebase.
/// Always needs the network — there is no radio-free shortcut like publish's
/// up-to-date check, because the whole point is asking origin what's new.
/// `paths` is the dirty-journal snapshot to commit locally before fetching.
fn pull_cycle(
    sys_loop: &EspSystemEventLoop,
    wifi: &mut Option<BlockingWifi<EspWifi<'static>>>,
    modem: &mut Option<Modem<'static>>,
    nvs: &mut Option<EspDefaultNvsPartition>,
    clock_synced: &mut bool,
    tls_ready: &mut bool,
    paths: &BTreeSet<String>,
) -> Result<PullOutcome> {
    if remote_url().is_empty() || gh_user().is_empty() || token().is_empty() || wifi_ssid().is_empty() {
        bail!("git config missing — provision the card's typoena.conf (installer / wizard) or set TW_* in firmware/.env and rebuild");
    }
    let t_total = Instant::now();
    ensure_online(sys_loop, wifi, modem, nvs, clock_synced, tls_ready)?;

    let t_pull = Instant::now();
    let outcome = pull_once(paths)?;
    log::info!(
        ":gl timing — fetch+ff {}ms, total {}ms",
        t_pull.elapsed().as_millis(),
        t_total.elapsed().as_millis(),
    );
    Ok(outcome)
}

/// One firmware-update check (`:update`): ensure connectivity, then hand off to
/// the OTA module, which compares the running version against the release
/// manifest and — only if it is newer — streams the image into the inactive OTA
/// slot. Always needs the network (the whole point is asking what the latest
/// release is). Reuses the net thread's Wi-Fi + clock bring-up; it needs no git
/// config (no remote/token), only Wi-Fi.
fn update_cycle(
    sys_loop: &EspSystemEventLoop,
    wifi: &mut Option<BlockingWifi<EspWifi<'static>>>,
    modem: &mut Option<Modem<'static>>,
    nvs: &mut Option<EspDefaultNvsPartition>,
    clock_synced: &mut bool,
    tls_ready: &mut bool,
) -> Result<UpdateOutcome> {
    if wifi_ssid().is_empty() {
        bail!("Wi-Fi not provisioned — run :setup / the installer first");
    }
    let t_total = Instant::now();
    ensure_online(sys_loop, wifi, modem, nvs, clock_synced, tls_ready)?;

    let t_ota = Instant::now();
    let outcome = match crate::infrastructure::ota::run_update()? {
        Some(version) => UpdateOutcome::Installed(version),
        None => UpdateOutcome::UpToDate(crate::infrastructure::ota::FW_VERSION.to_string()),
    };
    log::info!(
        ":update timing — ota(check+download) {}ms, total {}ms",
        t_ota.elapsed().as_millis(),
        t_total.elapsed().as_millis(),
    );
    Ok(outcome)
}

/// Bring Wi-Fi + wall clock + TLS trust store up, each once per session; a
/// warm call is a no-op. Shared by publish and pull, on the net thread. Logs
/// one timing line whenever any step actually ran (the session's first
/// operation pays them all; every later one skips straight to git).
fn ensure_online(
    sys_loop: &EspSystemEventLoop,
    wifi: &mut Option<BlockingWifi<EspWifi<'static>>>,
    modem: &mut Option<Modem<'static>>,
    nvs: &mut Option<EspDefaultNvsPartition>,
    clock_synced: &mut bool,
    tls_ready: &mut bool,
) -> Result<()> {
    // Bring Wi-Fi up once (on-demand: the radio stays off until the first use).
    let wifi_ms = if wifi.is_none() {
        let t = Instant::now();
        log::info!("first git op — bringing Wi-Fi up; free heap {}", free_heap());
        let m = modem.take().expect("modem taken once");
        let n = nvs.take().expect("nvs taken once");
        let mut w = BlockingWifi::wrap(
            EspWifi::new(m, sys_loop.clone(), Some(n))?,
            sys_loop.clone(),
        )?;
        connect_wifi(&mut w, wifi_ssid(), wifi_pass()).context("connecting Wi-Fi")?;
        let ip = w.wifi().sta_netif().get_ip_info()?;
        log::info!("Wi-Fi up — IP {}", ip.ip);
        *wifi = Some(w);
        t.elapsed().as_millis()
    } else {
        0u128
    };
    let clock_ms = if !*clock_synced {
        let t = Instant::now();
        sync_clock()?;
        *clock_synced = true;
        t.elapsed().as_millis()
    } else {
        0
    };
    let tls_ms = if !*tls_ready {
        let t = Instant::now();
        install_tls_trust_store()?;
        *tls_ready = true;
        t.elapsed().as_millis()
    } else {
        0
    };
    if wifi_ms + clock_ms + tls_ms > 0 {
        log::info!("online — wifi {wifi_ms}ms, clock {clock_ms}ms, tls {tls_ms}ms");
    }
    Ok(())
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

    // I/O attribution for the ~360 ms/loose-write residual (v0.7 follow-up):
    // bracket the splice with the p_mmap counters so the log says how many
    // mmap windows (≈ unique pack reads) and how many KB the whole splice
    // issued. Divided by the loose writes (~4/path: blob + tree chain), this
    // pins whether the residual is pack-read I/O or FAT directory ops — the
    // two candidates left after FASTSEEK.
    let (maps_before, read_kb_before) = map_counters();
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

    let sig = Signature::now(author_name(), author_email()).context("building signature")?;
    let message = format!("Typoena publish — unix {}", now_unix());
    let parents: Vec<&Commit> = parent.iter().collect();
    let t_commit = Instant::now();
    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, &message, &tree, &parents)
        .context("creating commit")?;
    let (maps_after, read_kb_after) = map_counters();
    log::info!(
        "commit split — splice {splice_ms}ms ({} path(s), {} mmaps / {} KB read), commit-obj {}ms; committed {} — free heap {} ({} internal)",
        paths.len(),
        maps_after - maps_before,
        read_kb_after - read_kb_before,
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
    // Progress + heap tracing through the otherwise-silent stretch between the
    // TLS verify and the first byte sent (66 s in runs 4 and 5, OOM both times).
    {
        // Time-gated, NOT count-gated: during AddingObjects libgit2 reports
        // `total` = 0 and `current` = objects inserted so far, and a two-commit
        // push only inserts a few dozen objects — run 5's `current >= last+256`
        // gate swallowed every callback and the grind stayed silent. libgit2
        // already rate-limits to ~2/s (MIN_PROGRESS_UPDATE_INTERVAL); gate to
        // ~1 line per 2 s on top of that.
        let mut last: Option<Instant> = None;
        cbs.pack_progress(move |stage, current, total| {
            if last.is_none_or(|t| t.elapsed() >= Duration::from_secs(2)) {
                last = Some(Instant::now());
                log_push_heap(&format!("pack {stage:?} {current}/{total}"));
            }
        });
        let mut next_bytes: usize = 0;
        cbs.push_transfer_progress(move |current, total, bytes| {
            if bytes >= next_bytes || (total > 0 && current == total) {
                next_bytes = bytes + 64 * 1024;
                log_push_heap(&format!("send {current}/{total} objects, {bytes} B"));
            }
        });
    }

    let mut opts = PushOptions::new();
    opts.remote_callbacks(cbs);
    log_push_heap("pre-push");

    // A non-fast-forward can also surface here, not just via the callback:
    // libgit2 compares against origin's advertised tips during negotiation and
    // errors out of push() with ErrorCode::NotFastForward before sending
    // anything, so `push_update_reference` never fires. It's still the
    // remote-moved-under-us case — reconcilable, not a transport failure
    // (bit the 2026-07-13 run 3: the real-repo rejection surfaced as "push
    // transport" and skipped the reconcile built for it).
    remote.push(&[refspec], Some(&mut opts)).map_err(|e| {
        // Heap post-mortem: runs 5–6 died on a ~7 KB inflateInit inside the
        // pack build ("failed to init zlib stream on unpack") — the min-ever
        // lines here say which pool zeroed and whether it was exhaustion or
        // fragmentation, even when no progress callback got a chance to fire.
        log_push_heap("push failed");
        if e.code() == git2::ErrorCode::NotFastForward {
            PushFailure::Rejected(format!("{refspec}: {}", e.message()))
        } else {
            PushFailure::Other(anyhow::Error::new(e).context("push transport"))
        }
    })?;

    if let Some(msg) = rejection.borrow().clone() {
        return Err(PushFailure::Rejected(msg));
    }
    log_push_heap("post-push");
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
    let theirs = fetch_origin(repo, branch)?;
    log::info!(
        "reconcile: resetting local {branch} onto origin @ {} (soft — ref move only, notes stay on the card)",
        short(theirs)
    );
    let their_obj = repo.find_object(theirs, None)?;
    repo.reset(&their_obj, git2::ResetType::Soft, None)
        .context("soft reset onto origin")?;
    Ok(())
}

/// Fetch `branch` from origin and return the fetched tip's commit id. Shared
/// by the pull and the post-rejection reconcile. Also refreshes the
/// remote-tracking ref, keeping [`tracking_tip`] (and with it publish's
/// radio-free up-to-date check) honest about what origin has.
fn fetch_origin(repo: &Repository, branch: &str) -> Result<Oid> {
    let mut remote = repo.find_remote("origin")?;
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(auth_callbacks());
    remote
        .fetch(&[branch], Some(&mut fo), None)
        .context("fetch origin")?;
    let theirs = repo
        .find_reference("FETCH_HEAD")
        .context("no FETCH_HEAD after fetch")?
        .peel_to_commit()
        .context("FETCH_HEAD is not a commit")?
        .id();
    update_tracking(repo, branch, theirs)?;
    Ok(theirs)
}

/// Point the remote-tracking ref at `tip` (which must already be in the local
/// odb). Keeps [`tracking_tip`] — and with it publish's radio-free up-to-date
/// check — honest about what origin has.
fn update_tracking(repo: &Repository, branch: &str, tip: Oid) -> Result<()> {
    repo.reference(
        &format!("refs/remotes/origin/{branch}"),
        tip,
        true,
        "typoena fetch",
    )
    .context("updating remote-tracking ref")?;
    Ok(())
}

/// Open `/sd/repo`, fetch origin, and integrate — **fast-forward when we can,
/// rebase when we must**, never a content merge. The non-failure shapes map to
/// [`PullOutcome`]: already current, we're strictly ahead (a stranded commit —
/// `:gp`'s job), a clean fast-forward, or a divergence — where instead of
/// refusing we replant our local commit(s) onto origin ([`rebase_local_onto`])
/// and end `LocalAhead` for `:gp` to publish.
///
/// `paths` is the dirty-journal snapshot. Before touching the network we fold
/// those saved-but-unpublished paths into a local commit ([`stage_and_commit`],
/// the commit half of `:gp` without the push): that makes `:gl` self-sufficient
/// — the ff/rebase below replants the commit onto origin so a plain `:gp`
/// finishes it, no computer needed — and, because the working copy now matches
/// the new HEAD, the SAFE belt can't fight a device-side save. The UI has
/// already confirmed this commit (it is user-visible). Empty `paths` is a plain
/// fetch.
///
/// The fast-forward is checkout-then-ref-move, with a **SAFE** checkout: it
/// refuses to overwrite a working-copy file whose content differs from HEAD's.
/// After the pre-fetch commit the card matches HEAD, so in normal use nothing
/// conflicts; the belt catches files edited behind git's back (e.g. desktop
/// edits made directly on the card — deliberately never committed by the device
/// since the splice landed). One FAT caveat, matching publish's index-avoidance:
/// the splice never updates the index, so its stat cache is stale and SAFE
/// re-hashes each file the pull wants to change — fine for a few notes, and
/// still O(changed), never O(tree).
fn pull_once(paths: &BTreeSet<String>) -> Result<PullOutcome> {
    log::info!(
        "pull started — {} unpublished path(s) to fold in, free heap {} ({} internal)",
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
    let mut head = repo.head()?.peel_to_commit()?.id();

    // Fold any saved-but-unpublished work into a local commit before the fetch,
    // so a divergence rebases it onto origin instead of refusing (and the card
    // ends matching HEAD, keeping the SAFE belt below quiet). Local only — no
    // network. stage_and_commit moves the branch ref (commits onto "HEAD") and
    // returns None when the paths are already committed (a no-op splice), so
    // `head` only advances when there was genuinely new work.
    if !paths.is_empty() {
        if let Some(committed) = stage_and_commit(&repo, paths)? {
            log::info!(
                "pull: committed {} unpublished path(s) locally as {} before fetch",
                paths.len(),
                short(committed)
            );
            head = committed;
        }
    }

    // ls-refs first, download only if needed: the ref advertisement alone
    // answers "anything new?", so the common shapes (up to date, local ahead)
    // never enter pack negotiation — the first on-device pull paid a 9.7 s
    // fetch just to learn it was up to date. When a download IS needed it
    // rides the same open connection (no second TLS handshake).
    let mut remote = repo.find_remote("origin")?;
    let t_ls = Instant::now();
    remote
        .connect_auth(git2::Direction::Fetch, Some(auth_callbacks()), None)
        .context("connecting to origin")?;
    let refname = format!("refs/heads/{branch}");
    let theirs = remote
        .list()
        .context("listing origin refs")?
        .iter()
        .find(|h| h.name() == refname)
        .map(|h| h.oid())
        .with_context(|| format!("origin does not advertise {refname}"))?;
    let ls_ms = t_ls.elapsed().as_millis();

    if theirs == head {
        let _ = remote.disconnect();
        update_tracking(&repo, &branch, theirs)?;
        log::info!("pull: origin @ {} == HEAD — up to date (ls-refs {ls_ms}ms, no fetch)", short(head));
        return Ok(PullOutcome::UpToDate);
    }
    // `theirs` an ancestor of HEAD ⇒ it is already in the local odb (all
    // ancestors of a local commit are local) — no download needed either.
    if repo.odb()?.exists(theirs)
        && repo
            .graph_descendant_of(head, theirs)
            .context("descendant check (local ahead)")?
    {
        let _ = remote.disconnect();
        update_tracking(&repo, &branch, theirs)?;
        log::info!(
            "pull: HEAD {} is ahead of origin {} — nothing to pull, :gp publishes it (ls-refs {ls_ms}ms, no fetch)",
            short(head),
            short(theirs)
        );
        return Ok(PullOutcome::LocalAhead);
    }

    // Origin has commits we lack: download them over the already-open
    // connection (callbacks were bound at connect_auth).
    let t_fetch = Instant::now();
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(auth_callbacks());
    remote
        .download(&[branch.as_str()], Some(&mut fo))
        .context("downloading from origin")?;
    let _ = remote.disconnect();
    update_tracking(&repo, &branch, theirs)?;
    let fetch_ms = t_fetch.elapsed().as_millis();
    log::info!(
        "pull: downloaded origin @ {} — ls-refs {ls_ms}ms, download {fetch_ms}ms, free heap {} ({} internal)",
        short(theirs),
        free_heap(),
        internal_free_heap()
    );

    if !repo
        .graph_descendant_of(theirs, head)
        .context("descendant check (fast-forward)")?
    {
        // Diverged: both sides moved. Rather than refuse, replant our local
        // commit(s) onto origin's tip so a plain `:gp` publishes them — no
        // computer needed. The branch ref moves LAST (after the card reflects
        // the rebased tree), so a power-pull mid-rebase leaves HEAD at the old
        // tip and the next `:gl` recomputes the identical commit idempotently.
        log::info!(
            "pull: origin {} and HEAD {} diverged — rebasing local work onto origin (last-writer-wins, no merge)",
            short(theirs),
            short(head)
        );
        let t_rebase = Instant::now();
        let rebased = rebase_local_onto(&repo, head, theirs)?;
        let rebase_ms = t_rebase.elapsed().as_millis();

        // Nothing of ours survived the replay (our edits were already upstream):
        // collapse to a plain fast-forward onto origin.
        if rebased == theirs {
            let t_apply = Instant::now();
            let changed = apply_tree_diff(&repo, head, theirs)?;
            repo.reference(
                &format!("refs/heads/{branch}"),
                theirs,
                true,
                "typoena pull: fast-forward (local work already upstream)",
            )
            .context("fast-forwarding the branch ref")?;
            log::info!(
                "pull: local work already upstream — fast-forwarded {branch} to {} — apply {}ms ({changed} file(s))",
                short(theirs),
                t_apply.elapsed().as_millis()
            );
            return Ok(PullOutcome::Pulled(short(theirs)));
        }

        // Bring the card from our old tree to the rebased tree: origin's
        // remote-only changes are written, our own edits are already on disk
        // (unchanged in the diff, so untouched). Ref moves last.
        let t_apply = Instant::now();
        let changed = apply_tree_diff(&repo, head, rebased)?;
        repo.reference(
            &format!("refs/heads/{branch}"),
            rebased,
            true,
            "typoena pull: rebase local onto origin",
        )
        .context("moving the branch ref to the rebased commit")?;
        log::info!(
            "pull: rebased {} onto origin {} -> {} — rebase {rebase_ms}ms, apply {}ms ({changed} file(s)), free heap {} ({} internal)",
            short(head),
            short(theirs),
            short(rebased),
            t_apply.elapsed().as_millis(),
            free_heap(),
            internal_free_heap()
        );
        return Ok(PullOutcome::Rebased(short(rebased)));
    }

    let t_co = Instant::now();
    let changed = apply_tree_diff(&repo, head, theirs)?;
    repo.reference(
        &format!("refs/heads/{branch}"),
        theirs,
        true,
        "typoena pull: fast-forward",
    )
    .context("fast-forwarding the branch ref")?;
    log::info!(
        "pull: fast-forwarded {branch} {} -> {} — fetch {fetch_ms}ms, apply {}ms ({changed} file(s)), free heap {} ({} internal)",
        short(head),
        short(theirs),
        t_co.elapsed().as_millis(),
        free_heap(),
        internal_free_heap()
    );
    Ok(PullOutcome::Pulled(short(theirs)))
}

/// Bring the working copy from `head`'s tree to `theirs`' tree by applying the
/// tree-to-tree diff directly: write each added/modified blob, unlink each
/// deleted path, and touch nothing else. Returns the number of files changed.
///
/// This is `checkout_tree`'s job, done the splice way. libgit2's SAFE checkout
/// iterates the whole **working directory** (readdir over SPI on every one of
/// ~1100 files) to decide what's dirty — the same O(tree) wall the splice
/// commit exists to avoid, and what actually killed the first on-device ff
/// attempt (2026-07-14): the walk ran with fetch memory still resident,
/// internal DRAM hit zero, and esp-idf's spi_master null-derefs on its own
/// failed-DMA-alloc path. The tree-to-tree diff never touches the workdir:
/// identical subtree OIDs are skipped wholesale, so both the diff and the
/// apply are O(changed).
///
/// Safety belt (what SAFE's rehash gave us, kept O(changed)): before touching
/// anything, every to-be-overwritten/deleted file whose disk content no longer
/// hashes to the OLD tree's blob aborts the pull — those are edits made behind
/// git's back (e.g. desktop edits directly on the card), and clobbering them
/// silently is worse than refusing. The check reads only the files the pull
/// wants to change; device-side saves are covered by the pre-fetch commit in
/// [`pull_once`] (which makes the card match HEAD before the diff runs).
///
/// Writes are unlink + tmp + rename (FAT f_rename won't overwrite), so a
/// power-pull mid-apply leaves at worst a `.gltmp` orphan and a half-applied
/// working copy with the ref NOT yet moved — the next `:gl` re-applies
/// idempotently on the same diff.
///
/// Media paths are invisible to both passes (skip-media-in-apply, 2026-07-14;
/// see docs/notes/git-sync-images-and-repo-size.md). The device never renders
/// them, and writing one means materializing the whole blob in RAM — history
/// holds 16 MB PNGs and a 38 MB mp3 against 8 MB of PSRAM, so a pulled image
/// was the one OOM path left in `:gl`. The blobs still arrive in `.git` via
/// the fetch (streamed, cheap); only the working-tree copy is skipped, so the
/// card's media files go stale/absent relative to HEAD. That's safe on the
/// commit side because the splice stages explicit journal paths — a missing
/// image can never be committed as a deletion. The belt hash skips them too:
/// hashing a stale 16 MB image would be the same OOM by another door.
fn apply_tree_diff(repo: &Repository, head: Oid, theirs: Oid) -> Result<usize> {
    use git2::Delta;

    let head_tree = repo.find_commit(head)?.tree().context("HEAD tree")?;
    let their_tree = repo.find_commit(theirs)?.tree().context("target tree")?;
    let diff = repo
        .diff_tree_to_tree(Some(&head_tree), Some(&their_tree), None)
        .context("diffing HEAD..origin trees")?;

    // Pass 1 — verify: refuse before the first write if any file we are about
    // to replace or remove was hand-edited (content no longer matches the old
    // blob). A missing file is fine (nothing to clobber).
    for d in diff.deltas() {
        let (old, path) = match d.status() {
            Delta::Modified | Delta::Typechange => {
                (d.old_file().id(), d.old_file().path())
            }
            Delta::Deleted => (d.old_file().id(), d.old_file().path()),
            _ => continue, // Added: nothing on disk to protect
        };
        let Some(rel) = path.and_then(|p| p.to_str()) else {
            continue;
        };
        if is_media_path(rel) {
            continue;
        }
        let abs = format!("{REPO_DIR}/{rel}");
        match fs::read(&abs) {
            Ok(bytes) => {
                let disk = Oid::hash_object(ObjectType::Blob, &bytes)
                    .with_context(|| format!("hashing {rel}"))?;
                if disk != old {
                    bail!("local change in {rel} — pull refused (edit made behind git; resolve on a computer)");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("reading {rel}")),
        }
    }

    // Pass 2 — apply.
    let mut changed = 0usize;
    let mut media_skipped = 0usize;
    for d in diff.deltas() {
        match d.status() {
            Delta::Added | Delta::Modified | Delta::Typechange => {
                let Some(rel) = d.new_file().path().and_then(|p| p.to_str()) else {
                    continue;
                };
                if is_media_path(rel) {
                    media_skipped += 1;
                    continue;
                }
                let abs = format!("{REPO_DIR}/{rel}");
                if let Some(dir) = std::path::Path::new(&abs).parent() {
                    fs::create_dir_all(dir).with_context(|| format!("mkdir for {rel}"))?;
                }
                let blob = repo
                    .find_blob(d.new_file().id())
                    .with_context(|| format!("reading blob for {rel}"))?;
                let tmp = format!("{abs}.gltmp");
                fs::write(&tmp, blob.content()).with_context(|| format!("writing {rel}"))?;
                let _ = fs::remove_file(&abs); // FAT rename won't overwrite
                fs::rename(&tmp, &abs).with_context(|| format!("landing {rel}"))?;
                changed += 1;
            }
            Delta::Deleted => {
                let Some(rel) = d.old_file().path().and_then(|p| p.to_str()) else {
                    continue;
                };
                if is_media_path(rel) {
                    media_skipped += 1;
                    continue;
                }
                match fs::remove_file(format!("{REPO_DIR}/{rel}")) {
                    Ok(()) => changed += 1,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e).with_context(|| format!("deleting {rel}")),
                }
            }
            _ => {}
        }
    }
    if media_skipped > 0 {
        log::info!(
            "pull: skipped {media_skipped} media file(s) — blobs live in .git, the card's working copy stays text-only"
        );
    }
    Ok(changed)
}

/// Rebase the device's local-only work onto origin's tip and return the new
/// commit id — a single squashed commit whose tree is origin's tree with our
/// edits spliced back on top. This is `:gl`'s answer to a divergence (a
/// stranded local commit while origin also moved): rather than refuse and send
/// the user to a computer, we replant our work on the new base so a plain `:gp`
/// publishes it.
///
/// Last-writer-wins by design, exactly like publish's post-rejection reconcile
/// ([`reconcile_onto_origin`]): the replay set is the paths our side changed
/// since the fork point (`merge_base..head`), each spliced from the **card**
/// (the source of truth) onto origin's tree. A note both sides edited resolves
/// to ours; every remote-only change rides along by OID from origin's tree.
/// It is a rebase of one squashed commit, not a content merge — the device
/// still has no merge engine (that stays increment-B work).
///
/// Returns `theirs` unchanged when nothing of ours survives the replay (our
/// edits were already upstream) so the caller can collapse to a fast-forward
/// instead of writing an empty commit. The commit is created with
/// `commit(None, …)`: it does **not** move the branch ref. The caller applies
/// the merged tree to the card and moves the ref last, so a power-pull
/// mid-rebase leaves HEAD at the old tip and the next `:gl` recomputes the
/// identical commit idempotently.
fn rebase_local_onto(repo: &Repository, head: Oid, theirs: Oid) -> Result<Oid> {
    let base = repo
        .merge_base(head, theirs)
        .context("finding the merge-base to rebase onto origin")?;
    let base_tree = repo.find_commit(base)?.tree().context("merge-base tree")?;
    let head_tree = repo.find_commit(head)?.tree().context("HEAD tree")?;
    let their_commit = repo.find_commit(theirs)?;
    let their_tree = their_commit.tree().context("origin tree")?;
    let their_tree_id = their_tree.id();

    // Our side's changes since the fork are the replay set: splice each onto
    // origin's tree, read from the card (a path missing on disk splices out — a
    // local delete), mirroring stage_and_commit's working-tree-as-truth model.
    let diff = repo
        .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)
        .context("diffing merge-base..HEAD for the replay set")?;
    let paths: BTreeSet<String> = diff
        .deltas()
        .filter_map(|d| {
            d.new_file()
                .path()
                .or_else(|| d.old_file().path())
                .and_then(|p| p.to_str())
                .map(str::to_string)
        })
        .collect();

    let mut tree = their_tree;
    for path in &paths {
        // Media is never a device commit (the splice stages journal paths only,
        // and the card is text-only) — but reading a stray 16 MB blob would OOM,
        // so skip it and keep origin's version, consistent with apply_tree_diff.
        if is_media_path(path) {
            log::warn!("rebase: skipping media path {path} in the replay set (kept origin's version)");
            continue;
        }
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            continue;
        }
        let blob = match fs::read(format!("{REPO_DIR}/{path}")) {
            Ok(bytes) => Some(repo.blob(&bytes).with_context(|| format!("blob for {path}"))?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None, // local delete
            Err(e) => return Err(e).with_context(|| format!("reading {path}")),
        };
        let spliced = splice(repo, Some(&tree), &parts, blob)
            .with_context(|| format!("splicing {path} onto origin"))?;
        tree = repo.find_tree(spliced).context("loading spliced tree")?;
    }

    // Nothing of ours survived (edits already upstream) — signal a fast-forward.
    if tree.id() == their_tree_id {
        return Ok(theirs);
    }

    let sig = Signature::now(author_name(), author_email()).context("building signature")?;
    let message = format!("Typoena rebase onto origin — unix {}", now_unix());
    repo.commit(None, &sig, &sig, &message, &tree, &[&their_commit])
        .context("creating the rebased commit")
}

/// Paths [`apply_tree_diff`] never writes, deletes, or belt-hashes: binary
/// media the device can't render and can't afford to hold in RAM. Matched by
/// extension, case-insensitive. Text-ish assets (svg, csv…) stay eligible —
/// the criterion is blob size risk, not "is it a note".
fn is_media_path(rel: &str) -> bool {
    const MEDIA_EXT: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "bmp", "webp", "heic", "tiff", "ico", "pdf", "mp3", "mp4",
        "m4a", "wav", "mov", "avi", "mkv", "zip",
    ];
    std::path::Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA_EXT.iter().any(|m| e.eq_ignore_ascii_case(m)))
}

/// Auth + cert callbacks shared by fetch and push. Captures only statics
/// (card conf / baked consts), so a fresh set can be built per operation. The
/// token is handed to libgit2 here and never logged.
fn auth_callbacks<'a>() -> RemoteCallbacks<'a> {
    let mut cbs = RemoteCallbacks::new();
    cbs.credentials(|_url, _user_from_url, allowed| {
        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
            return Cred::userpass_plaintext(gh_user(), token());
        }
        Err(git2::Error::from_str(
            "server did not offer USER_PASS_PLAINTEXT — cannot authenticate with a token",
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

/// A short, panel-friendly reason from an error chain (first line, clamped),
/// prefixed with the operation ("sync" / "pull"). The full chain is logged
/// separately; the editor clamps this to the panel width.
fn short_reason(op: &str, e: &anyhow::Error) -> String {
    let full = format!("{e}");
    let first = full.lines().next().unwrap_or("failed");
    format!("{op}: {}", first.chars().take(24).collect::<String>())
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

unsafe extern "C" {
    /// Counters from the p_mmap emulation in `components/libgit2/esp_map.c`.
    /// Post cache-removal: `hits` is always 0, `misses` counts every mapping,
    /// `cached_kb` reports the LIVE mapped bytes — every libgit2 "mmap"
    /// (mwindow windows AND whole-file pack .idx maps) is a real PSRAM malloc
    /// there, so this splits map memory from everything else git allocates.
    fn esp_map_stats(hits: *mut u32, misses: *mut u32, read_kb: *mut u32, cached_kb: *mut u32);
}

/// The p_mmap emulation's cumulative counters: (mappings created, KB read).
/// Deltas around an operation attribute its pack-read I/O (each mapping is a
/// real lseek+read over SPI in esp_map.c).
fn map_counters() -> (u32, u32) {
    let (mut maps, mut read_kb) = (0u32, 0u32);
    // SAFETY: esp_map_stats only writes the non-null out-params.
    unsafe { esp_map_stats(std::ptr::null_mut(), &mut maps, &mut read_kb, std::ptr::null_mut()) };
    (maps, read_kb)
}

/// One-line heap + odb-cache + mmap snapshot for the push path. Runs 4–6
/// (2026-07-13) each spent ~65 s inside `remote.push()` while something
/// consumed ~6 MB of PSRAM (run 4: the UI aborted on a framebuffer alloc;
/// runs 5–6: a ~7 KB inflateInit failed inside the pack build; run 6 pinned
/// min-ever PSRAM at 684 B with the odb cache at 59 KB — exonerated). These
/// lines exist to name the consumer: if `mmap live` tracks the PSRAM drop the
/// eater is mwindow windows / idx maps, otherwise it's non-map allocations
/// (parsed objects, delta chains). `largest PSRAM` distinguishes exhaustion
/// from fragmentation, and the min-evers survive to the failure log even when
/// the spike itself fell between two callbacks.
fn log_push_heap(stage: &str) {
    let (mut cached, mut allowed): (isize, isize) = (0, 0);
    // SAFETY: GET_CACHED_MEMORY only writes the two out-params.
    unsafe {
        libgit2_sys::git_libgit2_opts(
            libgit2_sys::GIT_OPT_GET_CACHED_MEMORY as i32,
            &mut cached as *mut isize,
            &mut allowed as *mut isize,
        );
    }
    let (largest_psram, min_psram, min_internal) = unsafe {
        (
            sys::heap_caps_get_largest_free_block(sys::MALLOC_CAP_SPIRAM),
            sys::heap_caps_get_minimum_free_size(sys::MALLOC_CAP_SPIRAM),
            sys::heap_caps_get_minimum_free_size(sys::MALLOC_CAP_INTERNAL),
        )
    };
    let (mut maps, mut read_kb, mut live_kb) = (0u32, 0u32, 0u32);
    // SAFETY: esp_map_stats only writes the non-null out-params.
    unsafe { esp_map_stats(std::ptr::null_mut(), &mut maps, &mut read_kb, &mut live_kb) };
    log::info!(
        "push heap [{stage}]: free {} ({} internal), largest PSRAM {}, min-ever PSRAM {} / internal {}, mmap live {live_kb} KB ({maps} maps, {read_kb} KB read), odb cache {}/{} KB",
        free_heap(),
        internal_free_heap(),
        largest_psram,
        min_psram,
        min_internal,
        cached / 1024,
        allowed / 1024
    );
}

// ---- app::NetService port adapter ----------------------------------------

use crate::infrastructure::storage_sd::Storage;

/// [`app::NetService`] backed by the net thread (git `:gp`/`:gl` + `:update`
/// OTA). Owns the request/outcome channels and a handle to the card's dirty
/// journal, which it takes on publish and settles when the outcome lands (OTA
/// touches no journal). Behind the `full` feature — it pulls libgit2.
pub struct NetService {
    card: Rc<Storage>,
    tx: Sender<NetRequest>,
    rx: Receiver<NetOutcome>,
}

impl NetService {
    pub fn new(card: Rc<Storage>, tx: Sender<NetRequest>, rx: Receiver<NetOutcome>) -> Self {
        Self { card, tx, rx }
    }
}

impl app::NetService for NetService {
    fn publish(&self) -> app::PublishDispatch {
        let paths = self.card.take_dirty();
        match self.tx.send(NetRequest::Publish(PublishRequest { paths })) {
            Ok(()) => app::PublishDispatch::Dispatched,
            Err(_) => {
                // Thread gone — nothing will report back, so return the snapshot
                // to pending ourselves.
                self.card.publish_failed();
                app::PublishDispatch::ThreadDown
            }
        }
    }

    fn pull(&self, commit_dirty: bool) -> app::PullDispatch {
        // A bare `:gl` with unpublished saves doesn't refuse anymore — it folds
        // them into a local commit first so the fetch can rebase them onto
        // origin. But that commit is user-visible, so the first pass asks the UI
        // to confirm; the confirmed retry arrives with commit_dirty = true.
        if !commit_dirty && self.card.has_dirty() {
            log::info!(":gl — dirty journal non-empty; asking to confirm the pre-fetch commit");
            return app::PullDispatch::NeedsCommitConfirm;
        }
        // Snapshot the journal into `in_flight` (empty when nothing was dirty);
        // the net thread commits those paths before fetching, and poll_outcome
        // settles the snapshot when the outcome lands — succeeded forgets it,
        // failed returns it to pending — exactly as publish does.
        let paths = self.card.take_dirty();
        match self.tx.send(NetRequest::Pull(paths)) {
            Ok(()) => app::PullDispatch::Dispatched,
            Err(_) => {
                // Thread gone — nothing will report back, so return the snapshot
                // to pending ourselves (mirrors publish's ThreadDown path).
                self.card.publish_failed();
                app::PullDispatch::ThreadDown
            }
        }
    }

    fn update(&self) -> app::UpdateDispatch {
        // No dirty journal to snapshot — OTA doesn't touch the working copy.
        match self.tx.send(NetRequest::Update) {
            Ok(()) => app::UpdateDispatch::Dispatched,
            Err(_) => app::UpdateDispatch::ThreadDown,
        }
    }

    fn poll_outcome(&self) -> Option<app::NetOutcome> {
        let outcome = self.rx.try_recv().ok()?;
        Some(match outcome {
            NetOutcome::Update(o) => app::NetOutcome::Update(match o {
                // OTA settles no dirty journal; just mirror the outcome across
                // the app boundary.
                UpdateOutcome::Installed(v) => app::UpdateOutcome::Installed(v),
                UpdateOutcome::UpToDate(v) => app::UpdateOutcome::UpToDate(v),
                UpdateOutcome::Failed(reason) => app::UpdateOutcome::Failed(reason),
            }),
            NetOutcome::Publish(o) => {
                // Settle the dirty snapshot this publish took: confirmed
                // published (or up to date) → forget it; failed → back to pending.
                match &o {
                    PublishOutcome::Pushed(_) | PublishOutcome::UpToDate => {
                        self.card.publish_succeeded()
                    }
                    PublishOutcome::Failed(_) => self.card.publish_failed(),
                }
                app::NetOutcome::Publish(match o {
                    PublishOutcome::Pushed(oid) => app::PublishOutcome::Pushed(oid),
                    PublishOutcome::UpToDate => app::PublishOutcome::UpToDate,
                    PublishOutcome::Failed(reason) => app::PublishOutcome::Failed(reason),
                })
            }
            NetOutcome::Pull(o) => {
                // Settle the dirty snapshot this pull took (it folds unpublished
                // saves into a local commit before fetching, like publish):
                // integrated → forget it (the work is committed, and a stranded
                // local commit is pushed by the next `:gp`); failed → back to
                // pending. Empty snapshot → both are no-ops.
                match &o {
                    PullOutcome::Failed(_) => self.card.publish_failed(),
                    _ => self.card.publish_succeeded(),
                }
                app::NetOutcome::Pull(match o {
                    PullOutcome::Pulled(oid) => app::PullOutcome::Pulled(oid),
                    PullOutcome::Rebased(oid) => app::PullOutcome::Rebased(oid),
                    PullOutcome::UpToDate => app::PullOutcome::UpToDate,
                    PullOutcome::LocalAhead => app::PullOutcome::LocalAhead,
                    PullOutcome::Failed(reason) => app::PullOutcome::Failed(reason),
                })
            }
        })
    }
}
