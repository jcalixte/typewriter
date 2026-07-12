//! git-level micro-benchmark — localizes the ~700 ms/object libgit2 overhead the
//! `:sync` commit split showed (2026-07-12), now that `sd_bench` proved the raw
//! card does a *full* loose-object write (stat+create+write+rename) in ~86 ms.
//! The ~8× gap between that and `write_tree`'s 710 ms lives inside libgit2, not
//! FAT — this bench times the git2 ODB/index primitives in isolation to find it.
//!
//! HEADLINE OP (since the 2026-07-12 real-repo run): `splice stage→tree` — the
//! O(depth) TreeBuilder walk that replaces the index-based commit entirely
//! (docs/notes/sync-commit-handoff.md). It runs FIRST so its first iteration is
//! the cold number; acceptance bar: **sub-second cold on the real 570 MB-pack
//! clone, heap staying healthy**. The index paths it supersedes run LAST, for
//! regression tracking — they previously OOM'd, and a late crash can't cost the
//! splice data.
//!
//! Read-mostly on `/sd/repo`: the only writes are unreferenced ("orphan") loose
//! blobs/trees/commits — never reachable from a ref, so never pushed, and
//! gc-able. Safe on the test card.
//!
//! Flash with `just flash-gitbench` (needs the `git` feature; env in the recipe).

use std::time::Instant;

use anyhow::{Context, Result};
use esp_idf_svc::hal::delay::FreeRtos;
use git2::{IndexEntry, IndexTime, ObjectType, Oid, Repository, Signature, Tree};

use firmware::git_sync::GIT_STACK;
use firmware::persistence::{Storage, REPO_DIR};

const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

/// Iterations per op. Small — some ops write to the card, and the first vs rest
/// spread (min vs max) is itself the signal (e.g. cold vs warm, write vs
/// freshen-skip). Kept low (3) on the real 570 MB-pack clone so a slow op still
/// finishes in seconds.
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

    // 1) THE FIX — `splice stage→tree`, the O(depth) TreeBuilder walk
    //    (docs/notes/sync-commit-handoff.md): patch the edited file's ancestor
    //    subtree chain onto HEAD's tree; never materialise the 1179-entry index,
    //    never index.write(), never read_tree the whole tree. Runs FIRST so
    //    iteration #1 is genuinely cold (only `open` has touched the pack).
    let head_tree = repo
        .head()?
        .peel_to_commit()
        .context("HEAD → commit")?
        .tree()
        .context("HEAD tree")?;
    // A nested path that already exists in HEAD's tree, found by an O(depth)
    // descent — NOT read_tree, which is itself the 77 s op — so the splice
    // REPLACES a real file and rebuilds a real ancestor chain, not just the root.
    let edit_path = find_edit_path(&repo, &head_tree)?;
    log::info!(
        "splice: editing {} (depth {})",
        edit_path.join("/"),
        edit_path.len()
    );

    // The blob write is inside the timing: the real commit pays blob + trees, and
    // it keeps the number comparable to `index-free stage→tree` below. Not
    // measured here: the ref/reflog update (commit(Some("HEAD"))) — flat FAT
    // writes, ~350 ms on the toy repo.
    bench("splice stage→tree", |i| {
        let data = format!("typoena splice bench edit #{i}\n");
        let oid = repo.blob(data.as_bytes()).context("write blob")?;
        let parts: Vec<&str> = edit_path.iter().map(String::as_str).collect();
        splice(&repo, Some(&head_tree), &parts, Some(oid)).map(|_| ())
    })?;
    log_map_stats("splice");

    // 2) commit(None, …) — create a commit OBJECT without moving HEAD or writing a
    //    reflog (update_ref = None → an orphan commit, gc-able). Isolates commit-
    //    object creation from the ref-update + reflog cost; splice + this projects
    //    the full real-repo commit. Reuses the parent's tree (no new tree needed);
    //    unique message each iter forces a real write.
    let parent = repo.head()?.peel_to_commit().context("HEAD → commit")?;
    let sig = Signature::now("typoena-bench", "bench@typoena.local").context("sig")?;
    bench("commit(None) orphan obj", |i| {
        let msg = format!("typoena git_bench orphan commit #{i}");
        repo.commit(None, &sig, &sig, &msg, &head_tree, &[&parent])
            .map(|_| ())
            .context("commit(None)")
    })?;
    log_map_stats("commit");

    // 3) odb.write(blob) in isolation — unique content each iter forces a real
    //    write (no freshen-skip). If ~100 ms the ODB write path is fine and any
    //    slow op above is in the tree/ref layer; if ~1 s the cost is inside the
    //    ODB write itself (deflate/sha/freshen) and the mmap cache regressed.
    let odb = repo.odb().context("opening odb")?;
    bench("odb.write(blob)", |i| {
        let data = format!("typoena git_bench orphan blob #{i} — unique so the write is real\n");
        odb.write(ObjectType::Blob, data.as_bytes())
            .map(|_| ())
            .context("odb.write")
    })?;
    log_map_stats("odb.write");

    // 3b) LOCALIZE the commit-vs-blob gap. The fast-seek A/B (2026-07-12) left
    //     `commit(None)` at 1.7 s while `odb.write` dropped to ~0.4 s — commit
    //     additionally VALIDATES its parent + tree OIDs against the odb (strict
    //     object creation → pack header resolves) and freshens the packed tree.
    //     Price the two suspects, then re-bench commit + splice with strict
    //     creation OFF. If commit collapses toward odb.write, validation was the
    //     gap — and git_sync can ship with strict off (every OID it inserts
    //     comes from HEAD's tree or a blob it just wrote).
    let parent_id = parent.id();
    let tree_id = head_tree.id();
    bench("odb.read_header(packed)", |i| {
        let id = if i % 2 == 0 { tree_id } else { parent_id };
        odb.read_header(id).map(|_| ()).context("read_header")
    })?;
    bench("odb.exists(missing)", |i| {
        let id = Oid::from_str(&format!("{:040x}", 0xdead_beef_u64 + i as u64))?;
        let _ = odb.exists(id); // miss → freshen fails → git_odb_refresh path
        Ok(())
    })?;
    log_map_stats("probes");

    // Process-global libgit2 flag; this bench owns the process.
    git2::opts::strict_object_creation(false);
    bench("commit(None) [strict off]", |i| {
        let msg = format!("typoena git_bench strict-off commit #{i}");
        repo.commit(None, &sig, &sig, &msg, &head_tree, &[&parent])
            .map(|_| ())
            .context("commit strict-off")
    })?;
    bench("splice [strict off]", |i| {
        let data = format!("typoena splice strict-off edit #{i}\n");
        let oid = repo.blob(data.as_bytes()).context("write blob")?;
        let parts: Vec<&str> = edit_path.iter().map(String::as_str).collect();
        splice(&repo, Some(&head_tree), &parts, Some(oid)).map(|_| ())
    })?;
    git2::opts::strict_object_creation(true);
    log_map_stats("strict-off");

    // 4) on-disk index LOAD (no write). Times loading all ~1179 entries from the
    //    card and prints the count. We deliberately do NOT bench index.write():
    //    it calls truncate_racily_clean, which diffs the whole working tree
    //    against the index and — because a fresh FAT clone makes every entry look
    //    "racy" (2 s mtime granularity) — re-hashes ~170 MB over SPI, up to ~10 min
    //    on this repo (proven 2026-07-12, index.write max 611 s). The splice
    //    never touches the on-disk index, so that path never runs.
    bench("repo.index() load", |_| {
        repo.index().map(|_| ()).context("index open")
    })?;
    let n_entries = repo.index().map(|i| i.len()).unwrap_or(0);
    log::info!("on-disk index has {n_entries} entries");
    log_map_stats("index load");

    // 5) REFUTED ALTERNATIVE — the index-free in-memory-index commit
    //    (read_tree(HEAD) + add + write_tree_to). It dodges truncate_racily_clean
    //    but is still O(N_tree): the 2026-07-12 real-repo run measured ~77 s for
    //    the cold read_tree and drove the mmap cache to 7.4 MB (zlib OOM). Kept
    //    for regression tracking, run LAST so a crash here can't cost the splice
    //    data above. The cold read_tree is now timed explicitly (the 77 s was
    //    previously visible only via log timestamps); the ops above warmed only
    //    ~depth of the ~158 tree windows, so this is still ~cold.
    let t = Instant::now();
    {
        let mut idx = git2::Index::new().context("Index::new")?;
        idx.read_tree(&head_tree).context("seed read_tree")?;
    }
    log::info!(
        "seed read_tree(HEAD) cold  {:.1} ms",
        t.elapsed().as_micros() as f64 / 1000.0
    );
    log_map_stats("read_tree");

    // Warm repeats: windows resident → pure CPU + cache lookups.
    bench("Index::new + read_tree", |_| {
        let mut idx = git2::Index::new().context("Index::new")?;
        idx.read_tree(&head_tree).context("read_tree")?;
        Ok(())
    })?;

    let edit_path_bytes = edit_path.join("/").into_bytes();
    bench("index-free stage→tree", |i| {
        let mut idx = git2::Index::new().context("Index::new")?;
        idx.read_tree(&head_tree).context("read_tree")?;
        let data = format!("typoena index-free bench edit #{i}\n");
        let oid = repo.blob(data.as_bytes()).context("write blob")?;
        idx.add(&blob_entry(&edit_path_bytes, oid)).context("index.add")?;
        idx.write_tree_to(&repo).map(|_| ()).context("write_tree_to")
    })?;
    log_map_stats("index-free");

    Ok(())
}

/// PROTOTYPE of the real fix (destined for `git_sync::stage_and_commit`): return
/// a new tree OID equal to `base` with `path` set to `new` — `Some(blob)` to
/// add/replace, `None` to delete. Reads ~depth subtree objects, writes ~depth
/// trees; every other entry (all 1179 files, the 150 MB of images) is carried
/// forward by OID without ever being read. `base = None` builds a fresh subtree
/// chain (new file in a new directory). The git_sync version must additionally
/// drop a directory entry when a delete empties its subtree; the bench only
/// exercises replace.
fn splice(repo: &Repository, base: Option<&Tree>, path: &[&str], new: Option<Oid>) -> Result<Oid> {
    let (head, rest) = path.split_first().context("splice: empty path")?;
    let mut tb = repo.treebuilder(base).context("treebuilder")?;
    if rest.is_empty() {
        match new {
            Some(oid) => {
                tb.insert(*head, oid, 0o100644).context("insert blob")?;
            }
            None => {
                let _ = tb.remove(*head); // already absent ⇒ nothing to delete
            }
        }
    } else {
        let sub = match base.and_then(|b| b.get_name(head)) {
            Some(e) if e.kind() == Some(ObjectType::Tree) => {
                Some(repo.find_tree(e.id()).context("loading subtree")?)
            }
            _ => None, // no such dir yet (or a blob in the way): build from empty
        };
        let new_sub = splice(repo, sub.as_ref(), rest, new)?;
        tb.insert(*head, new_sub, 0o040000).context("insert subtree")?;
    }
    tb.write().context("treebuilder write")
}

/// Find a real file to "edit": descend the first subtree at each level (capped),
/// then take the first blob of the deepest tree reached. Reads O(depth) tree
/// objects — never `read_tree`/materialise the whole tree (that's the 77 s op
/// this bench exists to retire).
fn find_edit_path(repo: &Repository, root: &Tree) -> Result<Vec<String>> {
    let mut path = Vec::new();
    let mut cur_id = root.id();
    for _ in 0..6 {
        let cur = repo.find_tree(cur_id).context("descending tree")?;
        match cur.iter().find(|e| e.kind() == Some(ObjectType::Tree)) {
            Some(sub) => {
                path.push(sub.name().context("non-utf8 tree name")?.to_string());
                cur_id = sub.id();
            }
            None => break,
        }
    }
    let cur = repo.find_tree(cur_id).context("leaf tree")?;
    let blob = cur
        .iter()
        .find(|e| e.kind() == Some(ObjectType::Blob))
        .context("no blob along the first-subtree chain — pick an edit path manually")?;
    path.push(blob.name().context("non-utf8 blob name")?.to_string());
    Ok(path)
}

unsafe extern "C" {
    /// Counters from the p_mmap emulation in `components/libgit2/esp_map.c`.
    /// Post cache-removal: `hits` is always 0, `misses` counts every mapping,
    /// `cached_kb` reports the LIVE mapped bytes (the mwindow working set).
    fn esp_map_stats(hits: *mut u32, misses: *mut u32, read_kb: *mut u32, cached_kb: *mut u32);
}

/// Log the p_mmap counters — mappings performed, total KB read from the card,
/// and KB currently live-mapped (should track mwindow's open windows and stay
/// well under MWINDOW_MAPPED_LIMIT now that munmap frees immediately).
fn log_map_stats(label: &str) {
    let (mut hits, mut misses, mut read_kb, mut cached_kb) = (0u32, 0u32, 0u32, 0u32);
    unsafe { esp_map_stats(&mut hits, &mut misses, &mut read_kb, &mut cached_kb) };
    let _ = hits; // always 0 since the cache removal; slot kept for ABI stability
    // Free heap spans PSRAM here; a drop toward 0 during write_tree/commit on the
    // real repo would point at mwindow/idx allocation pressure (or thrash) as the
    // cause of an apparent hang, not CPU.
    let free_kb = unsafe { esp_idf_svc::sys::esp_get_free_heap_size() } / 1024;
    log::info!(
        "mmap @ {label:<11} {misses} maps, {read_kb} KB read, {cached_kb} KB live, {free_kb} KB heap free"
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
