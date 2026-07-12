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
it into the firmware.

## Firmware plumbing (after the bench validates)

1. **`firmware/src/git_sync.rs`**
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

2. **Dirty-set source — `firmware/src/persistence.rs` + `main.rs`**
   - Writes funnel through `Storage::save_path` (~L296) and deletes through
     `Storage::delete_path` (~L332), both `&self`. Accumulate a dirty/deleted set
     (needs `RefCell` interior mutability, or move the set up to `main.rs`).
   - `Effect::Publish` handler in `main.rs` (~L222) builds the `PublishRequest` from
     that set and clears it on a successful `Pushed`/`UpToDate` outcome.

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

**Proven (2026-07-12, real repo):** `odb.write` 142 ms (mmap cache holds);
`index.write` 611 s (whole-tree re-hash via `truncate_racily_clean`, index.c:822 /
index.h:117); index-free `read_tree` 77 s cold; mmap cache OOM at 7.4 MB → zlib
crash. `Repository::open` 88 ms, odb-open ~6 s cold (maps 1.7 MB `.idx`).

**Open:** the TreeBuilder walk is **designed but not yet benched or built.** Confirm
its cold-real-repo latency and heap before wiring. Push (network half, ~6.5 s) is a
separate floor, untouched here.
