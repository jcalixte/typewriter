# Commit-staging cost vs working-tree size

> **Decision (pending measurement):** keep the file-agnostic `add_all(["*"])`
> staging, or switch to explicit-path staging (`add_path` over the editor's dirty
> set)? The fork is worth taking **only if the FAT working-tree walk dominates the
> ~4 s commit** — which the split-timer added to
> [`../../firmware/src/git_sync.rs`](../../firmware/src/git_sync.rs)
> (`stage_and_commit`, the `commit split —` log line) resolves. This note records
> the cost model and the rule the measurement decides.
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
config/attributes re-reads). `git_bench` (`firmware/src/bin/git_bench.rs`) times
`odb.write` / `index.write` / `write_tree` in isolation to localize it — **run
pending.**

### The walk is ~1.4 s even at N ≈ 2

Mostly fixed cost — the worktree-diff setup and the second (`update_all`) pass —
not per-file `stat` (one raw `stat` is ~5 ms, so N ≈ 2 can't be the 1.4 s). The
O(N) slope only bites on the real `jcalixte/notes` clone (N ≈ 1179), which this run
did **not** exercise. That slope is still unmeasured.

For orientation: `publish(commit+push)` was 9846 ms cold, so the **network half is
~6.5 s** — still the biggest single block of a warm sync (10.1 s total), a separate
floor ([`../notes/sync-latency.md`](../notes/sync-latency.md)).

## The verdict (provisional — pending `git_bench`)

Two things are now settled and one is open:

- **Settled: the card is fast.** The SD-clock and better-card levers are off the
  table — they target I/O that costs ~86 ms, not the ~700 ms we see. Do not spend
  the PCB's 20 MHz budget expecting a commit-latency win here.
- **Settled: explicit-path staging is still worth doing** — but on *design +
  big-repo* grounds, not toy-repo latency (its measured payoff there is ~0.7 s). It
  **caps the O(N) walk on the 1179-file target**, **never visits the ~260 images**
  (150 MB it would otherwise scan), lets us **drop the macOS-cruft filter**, and
  aligns the git layer with what the editor changed.
- **Open: the ~600 ms/op libgit2 overhead** is now the largest single mystery in
  the commit and likely the highest-value fix — if it's ODB refresh or reflog
  writes, it may be a cheap config/flag change that speeds up *every* commit
  regardless of repo size or staging. `git_bench` decides. **Localize it before
  committing effort to (a).**

**Recommendation:** run `git_bench` to pin the libgit2 overhead; then implement
explicit-path staging for the design + big-repo reasons; the SD/card levers are
retired.

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
