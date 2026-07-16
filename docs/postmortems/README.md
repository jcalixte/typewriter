# Postmortems

> Bench + bring-up debugging write-ups: what broke, how we found the root cause,
> and the decisions that came out of it. One file per incident, named
> `YYYY-MM-DD-<slug>.md`. These capture _why_ a spike stalled or a design turned
> — the kind of context that's expensive to reconstruct later.
>
> Project overview: [`../../README.md`](../../README.md). Bring-up spikes:
> [`../v0.1-mvp-technical.md`](../v0.1-mvp-technical.md#hardware-bring-up-order).

| Date       | Incident                                                                                                         | Status                                                                               |
| ---------- | ---------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| 2026-07-05 | [Spike 3 (SD) — card rejects CMD59 (SPI-mode CRC)](2026-07-05-spike3-sd-cmd59.md)                                | Paused — awaiting a compliant microSD; wiring + firmware proven                      |
| 2026-07-05 | [Spike 7 (git push) — ADR-004 kill-switch fired: gix can't push over HTTPS](2026-07-05-spike7-gix-https-push.md) | Turned — pivoted to libgit2; git mechanics proven on desktop, device build next      |
| 2026-07-11 | [Editor freeze — SPI-DMA OOM during a background `:sync`](2026-07-11-editor-freeze-spi-dma-oom-during-sync.md)   | Safety net shipped (paints non-fatal); root-cause eradication specced, not yet built |
| 2026-07-16 | [Display "buffer toggling" — partial refresh left one RAM bank two frames stale](2026-07-16-partial-refresh-bank-toggle.md) | Fixed + verified on device — band re-written to both banks post-refresh (ping-pong) |
