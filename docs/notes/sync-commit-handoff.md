# Handoff — the on-device commit must become an O(depth) TreeBuilder walk

> **Start here if you're picking up `:sync` performance.** Written 2026-07-12 after
> benching `git_bench` on a full clone of the real `jcalixte/notes` repo. Full
> analysis + numbers: [`../tradeoff-curves/sync-commit-staging.md`](../tradeoff-curves/sync-commit-staging.md).
> Latency memory: `sync-timing`. Why the repo can't shrink:
> [`git-sync-images-and-repo-size.md`](git-sync-images-and-repo-size.md).

## TL;DR

The current `firmware::git_sync::stage_and_commit` (`add_all` → `index.write` →
`write_tree`) **cannot commit the real repo** — it is O(N_tree) and the real repo is
1179 files / 158 dirs / 570 MB pack / 150 MB of images. Measured on device:

- `index.write()` re-hashes the whole working tree → **up to 611 s** (10-min freeze).
- The index-free alternative (`Index::new` + `read_tree(HEAD)` + `write_tree_to`)
  still reads the whole tree cold → **77 s**, and drove the `esp_map.c` mmap cache
  to 7.4 MB, starving zlib so `repo.blob()` crashed (`zlib (5)`, 508 KB heap left).

**The real repo has almost certainly never completed a `:sync` on device** — only
the toy `typoena-test` (`notes.md`) has. The repo cannot be shrunk (the 150 MB of
images serve another app — see the images note). So the fix is a **new commit
mechanism**, not a tuning knob.

> **Splice bench result (2026-07-12, later the same day): 6.5 s — do NOT wire it
> in yet.** The walk is in `git_bench` (`splice stage→tree`) and its O(depth)
> shape is confirmed on the real repo (flat pack reads, heap healthy, no OOM),
> but each of its 4 loose-object writes costs **~1.5 s** — isolated `odb.write`
> regressed 142 ms → 1.5 s vs the previous run, with **0 mmap-cache hits**.
> Projected full commit ≈ 8–9 s.
>
> **Root cause found the same evening (sd_bench seek op): FatFS lseek walks the
> FAT cluster chain** — seek+read(4 KB) into the 263 MB pack costs **198.7 ms at
> the end vs 5.8 ms at offset 0**, and each loose write pays ~8 such walks via
> the freshen path's small `p_mmap`s → the ~1.5 s. Fix landed in
> `sdkconfig.defaults`: `CONFIG_FATFS_USE_FASTSEEK=y` +
> `FAST_SEEK_BUFFER_SIZE=256` (O(1) lseek for read-mode files, i.e. the pack —
> see [FatFS f_lseek/CLMT](http://elm-chan.org/fsw/ff/doc/lseek.html) and the
> [ESP-IDF FatFS docs](https://docs.espressif.com/projects/esp-idf/en/latest/esp32/api-reference/storage/fatfs.html)).
> **Verified on device: far seek 198.7 → 20.4 ms.** The A/B (splice 6.5 → 2.8 s,
> odb.write 1.5 s → 416 ms) plus new probes then localized the residual to ~7–8
> repeated small (~4 KB) pack reads per op that the mmap cache's 64 KB floor
> excluded (`odb.read_header(packed)` = 470 ms; the strict-object-creation
> theory was refuted — strict-off changed nothing). **esp_map.c v2 built**:
> cache admission keyed on FILE size ≥ 1 MB so the hot small windows cache, and
> evict-on-`p_munmap` to a 2 MB low-water mark fixes the 7.4 MB OOM.
>
> **Final `git_bench` verdict (run 4, same evening): the memory fix is
> VERIFIED (resident 1833 KB flat, 6.4 MB heap free all run — no OOM), the
> window-cache theory is REFUTED (v2 retains the small maps and still scored
> 0 hits in 313 misses — the small reads hit unique offsets every time;
> `mwindow` already absorbs true repetition), and the sub-second bar FAILED:
> splice 2.83 s cold / ~2 s steady-state, commit(None) 713 ms. Decision:
> WIRE IT IN ANYWAY** — ~2–2.8 s on the real repo vs 611 s/OOM for every
> alternative puts a full cold `:sync` at ~9–10 s, which ships. The residual
> ~360 ms/loose-write ≈ 8 unique small SD round-trips; next suspect is FAT
> directory-op cost in freshen/refresh (instrumentation pass for later, not a
> prerequisite). **The cache was then REMOVED entirely (run 5 confirmation):
> esp_map.c is now the plain malloc-read/free-at-munmap emulation** — run 5's
> read pattern was byte-identical to run 4 (even 15 reads better; v2's
> eviction fought mwindow), warm splice identical at 1.95 s, and the ~1.85 MB
> "resident" turned out to be mwindow's live open-window set, not cache
> retention — which is one more reason the mwindow opts below are mandatory
> in shipping `git_sync.rs`. **Proceed with the firmware plumbing below.**
> Full trail: the splice-bench, root-cause, second-localization and
> final-bench sections of the tradeoff curve.

## The fix — O(depth) TreeBuilder walk

Rebuild only the edited file's ancestor subtree chain onto HEAD's tree. Never
materialise the 1179-entry index; never `index.write()`; never `read_tree` the whole
tree. Cost is O(depth × dirty_files), flat in repo size, tiny heap, and it carries
every unchanged entry (all 260 images, the other 1176 files) forward untouched —
which means **the device doesn't even need the images in its working tree.**

Sketch (git2 0.20 API — all present: `Repository::treebuilder(Option<&Tree>)`,
`TreeBuilder::{insert,remove,write}`, `Tree::{get_name,get_path}`,
`TreeEntry::{to_object,filemode}`):

```rust
/// Return a new tree OID = `base` with `path` set to `new` (Some(blob_oid) to
/// add/replace, None to delete). Reads ~depth subtree objects, writes ~depth trees.
fn splice(repo: &Repository, base: &Tree, path: &[&str], new: Option<Oid>) -> Result<Oid> {
    let (head, rest) = path.split_first().unwrap();
    if rest.is_empty() {
        // leaf level: patch this tree directly
        let mut tb = repo.treebuilder(Some(base))?;
        match new {
            Some(oid) => tb.insert(head, oid, 0o100644)?,   // FileMode::Blob
            None       => { tb.remove(head).ok(); }          // delete
        };
        return Ok(tb.write()?);
    }
    // descend: find (or synthesize) the subtree, recurse, re-insert its new OID
    let sub = base.get_name(head)
        .and_then(|e| e.to_object(repo).ok())
        .and_then(|o| o.peel_to_tree().ok());
    let empty; let sub_ref = match &sub { Some(t) => t, None => { empty = repo.treebuilder(None)?; /* ... */ unreachable!() } };
    let new_sub = splice(repo, sub_ref, rest, new)?;
    let mut tb = repo.treebuilder(Some(base))?;
    // if new_sub is the empty tree and this was a delete, remove the dir entry instead
    tb.insert(head, new_sub, 0o040000)?;                    // FileMode::Tree
    Ok(tb.write()?)
}
```

Then, for the dirty set, fold each change through `splice` (thread the running root
tree), and `commit(Some("HEAD"), …, &final_tree, &[&parent])`. Edge cases to handle:
new files where an intermediate dir doesn't exist yet (build subtree from `None`),
deletes that empty a directory (remove the dir entry rather than insert an empty
tree), and path splitting on `/` (paths are repo-relative POSIX).

**Bench it first** (per the established discipline): add a `treebuilder splice→tree`
op to `firmware/src/bin/git_bench.rs` alongside the existing ones and confirm it's
sub-second cold on the real repo, with heap staying healthy (no OOM). Only then wire
it into the firmware. While in there, two bench-hygiene fixes: (a) the `edit_path`
seed block does the cold `read_tree(HEAD)` **untimed** (the 77 s number came from
log timestamps), so the `Index::new + read_tree` bench only ever measures warm —
wrap the seed in a timer so a re-run prints the cold number; (b) the "3) THE
PROPOSED FIX — index-free commit tree" comment is now stale — the real-repo run
refuted that path (77 s + OOM) and this handoff supersedes it — retitle it as the
refuted alternative.

## Firmware plumbing (after the bench validates)

1. **`firmware/src/git_sync.rs`**
   - **Set the mwindow options at service start** (before the first
     `Repository::open`): `git2::opts::set_mwindow_size(256 * 1024)` +
     `set_mwindow_mapped_limit(4 * 1024 * 1024)`. Today only `git_bench.rs` sets
     them — the shipping service runs libgit2's 32-bit defaults (**32 MB window /
     256 MB mapped limit**, mwindow.c:16), so the first pack access on the 570 MB
     clone would try to `git__malloc` a 32 MB window and die on the 8 MB PSRAM
     heap before the walk even runs.
   - Rewrite `stage_and_commit` (currently ~L271–332) to the `splice`-walk above.
     Drop `add_all`/`update_all`/`index.write`/`index.write_tree`. Keep the
     `commit split —` timing log, the `tree unchanged → nothing to publish` check,
     and the signature/message code.
   - `PublishRequest` (~L79) is currently an **empty struct** — it must carry the
     dirty set: `{ changed: Vec<(String /*repo-rel path*/, ...)>, deleted: Vec<String> }`.
     The commit needs the blob content or a way to read it; simplest is to pass the
     paths and let the git thread `repo.blob_path(abs)` / read `/sd/repo/<path>`.
   - `reconcile_onto_origin` (~L377/L394) uses `repo.reset(Mixed)` — with an
     index-free commit there's no index to reset; switch to `ResetType::Soft` (move
     HEAD only) or drop the reset and just re-`splice` onto the new origin tip.
   - The macOS-cruft filter (`skip_macos_cruft`) is no longer needed — the walk only
     ever touches paths the editor explicitly hands it, so `._*`/`.DS_Store` can't
     sneak in. (Keep a note; don't silently lose the Spike-14 lesson.)
   - **Deliberate behavior change to record:** the walk commits *only* the
     editor's dirty set. Files changed on the card outside the editor (e.g. the
     card mounted on a Mac) were swept in by `add_all` before; they will now never
     be committed, and the working tree will show a permanent diff against HEAD if
     inspected on a desktop. Correct for the appliance (it's also what makes the
     cruft filter unnecessary), but it must be intentional, not accidental.

2. **Dirty-set source — `firmware/src/persistence.rs` + `main.rs`**
   - Writes funnel through `Storage::save_path` (~L316) and deletes through
     `Storage::delete_path` (~L352), both `&self`. Accumulate a dirty/deleted set
     (needs `RefCell` interior mutability, or move the set up to `main.rs`).
   - `Effect::Publish` handler in `main.rs` (~L222) builds the `PublishRequest` from
     that set and clears it on a successful `Pushed`/`UpToDate` outcome.
   - **FD budget: `main.rs` (~L413) mounts with `Storage::mount()` = 4 open
     files, and the git thread shares that mount.** libgit2 keeps the pack +
     `.idx` (+ commit-graph) descriptors open and opens loose objects on top —
     that's why `git_bench` needed `mount_for_git` (16). On the real repo the
     shipping `:sync` will fail with "no free file descriptors" long before any
     latency question. Either mount with the 16-file budget in `main.rs` (the
     editor's 2-FD peak coexists fine) or split the budgets some other way.

3. **`esp_map.c` cache fix (same pass) — `firmware/components/libgit2/esp_map.c`**
   - Bug: cached buffers are freed only lazily in `p_mmap`'s `evict_for`, so a
     window libgit2 has `p_munmap`'d stays resident until the *next* map — defeating
     `MWINDOW_MAPPED_LIMIT` and starving non-mmap `git__malloc` (zlib). 
   - Fix: on `p_munmap` when refcount hits 0, evict down to a low-water mark (or free
     outright); lower `ESP_MAP_CACHE_CAP` (4 MB → ~1.5–2 MB) and/or `SLOTS`.
   - ⚠️ Editing this `.c` needs the fingerprint dance or the change won't rebuild —
     see the `esp-idf-component-rebuild` memory:
     `rm -rf firmware/target/xtensa-esp32s3-espidf/release/.fingerprint/esp-idf-sys-*`
     then rebuild.

## How to bench / flash

`git_bench.rs` runs git ops on the 96 KB `GIT_STACK` thread (the main task stack
overflows on these ops — that's why the real service has a dedicated thread). It's
Rust-only, so a plain rebuild picks it up (no fingerprint dance unless you also
touched `esp_map.c`).

```
just flash-gitbench
# = . ~/export-esp.sh && LIBGIT2_SRC=<repo>/firmware/components/libgit2/vendor \
#     LIBGIT2_NO_VENDOR=1 PKG_CONFIG_ALLOW_CROSS=1 \
#     PKG_CONFIG_LIBDIR=<repo>/firmware/pkgconfig \
#     cargo run --release --bin git_bench --features git
```

Bench on the **real repo** clone (`/sd/repo` = full `jcalixte/notes`), not the toy —
the toy pack understates everything by ~2 orders of magnitude.

## What's proven vs open

**Proven (2026-07-12, real repo):** `index.write` 611 s (whole-tree re-hash via
`truncate_racily_clean`, index.c:822 / index.h:117); index-free `read_tree`
77–82 s cold (reproduced across both runs); mmap cache OOM at 7.4 MB → zlib crash
(reproduced). `Repository::open` ~90–99 ms, odb-open ~6–8 s cold (maps 1.7 MB
`.idx`). **Splice walk benched (second run): 6.5 s p50, warm ≈ cold** — O(depth)
shape confirmed (flat reads ~40 KB/write, 6.4 MB heap free), cost is 4 loose
writes × ~1.6 s. `commit(None)` 1.7 s.

**Open — the new gating question: why does one loose-object write cost ~1.5 s?**
`odb.write(blob)` measured 142 ms in the first real-repo run but 1.5 s in the
second (same `esp_map.c`, same card, **0 cache hits** the whole second run) — the
two runs are unreconciled. Suspects, cheapest first: (a) FAT free-cluster scan on
the ~740 MB-full card → re-run `sd_bench` as-is on this card; (b) loose-write
internals (filebuf tmp + rename, per-write `git_odb_refresh` readdir) under the
accumulating orphan objects from bench runs → re-provision a fresh clone and A/B;
(c) the ~10 small 4 KB `p_mmap`s per write (sub-64 KB, uncacheable) — bounded
~100 ms, secondary. Also open: whether the esp_map cache earns its keep at all
(0 hits this run), the ref/reflog-update cost on the real repo, and the push
(network ~6.5 s) floor — all untouched here.
