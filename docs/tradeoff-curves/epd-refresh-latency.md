# E-ink refresh latency vs rows driven

> **Model:** on this GDEY0579T93 (SSD1683 dual-controller) panel, refresh time is
> set by **how many gate lines (rows, the Y axis) are driven** and **which
> waveform LUT runs** — _not_ by the pixel column width. Three refresh modes fall
> out of that: full refresh (~1870 ms), full-area partial (~630 ms), and
> windowed-Y partial (~100–130 ms for one text line). This is the cost model
> behind per-keystroke typing, the boot splash→editor swap
> ([`../notes/boot-time-budget.md`](../notes/boot-time-budget.md)), and the
> scroll/gutter spikes.
>
> Tradeoff-curves index: [`README.md`](README.md). Docs index:
> [`../README.md`](../README.md). Driver:
> [`../../firmware/src/epd.rs`](../../firmware/src/epd.rs). Bench origin:
> [`../spikes.md`](../spikes.md) (Spikes 5 + 8).

## The model

A refresh is three serial costs:

```
set RAM window  →  clock the pixels out over SPI  →  run the update waveform
   (fixed)          (scales with bytes = rows)        (scales with gate lines
                                                        AND with which LUT)
```

Two things do **not** help, and one thing does:

- **Column width (X) is free to keep full.** The panel is a master (`0x00`) +
  slave (`0x80`) pair with the framebuffer split at the seam; every refresh
  drives _both_ controllers full width so the seam/mirror math stays intact
  (`update_part` / `write_frame_bank` in
  [`epd.rs`](../../firmware/src/epd.rs)). Narrowing the _column_ saves nothing —
  "the waveform time dominates, not the data clock-out."
- **Row count (Y) is the knob.** E-paper drive time scales with the number of
  gate lines transitioned, so restricting the refresh to a horizontal band of
  rows is the real win — a one-line band is far cheaper than the whole panel.
- **The LUT sets the tier.** A _partial_ update (`0x22`←`0xFF`) runs a short
  waveform that only nudges pixels that changed. A _full_ update (`0x22`←`0xD7`,
  the fast-full LUT) runs ~3× as many frames per pixel to fully clear-and-set —
  slower, but it erases ghosting and re-establishes a known image.

Within partial mode the latency is roughly linear in rows:

```
t_partial(rows) ≈ 90 ms + 2 ms · rows
                  └ floor ┘  └ per-gate-line slope ┘
```

Floor ≈ fixed SPI setup + border/VCOM commands + waveform ramp; slope ≈ the
per-row waveform drive. Full refresh sits in its own flat tier (~1870 ms, all 272
rows, full-clear LUT) and **cannot be windowed** — the clear waveform needs the
whole panel.

## The curve

```
  Partial-refresh latency vs rows driven (Y-band)     t ≈ 90 ms + 2 ms · rows

  ms
 1870 |=========================================  FULL refresh — separate tier:
      |                                            all 272 rows, full-clear LUT,
      |                                            un-windowable. ~3× the partial
      |                                            waveform. De-ghosts + seeds a
      |                                            known image.
      |
  630 |                                       * ← full-area partial (272 rows)
      |                                 . ·
      |                           . ·
      |                     . ·         slope ≈ 2 ms / gate line
  300 |               . ·
      |         . ·
  110 |    *  ← one text line (~10 rows): the per-keystroke path
   90 | *  ← floor (SPI setup + border/VCOM + waveform ramp)
      +----+----+----+----+----+----+----+----+----+----+----+---
      0   25   50   75  100  125  150  175  200  225  250  272  rows
```

| Mode               |  Rows driven |                                Latency | LUT        | Used for                                                                           |
| ------------------ | -----------: | -------------------------------------: | ---------- | ---------------------------------------------------------------------------------- |
| Full refresh       |    272 (all) |                **~1870 ms** (measured) | full-clear | first cold-boot image; periodic de-ghost (every `FULL_REFRESH_EVERY` = 64 updates) |
| Full-area partial  |          272 | **~630 ms** (measured; 680 ms at boot) | partial    | deletes, caret moves, mode switches, the snackbar, and the boot splash→editor swap |
| Windowed-Y partial | ~10 (1 line) |           **~100–130 ms** (estimated¹) | partial    | additive per-keystroke typing                                                      |

¹ The single-line windowed figure is projected from the floor+slope model and the
Spike 5 full-area measurement; the exact bench number is still to be confirmed
from the on-device refresh log (`{mode} refresh #N … {ms} ms` in
[`main.rs`](../../firmware/src/main.rs)). The 1870 ms full and 630 ms full-area
figures are measured.

## Two things that bound it

**Ghosting caps the partial streak.** Partial updates leave faint residue, so a
full refresh every 64 updates resets clarity and panel state. You can't "always
partial" — the ~1870 ms tier is a periodic tax paid for longevity, not a mode you
can retire.

**The first cold-boot image must be a full refresh.** After power-on the `0x26`
"previous" bank holds garbage, and a partial refresh _diffs against it_ — so the
very first clean paint has to be the full tier. This is why boot pays exactly one
unavoidable ~1.9 s full refresh, and why the splash (which rides it) is nearly
free while the _editor's_ first frame can be a cheap partial on top. Full
derivation: [`../notes/boot-time-budget.md`](../notes/boot-time-budget.md).

## What it decides

- **Per-keystroke typing → windowed-Y partial.** Only the touched line's band is
  driven (~100–130 ms), so typing keeps up with the keyboard; the panel never
  repaints per keystroke off that line.
- **Boot splash→editor → full-area partial (~630 ms), not a second full refresh
  (~1870 ms).** The splash already seeded the baseline, so the editor rides in on
  a partial — the ~1.25 s cold-boot win recorded in the boot-time budget.
- **Deletes, caret moves, mode flips, snackbar → full-area partial.** These erase
  ink or change the panel off the cursor line, which a windowed band would ghost,
  so they take the whole-panel partial rather than a windowed one.
- **Splash + periodic → full refresh.** The unavoidable first image and the
  every-64 de-ghost.
