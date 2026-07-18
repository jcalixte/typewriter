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

use std::cell::RefCell;
use std::collections::BTreeSet;
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
/// Device runtime config (Wi-Fi, remote, token, author) — the card is the
/// source of truth, the .env-baked values the fallback. Written by the
/// installer or the on-device wizard; card root, so it's never staged.
pub const CONF_PATH: &str = "/sd/typoena.conf";
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

/// Dirty-path journal — one repo-relative path per line, mirroring the in-RAM
/// dirty set (see [`Storage::take_dirty`]). At the card root, *outside*
/// `/sd/repo`, so it can never itself be committed. Without it a power pull
/// would strand every file saved-but-not-yet-published in that session: the
/// splice commit only visits recorded paths (nothing walks the tree anymore),
/// so an unrecorded change would never reach the remote.
const DIRTY_JOURNAL: &str = "/sd/.typoena-dirty";

/// Last-active-file marker — the absolute path of the buffer that was active
/// when the device powered off, one line. Read at boot when the
/// `open_last_on_boot` pref is set; rewritten by the main loop on every buffer
/// switch. Device *state*, not shared behaviour, so like the dirty journal it
/// lives at the card root, outside `/sd/repo` — never committed, and two
/// devices never fight over one "last file".
const LAST_FILE: &str = "/sd/.typoena-last";

/// `:setup` reboot marker. The running editor can't reclaim the radio from the
/// git thread, so `:setup` writes this and reboots; the boot gate sees it and
/// re-enters the wizard (prefilled from the card conf) even on a configured
/// card. A one-shot: cleared as soon as the boot gate reads it. Card root,
/// outside `/sd/repo` — never committed.
const SETUP_MARKER: &str = "/sd/.typoena-setup";

/// Local scratch — [`REPO_DIR`]'s never-published sibling (mirrors the editor
/// crate's `LOCAL_DIR`). Here it bounds what [`Storage::last_file`] will
/// resume.
pub const LOCAL_DIR: &str = "/sd/local";

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
    /// Repo-relative paths saved or `:delete`d since the last confirmed
    /// publish — the editor-side half of the O(depth) splice commit
    /// (`git_sync::stage_and_commit` visits exactly these paths and nothing
    /// else). Mirrored to [`DIRTY_JOURNAL`] whenever it changes, so the record
    /// survives a power pull. `RefCell` because recording happens inside
    /// `&self` save/delete calls; `Storage` already lives on one task only.
    dirty: RefCell<Dirty>,
}

/// The two halves of the dirty record: `pending` accumulates between syncs;
/// `take_dirty` moves it to `in_flight` for the duration of a publish so a
/// failure can put it back (and a save landing *during* the publish re-enters
/// `pending`, riding the next one). The journal always carries the union.
#[derive(Default)]
struct Dirty {
    pending: BTreeSet<String>,
    in_flight: BTreeSet<String>,
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

        let storage = Storage {
            card,
            dirty: RefCell::new(Dirty::default()),
        };
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
        let carried = storage.load_dirty_journal();
        if carried > 0 {
            log::info!(
                "dirty journal: {carried} unpublished path(s) carried over from a previous \
                 session — the next :gp will commit them"
            );
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
        // Record BEFORE writing: a crash in between leaves an over-approximate
        // journal (the splice of an unchanged path is a no-op), whereas the
        // reverse order could leave a changed file no record ever points at.
        self.record_dirty(path);
        Self::atomic_write(path, contents)
    }

    /// The atomic write primitive behind [`Storage::save_path`] and the dirty
    /// journal: write `{path}.tmp`, fsync, unlink the target, rename over it.
    fn atomic_write(path: &str, contents: &str) -> Result<()> {
        // Make sure the target's directory exists first — a note in a subdir that
        // isn't on the card yet (the first `:inbox` note when `_inbox/` is absent)
        // would otherwise fail at `File::create` below. Guarded on the parent being
        // missing so the common case (and every fixed-path caller under `/sd`, whose
        // parent always exists) skips the FatFS `mkdir` entirely.
        if let Some(parent) = Path::new(path).parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create parent dir for {path}"))?;
            }
        }
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

    /// Persist the device conf (wizard `WriteConf`). Same atomic swap as
    /// [`Storage::save_path`] but never journaled — `typoena.conf` is card
    /// infrastructure, not a note for `:gp` to publish.
    pub fn write_conf(&self, contents: &str) -> Result<()> {
        Self::atomic_write(CONF_PATH, contents)
    }

    /// Drop the `:setup` reboot marker. The editor calls this (then reboots)
    /// so the next boot re-enters the wizard prefilled — the running editor
    /// can't reclaim the radio from the git thread to run it inline.
    pub fn request_setup(&self) -> Result<()> {
        Self::atomic_write(SETUP_MARKER, "1\n")
    }

    /// Whether a `:setup` reboot is pending (marker present).
    pub fn setup_requested(&self) -> bool {
        Path::new(SETUP_MARKER).is_file()
    }

    /// Clear the `:setup` marker — the boot gate calls this as soon as it reads
    /// it, so the trigger fires exactly once. Best-effort: a stale marker only
    /// costs re-entering setup once more, never data.
    pub fn clear_setup_request(&self) {
        if let Err(e) = fs::remove_file(SETUP_MARKER) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("could not clear {SETUP_MARKER}: {e}");
            }
        }
    }

    /// Factory reset (wizard `Effect::FactoryReset`): erase the card back to a
    /// blank-card state — the git working copy, the local scratch, the device
    /// conf, the TLS trust store, and every `.typoena-*` marker. `progress`
    /// gets a coarse line per stage for the panel; the repo delete is minutes
    /// on FAT-over-SPI (~1100 files). The caller reboots after: the next boot
    /// sees an unconfigured card and runs the first-boot wizard.
    ///
    /// Order is load-bearing for power-loss safety. The repo tree is removed
    /// **first** and the conf **last**, so a power-pull at any point still
    /// reads as unconfigured at boot (repo missing, or a required conf field
    /// gone) and re-enters the wizard — never a normal boot pointing at a
    /// half-erased card.
    pub fn factory_reset(&self, progress: &mut dyn FnMut(&str)) -> Result<()> {
        progress("erasing your notes");
        Self::remove_tree(Path::new(REPO_DIR)).context("erasing the repo")?;
        progress("erasing local scratch");
        Self::remove_tree(Path::new(LOCAL_DIR)).context("erasing local scratch")?;
        progress("clearing settings");
        // ca.pem is git_sync's embedded trust store (written on the SD root); a
        // blank card should not carry it. The markers are card infrastructure.
        for p in [CONF_PATH, "/sd/ca.pem", DIRTY_JOURNAL, LAST_FILE, SETUP_MARKER] {
            match fs::remove_file(p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e).with_context(|| format!("removing {p}")),
            }
        }
        // Drop the in-RAM dirty set too, so nothing lingers if the caller does
        // not reboot immediately.
        *self.dirty.borrow_mut() = Dirty::default();
        Ok(())
    }

    /// Erase just the git working copy — the `:setup` repo-switch delete phase
    /// (v0.9 slice 5c). Removes `/sd/repo` with the FAT-safe [`Storage::remove_tree`],
    /// leaving the conf, `/sd/local`, and the markers in place. Idempotent (a
    /// missing repo is success), so a retry after a partial delete is safe. The
    /// caller only reaches here past the wizard's mandatory dirty guard, so
    /// nothing unpublished is lost. Minutes on FAT (~1100 files) — the wizard
    /// shows a progress line while it runs.
    pub fn wipe_repo(&self) -> Result<()> {
        Self::remove_tree(Path::new(REPO_DIR)).context("removing the old repo")
    }

    /// `fs::remove_dir_all` replacement for FAT. std's version trusts the
    /// dirent file type, and the prebuilt std decodes esp-idf's DT constants
    /// with the generic-unix table (files read as fifos, directories as char
    /// devices — the same story as the palette walk in main.rs). It therefore
    /// `unlink`s subdirectories, which FatFS refuses with FR_DENIED when
    /// they're non-empty. Decode the type the same both-tables way and recurse
    /// ourselves. A missing `dir` is success (idempotent). Mirrors the proven
    /// `remove_tree` in the `sd_bench` / `git_sync` binaries.
    fn remove_tree(dir: &Path) -> Result<()> {
        use std::os::unix::fs::FileTypeExt;
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e).with_context(|| format!("read_dir {}", dir.display())),
        };
        let children: Vec<_> = entries
            .flatten()
            .filter_map(|e| e.file_type().ok().map(|t| (e.path(), t)))
            .collect();
        for (path, ftype) in children {
            let is_dir = if ftype.is_dir() || ftype.is_char_device() {
                true
            } else if ftype.is_file() || ftype.is_fifo() {
                false
            } else {
                fs::metadata(&path)?.is_dir()
            };
            if is_dir {
                Self::remove_tree(&path)?;
            } else {
                fs::remove_file(&path).with_context(|| format!("unlink {}", path.display()))?;
            }
        }
        fs::remove_dir(dir).with_context(|| format!("rmdir {}", dir.display()))
    }

    /// Unlink a file under `/sd` (`:delete`). Tolerates a missing target — an
    /// already-gone file is a success, so the call is idempotent. Also clears a
    /// stray `{path}.tmp` best-effort, so a crash-interrupted save can't leave the
    /// file half-present after a delete. For a Tracked file this leaves the
    /// working copy short one file; the next publish's `add --all` stages it.
    pub fn delete_path(&self, path: &str) -> Result<()> {
        // Same record-first rule as `save_path`: the splice treats a recorded
        // path with no file behind it as "remove from the tree".
        self.record_dirty(path);
        let _ = fs::remove_file(format!("{path}.tmp"));
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).with_context(|| format!("unlink {path}")),
        }
    }

    /// Record `path` as the last-active buffer ([`LAST_FILE`]), so
    /// `open_last_on_boot` can resume it. Best-effort: a failed marker write
    /// costs "boot where you left off", not data, so it logs instead of
    /// erroring the main loop.
    pub fn record_last_file(&self, path: &str) {
        if let Err(e) = Self::atomic_write(LAST_FILE, path) {
            log::warn!("last-file marker not written ({e:#})");
        }
    }

    /// The recorded last-active file, if it still names a real note: a path
    /// under [`REPO_DIR`] or [`LOCAL_DIR`] whose file exists. Anything else —
    /// no marker yet, garbage from an interrupted write, or a file deleted
    /// since (here, or on another device via `:gl`) — is `None`, and boot
    /// falls back to the default note.
    pub fn last_file(&self) -> Option<String> {
        let raw = fs::read_to_string(LAST_FILE).ok()?;
        let path = raw.trim();
        let in_scope = path
            .strip_prefix(REPO_DIR)
            .or_else(|| path.strip_prefix(LOCAL_DIR))
            .and_then(|r| r.strip_prefix('/'))
            .is_some_and(|r| !r.is_empty());
        if !in_scope {
            return None;
        }
        let is_file = fs::metadata(path).map(|m| m.is_file()).unwrap_or(false);
        is_file.then(|| path.to_string())
    }

    /// Note a working-copy file as (possibly) differing from HEAD. Paths
    /// outside `/sd/repo` (`/sd/local`, `/sd/ca.pem`, the journal itself) are
    /// not git's business and are skipped. The journal is rewritten only when
    /// the set actually grows, so re-saving the same note between syncs costs
    /// no extra card I/O.
    fn record_dirty(&self, abs_path: &str) {
        let Some(rel) = abs_path
            .strip_prefix(REPO_DIR)
            .and_then(|r| r.strip_prefix('/'))
        else {
            return;
        };
        if rel.is_empty() {
            return;
        }
        let grew = self.dirty.borrow_mut().pending.insert(rel.to_string());
        if grew {
            self.persist_dirty();
        }
    }

    /// Whether any saved-but-unpublished paths are recorded (pending or riding
    /// an in-flight publish). `:gl` refuses to pull while this is true: a
    /// fast-forward checkout would fight those files, and `:gp` first is the
    /// single-writer appliance's natural order anyway.
    pub fn has_dirty(&self) -> bool {
        let d = self.dirty.borrow();
        !d.pending.is_empty() || !d.in_flight.is_empty()
    }

    /// Snapshot the dirty paths for a publish (repo-relative). The snapshot
    /// moves to `in_flight` — the journal keeps carrying it — until the UI
    /// task reports the outcome: [`Storage::publish_succeeded`] forgets it,
    /// [`Storage::publish_failed`] returns it to pending for the next `:gp`.
    pub fn take_dirty(&self) -> BTreeSet<String> {
        let mut d = self.dirty.borrow_mut();
        let taken = std::mem::take(&mut d.pending);
        d.in_flight.extend(taken.iter().cloned());
        taken
    }

    /// The publish that took the last snapshot committed (or confirmed
    /// up-to-date): drop its paths and shrink the journal. Anything saved
    /// while it ran is still in `pending` and rides the next sync.
    pub fn publish_succeeded(&self) {
        self.dirty.borrow_mut().in_flight.clear();
        self.persist_dirty();
    }

    /// The publish failed: return its snapshot to pending so the next `:gp`
    /// retries it (the splice is idempotent, so a retry of an already-clean
    /// path is free). The journal already carries these paths — no rewrite.
    pub fn publish_failed(&self) {
        let mut d = self.dirty.borrow_mut();
        let inflight = std::mem::take(&mut d.in_flight);
        d.pending.extend(inflight);
    }

    /// Mirror `pending ∪ in_flight` to [`DIRTY_JOURNAL`], atomically.
    /// Best-effort: a failed journal write must not fail the save that
    /// triggered it — the set stays correct in RAM and the journal heals on
    /// the next change.
    fn persist_dirty(&self) {
        let contents = {
            let d = self.dirty.borrow();
            let mut out = String::new();
            for p in d.pending.union(&d.in_flight) {
                out.push_str(p);
                out.push('\n');
            }
            out
        };
        if let Err(e) = Self::atomic_write(DIRTY_JOURNAL, &contents) {
            log::warn!("dirty journal write FAILED ({e:#}); set kept in RAM only");
        }
    }

    /// Seed the dirty set from the journal at mount — the paths a previous
    /// session saved but never got confirmed as published (power pull, failed
    /// sync, or simply no `:gp` before shutdown). Returns how many.
    fn load_dirty_journal(&self) -> usize {
        let Ok(text) = fs::read_to_string(DIRTY_JOURNAL) else {
            return 0; // no journal yet — nothing carried over
        };
        let mut d = self.dirty.borrow_mut();
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            d.pending.insert(line.to_string());
        }
        d.pending.len()
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
