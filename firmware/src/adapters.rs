//! esp-idf adapters implementing the `app` ports.
//!
//! Each type here fulfils one application-layer contract (`app::Storage`,
//! `app::SyncService`, `app::Clock`, `app::System`, `app::FileIndex`) using the
//! firmware's concrete drivers and infrastructure. `main.rs` constructs them at
//! boot and injects them into [`app::Runtime`], which names only the traits.
//!
//! The `git` feature selects the sync/system pair: a full build injects the
//! git-backed [`GitSyncService`] / [`EspSystem`]; a light editor build the no-op
//! [`NullSyncService`] / [`NullSystem`]. The run loop is identical either way.

use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};

use esp_idf_svc::hal::delay::FreeRtos;

use crate::persistence::{Storage as Card, LOCAL_DIR, REPO_DIR};

// ---- Storage --------------------------------------------------------------

/// [`app::Storage`] over the SD/FAT [`Storage`](crate::persistence::Storage).
/// Shared (`Rc`) with the git sync + system adapters, which reach the same card
/// and its dirty journal — all on the single-threaded UI task, so `Rc` suffices.
pub struct SdStorage(pub Rc<Card>);

impl app::Storage for SdStorage {
    fn save_path(&self, path: &str, contents: &str) -> anyhow::Result<()> {
        self.0.save_path(path, contents)
    }
    fn load_path(&self, path: &str) -> anyhow::Result<String> {
        self.0.load_path(path)
    }
    fn delete_path(&self, path: &str) -> anyhow::Result<()> {
        self.0.delete_path(path)
    }
    fn record_last_file(&self, path: &str) {
        self.0.record_last_file(path)
    }
}

// ---- Clock ----------------------------------------------------------------

/// [`app::Clock`] over the esp wall clock and the FreeRtos tick.
pub struct EspClock;

impl app::Clock for EspClock {
    fn today(&self) -> Option<editor::Date> {
        today_date()
    }
    fn idle_yield(&self) {
        FreeRtos::delay_ms(8);
    }
}

/// Today's date from the wall clock, or `None` when the clock is not yet
/// trustworthy. The editor boot path never runs SNTP, so the clock sits at the
/// epoch until a `:gl`/`:gp` sync sets it this power cycle (no battery-backed
/// RTC) — a year before 2020 means "unset". Honours the timezone applied at boot.
fn today_date() -> Option<editor::Date> {
    let mut now: esp_idf_svc::sys::time_t = 0;
    let mut t: esp_idf_svc::sys::tm = unsafe { core::mem::zeroed() };
    // SAFETY: `now`/`t` are valid, owned locals; `time` fills `now`, `localtime_r`
    // fills `t` from it (the reentrant form writes into our `t`, no shared state).
    unsafe {
        esp_idf_svc::sys::time(&mut now);
        esp_idf_svc::sys::localtime_r(&now, &mut t);
    }
    let year = t.tm_year + 1900;
    if year < 2020 {
        return None; // clock unset (still at the epoch) — no sync yet this boot
    }
    Some(editor::Date {
        year,
        month: (t.tm_mon + 1) as u32, // tm_mon is 0-11
        day: t.tm_mday as u32,
    })
}

// ---- FileIndex ------------------------------------------------------------

/// [`app::FileIndex`] — owns the palette file-walk channel. A rewalk spawns a
/// short-lived thread that enumerates the card and sends the path blob back.
pub struct EspFileWalk {
    tx: Sender<String>,
    rx: Receiver<String>,
}

impl EspFileWalk {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Self { tx, rx }
    }
}

impl Default for EspFileWalk {
    fn default() -> Self {
        Self::new()
    }
}

impl app::FileIndex for EspFileWalk {
    fn request_rewalk(&self) {
        spawn_file_walk(self.tx.clone());
    }
    fn poll_result(&self) -> Option<String> {
        self.rx.try_recv().ok()
    }
}

/// Enumerate the palette's openable files: the regular files under `/sd/repo`
/// and `/sd/local`, recursively, as absolute paths — **one newline-joined
/// blob**, not a `Vec<String>`. 1099 paths as individual small `String`s
/// measured 182 KB of *internal* DRAM resident, which starved the SD DMA pool
/// during the first on-device pull (2026-07-14). The blob is seeded past the
/// 16 KB SPIRAM-malloc threshold so it and its growth reallocs land in PSRAM.
/// The editor sorts and dedupes span-side.
fn enumerate_files() -> String {
    let start = std::time::Instant::now();
    // 64 KB seed: comfortably past the 16 KB SPIRAM threshold and roomy enough
    // that a ~1100-file tree never reallocs.
    let mut out = String::with_capacity(64 * 1024);
    let mut count = 0usize;
    for dir in [REPO_DIR, LOCAL_DIR] {
        walk_files(std::path::Path::new(dir), 0, &mut out, &mut count);
    }
    log::info!("file walk: {count} files in {}ms", start.elapsed().as_millis());
    out
}

/// Run [`enumerate_files`] on its own short-lived thread and send the result
/// over `tx`; the run loop's idle branch feeds it to the editor. Off the UI loop
/// because the walk takes seconds on a big tree and the palette is not mandatory
/// for typing.
fn spawn_file_walk(tx: Sender<String>) {
    // Explicit stack: the default pthread stack (4 KB) is tight for 8 levels of
    // readdir recursion plus FatFS underneath.
    let spawned = std::thread::Builder::new()
        .name("walk".into())
        .stack_size(16 * 1024)
        .spawn(move || {
            let dram_before = internal_free_heap();
            let files = enumerate_files();
            let dram_after = internal_free_heap();
            log::info!(
                "file list: internal heap {dram_before} -> {dram_after} ({} KB consumed), blob {} KB",
                dram_before.saturating_sub(dram_after) / 1024,
                files.len() / 1024
            );
            let _ = tx.send(files); // receiver gone = shutting down; nothing to do
        });
    if let Err(e) = spawned {
        log::warn!("file-walk thread spawn FAILED ({e}); palette list not refreshed");
    }
}

/// Depth bound for [`walk_files`] — belt-and-braces against pathological nesting
/// on a hand-edited card; notes trees are a couple of levels deep.
const WALK_MAX_DEPTH: usize = 8;

/// Recursive helper for [`enumerate_files`]: push `dir`'s files onto `out`, then
/// descend. Reads each directory fully before recursing, so only one FatFS
/// directory handle is open at a time regardless of depth — relevant on the
/// FD-bounded SD mount.
fn walk_files(dir: &std::path::Path, depth: usize, out: &mut String, count: &mut usize) {
    if depth > WALK_MAX_DEPTH {
        log::warn!("file walk: {} exceeds depth {WALK_MAX_DEPTH}, skipped", dir.display());
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    // Keep the dirent's own file type — a per-entry `metadata()` stat re-walks
    // the directory by path every time (~32ms/file on the SD card). But the type
    // needs decoding: esp-idf's dirent.h says DT_REG=1 / DT_DIR=2, while std was
    // built against a libc whose generic unix table has DT_FIFO=1 / DT_CHR=2 /
    // DT_DIR=4 / DT_REG=8 — so through std's eyes every card file looks like a
    // "fifo" and every directory a "char device". FAT can't hold fifos or device
    // nodes, so reading fifo-as-file / chardev-as-dir is unambiguous here, and
    // the is_file()/is_dir() arms take over the day the toolchain's libc catches
    // up. A type matching neither pair pays the one stat rather than being dropped.
    use std::os::unix::fs::FileTypeExt;
    let children: Vec<_> = entries
        .flatten()
        .filter_map(|e| e.file_type().ok().map(|t| (e.path(), t)))
        .collect();
    for (path, ftype) in children {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let (is_file, is_dir) = if ftype.is_file() || ftype.is_fifo() {
            (true, false)
        } else if ftype.is_dir() || ftype.is_char_device() {
            (false, true)
        } else {
            match std::fs::metadata(&path) {
                Ok(m) => (m.is_file(), m.is_dir()),
                Err(_) => continue,
            }
        };
        if is_file {
            if let Some(p) = path.to_str() {
                out.push_str(p);
                out.push('\n');
                *count += 1;
            }
        } else if is_dir {
            walk_files(&path, depth + 1, out, count);
        }
    }
}

/// Free internal DRAM (excludes the 8 MB PSRAM pool, which masks DRAM
/// exhaustion). Same reading `git_sync` logs.
fn internal_free_heap() -> u32 {
    use esp_idf_svc::sys;
    unsafe { sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL) as u32 }
}

// ---- SyncService ----------------------------------------------------------

/// [`app::SyncService`] backed by the git thread (the `:gp`/`:gl` transport).
/// Owns the request/outcome channels and a handle to the card's dirty journal,
/// which it takes on publish and settles when the outcome lands.
#[cfg(feature = "git")]
pub struct GitSyncService {
    card: Rc<Card>,
    tx: Sender<crate::git_sync::GitRequest>,
    rx: Receiver<crate::git_sync::GitOutcome>,
}

#[cfg(feature = "git")]
impl GitSyncService {
    pub fn new(
        card: Rc<Card>,
        tx: Sender<crate::git_sync::GitRequest>,
        rx: Receiver<crate::git_sync::GitOutcome>,
    ) -> Self {
        Self { card, tx, rx }
    }
}

#[cfg(feature = "git")]
impl app::SyncService for GitSyncService {
    fn publish(&self) -> app::PublishDispatch {
        use crate::git_sync::{GitRequest, PublishRequest};
        let paths = self.card.take_dirty();
        match self.tx.send(GitRequest::Publish(PublishRequest { paths })) {
            Ok(()) => app::PublishDispatch::Dispatched,
            Err(_) => {
                // Thread gone — nothing will report back, so return the snapshot
                // to pending ourselves.
                self.card.publish_failed();
                app::PublishDispatch::ThreadDown
            }
        }
    }

    fn pull(&self) -> app::PullDispatch {
        use crate::git_sync::GitRequest;
        if self.card.has_dirty() {
            log::info!(":gl refused — dirty journal non-empty; :gp first");
            app::PullDispatch::RefusedDirty
        } else {
            match self.tx.send(GitRequest::Pull) {
                Ok(()) => app::PullDispatch::Dispatched,
                Err(_) => app::PullDispatch::ThreadDown,
            }
        }
    }

    fn poll_outcome(&self) -> Option<app::SyncOutcome> {
        use crate::git_sync::{GitOutcome, PublishOutcome, PullOutcome};
        let outcome = self.rx.try_recv().ok()?;
        Some(match outcome {
            GitOutcome::Publish(o) => {
                // Settle the dirty snapshot this publish took: confirmed
                // published (or up to date) → forget it; failed → back to pending.
                match &o {
                    PublishOutcome::Pushed(_) | PublishOutcome::UpToDate => {
                        self.card.publish_succeeded()
                    }
                    PublishOutcome::Failed(_) => self.card.publish_failed(),
                }
                app::SyncOutcome::Publish(match o {
                    PublishOutcome::Pushed(oid) => app::PublishOutcome::Pushed(oid),
                    PublishOutcome::UpToDate => app::PublishOutcome::UpToDate,
                    PublishOutcome::Failed(reason) => app::PublishOutcome::Failed(reason),
                })
            }
            GitOutcome::Pull(o) => app::SyncOutcome::Pull(match o {
                PullOutcome::Pulled(oid) => app::PullOutcome::Pulled(oid),
                PullOutcome::Rebased(oid) => app::PullOutcome::Rebased(oid),
                PullOutcome::UpToDate => app::PullOutcome::UpToDate,
                PullOutcome::LocalAhead => app::PullOutcome::LocalAhead,
                PullOutcome::Failed(reason) => app::PullOutcome::Failed(reason),
            }),
        })
    }
}

/// [`app::SyncService`] for a light editor build (no `git` feature): publish and
/// pull are inert, logging the same "skipped" line the old inline path did.
#[cfg(not(feature = "git"))]
pub struct NullSyncService;

#[cfg(not(feature = "git"))]
impl app::SyncService for NullSyncService {
    fn publish(&self) -> app::PublishDispatch {
        log::info!(":gp — saved; light build (no `git` feature) — push skipped");
        app::PublishDispatch::Skipped
    }
    fn pull(&self) -> app::PullDispatch {
        log::info!(":gl — light build (no `git` feature) — pull skipped");
        app::PullDispatch::Skipped
    }
    fn poll_outcome(&self) -> Option<app::SyncOutcome> {
        None
    }
}

// ---- System ---------------------------------------------------------------

/// [`app::System`] for a full build: `:setup` writes the boot marker (then the
/// caller reboots into the wizard), and `reboot` restarts the chip.
#[cfg(feature = "git")]
pub struct EspSystem(pub Rc<Card>);

#[cfg(feature = "git")]
impl app::System for EspSystem {
    fn prepare_setup(&self) -> app::SetupDispatch {
        // One-shot marker: the boot gate re-enters the wizard prefilled. The
        // running editor can't reclaim the radio from the git thread, so `:setup`
        // reboots rather than reopening in place.
        match self.0.request_setup() {
            Ok(()) => app::SetupDispatch::Ready,
            Err(e) => {
                log::warn!(":setup marker write failed: {e:#}");
                app::SetupDispatch::MarkerFailed
            }
        }
    }
    fn reboot(&self) -> ! {
        // esp_restart resets the chip and does not return; the loop makes the
        // divergence explicit to the type system.
        loop {
            unsafe { esp_idf_svc::sys::esp_restart() };
        }
    }
}

/// [`app::System`] for a light build: no wizard to reboot into, but `:reboot`
/// still restarts the chip.
#[cfg(not(feature = "git"))]
pub struct NullSystem;

#[cfg(not(feature = "git"))]
impl app::System for NullSystem {
    fn prepare_setup(&self) -> app::SetupDispatch {
        app::SetupDispatch::Unsupported
    }
    fn reboot(&self) -> ! {
        loop {
            unsafe { esp_idf_svc::sys::esp_restart() };
        }
    }
}
