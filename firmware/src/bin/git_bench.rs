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
/// spread (min vs max) is itself the signal (e.g. write vs freshen-skip).
const N: usize = 10;

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
    let _sd = Storage::mount().context("mounting SD")?;

    // Repository open — one-time, but shows the cost of scanning .git (config,
    // refs, ODB backends/packs) which every later op may implicitly refresh.
    let t = Instant::now();
    let repo = Repository::open(REPO_DIR)
        .with_context(|| format!("opening git repo at {REPO_DIR}"))?;
    log::info!("Repository::open           {:.1} ms", t.elapsed().as_micros() as f64 / 1000.0);

    // 1) odb.write(blob) in isolation — unique content each iter forces a real
    //    write (no freshen-skip). This is the single number that localizes it: if
    //    ~86 ms the ODB write path is fine and the cost is in the tree/ref layer;
    //    if ~700 ms the cost is inside the ODB write itself (deflate/sha/freshen).
    let odb = repo.odb().context("opening odb")?;
    summarize("odb.write(blob)", time_each(|i| {
        let data = format!("typoena git_bench orphan blob #{i} — unique so the write is real\n");
        odb.write(ObjectType::Blob, data.as_bytes())
            .map(|_| ())
            .context("odb.write")
    })?);

    // 2) repo.index() — cost of loading the index from the card each time.
    summarize("repo.index() open", time_each(|_| {
        repo.index().map(|_| ()).context("index open")
    })?);

    // 3) index.write() — serialize + checksum + filebuf (index.lock → rename).
    let mut idx = repo.index().context("opening index")?;
    summarize("index.write()", time_each(|_| {
        idx.write().context("index.write")
    })?);

    // 4) index.write_tree() — build tree(s) from the index and write to the ODB.
    //    First call writes the tree; later calls find it exists (freshen-skip), so
    //    min≈build+exists and max≈build+write — the spread separates the two.
    summarize("index.write_tree()", time_each(|_| {
        idx.write_tree().map(|_| ()).context("write_tree")
    })?);

    Ok(())
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
