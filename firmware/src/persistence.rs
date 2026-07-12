//! SD-card persistence — mount, atomic save, crash recovery.
//!
//! The editor's notes live at `/sd/repo/notes.md` on a FAT filesystem on the
//! microSD card. This module owns bringing that card up and reading/writing the
//! buffer safely across power loss. It is the graduation of the Spike 3 bench
//! binary (`src/bin/sd_fat.rs`): that spike proved the raw stack on hardware
//! (verified 2026-07-11); the proven bits now live here so the editor and the
//! spike share one implementation instead of the spike being a dead-end proof.
//!
//! ## Storage split (ADR-007)
//!
//! FAT-on-SD holds the git working copy (`/sd/repo/`) and local scratch
//! (`/sd/local/`); device config is compiled into the binary in v0.1. The repo
//! is provisioned host-side (`just init` / `just load` copy a clone onto the
//! card) and opened — not cloned — on device, so this module never creates the
//! repo directory: a missing `/sd/repo` means the card wasn't provisioned, which
//! the boot path surfaces as a fatal "re-run `just init`" rather than silently
//! papering over.
//!
//! ## Dedicated SPI3 bus (ADR-012)
//!
//! The card sits on its own SPI3 host (SCK 14, MOSI 15, MISO 13, CS 10). The EPD
//! keeps SPI2. The EPD driver holds an exclusive `spi_device_acquire_bus` lock
//! for its whole lifetime, so a shared bus would lock the SD out; giving the SD
//! its own host sidesteps that for ~2 GPIOs. See the `mount` docs and ADR-012.
//!
//! ## Atomic save + crash recovery (the load-bearing part)
//!
//! FAT gives weak power-loss guarantees, so a save is: write `notes.md.tmp`,
//! `fsync`, unlink the target, rename the tmp over it. On FAT that unlink is
//! mandatory — FatFS's `f_rename` returns `FR_EXIST` on an existing destination
//! (it does *not* replace like POSIX `rename(2)`; Spike 3 finding). That unlink
//! opens a small window where the target is gone while the complete new content
//! sits in the tmp. [`Storage::recover`] closes the loop at boot — see its docs
//! for the exact case analysis, which is subtler than "promote the tmp."

use std::fs;
use std::io::Write as _;
use std::mem::MaybeUninit;
use std::path::Path;
use std::ptr;

use anyhow::{bail, Context, Result};
use esp_idf_svc::sys::{self, esp};

/// SD wiring on its own SPI3 host (ADR-012).
const PIN_SCK: i32 = 14;
const PIN_MOSI: i32 = 15;
const PIN_MISO: i32 = 13;
const PIN_CS: i32 = 10;

/// SD clock. Conservative for bench jumper wires: SDSPI's 20 MHz default is
/// prone to CRC errors on long unterminated jumpers, which look like a stack
/// failure when they're really signal integrity. 10 MHz keeps margin; raise
/// toward 20 MHz on a clean PCB. Init always runs at 400 kHz regardless.
const SD_FREQ_KHZ: i32 = 10_000;

/// Host flags from `sd_protocol_types.h` — `BIT(3)` / `BIT(5)`. Inlined because
/// bindgen doesn't fold the nested `BIT()` macro into a constant.
const SDMMC_HOST_FLAG_SPI: u32 = 1 << 3;
const SDMMC_HOST_FLAG_DEINIT_ARG: u32 = 1 << 5;

/// FAT mount point.
pub const MOUNT: &str = "/sd";
/// Git working copy — provisioned host-side, opened on device.
pub const REPO_DIR: &str = "/sd/repo";
/// The one file v0.1 opens.
pub const NOTES: &str = "/sd/repo/notes.md";
/// Staging name for the atomic save. Two dots → needs long-filename support
/// (`CONFIG_FATFS_LFN_HEAP=y`, set in sdkconfig.defaults).
const NOTES_TMP: &str = "/sd/repo/notes.md.tmp";

/// Largest file [`Storage::load`] will read into the buffer. v0.1 caps notes at
/// 256 KiB; a larger file refuses to open with a clear message rather than
/// exhausting the rope. Saving is *not* capped — never refuse to persist the
/// user's work once it's in the buffer.
pub const MAX_FILE_BYTES: u64 = 256 * 1024;

/// The C mount point (`/sd\0`) for the esp-idf FFI calls.
const MOUNT_C: &std::ffi::CStr = c"/sd";

/// VFS open-file budget for the editor path: it opens only a note and its
/// `*.tmp`, so a tight budget keeps FatFS's per-file buffers off the heap.
const MAX_FILES_EDITOR: i32 = 4;
/// VFS open-file budget for the git tooling. libgit2 keeps the pack + `.idx`
/// (and commit-graph) descriptors open for the repo's lifetime and opens loose
/// objects on top, so a `read_tree` walk overruns [`MAX_FILES_EDITOR`] with a
/// "no free file descriptors" error. Matches the flash-FAT git binaries' 16.
const MAX_FILES_GIT: i32 = 16;

/// A mounted SD card. Holds the live card handle for its lifetime; v0.1 never
/// unmounts (the card stays up for the whole power session). Not `Send` — the
/// handle lives on the task that mounted it (the ui/main task). The git thread
/// reaches `/sd/repo` through plain `std::fs`; FatFS's per-volume reentrancy
/// lock serialises the two, so no extra mutex is needed here.
pub struct Storage {
    card: *mut sys::sdmmc_card_t,
}

/// What [`Storage::recover`] did with a leftover `*.tmp` at boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// No `*.tmp` present — clean shutdown last time.
    Clean,
    /// `*.tmp` and the target both present: the crash could have landed
    /// mid-write, so the tmp is untrustworthy. Kept the committed target,
    /// discarded the tmp. The in-flight (unsaved) edit is lost — the documented
    /// "you get the previous version" behaviour.
    DiscardedTmp,
    /// Only `*.tmp` present: the target had already been unlinked, so the tmp is
    /// the newest complete, fsync'd copy. Promoted it to the target.
    PromotedTmp,
}

impl Storage {
    /// Bring up SPI3 and mount the FAT filesystem at `/sd`, then run crash
    /// recovery ([`Storage::recover`]) so storage is in a consistent state
    /// before the caller reads anything.
    ///
    /// `format_if_mount_failed` is **false**: this is the user's card with their
    /// writing on it, so a transient mount hiccup must never trigger a reformat.
    /// (The Spike 3 bench binary sets it true for convenience on blank cards;
    /// this path must not.)
    pub fn mount() -> Result<Self> {
        Self::mount_with_max_files(MAX_FILES_EDITOR)
    }

    /// Like [`Storage::mount`], but with the larger [`MAX_FILES_GIT`] open-file
    /// budget the git tooling (bench / sync) needs — libgit2 holds several
    /// descriptors open at once, which the editor's default budget can't cover.
    pub fn mount_for_git() -> Result<Self> {
        Self::mount_with_max_files(MAX_FILES_GIT)
    }

    fn mount_with_max_files(max_files: i32) -> Result<Self> {
        // 1) SPI3 with the SD's four lines. Dedicated bus (ADR-012) — no EPD
        //    deselect needed: the panel is on SPI2 and can't contend here.
        // SAFETY: a zeroed spi_bus_config_t is valid (all pins default 0); we
        // set the used pins and mark the quad lines unused (-1).
        let mut bus: sys::spi_bus_config_t = unsafe { MaybeUninit::zeroed().assume_init() };
        bus.__bindgen_anon_1.mosi_io_num = PIN_MOSI;
        bus.__bindgen_anon_2.miso_io_num = PIN_MISO;
        bus.sclk_io_num = PIN_SCK;
        bus.__bindgen_anon_3.quadwp_io_num = -1;
        bus.__bindgen_anon_4.quadhd_io_num = -1;
        bus.max_transfer_sz = 4096;
        esp!(unsafe {
            sys::spi_bus_initialize(
                sys::spi_host_device_t_SPI3_HOST,
                &bus,
                sys::spi_common_dma_t_SPI_DMA_CH_AUTO as _,
            )
        })
        .context("spi_bus_initialize(SPI3)")?;

        // 1b) Internal pull-ups on the SD lines. The SD spec wants ~10 kΩ
        //     pull-ups; bench jumpers have none, so MISO floats between response
        //     bytes and a stray bit reads back as a spurious R1 "illegal
        //     command" that fails init. The ESP32's internal ~45 kΩ pull-ups are
        //     usually enough on short wires; an external 10 kΩ MISO→3V3 is the
        //     proper fix on a real board.
        for pin in [PIN_SCK, PIN_MOSI, PIN_MISO, PIN_CS] {
            esp!(unsafe { sys::gpio_set_pull_mode(pin, sys::gpio_pull_mode_t_GPIO_PULLUP_ONLY) })
                .with_context(|| format!("pull-up on GPIO {pin}"))?;
        }

        // 2) SDSPI host descriptor — hand-rolled SDSPI_HOST_DEFAULT() (bindgen
        //    drops the macro). The fn pointers are esp-idf's sdspi_host_* ops.
        // SAFETY: zeroed is a valid start (all fn-pointer Options = None); we
        // fill exactly the fields the C macro sets.
        let mut host: sys::sdmmc_host_t = unsafe { MaybeUninit::zeroed().assume_init() };
        host.flags = SDMMC_HOST_FLAG_SPI | SDMMC_HOST_FLAG_DEINIT_ARG;
        host.slot = sys::spi_host_device_t_SPI3_HOST as i32;
        host.max_freq_khz = SD_FREQ_KHZ;
        host.io_voltage = 3.3;
        host.driver_strength = sys::sdmmc_driver_strength_t_SDMMC_DRIVER_STRENGTH_B;
        host.current_limit = sys::sdmmc_current_limit_t_SDMMC_CURRENT_LIMIT_200MA;
        host.init = Some(sys::sdspi_host_init);
        host.set_card_clk = Some(sys::sdspi_host_set_card_clk);
        host.do_transaction = Some(sys::sdspi_host_do_transaction);
        host.__bindgen_anon_1.deinit_p = Some(sys::sdspi_host_remove_device);
        host.io_int_enable = Some(sys::sdspi_host_io_int_enable);
        host.io_int_wait = Some(sys::sdspi_host_io_int_wait);
        host.get_real_freq = Some(sys::sdspi_host_get_real_freq);
        host.input_delay_phase = sys::sdmmc_delay_phase_t_SDMMC_DELAY_PHASE_0;
        host.check_buffer_alignment = Some(sys::sdspi_host_check_buffer_alignment);

        // 3) Device (slot) config — CS 10, no card-detect / write-protect / int.
        // SAFETY: zeroed is valid; we set the host, CS, and mark the rest unused.
        let mut slot: sys::sdspi_device_config_t = unsafe { MaybeUninit::zeroed().assume_init() };
        slot.host_id = sys::spi_host_device_t_SPI3_HOST;
        slot.gpio_cs = PIN_CS;
        slot.gpio_cd = -1;
        slot.gpio_wp = -1;
        slot.gpio_int = -1;

        // 4) Mount config. format_if_mount_failed = FALSE — see method docs.
        let mount = sys::esp_vfs_fat_mount_config_t {
            format_if_mount_failed: false,
            max_files,
            allocation_unit_size: 16 * 1024,
            disk_status_check_enable: false,
            use_one_fat: false,
        };

        let mut card: *mut sys::sdmmc_card_t = ptr::null_mut();
        let rc = unsafe {
            sys::esp_vfs_fat_sdspi_mount(MOUNT_C.as_ptr(), &host, &slot, &mount, &mut card)
        };

        // Turn the driver's opaque error into something actionable. The one we
        // hit in practice: a card that rejects CMD59 (SPI-mode CRC on/off) after
        // CMD0/CMD8 succeed. That's a card-firmware limitation (common on
        // large/counterfeit SDXC), not a wiring fault — and we keep CRC required
        // rather than run the user's notes over an unchecked bus.
        if rc == sys::ESP_ERR_NOT_SUPPORTED {
            bail!(
                "SD card rejected CMD59 (SPI-mode CRC). CMD0/CMD8 succeeded, so wiring is \
                 fine — this card's firmware just doesn't support CRC in SPI mode (common on \
                 large/counterfeit SDXC). Use a genuine card, ideally ≤32 GB. We keep CRC \
                 required on purpose: a writing device shouldn't run over an unchecked bus."
            );
        }
        esp!(rc).context("esp_vfs_fat_sdspi_mount (card present? inserted? FAT-formatted?)")?;

        let storage = Storage { card };
        let (max_khz, real_khz) = storage.negotiated_khz();
        log::info!("SD mounted at {MOUNT} — max {max_khz} kHz, negotiated {real_khz} kHz");

        match storage.recover().context("boot crash recovery")? {
            Recovery::Clean => {}
            Recovery::DiscardedTmp => log::warn!(
                "recovery: found {NOTES_TMP} alongside {NOTES} — last save didn't finish; \
                 kept the committed file, discarded the incomplete tmp"
            ),
            Recovery::PromotedTmp => log::warn!(
                "recovery: found {NOTES_TMP} with no {NOTES} — promoted the tmp (it is the \
                 newest complete copy)"
            ),
        }
        Ok(storage)
    }

    /// The card's ceiling and negotiated SPI clock, in kHz (`(max, real)`).
    /// `real` is what SDSPI settled on after init and is the speed reads/writes
    /// actually run at — worth logging on the bench where wiring caps it.
    pub fn negotiated_khz(&self) -> (i32, i32) {
        // SAFETY: `card` is a live handle for the lifetime of `self` (the mount
        // is never torn down while a `Storage` exists).
        unsafe {
            (
                (*self.card).max_freq_khz as i32,
                (*self.card).real_freq_khz as i32,
            )
        }
    }

    /// Total / free bytes on the FAT volume.
    pub fn usage(&self) -> Result<(u64, u64)> {
        let mut total: u64 = 0;
        let mut free: u64 = 0;
        esp!(unsafe { sys::esp_vfs_fat_info(MOUNT_C.as_ptr(), &mut total, &mut free) })
            .context("esp_vfs_fat_info")?;
        Ok((total, free))
    }

    /// Whether the working copy exists. A missing `/sd/repo` means the card
    /// wasn't provisioned (`just init`); the boot path treats that as fatal.
    pub fn repo_present(&self) -> bool {
        Path::new(REPO_DIR).is_dir()
    }

    /// Read `notes.md` into a `String` — the boot default note. Thin wrapper over
    /// [`Storage::load_path`].
    pub fn load(&self) -> Result<String> {
        self.load_path(NOTES)
    }

    /// Read an arbitrary file under `/sd` into a `String`. Returns an empty string
    /// if the file doesn't exist yet (a `:e` of a not-yet-created name, or a fresh
    /// repo). Refuses a file larger than [`MAX_FILE_BYTES`] rather than loading it.
    ///
    /// The multi-file (v0.5) load path: the editor names the file, the host reads
    /// it here and hands the text back through `Editor::install_loaded`.
    pub fn load_path(&self, path: &str) -> Result<String> {
        match fs::metadata(path) {
            Ok(m) if m.len() > MAX_FILE_BYTES => bail!(
                "{path} is {} KiB — over the {} KiB limit; open it on a computer to split it",
                m.len() / 1024,
                MAX_FILE_BYTES / 1024
            ),
            // Read the file verbatim. The editor's `rows = #\n + 1` model renders a
            // trailing '\n' as an empty last line, and we *want* that: a note ends
            // with a visible blank line that reflects its POSIX terminator. Since
            // `save_path` guarantees that terminator, this load and that save form an
            // identity round-trip for any device-written file (which always ends in
            // '\n') — no strip needed, and none wanted.
            Ok(_) => fs::read_to_string(path).with_context(|| format!("reading {path}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e).with_context(|| format!("stat {path}")),
        }
    }

    /// Atomically persist `contents` to `notes.md`. Thin wrapper over
    /// [`Storage::save_path`].
    pub fn save(&self, contents: &str) -> Result<()> {
        self.save_path(NOTES, contents)
    }

    /// Atomically persist `contents` to an arbitrary file under `/sd`: write the
    /// tmp, fsync, unlink the target, rename over it. See the module docs for why
    /// the unlink is mandatory on FAT. Boot recovery ([`Storage::recover`]) still
    /// only covers the default `notes.md`; per-file recovery for the other v0.5
    /// buffers is deferred to the v0.9 crash-safety work — the atomic swap here
    /// already protects each individual save.
    pub fn save_path(&self, path: &str, contents: &str) -> Result<()> {
        let tmp = format!("{path}.tmp");
        {
            let mut f = fs::File::create(&tmp)
                .with_context(|| format!("create {tmp} (does its directory exist?)"))?;
            f.write_all(contents.as_bytes())
                .with_context(|| format!("write {tmp}"))?;
            // Insert a final newline only if the buffer lacks one (POSIX text
            // convention; keeps git from flagging "No newline at end of file").
            // `load_path` reads verbatim, so this is the sole place the terminator is
            // guaranteed — and because it's guarded, the file mirrors the buffer's
            // trailing newlines exactly: one visible trailing blank line stays one,
            // never doubled. A buffer that already ends in '\n' passes through as-is.
            if !contents.ends_with('\n') {
                f.write_all(b"\n")
                    .with_context(|| format!("write final newline to {tmp}"))?;
            }
            // FatFS f_sync — flush the tmp fully before it can replace the target.
            f.sync_all().with_context(|| format!("fsync {tmp}"))?;
        }
        // FatFS f_rename won't overwrite, so unlink the target first (tolerate a
        // missing target: the first-ever save has nothing to remove).
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("unlink {path} before rename")),
        }
        fs::rename(&tmp, path).with_context(|| format!("rename {tmp} -> {path}"))?;
        Ok(())
    }

    /// Unlink a file under `/sd` (`:delete`). Tolerates a missing target — an
    /// already-gone file is a success, so the call is idempotent. Also clears a
    /// stray `{path}.tmp` best-effort, so a crash-interrupted save can't leave the
    /// file half-present after a delete. For a Tracked file this leaves the
    /// working copy short one file; the next publish's `add --all` stages it.
    pub fn delete_path(&self, path: &str) -> Result<()> {
        let _ = fs::remove_file(format!("{path}.tmp"));
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).with_context(|| format!("unlink {path}")),
        }
    }

    /// Reconcile a leftover `notes.md.tmp` at boot. The save sequence is
    /// write-tmp → fsync → unlink-target → rename, so a lingering tmp means the
    /// last save was interrupted. Which way to recover depends on whether the
    /// target survived:
    ///
    /// - **tmp + target both present** — the crash could have been *during* the
    ///   tmp write (before fsync completed), so the tmp may be partial. The
    ///   target is the last fully-committed version. Keep it, delete the tmp.
    ///   Promoting a possibly-partial tmp over good data would be data loss.
    /// - **tmp only, target absent** — the target was already unlinked, so we
    ///   crashed between unlink and rename. The tmp is the newest complete,
    ///   fsync'd copy and the only one left. Promote it (rename over the target).
    /// - **neither / target only** — nothing to do.
    ///
    /// Idempotent and safe to call on every mount; a no-op when `/sd/repo`
    /// doesn't exist (no tmp can be there).
    fn recover(&self) -> Result<Recovery> {
        if fs::metadata(NOTES_TMP).is_err() {
            return Ok(Recovery::Clean);
        }
        if fs::metadata(NOTES).is_ok() {
            fs::remove_file(NOTES_TMP)
                .with_context(|| format!("discard stale {NOTES_TMP}"))?;
            Ok(Recovery::DiscardedTmp)
        } else {
            fs::rename(NOTES_TMP, NOTES)
                .with_context(|| format!("promote {NOTES_TMP} -> {NOTES}"))?;
            Ok(Recovery::PromotedTmp)
        }
    }
}
