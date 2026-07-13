//! SD/FAT primitive-op micro-benchmark — investigating the ~700 ms-per-loose-
//! object write floor found in the `:sync` commit split (2026-07-12, see
//! `docs/tradeoff-curves/sync-commit-staging.md`).
//!
//! The split showed a single small git loose object (`write_tree` = one tree
//! object) takes ~710 ms to land on the card, and it is **not** fsync
//! (`GIT_OPT_ENABLE_FSYNC_GITDIR` is off). libgit2's loose-object write
//! (`odb_loose.c` `loose_backend__write` → `git_filebuf_commit_at`) is, per object:
//!
//!   stat(final)      — freshen probe, misses (our `utimes` stub → `stat`)
//!   open+write+close — a temp file (`GIT_FILEBUF_TEMPORARY`)
//!   [mkdir objects/xx once per fan-out]
//!   p_rename         — our stub: remove(final) [ENOENT] + rename(temp → final)
//!
//! i.e. **two directory-mutating writes** (temp create + rename) per object. This
//! bench times each FAT primitive in isolation, then a composite that mirrors the
//! sequence above, so we can attribute the ~700 ms to specific ops and get a
//! baseline to compare an A1/A2 card or a 20 MHz bus against. All writes go to
//! `/sd/sdbench` (cleaned up at the end); the pack-seek op additionally opens
//! `/sd/repo`'s packfile READ-ONLY — it never writes there.
//!
//! Flash with `just flash-bench`. Needs no `.env`, no `git` feature (pure SD).

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::time::Instant;

use anyhow::{Context, Result};
use esp_idf_svc::hal::delay::FreeRtos;

use firmware::persistence::Storage;

/// Injected by build.rs so serial output identifies the exact build.
const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

/// Scratch dir on the card ROOT — outside `/sd/repo`, so a later `:sync` never
/// stages it and the user's notes are never touched.
const BENCH_DIR: &str = "/sd/sdbench";
/// Iterations per op: enough to read min/p50/mean past controller jitter, few
/// enough that total write volume stays tiny.
const N: usize = 20;
/// ~ the size of a small deflated git loose object (blob/tree/commit).
const PAYLOAD: [u8; 200] = [b'x'; 200];

fn main() -> Result<()> {
    // Required once before any esp-idf-svc call (see esp-idf-template#71).
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Typoena — SD primitive bench, {BUILD_TAG}");
    match run() {
        Ok(()) => log::info!("sd_bench: done"),
        Err(e) => log::error!("sd_bench failed: {e:?}"),
    }
    loop {
        FreeRtos::delay_ms(1000);
    }
}

fn run() -> Result<()> {
    let sd = Storage::mount().context("mounting SD")?;
    let (max_khz, real_khz) = sd.negotiated_khz();
    log::info!("bus: max {max_khz} kHz, negotiated {real_khz} kHz — {N} iters, {}-byte payload", PAYLOAD.len());

    // Fresh scratch dir.
    let _ = fs::remove_dir_all(BENCH_DIR);
    fs::create_dir_all(BENCH_DIR).with_context(|| format!("creating {BENCH_DIR}"))?;

    // Warm-up: the first write after mount pays one-time settling — don't measure it.
    {
        let mut f = File::create(format!("{BENCH_DIR}/warmup"))?;
        f.write_all(&PAYLOAD)?;
    }

    // 1) create + write(200B) + close, a fresh unique file each time. The drop at
    //    the block's end is the close (FatFS f_close flushes dir entry + data).
    summarize("create+write(200B)+close", time_each(|i| {
        let mut f = File::create(format!("{BENCH_DIR}/c{i}"))?;
        f.write_all(&PAYLOAD)?;
        Ok(())
    })?);

    // 2) rename c{i} -> o{i}. Sources exist from step 1 (untimed setup).
    summarize("rename", time_each(|i| {
        fs::rename(format!("{BENCH_DIR}/c{i}"), format!("{BENCH_DIR}/o{i}"))
            .map_err(Into::into)
    })?);

    // 3) stat, hit.
    summarize("stat (hit)", time_each(|i| {
        fs::metadata(format!("{BENCH_DIR}/o{i}")).map(|_| ()).map_err(Into::into)
    })?);

    // 4) stat, miss (ENOENT) — the freshen-probe analogue. A read, expected cheap.
    summarize("stat (miss/ENOENT)", time_each(|i| {
        let _ = fs::metadata(format!("{BENCH_DIR}/nope{i}"));
        Ok(())
    })?);

    // 5) remove o{i}.
    summarize("remove", time_each(|i| {
        fs::remove_file(format!("{BENCH_DIR}/o{i}")).map_err(Into::into)
    })?);

    // 6) Composite: the exact loose-object write sequence libgit2 performs, with a
    //    git-length (38-hex) final name so LFN directory-entry cost is included.
    //    If the model is right this lands near the ~700 ms/object from the split.
    summarize("loose-object composite", time_each(|i| {
        let tmp = format!("{BENCH_DIR}/tmp_obj{i}");
        let fin = format!("{BENCH_DIR}/{i:038x}");
        let _ = fs::metadata(&fin); // freshen probe, misses
        {
            let mut f = File::create(&tmp)?; // temp create + write + close
            f.write_all(&PAYLOAD)?;
        }
        let _ = fs::remove_file(&fin); // p_rename's remove(to) — ENOENT
        fs::rename(&tmp, &fin)?; // temp -> final
        Ok(())
    })?);

    // 6b) Directory-entry scaling — the ~360 ms/loose-write residual suspect
    //     (2026-07-13, post-FASTSEEK; see sync-commit-staging.md). FAT has no
    //     directory index: every path resolution scans the parent's entries
    //     linearly over SPI, so op cost should grow with sibling count. A repo
    //     `.git/objects/` accumulates up to 256 fan-out dirs, and every loose
    //     write resolves multi-component paths under it several times (freshen
    //     stat, temp create, remove, rename). If stat/create cost climbs with N
    //     here, the residual is directory scans, not data I/O — and the miss
    //     case (full scan, no early exit) is the worst-case bound.
    for n in [8usize, 64, 256] {
        let dir = format!("{BENCH_DIR}/fan{n}");
        fs::create_dir_all(&dir)?;
        for j in 0..n {
            File::create(format!("{dir}/e{j:04}"))?; // sibling entries (untimed setup)
        }
        summarize(&format!("stat hit, {n:>3} siblings"), time_each(|i| {
            fs::metadata(format!("{dir}/e{:04}", i % n)).map(|_| ()).map_err(Into::into)
        })?);
        summarize(&format!("stat miss, {n:>3} siblings"), time_each(|i| {
            let _ = fs::metadata(format!("{dir}/nope{i}"));
            Ok(())
        })?);
        summarize(&format!("loose composite, {n:>3} sib"), time_each(|i| {
            let tmp = format!("{dir}/tmp_obj{i}");
            let fin = format!("{dir}/{i:038x}");
            let _ = fs::metadata(&fin); // freshen probe, misses
            {
                let mut f = File::create(&tmp)?;
                f.write_all(&PAYLOAD)?;
            }
            let _ = fs::remove_file(&fin); // p_rename's remove(to) — ENOENT
            fs::rename(&tmp, &fin)?;
            Ok(())
        })?);
    }

    // Clean up so the card is left as we found it.
    fs::remove_dir_all(BENCH_DIR).with_context(|| format!("removing {BENCH_DIR}"))?;

    // 7) THE ~1.5 s LOOSE-WRITE SUSPECT (git_bench, 2026-07-12 second real-repo
    //    run): lseek inside a huge file. Without CONFIG_FATFS_USE_FASTSEEK,
    //    FatFS resolves lseek by walking the file's FAT cluster chain — forward
    //    from the current position, from the CHAIN HEAD on any backward seek.
    //    The 570 MB pack is ~36k clusters ≈ ~146 KB of FAT reads over SPI per
    //    long walk. `p_mmap` (esp_map.c) does lseek+read per window, and
    //    libgit2's freshen path probes the pack TRAILER (near the end) while
    //    tree windows sit at low offsets — so each loose write pays ~one full
    //    walk. Prediction: "@start" stays ~ms; "@end" costs ~1.5 s per iter.
    //    If so, the fix is CONFIG_FATFS_USE_FASTSEEK=y (fast-seek applies to
    //    read-mode files only — exactly how the pack is opened).
    match find_pack()? {
        Some(pack) => {
            let len = fs::metadata(&pack)?.len();
            log::info!("pack seek bench: {pack} ({} MB)", len / (1024 * 1024));
            if len < 1024 * 1024 {
                log::info!("pack too small to show chain-walk cost — skipping (toy card?)");
            } else {
                let mut f = File::open(&pack).with_context(|| format!("opening {pack}"))?;
                let mut buf = vec![0u8; 4096];
                // Baseline: rewind + read at the chain head — no walk to resolve.
                summarize("pack seek+read 4KB @start", time_each(|_| {
                    f.seek(SeekFrom::Start(0))?;
                    f.read_exact(&mut buf)?;
                    Ok(())
                })?);
                // Rewind (cheap, measured above), then seek near the end — pays
                // one full cluster-chain walk per iteration if fast-seek is off.
                let high = len - 4096;
                summarize("pack seek+read 4KB @end", time_each(|_| {
                    f.seek(SeekFrom::Start(0))?;
                    f.read_exact(&mut buf)?;
                    f.seek(SeekFrom::Start(high))?;
                    f.read_exact(&mut buf)?;
                    Ok(())
                })?);
            }
        }
        None => log::info!("no packfile under /sd/repo/.git/objects/pack — skipping seek bench"),
    }
    Ok(())
}

/// Largest `*.pack` under the repo's pack dir, if the card carries a clone.
/// Skips macOS AppleDouble sidecars (`._pack-*.pack`, 4 KB of Finder metadata) —
/// the Spike-14 cruft in its latest disguise.
fn find_pack() -> Result<Option<String>> {
    let Ok(entries) = fs::read_dir("/sd/repo/.git/objects/pack") else {
        return Ok(None);
    };
    let mut best: Option<(u64, String)> = None;
    for e in entries.flatten() {
        let p = e.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("._") || !name.ends_with(".pack") {
            continue;
        }
        let len = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        if best.as_ref().is_none_or(|(l, _)| len > *l) {
            best = Some((len, p.to_string_lossy().into_owned()));
        }
    }
    Ok(best.map(|(_, p)| p))
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
