# Sync latency — where the ~16 s cold `:sync` goes

> **Measured 2026-07-11** on hardware, via the `:sync timing —` log line in
> [`firmware::git_sync`](../../firmware/src/git_sync.rs) (`publish_cycle`). A
> **cold** `:sync` (first of a power cycle) is **~16.0 s** power-on of Wi-Fi →
> `push done`; a **warm** one skips the one-time setup and is just the ~10 s
> publish. This note breaks the number down and records why most of it is a
> floor, not a bug.
>
> Notes index: [`README.md`](README.md). Docs index:
> [`../README.md`](../README.md). Why the raw number matters less than it looks:
> [`ctrl-g-perceived-latency.md`](ctrl-g-perceived-latency.md). Energy/keep-Wi-Fi-up
> tradeoff: [`../tradeoff-curves/wifi-auto-sync.md`](../tradeoff-curves/wifi-auto-sync.md).
> Sibling timing note: [`boot-time-budget.md`](boot-time-budget.md).

## The waterfall (cold sync)

From the serial log, first `:sync` after a cold boot
(`… wifi 3654ms, clock 2108ms, tls 304ms, publish(commit+push) 9944ms, total 16012ms`):

| Phase                             |        ~ms | One-time?             | Lever                                                                      |
| --------------------------------- | ---------: | --------------------- | -------------------------------------------------------------------------- |
| Wi-Fi assoc + DHCP                |      ~3650 | yes (per power cycle) | radio off until first `:sync`; association floor                           |
| SNTP first sync                   |      ~2100 | yes                   | varies with NTP RTT (4.2 s the prior run); needed before TLS + commit time |
| TLS trust store install           |       ~300 | yes                   | write ~6 KB CA bundle to SD + set libgit2 option                           |
| **publish** = stage+commit + push |  **~9900** | **every sync**        | see below                                                                  |
| **Total**                         | **~16000** |                       |                                                                            |

The three one-time phases (~6.1 s) only pay on the _first_ sync of a power cycle —
Wi-Fi, the clock, and the trust store are set up once and reused, so a **warm sync
is just the ~10 s publish**. Publish splits as:

| Sub-phase                     |   ~ms | Note                                                                 |
| ----------------------------- | ----: | -------------------------------------------------------------------- |
| stage + commit                | ~3150 | `add_all(["*"])` walking the SD/FAT working tree, then commit to FAT |
| push: TLS handshake           | ~2400 | one mbedTLS handshake to github.com                                  |
| push: pack negotiate + upload | ~4400 | tiny delta — cost is negotiation/round-trips, not payload            |

## The win: one TLS handshake, not two

The first hardware run (2026-07-11) measured **23.7 s** because it did a
**pre-commit fetch** — a second full TLS handshake plus a ref exchange — on every
sync, to absorb a foreign push before committing. That's ~3 s wasted on a normal
sync (remote unchanged), and it did ~6 s of real work the one time it absorbed a
maintenance commit.

The optimistic-retry rewrite (commit `3386969`) drops it: **push onto the current
tip first**; only if the remote _rejects_ the push non-fast-forward do we fetch,
reconcile, and retry. The happy path — what runs ~99 % of the time — is now a
**single** handshake. That took the true normal-cold baseline from ~19 s to
**16.0 s** (and the inflated 23.7 s figure will never recur, since it was the
one-time reconcile).

## Foreign pushes: reconcile-and-replay, last-writer-wins

On a rejected push, `reconcile_onto_origin` fetches origin and does a **mixed**
reset onto it — moving the branch ref + index but leaving the working tree, so the
just-saved note survives — then `stage_and_commit` replays the note on the new tip
and retries. For this **single-writer appliance** that resolves last-writer-wins:
a concurrent remote _edit_ to the same note loses to ours, and a remote-only
_added_ file the card doesn't have would be dropped by the replay's `add --all`.
Both need a real merge (increment B) and don't arise from the device's own use.
This path is **hardware-verified for the happy case** (`9b635c42` fast-forwarded
clean); the reconcile branch itself is compile-verified but not yet exercised on
device.

## Can cold sync go lower?

The big rocks are physics or protocol, not slack:

- **Wi-Fi assoc ~3.6 s** and **SNTP ~2–4 s** are one-time per power cycle and
  mostly out of our hands (association floor, NTP RTT). Keeping Wi-Fi up between
  syncs trades battery for latency — see
  [`../tradeoff-curves/wifi-auto-sync.md`](../tradeoff-curves/wifi-auto-sync.md).
- **TLS handshake ~2.4 s** and **push negotiate/upload ~4.4 s** are inherent to
  libgit2-over-mbedTLS on this part; the payload is tiny, so there's little to
  shave.
- **stage + commit ~3.1 s** is the one soft spot: staging over the editor's dirty
  set (`add_path`) instead of `add_all(["*"])` would skip the SD/FAT tree walk
  (likely → sub-second) _without_ losing multi-file — the dirty set is the file
  list. Whether the walk actually dominates the ~4 s commit is now being measured
  by the `commit split —` log line; the cost model and the rule it decides live in
  [`../tradeoff-curves/sync-commit-staging.md`](../tradeoff-curves/sync-commit-staging.md).

**Conclusion:** ~16 s cold / ~10 s warm is close to the floor for "commit to FAT +
one TLS push over Wi-Fi with a fresh clock." It reads as slow only if you wait on
it — and by design you don't: `:sync` is a deliberate action with a snackbar, and
[`ctrl-g-perceived-latency.md`](ctrl-g-perceived-latency.md) argues the perceived
cost is set by _when durability is surfaced_, not by wall-clock. Recorded here so
the number is scoped against the protocol, not treated as a regression.
