//! Spike 3 — SD card (FAT) on its own SPI3 host, now a thin on-device harness
//! over the real [`firmware::persistence`] module.
//!
//! The raw storage stack (SPI3 bring-up, hand-rolled SDSPI descriptors, the
//! atomic write/fsync/unlink/rename dance) was proven here first and has since
//! graduated into `firmware::persistence` so the editor and this spike share one
//! implementation. This binary now just drives that module on hardware:
//!
//!   1. [`Storage::mount`] — SPI3 (SCK 14, MOSI 15, MISO 13, CS 10; ADR-012) +
//!      FAT mount at `/sd` + boot crash-recovery. `format_if_mount_failed` is
//!      false in the module, so the card must already be FAT-formatted (it is —
//!      Spike 3 formatted it 2026-07-11).
//!   2. Report the card's negotiated clock and FAT usage.
//!   3. Load `/sd/repo/notes.md` (non-destructive) and report its size.
//!   4. Only if there is no notes.md yet (a blank bench card, nothing to lose)
//!      exercise the real [`Storage::save`] → [`Storage::load`] round-trip and
//!      byte-compare. On a provisioned card the write test is skipped so the
//!      user's writing is never clobbered by the bench tool.
//!
//! Flash with `just flash-sd`. Needs no `.env`.

use std::fs;

use anyhow::{bail, Context, Result};
use esp_idf_svc::hal::delay::FreeRtos;

use firmware::persistence::{Storage, MAX_FILE_BYTES, NOTES, REPO_DIR};

/// Injected by build.rs so serial output identifies the exact build.
const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

fn main() -> Result<()> {
    // Required once before any esp-idf-svc call; some runtime patches only link
    // if this symbol is referenced. See esp-idf-template#71.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Typoena — Spike 3 (SD/FAT via firmware::persistence), {BUILD_TAG}");

    match run() {
        Ok(()) => log::info!("✅ Spike 3 complete — persistence::Storage mounts and round-trips"),
        Err(e) => log::error!("❌ Spike 3 failed: {e:?}"),
    }

    // Idle instead of returning, so the result stays on the monitor.
    loop {
        FreeRtos::delay_ms(1000);
    }
}

fn run() -> Result<()> {
    let storage = Storage::mount().context("mounting SD via persistence module")?;

    let (max_khz, real_khz) = storage.negotiated_khz();
    log::info!("card clock — max {max_khz} kHz, negotiated {real_khz} kHz");

    let (total, free) = storage.usage().context("reading FAT usage")?;
    log::info!(
        "FAT usage — {} MiB total, {} MiB free",
        total / (1024 * 1024),
        free / (1024 * 1024)
    );

    if storage.repo_present() {
        log::info!("{REPO_DIR} present (card is provisioned)");
    } else {
        log::warn!("{REPO_DIR} missing — card not provisioned; run `just init` on the host");
    }

    // Read-back is always safe. An empty string means "no notes.md yet".
    let existing = storage.load().context("loading notes.md")?;
    log::info!("notes.md load OK — {} bytes", existing.len());

    if existing.is_empty() {
        write_test(&storage).context("save/load round-trip")?;
    } else {
        log::info!(
            "notes.md already has content — skipping the destructive write test to protect it \
             (v0.1 caps notes at {} KiB)",
            MAX_FILE_BYTES / 1024
        );
    }
    Ok(())
}

/// Exercise the module's real atomic save + load, then confirm the bytes match.
/// Only called when notes.md is empty, so nothing of the user's is at risk.
fn write_test(storage: &Storage) -> Result<()> {
    // The module deliberately never creates the repo dir (a missing one means an
    // unprovisioned card, which the editor treats as fatal). On a blank bench
    // card there's nothing to protect, so create it here as explicit bench setup
    // to give `Storage::save` a directory to write into.
    if !storage.repo_present() {
        log::info!("{REPO_DIR} missing — creating it (bench setup) so the write test can run");
        fs::create_dir_all(REPO_DIR).with_context(|| format!("create {REPO_DIR}"))?;
    }
    // Newline-free, matching a real editor buffer: `save` appends one POSIX
    // terminator and `load` strips one back, so the round-trip is byte-for-byte
    // identical for any payload (one ending in '\n' would too — it's just cleaner
    // to keep the fixture terminator-free, mirroring the buffer convention).
    let payload = format!("typoena spike 3\n{BUILD_TAG}\ndedicated SPI3: SCK14 MOSI15 MISO13 CS10");
    storage.save(&payload).context("Storage::save")?;
    let back = storage.load().context("Storage::load after save")?;
    if back != payload {
        bail!(
            "read-back mismatch: wrote {} bytes, read {} bytes",
            payload.len(),
            back.len()
        );
    }
    log::info!(
        "round-trip OK — {} bytes: save {NOTES} (tmp→fsync→unlink→rename) → load identical",
        payload.len()
    );
    Ok(())
}
