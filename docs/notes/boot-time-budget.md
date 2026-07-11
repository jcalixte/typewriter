# Boot-time budget — where the ~4.3 s to cursor goes

> **Measured 2026-07-11:** cold boot is **4258 ms** power-on → cursor (the
> `boot: cursor ready` log prefix). That clears the **≤ 5 s v0.1 gate** with
> ~742 ms to spare. This note breaks the number down, and argues that the
> **≤ 3 s v1.0 target** is hard on this panel because one ~1.9 s full refresh is
> architecturally unavoidable at cold boot.
>
> Notes index: [`README.md`](README.md). Docs index:
> [`../README.md`](../README.md). Backs the boot-time acceptance criterion in
> [`../v0.1-mvp-product.md`](../v0.1-mvp-product.md#acceptance-criteria) and the
> v1.0 goal in [`../macroplan.md`](../macroplan.md). Refresh cost model:
> [`../tradeoff-curves/epd-refresh-latency.md`](../tradeoff-curves/epd-refresh-latency.md).

## The waterfall

From the boot serial log (power-on → first editor frame + input loop live):

| Phase | ~ms | Lever |
| --- | ---: | --- |
| ROM + 2nd-stage bootloader + app image load | ~550 | flash speed (DIO now; QIO / 80 MHz ≈ −200 ms, speculative) |
| PSRAM init + **memtest** + heap | ~920 | `CONFIG_SPIRAM_MEMTEST=n` → **−730 ms** (kept **on**: a real HW sanity check on a hand-wired board) |
| EPD reset + init | ~130 | fixed panel bring-up |
| **Splash full refresh** | ~1850 | e-ink floor — see below |
| SD mount + note load | ~70 | quick on the genuine 32 GB SDHC |
| USB host install + git thread spawn | ~60 | background |
| **First editor render** (full-area partial) | ~680 | already fixed from ~1870 ms (was a *second* full refresh) |
| **Total** | **~4260** | |

Two lines carry the weight: the **splash full refresh (~1.85 s)** and the
**first editor render (~0.68 s)**. Everything else is ≤ ~0.9 s combined, and the
biggest of *those* — the ~0.73 s PSRAM memtest — is a deliberate keep.

## The insight: one full refresh is unavoidable, so the splash is nearly free

After power-on the panel controller's `0x26` "previous" RAM bank holds garbage. A
partial refresh *diffs the new image against that bank*
([`../tradeoff-curves/epd-refresh-latency.md`](../tradeoff-curves/epd-refresh-latency.md)),
so the **first clean paint must be a full refresh** (~1.9 s) to establish a known
image. There is no way around this on this panel short of a different waveform.

That reframes two things:

- **The splash costs almost nothing.** Boot needs one full refresh regardless; the
  splash simply *is* that refresh, turned into a "boot is happening" affordance.
  Dropping the splash would **not** save the 1.9 s — the editor's first frame would
  then have to be the full refresh instead. (This is exactly what the old boot did
  and why it paid *two* full refreshes.)
- **The v0.1 win was removing the second full refresh, not the first.** Once the
  splash has seeded a clean baseline, the editor rides in on a full-area *partial*
  (~0.63 s) instead of a second full refresh (~1.9 s) — the ~1.25 s saving that
  took cold boot from ~5.5 s to ~4.26 s. Verified clean on-panel (no splash
  ghost behind the editor text).

## Is ≤ 3 s (v1.0) reachable?

To go from ~4.26 s to ≤ 3 s needs ~1.26 s cut. The honest lever list:

- **PSRAM memtest off:** −0.73 s → ~3.5 s. Costs the boot-time hardware check;
  reasonable once the board is no longer hand-wired.
- **Faster flash boot (QIO / 80 MHz):** ~−0.2 s, speculative, needs a bench check.
- **Overlap cheap init under the splash busy-wait:** SD mount + note load + USB
  install (~0.13 s total) currently run *after* the splash refresh returns, but
  the refresh is a `wait_while_busy` spin — those could be kicked off before it.
  Saves ~0.1 s at most.

Even stacked, that lands around **~3.2 s** — still over. The ~1.9 s full-refresh
floor is the wall, and it can't be cut without dropping the clean first image or
moving to a faster panel/waveform. **Conclusion:** ≤ 3 s is marginal-to-unreachable
on the GDEY0579T93 as driven today. When v1.0 comes, either revisit the target
(≤ 3.5 s is achievable with the memtest off), or accept the splash as the
deliberate cover for the one refresh e-ink makes us pay. Recorded here so the v1.0
boot-time item is scoped against physics, not optimism.
