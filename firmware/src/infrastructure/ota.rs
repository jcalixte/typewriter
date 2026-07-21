//! Over-the-air firmware update — the logic behind the editor's `:update`.
//!
//! Runs on the net thread (see [`crate::infrastructure::net`]), which already
//! owns the Wi-Fi modem and has brought the radio + clock up via `ensure_online`
//! before calling in here. No libgit2, no git: just esp-idf's HTTPS client and
//! the A/B OTA machinery ([`EspOta`]). Two GETs against a plain-text manifest —
//! deliberately no JSON parser on-device:
//!
//! ```text
//!   <base>/latest              → the newest release's semver, one line
//!   <base>/typoena-<ver>.bin   → that release's app image
//! ```
//!
//! The image streams straight into the **inactive** OTA slot; the running slot
//! is never touched, so a mid-download power loss is safe — the next boot still
//! runs the current image. Only on a clean `complete()` does the new slot become
//! the boot target; the caller then reboots into it, and the boot path
//! ([`mark_running_firmware_valid`]) self-tests and confirms it, or the
//! bootloader rolls back here (needs `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE`).

use anyhow::{bail, Context, Result};
use embedded_svc::http::Method;
use esp_idf_svc::http::client::{
    Configuration as HttpConfig, EspHttpConnection, FollowRedirectsPolicy,
};
use esp_idf_svc::ota::{EspOta, SlotState};

/// The running firmware's semantic version, baked from the crate version so a
/// bump in `firmware/Cargo.toml` is the single source of truth the update check
/// compares against. Also stamped into the app descriptor for the bootloader
/// (see `CONFIG_APP_PROJECT_VER` in `sdkconfig.defaults`).
pub const FW_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where release artifacts live (no trailing slash). Baked at build time from
/// `TW_UPDATE_BASE_URL` when set, else the public site. A later step can move
/// this to `typoena.conf` alongside the other provisioning fields.
const DEFAULT_UPDATE_BASE_URL: &str = "https://typoena.dev/firmware";

fn update_base_url() -> &'static str {
    option_env!("TW_UPDATE_BASE_URL").unwrap_or(DEFAULT_UPDATE_BASE_URL)
}

/// Check for a newer release and, if one exists, download + install it into the
/// inactive OTA slot (which `complete()` then sets as the boot target).
///
/// Returns `Ok(Some(version))` when a newer image was installed (the caller
/// reboots into it), `Ok(None)` when the running firmware is already current,
/// and `Err` on any transport/flash failure — in which case the running slot is
/// untouched and the device keeps booting the current image.
pub fn run_update() -> Result<Option<String>> {
    let latest = fetch_latest_version().context("checking the latest release")?;
    log::info!("OTA — running {FW_VERSION}, latest available {latest}");

    if !is_newer(&latest, FW_VERSION) {
        return Ok(None);
    }

    let url = format!("{}/typoena-{latest}.bin", update_base_url());
    let written = download_and_install(&url).with_context(|| format!("installing {latest}"))?;
    log::info!("OTA — installed {latest} ({written} bytes); new slot is the boot target");
    Ok(Some(latest))
}

/// GET `<base>/latest` and return the trimmed version line. The manifest is
/// a single short token, so a small bounded read is enough (and caps a bad URL
/// that returns HTML from flooding the heap).
fn fetch_latest_version() -> Result<String> {
    let url = format!("{}/latest", update_base_url());
    let mut conn = http_get(&url)?;
    let status = conn.status();
    if status != 200 {
        bail!("version manifest {url} → HTTP {status}");
    }
    // Drain up to a small cap; the manifest is one line like "0.8.0".
    let mut body = Vec::new();
    let mut buf = [0u8; 64];
    loop {
        let n = conn.read(&mut buf).context("reading version manifest")?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
        if body.len() > 256 {
            bail!("version manifest at {url} is not a short version line");
        }
    }
    let version = core::str::from_utf8(&body)
        .context("version manifest is not UTF-8")?
        .trim()
        .to_string();
    if version.is_empty() {
        bail!("empty version manifest at {url}");
    }
    Ok(version)
}

/// Stream the image at `url` into the inactive OTA slot and finalize it as the
/// boot target. Returns the byte count written. On any error the in-progress
/// `EspOtaUpdate` is dropped, which aborts the write — the running slot and the
/// boot pointer are left untouched.
fn download_and_install(url: &str) -> Result<usize> {
    let mut conn = http_get(url)?;
    let status = conn.status();
    if status != 200 {
        bail!("firmware image {url} → HTTP {status}");
    }

    let mut ota = EspOta::new().context("opening the OTA subsystem")?;
    // Erases the inactive slot and begins the write (esp_ota_begin).
    let mut update = ota
        .initiate_update()
        .context("beginning the OTA write (erase inactive slot)")?;

    let mut buf = [0u8; 4096];
    let mut written = 0usize;
    loop {
        let n = conn.read(&mut buf).context("reading firmware image body")?;
        if n == 0 {
            break;
        }
        update.write(&buf[..n]).context("writing the OTA slot")?;
        written += n;
    }

    if written == 0 {
        bail!("firmware image {url} was empty");
    }
    // esp_ota_end (validates the image) + esp_ota_set_boot_partition.
    update
        .complete()
        .context("finalizing the image (validate + set boot slot)")?;
    Ok(written)
}

/// Open an HTTPS GET, validating the server chain against the bundled roots (the
/// same CA bundle the git push uses) and following redirects (a release asset
/// may 302 to a CDN). Returns the connection with response headers already read,
/// ready for `status()` + `read()`. Mirrors the proven flow in the `wifi_tls`
/// spike (`initiate_request` → `initiate_response`).
fn http_get(url: &str) -> Result<EspHttpConnection> {
    let mut conn = EspHttpConnection::new(&HttpConfig {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        follow_redirects_policy: FollowRedirectsPolicy::FollowAll,
        buffer_size: Some(4096),
        ..Default::default()
    })
    .context("creating the HTTPS connection (TLS init)")?;

    // GitHub (and most hosts) reject requests without a User-Agent.
    conn.initiate_request(Method::Get, url, &[("User-Agent", "typoena-ota")])
        .context("TLS handshake / request send failed")?;
    conn.initiate_response().context("reading response headers")?;
    Ok(conn)
}

/// True if `candidate` is a strictly higher semantic version than `current`.
/// Compares the dotted numeric components left to right; a leading `v` and any
/// pre-release/build suffix (`-`/`+`) are ignored, and an unparseable component
/// sorts as 0 — so a malformed manifest never reads as an upgrade over a valid
/// running version.
fn is_newer(candidate: &str, current: &str) -> bool {
    fn parts(v: &str) -> [u64; 3] {
        let core = v.trim().trim_start_matches('v');
        let core = core.split(['-', '+']).next().unwrap_or(core);
        let mut out = [0u64; 3];
        for (i, seg) in core.split('.').take(3).enumerate() {
            out[i] = seg.trim().parse().unwrap_or(0);
        }
        out
    }
    parts(candidate) > parts(current)
}

/// Confirm the running firmware is healthy so the bootloader keeps it.
///
/// With rollback enabled (`CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE`), an image
/// installed over the air boots in the *pending-verify* state and is rolled back
/// on the next reset UNLESS it marks itself valid. Reaching this call means boot
/// got all the way to cursor-ready — SD mounted, note loaded, panel painting,
/// input running — which is our self-test bar, so we confirm.
///
/// We call `mark_running_slot_valid` ONLY when the running slot is actually in
/// the pending-verify (`Unverified`) state. A USB-flashed image (`just flash-ota`
/// / `just ship`) has empty otadata and reads back as `Unknown`/`Factory`;
/// marking-valid there is a no-op in esp-idf, but it logs `esp_ota_ops: Running
/// firmware is factory` at ERROR level on *every* such boot (until the first
/// OTA) — alarming for a freshly-shipped unit. Gating on `Unverified` keeps that
/// noisy path out of the normal boot log while still confirming a real OTA image.
pub fn mark_running_firmware_valid() {
    let mut ota = match EspOta::new() {
        Ok(ota) => ota,
        Err(e) => {
            log::debug!("OTA — skipping mark-valid; opening the OTA subsystem failed ({e})");
            return;
        }
    };
    match ota.get_running_slot().map(|slot| slot.state) {
        Ok(SlotState::Unverified) => match ota.mark_running_slot_valid() {
            Ok(()) => log::info!("OTA — running slot confirmed valid (rollback cancelled)"),
            Err(e) => {
                log::warn!("OTA — could not confirm running slot ({e}); it may roll back on reset")
            }
        },
        Ok(state) => log::debug!("OTA — running slot is {state:?}; nothing to confirm"),
        Err(e) => log::debug!("OTA — could not read running slot state ({e})"),
    }
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn strictly_higher_is_an_upgrade() {
        assert!(is_newer("0.8.0", "0.7.7"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.7.8", "0.7.7"));
    }

    #[test]
    fn same_or_lower_is_not() {
        assert!(!is_newer("0.7.7", "0.7.7"));
        assert!(!is_newer("0.7.6", "0.7.7"));
        assert!(!is_newer("0.6.9", "0.7.0"));
    }

    #[test]
    fn tolerates_v_prefix_and_suffixes() {
        assert!(is_newer("v0.8.0", "0.7.7"));
        assert!(is_newer("0.8.0-rc1", "0.7.7"));
        assert!(!is_newer("0.7.7+build9", "0.7.7"));
    }

    #[test]
    fn a_garbage_manifest_never_upgrades() {
        assert!(!is_newer("garbage", "0.7.7"));
        assert!(!is_newer("", "0.0.1"));
    }
}
