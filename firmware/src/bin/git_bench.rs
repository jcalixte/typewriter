//! git-level micro-benchmark — localizes the ~700 ms/object libgit2 overhead the
//! `:sync` commit split showed (2026-07-12), now that `sd_bench` proved the raw
//! card does a *full* loose-object write (stat+create+write+rename) in ~86 ms.
//! The ~8× gap between that and `write_tree`'s 710 ms lives inside libgit2, not
//! FAT — this bench times the git2 ODB/index primitives in isolation to find it.
//!
//! Read-mostly on `/sd/repo`: the only writes are unreferenced ("orphan") loose
//! blobs — never reachable from a ref, so never pushed, and gc-able — plus
//! rewrites of the existing index/tree (idempotent). Safe on the test card.
//!
//! Flash with `just flash-gitbench` (needs the `git` feature; env in the recipe).

use std::time::Instant;

use anyhow::{Context, Result};
use esp_idf_svc::hal::delay::FreeRtos;
use git2::{IndexEntry, IndexTime, ObjectType, Oid, Repository, Signature};

use firmware::git_sync::GIT_STACK;
use firmware::persistence::{Storage, REPO_DIR};

const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

/// Iterations per op. Small — some ops write to the card, and the first vs rest
/// spread (min vs max) is itself the signal (e.g. write vs freshen-skip). Kept
/// low (3) on the real 570 MB-pack clone so a slow op still finishes in seconds.
const N: usize = 3;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Typoena — git-level bench, {BUILD_TAG}");

    // libgit2 nests ~67 KB of GIT_PATH_MAX stack buffers (postmortem #3), so the
    // git work must run on the same 96 KB stack the real git service uses. On the
    // small main-task stack `index.write()` overflows → nested panic → boot loop.
    let handle = std::thread::Builder::new()
        .name("git_bench".into())
        .stack_size(GIT_STACK)
        .spawn(run)
        .expect("spawn git_bench thread");
    match handle.join() {
        Ok(Ok(())) => log::info!("git_bench: done"),
        Ok(Err(e)) => log::error!("git_bench failed: {e:?}"),
        Err(_) => log::error!("git_bench thread panicked"),
    }
    loop {
        FreeRtos::delay_ms(1000);
    }
}

fn run() -> Result<()> {
    // libgit2 holds the pack + idx (+ commit-graph) fds open and reads loose
    // objects on top; the editor's default 4-FD budget can't cover read_tree.
    let _sd = Storage::mount_for_git().context("mounting SD")?;

    // A 32 MB default mwindow window (mwindow.c) would git__malloc > PSRAM on the
    // real 570 MB pack; small windows keep each p_mmap read cheap, and the
    // esp_map cache keeps them from being re-read on every freshen→refresh.
    // SAFETY: process-global libgit2 options, set once before any repo work.
    unsafe {
        git2::opts::set_mwindow_size(256 * 1024).ok();
        git2::opts::set_mwindow_mapped_limit(4 * 1024 * 1024).ok();
    }

    // Repository open — one-time, but shows the cost of scanning .git (config,
    // refs, ODB backends/packs) which every later op may implicitly refresh.
    let t = Instant::now();
    let repo = Repository::open(REPO_DIR)
        .with_context(|| format!("opening git repo at {REPO_DIR}"))?;
    log::info!("Repository::open           {:.1} ms", t.elapsed().as_micros() as f64 / 1000.0);
    log_map_stats("open");

    // 1) odb.write(blob) in isolation — unique content each iter forces a real
    //    write (no freshen-skip). This is the single number that localizes it: if
    //    ~86 ms the ODB write path is fine and the cost is in the tree/ref layer;
    //    if ~700 ms the cost is inside the ODB write itself (deflate/sha/freshen).
    let odb = repo.odb().context("opening odb")?;
    bench("odb.write(blob)", |i| {
        let data = format!("typoena git_bench orphan blob #{i} — unique so the write is real\n");
        odb.write(ObjectType::Blob, data.as_bytes())
            .map(|_| ())
            .context("odb.write")
    })?;
    log_map_stats("odb.write");

    // 2) on-disk index LOAD (no write). Times loading all ~1179 entries from the
    //    card and prints the count. We deliberately do NOT bench index.write() any
    //    more: it calls truncate_racily_clean, which diffs the whole working tree
    //    against the index and — because a fresh FAT clone makes every entry look
    //    "racy" (2 s mtime granularity) — re-hashes ~170 MB over SPI, up to ~10 min
    //    on this repo (proven 2026-07-12, index.write max 611 s). The fix below
    //    never writes the on-disk index, so that path never runs.
    bench("repo.index() load", |_| {
        repo.index().map(|_| ()).context("index open")
    })?;
    let n_entries = repo.index().map(|i| i.len()).unwrap_or(0);
    log::info!("on-disk index has {n_entries} entries");
    log_map_stats("index load");

    // 3) THE PROPOSED FIX — index-free commit tree (what git_sync::stage_and_commit
    //    will do). Build the new tree from HEAD + one changed file in a FRESH
    //    in-memory index: read_tree(HEAD) leaves stamp=0 so truncate_racily_clean
    //    can NEVER fire; the changed file is written as a blob and added by OID;
    //    write_tree_to writes to the odb WITHOUT touching the on-disk index. Because
    //    read_tree seeds the tree cache and add invalidates only the changed path,
    //    write_tree rebuilds just that path's subtrees — O(changed), not O(1179).
    //    If this is sub-second on the real repo, the fix is validated.
    let head_tree = repo
        .head()?
        .peel_to_commit()
        .context("HEAD → commit")?
        .tree()
        .context("HEAD tree")?;
    // A real path already in the tree, so the add REPLACES it (a realistic edit) and
    // write_tree rebuilds its ancestor subtrees — not just the cheap root case.
    let edit_path: Vec<u8> = {
        let mut seed = git2::Index::new().context("seed index")?;
        seed.read_tree(&head_tree).context("seed read_tree")?;
        seed.get(0)
            .map(|e| e.path)
            .unwrap_or_else(|| b"notes.md".to_vec())
    };
    log::info!(
        "index-free: editing existing path {}",
        String::from_utf8_lossy(&edit_path)
    );

    // read_tree alone: populating the in-memory index from HEAD's tree (reads tree
    // objects through the mmap cache; NO working-file hashing).
    bench("Index::new + read_tree", |_| {
        let mut idx = git2::Index::new().context("Index::new")?;
        idx.read_tree(&head_tree).context("read_tree")?;
        Ok(())
    })?;
    log_map_stats("read_tree");

    // Full index-free staging → tree — this REPLACES add_all + index.write +
    // write_tree (the ~10-min hang) with an O(changed) path.
    bench("index-free stage→tree", |i| {
        let mut idx = git2::Index::new().context("Index::new")?;
        idx.read_tree(&head_tree).context("read_tree")?;
        let data = format!("typoena index-free bench edit #{i}\n");
        let oid = repo.blob(data.as_bytes()).context("write blob")?;
        idx.add(&blob_entry(&edit_path, oid)).context("index.add")?;
        idx.write_tree_to(&repo).map(|_| ()).context("write_tree_to")
    })?;
    log_map_stats("index-free");

    // 6) commit(None, …) — create a commit OBJECT without moving HEAD or writing a
    //    reflog (update_ref = None → an orphan commit, gc-able). Isolates commit-
    //    object creation from the ref-update + reflog cost. Reuses the parent's
    //    tree (no new tree needed); unique message each iter forces a real write.
    let parent = repo.head()?.peel_to_commit().context("HEAD → commit")?;
    let tree = parent.tree().context("parent tree")?;
    let sig = Signature::now("typoena-bench", "bench@typoena.local").context("sig")?;
    bench("commit(None) orphan obj", |i| {
        let msg = format!("typoena git_bench orphan commit #{i}");
        repo.commit(None, &sig, &sig, &msg, &tree, &[&parent])
            .map(|_| ())
            .context("commit(None)")
    })?;
    log_map_stats("commit");

    Ok(())
}

unsafe extern "C" {
    /// Counters from the p_mmap cache in `components/libgit2/esp_map.c`.
    fn esp_map_stats(hits: *mut u32, misses: *mut u32, read_kb: *mut u32, cached_kb: *mut u32);
}

/// Log the p_mmap cache counters — hits vs misses (SD reads avoided), total KB
/// read from the card, and KB currently resident. If the pack-read hypothesis is
/// right, hits climb and `KB read` stops growing across the write ops.
fn log_map_stats(label: &str) {
    let (mut hits, mut misses, mut read_kb, mut cached_kb) = (0u32, 0u32, 0u32, 0u32);
    unsafe { esp_map_stats(&mut hits, &mut misses, &mut read_kb, &mut cached_kb) };
    // Free heap spans PSRAM here; a drop toward 0 during write_tree/commit on the
    // real repo would point at mwindow/idx allocation pressure (or thrash) as the
    // cause of an apparent hang, not CPU.
    let free_kb = unsafe { esp_idf_svc::sys::esp_get_free_heap_size() } / 1024;
    log::info!(
        "mmap cache @ {label:<11} {hits} hit / {misses} miss, {read_kb} KB read, {cached_kb} KB resident, {free_kb} KB heap free"
    );
}

/// Announce, time, and summarize an op. The `→ label …` line prints BEFORE the op
/// runs, so if an op hangs on the real 570 MB-pack repo we can see which one it
/// entered — a bare `summarize` prints only after all N iters, hiding the culprit.
fn bench<F: FnMut(usize) -> Result<()>>(label: &str, op: F) -> Result<()> {
    log::info!("→ {label} …");
    summarize(label, time_each(op)?);
    Ok(())
}

/// A minimal index entry pointing at an already-written blob — for `index.add`,
/// which (unlike `add_frombuffer`) needs no repo owner, so it works on a bare
/// in-memory index. Only `id`, `path` and `mode` feed the tree write.
fn blob_entry(path: &[u8], oid: Oid) -> IndexEntry {
    IndexEntry {
        ctime: IndexTime::new(0, 0),
        mtime: IndexTime::new(0, 0),
        dev: 0,
        ino: 0,
        mode: 0o100644,
        uid: 0,
        gid: 0,
        file_size: 0,
        id: oid,
        flags: 0,
        flags_extended: 0,
        path: path.to_vec(),
    }
}

/// Run `op(i)` for `i in 0..N`, returning each call's wall time in microseconds.
fn time_each<F: FnMut(usize) -> Result<()>>(mut op: F) -> Result<Vec<u64>> {
    let mut times = Vec::with_capacity(N);
    for i in 0..N {
        let t = Instant::now();
        op(i)?;
        times.push(t.elapsed().as_micros() as u64);
    }
    Ok(times)
}

/// Log min / p50 / mean / max in ms for a set of per-call microsecond timings.
fn summarize(label: &str, mut times: Vec<u64>) {
    times.sort_unstable();
    let n = times.len();
    let mean = times.iter().sum::<u64>() / n as u64;
    let ms = |us: u64| us as f64 / 1000.0;
    log::info!(
        "{label:<26} min {:>6.1}  p50 {:>6.1}  mean {:>6.1}  max {:>6.1} ms",
        ms(times[0]),
        ms(times[n / 2]),
        ms(mean),
        ms(times[n - 1]),
    );
}
