# Tradeoff curves

> Where a design knob has a cost that bends — energy, latency, memory — against
> an interval or size, the curve and its knee live here, so the chosen default
> is traceable to a shape rather than a guess.
>
> Docs index: [`../README.md`](../README.md). Project overview:
> [`../../README.md`](../../README.md).

| Curve                                              | What it decides                                                                                                                                                                                                                                                                                                   |
| -------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`wifi-auto-sync.md`](wifi-auto-sync.md)           | `auto_sync` interval vs Wi-Fi energy (a `1/T` hyperbola) — why the default is 10 min and opportunistic, not a wall-clock timer.                                                                                                                                                                                   |
| [`epd-refresh-latency.md`](epd-refresh-latency.md) | E-ink refresh latency vs rows driven — the full / full-area-partial / windowed-Y cost model behind typing responsiveness and the boot splash→editor swap.                                                                                                                                                         |
| [`sync-commit-staging.md`](sync-commit-staging.md) | Commit-staging strategy vs working-tree size — RESOLVED: every index-based path is O(N_tree) and fails on the real repo (611 s / OOM); the O(depth) TreeBuilder splice is benched at ~2–2.8 s and ships. Holds the full measurement trail plus the firmware plumbing plan (merged from the retired handoff note). |
