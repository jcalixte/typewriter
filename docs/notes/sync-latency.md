# Sync latency — where the ~16 s cold `:gp` goes

> **Measured 2026-07-11** on hardware, via the timing log line in
> [`firmware::git_sync`](../../firmware/src/git_sync.rs) (`push_cycle`;
> the command was `:sync` then, renamed `:gp` 2026-07-14). A **cold** push
> (first of a power cycle) is **~16.0 s** power-on of Wi-Fi → `push done`; a
> **warm** one skips the one-time setup and is just the ~10 s push. This
> note breaks the number down and records why most of it is a floor, not a
> bug.
>
> **Update (2026-07-13/14) — the push half below is superseded.** The
> `add_all` index staging was replaced by the O(depth) splice over the
> journaled dirty set ([kaizen](../kaizen/real-repo-sync.md) · [measurement
> trail](../tradeoff-curves/sync-commit-staging.md)), reconcile became fetch +
> **soft** reset + journal replay, and TLS session resumption reuses the
> handshake on reconnects. On the **real notes repo** — which the method below
> could never complete at all — a cold `:gp` is **24.1 s**, a warm one ≈ 19 s
> (splice depth × loose-write cost dominates), and an up-to-date `:gl` is
> ≈ 4.7 s git-side. The waterfall below stands as the dev-repo record of
> 2026-07-11.
>
> Notes index: [`README.md`](README.md). Docs index:
> [`../README.md`](../README.md). Why the raw number matters less than it looks:
> [`ctrl-g-perceived-latency.md`](ctrl-g-perceived-latency.md). Energy/keep-Wi-Fi-up
> tradeoff: [`../tradeoff-curves/wifi-auto-sync.md`](../tradeoff-curves/wifi-auto-sync.md).
> Sibling timing note: [`boot-time-budget.md`](boot-time-budget.md).

## The waterfall (cold sync)

From the serial log, first `:sync` after a cold boot
(`… wifi 3654ms, clock 2108ms, tls 304ms, push(commit+push) 9944ms, total 16012ms`):

| Phase                             |        ~ms | One-time?             | Lever                                                                      |
| --------------------------------- | ---------: | --------------------- | -------------------------------------------------------------------------- |
| Wi-Fi assoc + DHCP                |      ~3650 | yes (per power cycle) | radio off until first `:sync`; association floor                           |
| SNTP first sync                   |      ~2100 | yes                   | varies with NTP RTT (4.2 s the prior run); needed before TLS + commit time |
| TLS trust store install           |       ~300 | yes                   | write ~6 KB CA bundle to SD + set libgit2 option                           |
| **push** = stage+commit + push |  **~9900** | **every sync**        | see below                                                                  |
| **Total**                         | **~16000** |                       |                                                                            |

The three one-time phases (~6.1 s) only pay on the _first_ sync of a power cycle —
Wi-Fi, the clock, and the trust store are set up once and reused, so a **warm sync
is just the ~10 s push**. Push splits as:

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

**Update 2026-07-14:** the full rejected-push → reconcile → replay → push
cycle is now hardware-verified (24.0 s end-to-end with TLS session
resumption). The mechanics also changed with the splice: the reset is
**soft** (there is no index anymore) and the replay splices only the
journaled dirty paths, so a remote-only added file now _survives_ — it is
carried forward by OID instead of being dropped by an `add --all`.

## Can cold sync go lower?

The big rocks are physics or protocol, not slack:

- **Wi-Fi assoc ~3.6 s** and **SNTP ~2–4 s** are one-time per power cycle and
  mostly out of our hands (association floor, NTP RTT). Keeping Wi-Fi up between
  syncs trades battery for latency — see
  [`../tradeoff-curves/wifi-auto-sync.md`](../tradeoff-curves/wifi-auto-sync.md).
- **TLS handshake ~2.4 s** and **push negotiate/upload ~4.4 s** are inherent to
  libgit2-over-mbedTLS on this part; the payload is tiny, so there's little to
  shave.
- **stage + commit ~3.1 s** was the one soft spot — and the one that got
  attacked. **Resolved 2026-07-13:** the index path was replaced outright by
  the O(depth) TreeBuilder splice over the journaled dirty set (the `add_path`
  staging this note originally proposed still hits the index's racy-clean
  wall). The cost
  model and how each hypothesis died live in
  [`../tradeoff-curves/sync-commit-staging.md`](../tradeoff-curves/sync-commit-staging.md);
  the residual is FAT directory-op cost per loose write, bounded and accepted.

**Conclusion:** ~16 s cold / ~10 s warm is close to the floor for "commit to FAT +
one TLS push over Wi-Fi with a fresh clock." It reads as slow only if you wait on
it — and by design you don't: `:gp` is a deliberate action with a snackbar, and
[`ctrl-g-perceived-latency.md`](ctrl-g-perceived-latency.md) argues the perceived
cost is set by _when durability is surfaced_, not by wall-clock. Recorded here so
the number is scoped against the protocol, not treated as a regression.
