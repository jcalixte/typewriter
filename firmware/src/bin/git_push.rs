//! Spike 7 — Path 2 finish: on-device `init → commit → push` over mbedTLS HTTPS.
//!
//! Gate D proved the `git2` safe API links and runs on device (SHA1 via
//! mbedTLS). This is the real prize: the full publish path the editor's `git`
//! module will run, end to end on hardware —
//!
//!   1. Wi-Fi assoc + SNTP. A valid wall clock is needed twice: mbedTLS checks
//!      the server cert's validity window, and the commit signature is stamped
//!      with the current time.
//!   2. Mount flash-FAT at /spiflash (partition `storage`, see partitions.csv).
//!      The working copy lives here — the ADR-007 storage question is settled
//!      *for the spike* as flash-FAT (sidesteps the still-unresolved SD card);
//!      SD stays the product plan of record.
//!   3. `git init` a fresh working copy, write a file, stage it, commit with the
//!      configured author (message = a timestamp), and push HEAD to a fresh
//!      per-boot branch `device/<unix>` on the HTTPS remote, PAT in the
//!      credential callback (never logged).
//!
//! Why push to a *fresh* branch and not `add` onto a clone: a fresh `init` each
//! boot has an unrelated history, so pushing onto an existing branch would be a
//! non-fast-forward and drag in merge-unrelated-histories handling. A unique
//! `device/<unix>` branch is always a clean create — it isolates the actual
//! unknown (does the push transport work on device) from history reconciliation.
//! The product will hold a *persistent clone* so real publishes fast-forward;
//! proving clone/fetch on device is a clean follow-up.
//!
//! ## Cert verification
//!
//! libgit2's mbedTLS stream verifies the server chain against whatever CA is set
//! via `GIT_OPT_SET_SSL_CERT_LOCATIONS`: the handshake is `VERIFY_OPTIONAL`, then
//! `verify_server_cert` turns a bad/untrusted chain into `GIT_ECERTIFICATE`. We
//! embed GitHub's root CAs (`github_roots.pem`), write them to `/spiflash/ca.pem`
//! and point libgit2 there (`install_tls_trust_store`). The `certificate_check`
//! callback then returns PASSTHROUGH — "honor libgit2's own result" — which the
//! http transport maps to `is_valid ? 0 : -1` (httpclient.c:805), so an
//! untrusted / MITM cert FAILS the push. Fail-closed, no blanket-accept.
//!
//! Scope note: esp-idf ships a full CA bundle (Spike 6), but it's attached to
//! esp-idf's own mbedtls config, not libgit2's private one — bridging it would
//! mean touching libgit2 sources. Embedding GitHub's roots is the minimal,
//! source-clean trust store; refresh it if GitHub rotates CAs (a product would
//! prefer esp-idf's bundle via a custom subtransport, or fetch roots at
//! provisioning). See ADR-005 / the Spike 7 postmortem.
//!
//! Build/flash with `just flash-git-push` (needs the git TW_* vars in .env).

use std::cell::RefCell;
use std::ffi::CStr;
use std::fs;
use std::io::Write;
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
use git2::{
    CertificateCheckStatus, Cred, CredentialType, IndexAddOption, PushOptions, RemoteCallbacks,
    Repository, Signature,
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

/// GitHub's root CAs, embedded so the push can verify the server's TLS chain
/// (see the "Cert verification" module docs). Written to FAT at runtime and
/// handed to libgit2 via GIT_OPT_SET_SSL_CERT_LOCATIONS.
const GITHUB_ROOTS_PEM: &str = include_str!("github_roots.pem");
/// Where install_tls_trust_store drops the bundle for libgit2's mbedTLS stream
/// to parse (needs a real path — CONFIG_MBEDTLS_FS_IO is on).
const CA_BUNDLE_PATH: &str = "/spiflash/ca.pem";

/// SNTP first-sync budget (same as Spike 6).
const SNTP_TIMEOUT: Duration = Duration::from_secs(20);

fn main() -> Result<()> {
    // Required once before any esp-idf-svc call; some runtime patches only link
    // if this symbol is referenced. See esp-idf-template#71.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Typoena — Spike 7 Path 2 finish (on-device git push), {BUILD_TAG}");

    if let Err(e) = run() {
        log::error!("❌ Spike 7 setup failed: {e:?}");
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

    // Wi-Fi is bound here and, on the happy path, NEVER dropped: run() idles at
    // the end rather than returning. Dropping EspWifi runs wifi_deinit, whose
    // free() asserts if the git work left the heap in a bad state — an earlier
    // revision crashed exactly there (tlsf_free in wifi_deinit), which masked
    // the git-thread logs. Keeping the radio up surfaces the real result.
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

    // Git work runs on the MAIN task, not a spawned thread. libgit2 (via mbedTLS
    // cert validation and FATFS timestamping) calls time()/gettimeofday, whose
    // newlib lock asserts when taken from a Rust std::thread but works on main
    // (Spike 6 ran TLS on main fine). The main stack is sized for it in
    // sdkconfig.defaults (CONFIG_ESP_MAIN_TASK_STACK_SIZE). Errors are LOGGED,
    // not propagated, so the radio stays up and the monitor shows the result.
    match git_publish() {
        Ok(summary) => log::info!("✅ Spike 7 complete — {summary}"),
        Err(e) => log::error!("❌ git_publish failed: {e:?}"),
    }

    log::info!("idling with Wi-Fi up — press reset to re-run");
    loop {
        FreeRtos::delay_ms(1000);
    }
}

/// Associate with the configured AP and wait for DHCP. Mirrors Spike 6.
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
/// and before committing (signature timestamp). Mirrors Spike 6.
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

/// Mount the flash-FAT `storage` partition at /spiflash, formatting on first
/// boot. Unlike the SD spike (which must never reformat a user's card),
/// `format_if_mount_failed = true` is correct here: `storage` is our own blank
/// partition, and a fresh flash needs an initial FAT.
fn mount_fat() -> Result<()> {
    let cfg = sys::esp_vfs_fat_mount_config_t {
        format_if_mount_failed: true,
        max_files: 16, // libgit2 opens several files at once (index, refs, objects)
        allocation_unit_size: 4096,
        disk_status_check_enable: false,
        use_one_fat: false,
    };
    // SAFETY: valid C strings + config; the driver fills `wl` on success.
    let mut wl: sys::wl_handle_t = 0;
    esp!(unsafe {
        sys::esp_vfs_fat_spiflash_mount_rw_wl(MOUNT.as_ptr(), FAT_LABEL.as_ptr(), &cfg, &mut wl)
    })
    .context("esp_vfs_fat_spiflash_mount_rw_wl (is the `storage` partition flashed?)")?;

    let (mut total, mut free) = (0u64, 0u64);
    // Best-effort usage report.
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
/// at them (GIT_OPT_SET_SSL_CERT_LOCATIONS). Must run after the FAT mount and
/// before the push. Once set, libgit2 verifies the server chain itself, so the
/// push fails closed on an untrusted cert (the push callback returns PASSTHROUGH
/// to honor that result — see the module docs).
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

/// The whole publish (on the main task): init a fresh working copy, write a
/// file, commit, and push to a fresh remote branch. Returns a one-line summary.
fn git_publish() -> Result<String> {
    log::info!("git_publish started — free heap {}", free_heap());
    let unix = now_unix();

    // Fresh working copy per boot in a UNIQUE dir — never deleted. libgit2
    // writes loose objects read-only, and FATFS refuses to f_unlink a read-only
    // file (→ EACCES), so a wipe-and-reinit strategy can't clean a prior repo.
    // Unique dirs sidestep that; the 4 MB partition holds many tiny repos, and
    // each boot pushes to its own branch anyway. (Cleanup of old dirs is a
    // product concern, not a spike one.)
    let repo_dir = format!("{MOUNT_STR}/wc-{unix}");
    let repo = Repository::init(&repo_dir).context("git init working copy")?;
    log::info!("init OK at {repo_dir} — free heap {}", free_heap());

    // One tracked file. Content is disposable; it just makes a non-empty tree.
    let path = format!("{repo_dir}/device.md");
    let body = format!("# Typoena on-device publish\n\nunix: {unix}\n{BUILD_TAG}\n");
    fs::File::create(&path)
        .and_then(|mut f| f.write_all(body.as_bytes()))
        .context("writing device.md")?;
    log::info!("wrote device.md");

    // Stage (add --all semantics) and commit with the configured author. Message
    // is the timestamp — the product's `git` module will use a proper ISO-8601
    // string (desktop spike uses chrono); unix seconds keep this bench binary
    // dependency-free.
    let mut index = repo.index().context("opening index")?;
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .context("staging (add --all)")?;
    index.write().context("writing index")?;
    let tree = repo.find_tree(index.write_tree().context("writing tree")?)?;
    log::info!("staged + tree written — free heap {}", free_heap());

    let sig = Signature::now(AUTHOR_NAME, AUTHOR_EMAIL).context("building signature")?;
    let message = format!("Typoena on-device publish — unix {unix}");
    repo.commit(Some("HEAD"), &sig, &sig, &message, &tree, &[])
        .context("creating commit")?;
    let local = repo
        .head()?
        .shorthand()
        .context("HEAD has no branch shorthand")?
        .to_string();
    log::info!("committed to {local} — free heap {}", free_heap());

    // Point origin at the HTTPS remote and push to a fresh per-boot branch.
    let remote_branch = format!("device/{unix}");
    let refspec = format!("refs/heads/{local}:refs/heads/{remote_branch}");
    repo.remote("origin", REMOTE_URL)
        .context("creating origin remote")?;
    log::info!("origin set; pushing {refspec} — free heap {}", free_heap());

    push(&repo, &refspec).with_context(|| format!("pushing {refspec}"))?;

    log::info!(
        "push returned — free heap {}, min-ever {}",
        free_heap(),
        min_free_heap()
    );
    Ok(format!("pushed {local} → origin/{remote_branch} over mbedTLS HTTPS"))
}

/// Push `refspec` to origin over HTTPS. Binds the PAT credential + the (spike)
/// cert-accept callback, and surfaces a server-side ref rejection as an error.
fn push(repo: &Repository, refspec: &str) -> Result<()> {
    let mut remote = repo.find_remote("origin")?;

    // Server-side per-ref status arrives via a callback, NOT as a push() error.
    // An Rc<RefCell<…>> lets the callback own a handle while we read the result
    // after push() returns (the desktop spike uses the same shape).
    let rejection: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let mut cbs = RemoteCallbacks::new();

    cbs.credentials(|_url, _user_from_url, allowed| {
        // GitHub over HTTPS asks for USER_PASS_PLAINTEXT: the PAT is the
        // password. The PAT is handed to libgit2 here and never logged.
        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
            return Cred::userpass_plaintext(GH_USER, PAT);
        }
        Err(git2::Error::from_str(
            "server did not offer USER_PASS_PLAINTEXT — cannot authenticate with a PAT",
        ))
    });

    // Real verification: libgit2's mbedTLS stream checks the server chain
    // against the CA bundle install_tls_trust_store() loaded. PASSTHROUGH tells
    // libgit2 to honor that result — the http transport maps it to
    // `is_valid ? 0 : -1` (httpclient.c:805), so an untrusted cert FAILS the
    // push (fail-closed). No blanket-accept.
    cbs.certificate_check(|_cert, host| {
        log::info!("verifying {host} TLS chain against embedded GitHub CA bundle");
        Ok(CertificateCheckStatus::CertificatePassthrough)
    });

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
    remote.push(&[refspec], Some(&mut opts)).context("push transport")?;

    if let Some(msg) = rejection.borrow().clone() {
        bail!("remote rejected ref: {msg}");
    }
    log::info!("push accepted by remote");
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
