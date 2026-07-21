//! The palette's background file index, behind [`app::FileIndex`].
//!
//! `EspFileWalk` owns the file-walk channel: a rewalk spawns a short-lived
//! thread that enumerates the card's openable files and sends the newline-joined
//! path blob back for the run loop's idle branch to feed the editor. Off the UI
//! loop because the walk takes seconds on a big tree and the palette is not
//! mandatory for typing.

use std::sync::mpsc::{channel, Receiver, Sender};

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;

use crate::infrastructure::storage_sd::{LOCAL_DIR, REPO_DIR};

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
///
/// The walk is pinned to the APP core (Core1) at the lowest usable priority — a
/// true background task. The esp-idf main task (the UI loop) runs at priority 1
/// on Core0, but `std::thread` spawns default to priority 5 with **no** core
/// affinity, so the default walk *preempted* the UI loop for its whole multi-
/// second run: a keystroke was buffered but could not be drained or painted until
/// the walk finished. The 2026-07-21 cold-boot trace showed it plainly — `o`
/// enqueued at 6554 ms, first paint at 9834 ms, right after the 5.4 s / 1100-file
/// walk ended (8684 ms): a ~3.3 s type-to-ink lag. Pinning to Core1 keeps the
/// walk off the UI core entirely (the main task is CPU0-pinned by default); the
/// priority floor is belt-and-braces should the scheduler ever float it to Core0.
fn spawn_file_walk(tx: Sender<String>) {
    // Background-task scheduling for the spawn below. `..Default::default()` keeps
    // the esp-idf-version-specific fields (e.g. `stack_alloc_caps` on IDF ≥ 5.3)
    // at their defaults. Explicit 16 KB stack: the default pthread stack (4 KB) is
    // tight for 8 levels of readdir recursion plus FatFS underneath.
    let cfg = ThreadSpawnConfiguration {
        name: None, // the std Builder below carries the "walk" name
        stack_size: 16 * 1024,
        priority: 1,
        inherit: false,
        pin_to_core: Some(Core::Core1),
        ..Default::default()
    };
    if let Err(e) = cfg.set() {
        log::warn!("walk thread cfg (Core1, prio 1) FAILED ({e}); spawning at pthread default");
    }

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

    // Restore the pthread default so later spawns from this (UI) thread aren't
    // silently pinned to Core1 / deprioritised. `Default` reads esp-idf's built-in
    // config, not the one just set above.
    if let Err(e) = ThreadSpawnConfiguration::default().set() {
        log::warn!("restoring default thread cfg FAILED ({e})");
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
/// exhaustion). Same reading `net` logs.
fn internal_free_heap() -> u32 {
    use esp_idf_svc::sys;
    unsafe { sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL) as u32 }
}
