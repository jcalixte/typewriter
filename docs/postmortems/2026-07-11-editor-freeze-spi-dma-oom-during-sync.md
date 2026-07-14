# Editor freeze — SPI-DMA out-of-memory during a background `:sync`

> Date: 2026-07-11 · Build at time of failure: `07-11 18:02Z @229c259-dirty`
> Status: **Safety net shipped and hardware-verified** (2026-07-13, during the
> [real-repo-sync kaizen](../kaizen/real-repo-sync.md): live scrolling through
> a real-repo push — the run-4 crash was this same paint-during-push family,
> and after the fixes the UI survived every subsequent push, success and
> failure alike). The paint path also **no longer allocates per repaint**
> (persistent two-frame `draw_into`/`swap` in `main.rs`, 2026-07-13), removing
> the frame-buffer half of the paint-time allocation. **Root cause not fully
> eradicated**: the EPD driver's per-refresh internal DMA scratch remains; the
> permanent fix (a persistent scratch buffer in `Epd`) is specced below and
> now tracked in [v0.9 robustness](../v0.9-robustness.md).
>
> Context: editor loop [`../../firmware/src/main.rs`](../../firmware/src/main.rs),
> EPD driver [`../../firmware/src/epd.rs`](../../firmware/src/epd.rs), git
> transport [`../../firmware/src/git_sync.rs`](../../firmware/src/git_sync.rs).
> Display medium [ADR-003](../adr.md#adr-003-display-medium--e-ink-gdey0579t93-panel);
> concurrency model (dedicated git thread) [ADR-006](../adr.md).

## Summary

First-ever field freeze. After an hour of clean editing, the user ran `:sync`;
the commit and push **succeeded** (`push accepted by remote`, commit
`48a2c0a8`), but partway through the push the panel stopped updating. Keystrokes
kept being logged, so the device wasn't hung — but nothing repainted again.

The editor task had died. A screen refresh that happened to run **while Wi-Fi +
TLS were up for the push** failed to allocate an internal DMA buffer
(`ESP_ERR_NO_MEM`), and that error propagated through a `?` straight out of
`main()`. ESP-IDF's `main_task` returned from `app_main()`; the editor loop lived
on that task, so it stopped. The USB-keyboard and git threads are **separate
FreeRTOS tasks** (ADR-006) and kept running — hence keys still logged while the
panel was frozen. A zombie, not a crash.

## Symptom

```
I (305749) firmware::git_sync: verifying github.com TLS chain against embedded GitHub CA bundle
I (305979) firmware::usb_kbd: key: HalfPageUp          ← scroll arrives → editor loop paints
E (306259) spi_master: setup_dma_priv_buffer(1206): Failed to allocate priv TX buffer
Error: ESP_ERR_NO_MEM
I (306259) main_task: Returned from app_main()         ← editor task exits
...
I (310119) firmware::git_sync: push accepted by remote ← git thread unaffected; commit landed
I (311069) firmware::usb_kbd: key: Escape              ← keys still logged, no refresh ever again
I (312729) firmware::usb_kbd: key: HalfPageDown
```

## Root cause

The EPD is on SPI2 configured `Dma::Auto(4096)` (`main.rs`), and the driver hands
frame data to `spi.write()` as ordinary Rust `Vec<u8>` buffers (`epd.rs`
`write_frame_bank` / `data`). With PSRAM added to the heap allocator, those
`Vec`s can be allocated **from PSRAM**, which is **not DMA-capable**. When a
transfer buffer isn't DMA-capable, esp-idf's `spi_master` bounces it through a
temporary internal buffer it mallocs on the fly (`setup_dma_priv_buffer`, using
`MALLOC_CAP_DMA` → **internal RAM only**).

For the whole session that bounce allocation succeeded, because internal RAM was
plentiful. The instant `:sync` brought up **Wi-Fi + TLS**, they consumed the
small internal pool. The next paint's bounce allocation returned
`ESP_ERR_NO_MEM`. The real defect is not the low-memory moment itself — it's that
the paint's `?` made a **transient, retryable** I/O failure **fatal to the whole
appliance**.

Two things made this easy to misread:

- **"8.4 MB free heap" is a red herring.** That figure is dominated by PSRAM.
  The starved pools are the tiny internal ones (~265 KB / 21 KB / 32 KB), and DMA
  bounce buffers can only come from those.
- **It looked like a git/sync bug, but the push was fine.** The git thread
  finished and the remote accepted the commit. Only the UI task died.

## Timeline

1. Hour of normal editing — hundreds of ~630 ms partial refreshes, no issue
   (internal RAM never under pressure; Wi-Fi off — the radio is lazy, ADR-006).
2. `:sync` → save → git thread brings up Wi-Fi, SNTP, TLS, commits `48a2c0a8`,
   starts the push. Internal RAM now under heavy Wi-Fi/TLS load.
3. User keeps scrolling (`HalfPageUp`) during the push. Each scroll triggers a
   full-area partial refresh → SPI-DMA transfer → bounce-buffer malloc.
4. One such malloc fails (`ESP_ERR_NO_MEM`); the `?` in the refresh call
   propagates out of `main()`; `main_task` returns from `app_main()`.
5. Push completes normally; Wi-Fi torn down; internal RAM freed — but the editor
   task is already gone, so nothing repaints. Keys log into the void.

## What it was _not_

- **Not out of heap** — plenty of PSRAM free; it was internal _DMA-capable_ RAM
  specifically.
- **Not a git or TLS bug** — the sync succeeded end to end.
- **Not an editor-core bug** — `editor`/`keymap` never ran; the failure is in the
  firmware paint path's error handling.
- **Not a panel/wiring fault** — the same paint path worked for an hour and works
  again after Wi-Fi is down.

## Remediation shipped — paints are non-fatal

A screen refresh is idempotent and retryable: the editor buffer is the source of
truth, so a dropped frame costs nothing and the next paint recovers. This is the
exact contract already written into `save_note` ("errors are logged, never
propagated"). Every paint site in the editor loop (`main.rs`) now:

- logs the failure and **drops the frame** instead of `?`-propagating it;
- leaves `shown` untouched so the next paint repaints the same diff;
- sets a `force_full` flag so the next paint is a **full refresh**, which
  rewrites both RAM banks and recovers from a partial that may have died
  mid-transfer and left the `0x24`/`0x26` banks inconsistent.

Effect: a paint that lands during a sync is dropped (panel goes stale for the
~15 s of the push), then self-heals the moment the push finishes and internal RAM
frees. A permanent brick becomes a brief stale window. The boot-time first render
stays `?`-fatal on purpose — it runs before Wi-Fi, can't hit this, and a dead
panel at boot is a legitimate hard fault.

## Root-cause eradication (specced, not yet built)

The safety net stops the brick but not the underlying **contention**: paints
during a sync still fail and drop. To keep the editor fully live while a push
runs — the whole point of the async git thread (ADR-006) — remove the on-the-fly
DMA allocation entirely.

**Fix: a persistent, internal, DMA-capable scratch buffer owned by `Epd`.**

- Allocate it **once at `Epd` construction** (boot, before Wi-Fi is ever up, when
  internal RAM is plentiful) via
  `heap_caps_malloc(SPI_CHUNK, MALLOC_CAP_DMA | MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT)`.
- `data()` already chunks writes to `SPI_CHUNK` (4096 B), so a **4 KB** scratch
  buffer is sufficient: copy each chunk into it and hand that DMA-capable slice to
  `spi.write()`.
- Because the source is now DMA-capable, `spi_master` DMAs directly from it —
  `setup_dma_priv_buffer` is never invoked, there is **no per-refresh
  allocation**, and paints no longer compete with Wi-Fi/TLS for internal RAM.
  Refreshes then succeed during a sync, and `force_full` recovery is only ever
  exercised by genuine faults, not by normal syncing.

**Cost:** ~4 KB of internal RAM reserved for the life of the device; one small
`unsafe` block for the alloc + slice; a `memcpy` per 4 KB chunk (negligible
against the ~630 ms panel waveform).

**Alternatives considered and rejected:**

- _Trim Wi-Fi's internal buffer counts_ to leave headroom — fragile tuning that
  risks Wi-Fi stability/throughput and only widens the margin instead of removing
  the failure mode.
- _Force the whole ~13.6 KB frame buffer internal_ (custom allocator / full
  `heap_caps` buffer) — larger reservation and more churn than the 4 KB
  chunk-scratch, for no extra benefit since `data()` already chunks.
- _Serialize paints against sync_ (skip painting while a push is in flight) —
  defeats the async-git design and freezes the panel for the whole push; the
  safety net already makes this unnecessary.

**Verification when built:** reproduce the exact failing scenario — edit,
`:sync`, and keep scrolling/paging through the _entire_ push. Expect: every
refresh succeeds (no `refresh … FAILED` warnings, no dropped frames), min-ever
internal heap stays comfortably above zero throughout, and `force_full` is not
triggered by the sync.

## Follow-ups

- [x] Make all editor-loop paints non-fatal + `force_full` recovery (`main.rs`);
      release build green.
- [x] Reflash and hardware-verify the safety net against the repro (edit →
      `:sync` → scroll through the push): panel must recover, not freeze.
      **Verified 2026-07-13** (real-repo-sync kaizen runs 5–9: concurrent
      typing/scrolling through full pushes, no freeze; bonus fix — repaints no
      longer allocate at all, closing the run-4 `Frame::new_white` OOM-abort
      variant of the same failure).
- [ ] Implement the persistent internal DMA scratch buffer in `Epd` (eradication
      above) if the stale-during-sync window proves annoying in real use —
      → tracked in [v0.9 robustness](../v0.9-robustness.md).
- [ ] After eradication, confirm refreshes succeed _during_ a push and drop the
      stale window entirely.
