# Commit-staging cost vs working-tree size

> **Decision (RESOLVED 2026-07-12, real-repo bench):** neither `add_all(["*"])`
> nor an index-free `read_tree`+`write_tree` is viable on the real
> `jcalixte/notes` clone — **both are O(N_tree)** and blow up on the 570 MB pack /
> 1179-file tree (611 s hash one end, 77 s tree-read + OOM the other; see
> [Real-repo run](#real-repo-run-2026-07-12-jcalixtenotes-570-mb-pack--the-index-is-the-wrong-primitive)
> below). The commit must be rebuilt with an **O(depth) TreeBuilder walk** —
> patch only the edited file's ancestor subtree chain onto HEAD's tree, never
> materialise all 1179 entries.
>
> **Splice-bench update (2026-07-12, later the same day):** the walk was built
> into `git_bench` and measured on the real repo — **6.5 s, failing the
> sub-second bar.** The O(depth) *shape* holds (flat pack reads, healthy heap),
> but each of its 4 loose-object writes costs **~1.5 s** (isolated `odb.write`
> regressed from the 142 ms above; the mmap cache scored **0 hits** this run).
> Localizing the loose-write cost is now the gating work — see
> [Splice bench](#splice-bench-2026-07-12-second-real-repo-run--the-walk-is-right-the-loose-object-write-is-the-new-wall).
> Shrinking the repo is **not** an option
> ([`../notes/git-sync-images-and-repo-size.md`](../notes/git-sync-images-and-repo-size.md):
> the images are load-bearing for another app), which is exactly why the O(depth)
> mechanism is the only lever left. This note records the cost model and the full
> measurement trail that got here.
>
> Tradeoff-curves index: [`README.md`](README.md). Docs index:
> [`../README.md`](../README.md). Where the whole sync goes:
> [`../notes/sync-latency.md`](../notes/sync-latency.md). Sibling curve on the
> radio cost of *how often* we sync: [`wifi-auto-sync.md`](wifi-auto-sync.md).

## The model

`:sync` commits the working tree on the SD/FAT card before it pushes. The commit
is two kinds of work against the card over SPI (10 MHz today, ADR-012):

```
  stage                                 write
  ───────────────────────────────       ─────────────────────────────
  add_all(["*"]) + update_all(["*"])     index.write + write_tree + commit-obj
  → stat() every file in the tree,       → serialise the index and three loose
    hash the ones whose stat moved          objects, each a FAT create+write+fsync
  cost ∝ tree size  (O(N_tree))          cost ∝ churn (O(N_changed)) + fixed
```

The two have different curves against **N = files in `/sd/repo`**:

- **Walk** rises with N. `add_all(["*"])` visits the whole working tree every
  sync regardless of how little changed, and each visit is a FAT `stat` (and a
  re-hash when the entry looks dirty) over SPI. This is the term explicit-path
  staging removes: the editor already knows which buffers are dirty and which
  were `:delete`d, so `index.add_path(p)` / `index.remove_path(p)` over that set
  touches `N_changed` files (≈1 for a writing appliance), not `N`.
- **Write** is flat in N. A text commit is the index + a blob + a tree + a commit
  object — a handful of small FAT writes whose cost is set by SPI clock and
  `fsync`, not by tree size. Explicit-path staging cannot shave this; only a
  faster card bus (SD 10 → 20 MHz on a clean PCB, `persistence.rs`) does.

```
  Commit latency vs working-tree size          two staging strategies

  ms
      |                                          walk-all: add_all(["*"])
 4000 |                                . *        stat()s every file in the
      |                          . *              tree each sync → O(N)
      |                    . *
 3000 |              . *
      |         . *
 2000 |     . *
      |   *
 1000 |·····································    explicit-path: add_path(dirty)
      |      FAT object-write floor              → O(churn); flat in N. The gap
    0 +----+----+----+----+----+----+----+---→   up to walk-all is the avoidable
        10   50  100  200  400  800  1179        per-sync tree walk.
                                     └── jcalixte/notes today (N files)
```

The gap between the lines at a given N is exactly what switching buys, and it
**grows without bound** as the notes tree fills.

### The real operating point (measured 2026-07-12, `jcalixte/notes`)

The device syncs into a clone of the actual notes repo, not a `notes.md` toy. Its
working tree is **not small**:

| | count | working-tree bytes |
| --- | ---: | ---: |
| Markdown (`.md`) | 875 | ~1.5 MB |
| Images (png/jpg/webp/bmp/gif) | ~260 | **~150 MB** |
| Other (json/ts/pdf/…) | ~44 | ~20 MB |
| **Total (N)** | **1179 files, 158 dirs** | **~170 MB** |
| `.git` history | | ~570 MB |

So `add_all(["*"])` walks **1179 files across 158 directories every sync** — and
~260 of them are images that a text edit never changes. That does two things the
toy-repo baseline hides:

1. **The walk term is large and paid on every sync** — 1179 `stat`s + 158 dir
   reads over SPI, for a one-line note change. This is the O(N) cost the curve
   above predicts, at N ≈ 1179 rather than N ≈ 2.
2. **Re-hash risk.** libgit2 decides a file is unchanged from `stat` metadata
   (mtime/size). FAT's coarse mtime and lack of a stable inode can make entries
   look racy, forcing a content re-hash. If even a slice of the ~150 MB of images
   gets re-hashed over a 10 MHz SPI bus, the commit balloons far past 4 s. The
   `walk` timer will show it; explicit-path staging sidesteps it entirely by never
   visiting the images.

## Measurement (2026-07-12, toy `notes.md` tree, N ≈ 2)

Split from two back-to-back `:sync`es on the small test repo (commits `95ac56ef`
cold, `ab260bde` warm), via the `commit split —` log lines:

| Sub-phase | Kind | Cold (ms) | Warm (ms) |
| --- | --- | ---: | ---: |
| `walk(add_all+update_all)` | scan (O(N)) + likely 1 blob write | 1402 | 1456 |
| `index.write` | FAT write | 204 | 204 |
| `write_tree` | **1 tree object → FAT** | 710 | 715 |
| `parent-load` | FAT read | 102 | 105 |
| `commit-obj` | **1 commit object + ref → FAT** | 914 | 924 |
| **commit total** | | **3332** | **3404** |

### It is not the card — it's libgit2 (`sd_bench`, 2026-07-12)

My first read of the table was "a loose-object write to this SD card costs
~700–900 ms." **That was wrong.** `sd_bench` (`firmware/src/bin/sd_bench.rs`) times
the raw FAT primitives on the same card at the same 10 MHz:

| Raw FAT op (200-byte payload) | p50 |
| --- | ---: |
| create + write + close | 21.7 ms |
| rename | 12.8 ms |
| stat (hit / miss) | ~5 ms |
| remove | 14.9 ms |
| **loose-object composite** (stat + create + write + rename) | **86 ms** |

The card does a *complete* loose-object write in **~86 ms**. Yet `write_tree`
(one tree object) took **710 ms** and `commit-obj` **914 ms** — an **~8× gap that
is pure libgit2 overhead, not FAT I/O.** So the earlier "object-write floor / SD
write amplification / better card / SPI-clock" framing is refuted: **the SD card is
not the bottleneck.** fsync is still confirmed off; the extra ~600 ms/op is CPU or
repeated `.git` I/O *inside* libgit2 (candidates: ODB refresh scanning
`objects/`, the treebuilder's per-entry `git_odb_exists`, ref-lock + reflog writes,
config/attributes re-reads). `git_bench` (`firmware/src/bin/git_bench.rs`) localizes
it — see below.

### It's the pack, read through an un-caching emulated mmap (`git_bench`, 2026-07-12)

`git_bench` times the git2 primitives in isolation on `/sd/repo` (git ops on the
same 96 KB thread the real service uses — the main-task stack overflows on
`index.write`, which is itself the reason the service has a dedicated thread):

| git2 op | p50 | note |
| --- | ---: | --- |
| `Repository::open` | 100 ms | one-time |
| `odb.write(blob)` (unique) | **45 ms** | writes a fresh object; touches no existing object |
| `repo.index()` open | ~0 ms | cached |
| `index.write()` | 376 ms | index + `index.lock` rename + tree-cache |
| `write_tree` [unchanged] | ~0 ms | tree exists → freshen-skips the write |
| **`write_tree` [changed]** | **1136 ms** | writes ONE 45 ms object |
| **`commit(None)` orphan obj** | **563 ms** | writes ONE 45 ms object, no ref/reflog |

Writing a fresh object is 45 ms; the ops that wrap one are 8–25×. The cause, from
the vendored source: `git_odb_write` calls `git_odb__freshen` (odb.c:1011), which
on a not-found object runs **`git_odb_refresh`** (re-reads the pack dir + reloads
pack indexes), and existence checks (`freshen(tree)` in `commit.c:169`, base-object
lookups in `write_tree`) hit the **pack**. Pack access goes through our
`p_mmap` (`esp_map.c`), which **`malloc`s and `read()`s the mapped range from the
card on every call — no cache** — with a 32 MB window on this 32-bit target. So
each write re-reads pack bytes from SD; `odb.write` of a fresh blob is 45 ms only
because it touches no packed object.

**This scales with pack size.** The toy repo's pack is tiny; the real
`jcalixte/notes` clone has a **570 MB pack**, and provisioning rsyncs a full clone
onto the card — so a real-repo commit has **never been benchmarked** and, on this
mechanism, will be far worse than the ~3.3 s toy number. That is the single biggest
open risk in sync.

### Real-repo run (2026-07-12, `jcalixte/notes`, 570 MB pack) — the index is the wrong primitive

`git_bench` was finally run against a full clone of the real repo (1179 files, 158
dirs, 570 MB pack). It settles the design: **any index-based commit is O(N_tree)
and does not fit this device.** Two independent walls:

| op | result | reading |
| --- | ---: | --- |
| `Repository::open` | 88 ms | fine |
| odb open (implicit) | ~6 s cold | maps the 1.7 MB pack `.idx` once (16 miss / 1790 KB) |
| `odb.write(blob)` | **142 ms** p50 | the mmap cache win **holds** (was 862 ms uncached) ✅ |
| `repo.index()` load (1179 entries) | 514 ms max | the on-disk index we were trying to avoid |
| `index.write()` | **min 360 ms / p50 12.8 s / max 611 s** | ⚠️ hangs — see root cause |
| **seed `read_tree(HEAD)` (cold, 1×)** | **~77 s** | ⚠️ reads all ~158 tree objects, 22.7 MB of pack windows |
| `Index::new + read_tree` (warm) | 447 ms p50 | windows still mapped → pure CPU |
| **index-free `stage→tree`** | **💥 crash** | `zlib (5)`: `deflateInit` failed, **508 KB heap left** |

**Wall 1 — `index.write()` hashes the whole working tree (up to 611 s).**
`git_index_write` unconditionally calls `truncate_racily_clean` (index.c:822),
which runs `git_diff_index_to_workdir` over **every** entry flagged "racy" and
re-hashes its file. On a fresh FAT clone the mtime granularity is 2 s and
`index.stamp.mtime <= entry.mtime` for ~all 1179 entries (index.h:117), so the
whole tree looks racy → it re-hashes ~170 MB (mostly the 150 MB of images) over
10 MHz SPI. The 611 s → 12.8 s → 360 ms decay across three iterations is the
signature: each write bumps the index mtime, shrinking the racy set. **Implication:
the shipping `stage_and_commit` calls `index.write()`, so `:sync` on the real repo
effectively bricks on the first commit** — the user sees a 10-minute freeze,
resets, the index mtime never advances, and it re-hashes forever. The real repo has
almost certainly never completed a sync on device (only the toy `typoena-test` has).

**Wall 2 — the index-free path is still O(N_tree), and the mmap cache OOMs.**
Skipping `index.write()` entirely (fresh `Index::new()`, stamp = 0, so
`truncate_racily_clean` can never fire) removes Wall 1. But to seed the in-memory
index, `read_tree(HEAD)` materialises all 1179 entries and reads every tree object
from the 570 MB pack — **77 s cold** (447 ms only once the windows are resident).
`write_tree_to` is O(changed), but you pay O(N_tree) to build the cache it needs, so
the index-free path only trades a 611 s hash for a 77 s tree-read. Worse, that
`read_tree` drove the `esp_map.c` cache to **7.4 MB resident** — past its own 4 MB
soft cap — which left 508 KB of heap and made `repo.blob()`'s zlib `deflateInit`
fail. **The one write we cannot skip crashed.** Root cause of the OOM: our cache
holds pack windows *after* libgit2 `p_munmap`s them (refcount 0, freed only lazily
on the next `p_mmap`), which **defeats `GIT_OPT_SET_MWINDOW_MAPPED_LIMIT`** —
libgit2 thinks it released the memory; we didn't.

**Conclusion — use an O(depth) TreeBuilder walk.** "Replace K files in a 1179-file
tree" should touch `O(depth × K)` objects, not `O(N_tree)`. Walk HEAD's tree down
the edited path (`tree.get_name`/`get_path` → read ~depth subtree objects), then
rebuild bottom-up with `repo.treebuilder(Some(&subtree))` → `insert`/`remove` →
`write()`, and `commit` the new root. That never materialises the 1179 entries,
never re-hashes anything, never visits the 150 MB of images, reads only a handful
of tree windows (so the cache stays small and zlib keeps its heap), and — crucially
— **carries the image entries forward untouched from HEAD's tree, so the device
does not even need the images in its working tree.** The `esp_map.c` cache still
needs an evict-on-`munmap` fix (drop the cap, free past a low-water mark) so it can
never again starve a downstream `git__malloc`, but with the TreeBuilder walk the
pressure it was under largely disappears.

### Splice bench (2026-07-12, second real-repo run) — the walk is right, the loose-object write is the new wall

The O(depth) splice op was added to `git_bench` (1 blob + 3 tree writes onto the
depth-3 path `.claude/commands/bsky.md`, run FIRST so its first iteration is
cold; the index ops moved last so their OOM can't cost the new data — it did
crash again, after everything was logged):

| op | result | reading |
| --- | ---: | --- |
| `splice stage→tree` (1 blob + 3 trees) | **6.5 s p50, warm ≈ cold** | O(depth) confirmed — cost is 4 loose writes × ~1.6 s |
| `commit(None)` orphan obj | 1.7 s p50 | one more loose write |
| `odb.write(blob)` | **1.5 s p50** | ⚠️ was 142 ms in the previous run |
| `repo.index()` load | 524 ms max | matches previous run |
| seed `read_tree(HEAD)` cold (now timed) | 81.6 s | reproduces the 77 s |
| `index-free stage→tree` | 💥 crash, 508 KB heap | reproduces the zlib OOM exactly |

Three readings:

1. **The splice mechanism is validated as a mechanism.** Pack reads stayed flat
   (~40 KB per write; 6.4 MB heap free through splice + commit + odb.write), so
   it really is O(depth) and it cannot OOM. The 6.5 s is not tree-walk cost.
2. **The wall moved to the loose-object write: ~1.5 s each, ×4 per splice.**
   The isolated `odb.write(blob)` — one tiny orphan blob — took 1.5 s where the
   raw FAT composite is 86 ms. Projected full commit (splice 6.5 s + commit-obj
   1.7 s + ref/reflog update) ≈ **8–9 s**: enormously better than 611 s, still
   far off the bar.
3. **The mmap cache scored 0 hits over the entire run** — the documented
   862→142 ms `odb.write` win did **not reproduce** (same `esp_map.c`, same
   card). Either the earlier run's conditions differed (orphan-object
   population? FAT allocation state?) or the win was misattributed. Whatever the
   1.5 s is, it is *not* SD data volume: each write moves ~40 KB read + ~1 KB
   written.

### ROOT CAUSE FOUND (2026-07-12, `sd_bench` seek op): FatFS lseek walks the cluster chain

Two `sd_bench` re-runs on the ~740 MB-full card settled it:

1. **Free-cluster-scan hypothesis: refuted.** Raw FAT write ops are unchanged on
   the full card — loose-object composite **77 ms p50** (was 86 ms), create
   20 ms, rename 10 ms. The card is exonerated a second time.
2. **Long seeks are the cost.** A new op opens the repo's largest packfile
   (263 MB — the "570 MB pack" was actually the whole `.git`) read-only and does
   seek+read(4 KB): **@offset 0 = 5.8 ms; @end = 198.7 ms** — dead constant
   across 20 iters. Without `CONFIG_FATFS_USE_FASTSEEK`, FatFS resolves lseek by
   walking the file's FAT cluster chain over SPI: forward from the current
   position, **from the chain head on any backward seek**. 263 MB ≈ 16.8k
   clusters ≈ ~67 KB of FAT reads ≈ ~190 ms per long walk.

**Why FAT behaves this way:** FAT has no extent map or inode — a file is a
singly-linked list of clusters, and the only way to find "byte 260,000,000" is
to follow that list entry by entry through the allocation table. FatFS walks
forward from the current position when it can, but a backward seek restarts
from the chain head ([FatFS `f_lseek` docs, elm-chan.org](http://elm-chan.org/fsw/ff/doc/lseek.html)).
The fast-seek feature fixes exactly this: a pre-computed **cluster link map
table (CLMT)** per file object, "(fragments + 1) × 2" words, after which "no
FAT access is occured in subsequent f_read/f_write/f_lseek" (same page). On
esp-idf it's `CONFIG_FATFS_USE_FASTSEEK` — the official docs recommend it "for
read-heavy workloads with long backward seeks" and note it does not apply to
files opened in write mode
([ESP-IDF FatFS docs](https://docs.espressif.com/projects/esp-idf/en/latest/esp32/api-reference/storage/fatfs.html)).

The budget closes: each loose write does ~8–10 small (~4 KB) `p_mmap`s (freshen
→ trailer/idx probes) interleaved with low-offset reads, so ~8 of them pay a
fresh ~190 ms walk → **~1.5 s per object**. It also explains everything the
cache couldn't: warm ≈ cold (the walk is paid inside `lseek` before any data
moves, and the maps are below the 64 KB cache floor), the 142 ms vs 1.5 s
run-1/run-2 discrepancy (run 1's `odb.write` bench ran first and hammered only
the trailer — the file position stayed there, so its seeks were forward/no-ops),
and a large slice of the 81.6 s `read_tree` (133 windows × backward seeks ≈ 25 s
of walking on top of the 25 MB of data).

**Fix (config, not code): `CONFIG_FATFS_USE_FASTSEEK=y` +
`CONFIG_FATFS_FAST_SEEK_BUFFER_SIZE=256`** (landed in `sdkconfig.defaults`
2026-07-12). Fast seek builds an in-memory cluster-link map per read-mode file —
exactly how the pack is opened — making lseek O(1); write-mode files fall back
to the walk transparently. 256 words = 1 KB per open read-only file, covering
~127 fragments (default 64 covers ~31; a fragmented pack would silently fall
back to slow seeks, so the headroom matters).

**A/B measured (same evening): a 2.3× partial win, not a full one.**

| op | fast-seek off | fast-seek on |
| --- | ---: | ---: |
| `splice stage→tree` | 6.5 s | **2.81 s** |
| `odb.write(blob)` | 1.5 s | **416 ms** |
| `commit(None)` | 1.7 s | **1.72 s — unchanged** |

`odb.write` dropped by almost exactly the ~6 chain walks the model predicted —
the seek theory holds — but two residuals remain: **~400 ms per loose write**
(vs the 77 ms raw-FAT floor) and **`commit(None)`'s ~1.3 s premium over a plain
write, which was never seek-bound at all**. Prime suspect for the commit
premium: strict object creation makes `git_commit_create` validate its parent +
tree OIDs with pack header resolves, and `git_treebuilder_insert` does the same
per inserted entry — `git_bench` grew `odb.read_header(packed)` /
`odb.exists(missing)` probes and strict-off re-benches to test it.

### Second localization round (2026-07-12, run 3b + sd_bench re-run)

**Fast-seek verified on the metal:** re-running the sd_bench seek op with
`CONFIG_FATFS_USE_FASTSEEK=y` dropped `pack seek+read 4KB @end` from
**198.7 ms → 20.4 ms** (the CLMT fits the pack in the 256-word buffer — the
pack is not too fragmented). A far seek is now ~15 ms, i.e. effectively fixed.

**The strict-creation theory is refuted; the probes found the real unit cost:**

| op | p50 | reading |
| --- | ---: | --- |
| `odb.read_header(packed)` | **470 ms** | ONE pack header resolve costs ~½ s |
| `odb.exists(missing)` | **968 ms** (±0.1 ms) | miss path (scan → refresh → rescan) ≈ 2× |
| `commit(None)` strict OFF | 1.80 s | vs 1.93 s strict on — validation is NOT the premium |
| `splice` strict OFF | 5.7 s | noise-worse; also not validation |

The ±0.1 ms constancy of `exists(missing)` = a fixed, deterministic SD-op
sequence. The map counters identify it: **~7–8 small (~4 KB) `p_mmap` reads per
op** — pack trailer probes, idx fanout reads and delta-base windows, repeated
at the *same offsets* on every freshen/refresh. Post-fast-seek those cost
~20 ms each (~150 ms/op); the rest of `read_header`'s 470 ms is CPU-side
delta-chain inflation on the 160 MHz core plus repeated re-reads. Two other
observations from run 3b: the loose-object orphan population from bench runs
is creeping costs upward (splice 2.81 s → 3.21 s between consecutive
fast-seek runs — a re-provision resets it), and the mmap cache STILL scored 0
hits — because its 64 KB map-length floor excluded exactly these hot small
maps.

**Fix built (esp_map.c v2, pending re-bench):** cache admission re-keyed from
map length to **file size ≥ 1 MB** — the pack/idx's small repeated windows now
cache (RAM hits after first touch) while small mutable working-tree files stay
excluded — plus **evict-on-`p_munmap` down to a 2 MB low-water mark**, fixing
the 7.4 MB OOM from the first real-repo run (released windows are actually
returned to `git__malloc`, so `MWINDOW_MAPPED_LIMIT` stays honest). Expected:
`read_header` collapses toward CPU-only, `odb.write` toward ~150–250 ms,
splice at or under the sub-second bar, and no end-of-run zlib OOM.

### The walk is ~1.4 s even at N ≈ 2

Mostly fixed cost — the worktree-diff setup and the second (`update_all`) pass —
not per-file `stat` (one raw `stat` is ~5 ms, so N ≈ 2 can't be the 1.4 s). The
O(N) slope only bites on the real `jcalixte/notes` clone (N ≈ 1179), which this run
did **not** exercise. That slope is still unmeasured.

For orientation: `publish(commit+push)` was 9846 ms cold, so the **network half is
~6.5 s** — still the biggest single block of a warm sync (10.1 s total), a separate
floor ([`../notes/sync-latency.md`](../notes/sync-latency.md)).

## The verdict

The real-repo run (above) overturned the earlier ranking. Both index strategies
are O(N_tree) and fail on the 570 MB-pack clone, and the repo cannot be shrunk. The
work, ranked:

1. **Rewrite the commit as an O(depth) TreeBuilder walk (the fix — build this).**
   Rebuild only the edited path's ancestor subtree chain onto HEAD's tree; never
   materialise the 1179-entry index, never `index.write()`, never `read_tree` the
   whole tree. This is the ONLY mechanism that fits: O(depth × dirty) reads/writes,
   flat in repo size, small heap, images carried forward untouched. Replaces
   `stage_and_commit`'s `add_all`/`update_all`/`index.write`/`write_tree`. Needs the
   editor's dirty set (+ deleted set) plumbed to the git service — the editor
   already knows both. **Benched 2026-07-12: 6.5 s — the shape is right but it
   fails the sub-second bar**; wiring it in is blocked on localizing the ~1.5 s
   loose-object write (see the splice-bench section above).
2. **Fix the `esp_map.c` cache so it can't OOM (do it alongside #1).** It grew to
   7.4 MB resident past its 4 MB cap and starved zlib. Evict on `p_munmap` (not
   only lazily on the next `p_mmap`) down to a low-water mark, and lower the cap, so
   a released window's memory is actually returned to `git__malloc`. The cache's
   `odb.write` win (862 → 142 ms) is real and worth keeping — this is a
   memory-discipline fix, not a removal.
3. **Retired: `add_all`/explicit-path *index* staging.** Explicit-path `add_path`
   still goes through the index and `index.write` → `truncate_racily_clean`, so it
   hits Wall 1 just the same. The TreeBuilder walk supersedes it entirely; the
   "explicit-path staging" idea survives only as "the editor's dirty set feeds the
   walk."
4. **Retired: SD clock / better card.** The card does a full object write in
   ~86 ms; raw I/O is not the bottleneck. Do not spend the PCB's 20 MHz budget
   expecting a commit-latency win.
5. **Kept: the mmap cache + mwindow tuning** (`GIT_OPT_SET_MWINDOW_*`, 256 KB
   window / 4 MB mapped limit). It fixed `odb.write` and the push read path; #2 just
   makes it well-behaved under memory pressure.

**Recommendation:** build #1 (O(depth) TreeBuilder walk) with #2 (cache
evict-on-munmap) in the same pass. See
[`../notes/sync-commit-handoff.md`](../notes/sync-commit-handoff.md) for the
concrete next-session plan (bench design, firmware plumbing, exact call sites).

## Adjacent lever: should the images be on the card at all?

Explicit-path staging makes the walk skip the images, but they still cost 150 MB
of SD space, inflate the 570 MB clone, and slow provisioning + the pull-before-push
paths. Whether the device should carry image blobs at all — vs. markdown-only, or
Git-LFS-style pointers — is a separate decision tracked in
[`../notes/git-sync-images-and-repo-size.md`](../notes/git-sync-images-and-repo-size.md).
That lever shrinks N and the clone; this one stops the walk from paying for N. They
compose.

## What this does *not* touch

The network half of `:sync` (TLS handshake + push round-trips, ~6.5 s of the warm
path) is a separate floor covered in [`sync-latency.md`](../notes/sync-latency.md);
this curve is only about the local commit. Radio *frequency* (how often we pay any
sync at all) is [`wifi-auto-sync.md`](wifi-auto-sync.md).
