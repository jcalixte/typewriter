//! Milestone #2, increment A — the PERSISTENT-CLONE publish cycle.
//!
//! `git_push.rs` proved the transport by doing a *fresh* `git init` each boot and
//! pushing an unrelated per-boot branch `device/<unix>` — deliberately sidestepping
//! history reconciliation. That is NOT how the editor publishes. The product keeps
//! ONE clone of the user's repo on the device and fast-forwards it on every
//! `Ctrl-G`. This binary proves the piece `git_push` never exercised on hardware:
//! `clone` + persistent `open` + a real fast-forward `push`, over mbedTLS HTTPS+PAT.
//!
//! Flow: Wi-Fi + SNTP + flash-FAT + TLS trust store (identical to `git_push`) →
//! `open` `/spiflash/repo` if it exists, else `clone` the remote → append a line
//! to a tracked `notes.md` (a real content change, unlike `git_push`'s throwaway
//! file) → stage + commit (parent = current HEAD, or none on an empty remote) →
//! push HEAD to its own branch as a fast-forward, with a fetch-and-reconcile on a
//! rejected push.
//!
//! ## Scope (increment A)
//!
//! The reconcile handles *up-to-date* and *fast-forward*. A true **divergence**
//! (a second writer pushed in between) needs a real merge commit written to
//! FATFS — the riskiest untested path — and is deferred to increment B; here it
//! bails with a clear message. The single-writer appliance case (one device
//! publishing) is always a fast-forward, so the happy path never hits it.
//!
//! Functions are kept standalone (config via the baked `TW_*` consts, no editor
//! coupling) so increment C can lift `open_or_clone`/`publish`/`push_with_retry`
//! into a reusable `git` module the editor's `Ctrl-G` calls.
//!
//! Build/flash with `just flash-git-sync` (same TW_* `.env` + partition table as
//! `git_push`). Git runs on a dedicated 96 KB thread (see `git_push.rs` / #3).

use std::cell::RefCell;
use std::ffi::CStr;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::{EspSntp, SyncStatus};
use esp_idf_svc::sys::{self, esp};
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{
    CertificateCheckStatus, Commit, Cred, CredentialType, FetchOptions, IndexAddOption,
    PushOptions, RemoteCallbacks, Repository, Signature,
};

/// Injected by build.rs so serial output identifies the exact build.
const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

// Baked in at build time from firmware/.env (see build.rs). Empty when unset;
// checked at runtime so the editor build never depends on them.
const WIFI_SSID: &str = env!("TW_WIFI_SSID");
const WIFI_PASS: &str = env!("TW_WIFI_PASS");
const REMOTE_URL: &str = env!("TW_REMOTE_URL");
const GH_USER: &str = env!("TW_GH_USER");
const PAT: &str = env!("TW_PAT");
const AUTHOR_NAME: &str = env!("TW_AUTHOR_NAME");
const AUTHOR_EMAIL: &str = env!("TW_AUTHOR_EMAIL");

/// flash-FAT partition (partitions.csv) and its VFS mount point.
const FAT_LABEL: &CStr = c"storage";
const MOUNT: &CStr = c"/spiflash";
const MOUNT_STR: &str = "/spiflash";

/// The persistent clone. Unlike `git_push`'s per-boot `wc-<unix>` dirs, this one
/// dir survives reboots (flash-FAT is only formatted on a failed mount): boot 1
/// clones it, every later boot opens it and fast-forwards — the product model.
const REPO_DIR: &str = "/spiflash/repo";
/// The tracked file we append to. Stands in for the editor's note file(s).
const NOTES_FILE: &str = "notes.md";

/// Debug/recovery + the read-only-delete test knob: when true, wipe REPO_DIR and
/// re-clone from scratch every boot instead of opening the existing clone.
/// Deleting the existing clone's objects only succeeds with the esp_stubs
/// p_open/p_creat fix (libgit2 objects are otherwise mode 0444 → AM_RDO →
/// un-deletable on FAT). Ships **false** — the product opens the persistent
/// clone and fast-forwards; flip true to validate the RO-delete fix or to force
/// a clean re-clone.
const RECLONE_EACH_BOOT: bool = false;

/// GitHub's root CAs, embedded so the push can verify the server's TLS chain.
/// Shared with `git_push` (same file). Written to FAT and handed to libgit2 via
/// GIT_OPT_SET_SSL_CERT_LOCATIONS.
const GITHUB_ROOTS_PEM: &str = include_str!("github_roots.pem");
const CA_BUNDLE_PATH: &str = "/spiflash/ca.pem";

/// SNTP first-sync budget (same as Spike 6).
const SNTP_TIMEOUT: Duration = Duration::from_secs(20);

/// Stack for the dedicated git thread — clone/checkout is at least as deep as the
/// init→push chain `git_push` measured (~67 KB), so keep the proven 96 KB. See #3.
const GIT_STACK: usize = 96 * 1024;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Typoena — persistent-clone publish cycle (milestone #2A), {BUILD_TAG}");

    if let Err(e) = run() {
        log::error!("❌ git_sync setup failed: {e:?}");
    }

    // Reached only on a setup error (run() idles forever on the happy path).
    loop {
        FreeRtos::delay_ms(1000);
    }
}

fn run() -> Result<()> {
    if WIFI_SSID.is_empty() {
        bail!("TW_WIFI_SSID is empty — set the network + git TW_* vars in firmware/.env");
    }
    if REMOTE_URL.is_empty() || GH_USER.is_empty() || PAT.is_empty() {
        bail!("TW_REMOTE_URL / TW_GH_USER / TW_PAT must all be set in firmware/.env");
    }

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // Wi-Fi is bound here and never dropped on the happy path (run() idles at the
    // end): dropping EspWifi runs wifi_deinit, which can assert if git left the
    // heap in a bad state and mask the real logs. Keeping the radio up surfaces
    // the true result. (Same rationale as git_push.rs.)
    let _wifi = {
        let mut wifi = BlockingWifi::wrap(
            EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
            sys_loop,
        )?;
        connect_wifi(&mut wifi)?;
        let ip = wifi.wifi().sta_netif().get_ip_info()?;
        log::info!("Wi-Fi up — IP {}", ip.ip);
        wifi
    };

    sync_clock()?;
    mount_fat().context("mounting flash-FAT")?;
    install_tls_trust_store().context("installing TLS trust store")?;

    // Git runs on a dedicated 96 KB thread, not the shared main task (see #3 /
    // git_push.rs). Errors are LOGGED, not propagated, so the radio stays up and
    // the monitor shows the result.
    let git = std::thread::Builder::new()
        .name("git".into())
        .stack_size(GIT_STACK)
        .spawn(publish)
        .context("spawning git thread")?;
    match git.join() {
        Ok(Ok(summary)) => log::info!("✅ publish cycle complete — {summary}"),
        Ok(Err(e)) => log::error!("❌ publish failed: {e:?}"),
        Err(_) => log::error!("❌ git thread panicked — likely stack overflow, raise GIT_STACK"),
    }

    log::info!("idling with Wi-Fi up — press reset to publish again");
    loop {
        FreeRtos::delay_ms(1000);
    }
}

/// The publish cycle (on the dedicated git thread): open-or-clone the persistent
/// repo, record a change, commit on top of the current branch, and fast-forward
/// push it. Returns a one-line summary.
fn publish() -> Result<String> {
    log::info!("publish started — free heap {}", free_heap());

    let (repo, cloned) = open_or_clone()?;
    log::info!(
        "repo ready ({}) — free heap {}",
        if cloned { "cloned" } else { "opened" },
        free_heap()
    );

    // Append a line to a tracked file: a real content change (the editor will
    // write the user's notes here). Create-or-append so it works on the first
    // publish and every one after.
    let unix = now_unix();
    let path = format!("{REPO_DIR}/{NOTES_FILE}");
    let line = format!("- publish @unix {unix} ({BUILD_TAG})\n");
    {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("opening {path}"))?;
        f.write_all(line.as_bytes()).context("appending note")?;
    }
    log::info!("appended a line to {NOTES_FILE}");

    // Stage everything (add --all: also stages deletions, which v0.x note-delete
    // needs) and build the tree.
    let mut index = repo.index().context("opening index")?;
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .context("staging (add --all)")?;
    index.write().context("writing index")?;
    let tree = repo.find_tree(index.write_tree().context("writing tree")?)?;

    // Commit on top of the current branch tip (None on an empty/unborn remote).
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());

    // Nothing-to-publish short-circuit (product spec): if the staged tree matches
    // the parent's, there is nothing new to push.
    if let Some(p) = &parent {
        if p.tree_id() == tree.id() {
            return Ok(format!("nothing to publish (tree unchanged @ {})", short(p.id())));
        }
    }

    let sig = Signature::now(AUTHOR_NAME, AUTHOR_EMAIL).context("building signature")?;
    let message = format!("Typoena publish — unix {unix}");
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
    Ok(format!(
        "published {} → origin/{branch} (persistent clone at {REPO_DIR})",
        short(oid)
    ))
}

/// Open the persistent clone, or clone it on first boot. Returns the repo and
/// whether a clone happened. Clone carries the auth + cert callbacks (the remote
/// may be private, and the TLS chain is verified either way).
fn open_or_clone() -> Result<(Repository, bool)> {
    if RECLONE_EACH_BOOT && Path::new(REPO_DIR).exists() {
        log::warn!("RECLONE_EACH_BOOT — removing {REPO_DIR} to re-clone from scratch");
        fs::remove_dir_all(REPO_DIR)
            .context("RECLONE_EACH_BOOT wipe — deletes libgit2 objects, needs the RO-delete fix")?;
    }
    match Repository::open(REPO_DIR) {
        Ok(repo) => {
            log::info!("opened existing clone at {REPO_DIR}");
            Ok((repo, false))
        }
        Err(_) => {
            // A leftover at REPO_DIR is a partial clone (libgit2 refuses to clone
            // into a non-empty dir, code=Exists). Remove it and re-clone. NOTE:
            // this only succeeds if the leftover is writable — libgit2 marks
            // objects/packs read-only, FATFS won't f_unlink a read-only file
            // (EACCES), and POSIX chmod does NOT clear the attribute on esp-idf's
            // FATFS VFS (verified on hardware). So a clone that got far enough to
            // write objects can't be cleared from here yet; the proper fix is an
            // AM_RDO-clearing unlink shim in esp_stubs.c (increment B, needed for
            // fetch/repack too). Until then the fallback is a one-time wipe:
            // `espflash erase-region 0x310000 0x400000`.
            if Path::new(REPO_DIR).exists() {
                log::warn!("{REPO_DIR} exists but is not a valid repo — removing stale clone");
                fs::remove_dir_all(REPO_DIR).with_context(|| {
                    format!(
                        "removing stale {REPO_DIR} — if this is EACCES (read-only files on \
                         FATFS), erase the storage partition once: \
                         `espflash erase-region 0x310000 0x400000`"
                    )
                })?;
            }
            log::info!("cloning {REMOTE_URL} → {REPO_DIR}");
            let mut fo = FetchOptions::new();
            fo.remote_callbacks(auth_callbacks());
            let repo = RepoBuilder::new()
                .fetch_options(fo)
                .clone(REMOTE_URL, Path::new(REPO_DIR))
                .context("clone")?;
            Ok((repo, true))
        }
    }
}

/// Push `branch` to origin; on a rejected push, fetch origin and reconcile, then
/// retry once. A true divergence (two writers) needs a merge commit and is
/// deferred to increment B — `fetch_and_integrate` bails there.
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
        repo.checkout_head(Some(CheckoutBuilder::new().force()))?;
        return Ok(());
    }
    bail!("origin/{branch} diverged from local — a real merge commit is needed (increment B, deferred)")
}

/// Auth + cert callbacks shared by clone, fetch, and push. Captures only the
/// baked consts, so a fresh set can be built per operation (RemoteCallbacks is
/// consumed by each). The PAT is handed to libgit2 here and never logged.
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

/// First 8 hex chars of an OID, for readable logs.
fn short(oid: git2::Oid) -> String {
    oid.to_string()[..8].to_string()
}

/// Associate with the configured AP and wait for DHCP. Mirrors Spike 6 / git_push.
fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> Result<()> {
    let auth_method = if WIFI_PASS.is_empty() {
        AuthMethod::None
    } else {
        AuthMethod::WPA2Personal
    };
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID.try_into().ok().context("SSID > 32 bytes")?,
        password: WIFI_PASS.try_into().ok().context("password > 64 bytes")?,
        auth_method,
        ..Default::default()
    }))?;
    wifi.start()?;
    log::info!("associating with \"{WIFI_SSID}\"…");
    wifi.connect().context("Wi-Fi association failed")?;
    wifi.wait_netif_up().context("DHCP / netif never came up")?;
    Ok(())
}

/// Kick off SNTP and block until first sync. Required before TLS (cert validity)
/// and before committing (signature timestamp). Mirrors Spike 6 / git_push.
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

/// Mount the flash-FAT `storage` partition at /spiflash, formatting only on a
/// failed mount (so the persistent clone survives reboots). Mirrors git_push.
fn mount_fat() -> Result<()> {
    let cfg = sys::esp_vfs_fat_mount_config_t {
        format_if_mount_failed: true,
        max_files: 16, // libgit2 opens several files at once (index, refs, objects)
        allocation_unit_size: 4096,
        disk_status_check_enable: false,
        use_one_fat: false,
    };
    let mut wl: sys::wl_handle_t = 0;
    esp!(unsafe {
        sys::esp_vfs_fat_spiflash_mount_rw_wl(MOUNT.as_ptr(), FAT_LABEL.as_ptr(), &cfg, &mut wl)
    })
    .context("esp_vfs_fat_spiflash_mount_rw_wl (is the `storage` partition flashed?)")?;

    let (mut total, mut free) = (0u64, 0u64);
    if unsafe { sys::esp_vfs_fat_info(MOUNT.as_ptr(), &mut total, &mut free) } == sys::ESP_OK {
        log::info!(
            "flash-FAT mounted at {MOUNT_STR} — {} KiB total, {} KiB free",
            total / 1024,
            free / 1024
        );
    } else {
        log::info!("flash-FAT mounted at {MOUNT_STR}");
    }
    Ok(())
}

/// Write the embedded GitHub root CAs to FAT and point libgit2's mbedTLS stream
/// at them. Must run after the FAT mount and before any TLS. Mirrors git_push.
fn install_tls_trust_store() -> Result<()> {
    fs::write(CA_BUNDLE_PATH, GITHUB_ROOTS_PEM)
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
