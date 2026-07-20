# Gate-scan restriction spike — refuted: partial waveform time is MUX-independent

> Date: 2026-07-16 · Status: **CLOSED — negative result, reverted the same day.**
> The panel's ~545 ms partial-refresh floor stands; the only lever left is a
> custom LUT (not attempted, trade-offs below).
>
> Context: follow-on to the same-day
> [bank-toggle postmortem](2026-07-16-partial-refresh-bank-toggle.md), whose
> timing analysis flagged this lever. Panel + driver port:
> [ADR-003](../adr.md#adr-003-display-medium--e-ink-gdey0579t93-panel),
> driver: [`../../firmware/src/epd.rs`](../../firmware/src/epd.rs).

## Summary

Hypothesis: partial refreshes take ~545 ms regardless of band height because
the SSD1683 rides its power-on defaults and scans **all 300 gate lines** (the
panel has 272) on every update — so programming the gate registers to scan
only the refresh band (20 lines for one text row) should cut the waveform to
a fraction and transform typing feel.

Hardware said no, twice:

1. **No scaling.** With the scan restricted to 20 gates, the partial still
   ran 571 ms (vs 543 ms full-scan baseline). The waveform's BUSY time is set
   by its phase schedule (the LUT), not by how many gates are driven.
2. **A write-only hazard.** Writing driver output control (`0x01`) with the
   *datasheet* power-on scan-order byte **mirrored the panel vertically** —
   the operating gate configuration is loaded from panel OTP at reset,
   differs from the datasheet POR, and cannot be read back. Every partial
   then painted mirrored bands, desyncing both RAM banks (splash ghosts
   persisted through refreshes).

Reverted the same session. Verdict: **never write `0x01`/`0x0F` on the
GDEY0579T93**; `update_part`'s doc comment carries the warning.

## The experiment

Neither GxEPD2 nor the Good Display factory demo ever writes driver output
control (`0x01`, MUX = number of gate lines scanned) or gate scan start
position (`0x0F`) for this panel — confirmed against upstream source. Both
ride the power-on defaults. Combined with the bank-toggle session's
measurement (windowed 20-row band 543 ms vs full-area 629 ms — only the SPI
transfer scales), the "scan time = MUX × line time" model predicted a
one-line refresh somewhere around 40–140 ms.

The spike added, inside `update_part` and mirrored to both controllers
(master `0x00` / slave `0x80`, like the RAM-window commands):

```
0x01  [h-1 low, h-1 high, 0x00]   # MUX = band height, datasheet-POR order byte
0x0F  [y0 low, y0 high]           # scan starts at the band's first row
… partial kick (0x3C/0x21/0x22/0x20), busy-wait …
0x01/0x0F restored to (0, 300)    # assumed power-on defaults
```

The full-refresh path was untouched.

## What the flash showed

- `windowed refresh: 571 ms` for a 20-gate scan — the registers demonstrably
  took effect (see next line), so this cleanly refutes MUX-proportional
  timing.
- The image came up **mirrored across the X axis**, and the boot splash
  (circle + wordmark) never cleared: with the gate mapping flipped, every
  partial landed its band at the mirrored position, so the two RAM banks
  desynced across the whole panel.
- Full-area partials ran 690 ms (vs 629 baseline) — even the "free" 272-vs-300
  gate trim bought nothing.

## Why

- **Refresh duration lives in the LUT, not the scan.** An e-ink refresh plays
  a per-transition waveform: phases of ±15 V held for fixed numbers of
  frames. BUSY ends when the phase schedule ends. Driving fewer gates per
  frame doesn't shorten the schedule.
- **Datasheet POR ≠ operating config.** Good Display panels load their real
  gate setup (scan order/direction, interlacing) from panel OTP at reset.
  The registers are write-only, so there is no safe value to "restore" — any
  write of `0x01` gambles against calibration you can't read. This is the
  same lesson shape as the bank-toggle bug: the factory init sequence is the
  contract, including the registers it *doesn't* touch.

## The remaining lever: a custom LUT (not attempted)

The LUT is the waveform program itself: for each pixel transition
(white→black, black→white, no-change) it encodes which voltage to apply and
for how many frames. Our `init()` loads the factory-tuned one from panel OTP
(`0x22 = 0xB1`), temperature-compensated via the sensor reads. Writing our
own via `0x32` with shorter/fewer phases is how hobbyist "fast partial
refresh" projects reach ~100–200 ms on similar panels.

Known costs, which is why this stays parked:

- **Ghosting & contrast** — shorter pulses move the particles less; erased
  pixels shadow sooner and blacks gray out, demanding more frequent full
  refreshes.
- **Panel longevity** — the factory waveform is DC-balanced; a hand-rolled
  unbalanced one can permanently degrade the panel over months.
- **Temperature** — the OTP LUT is selected per temperature reading; one
  custom table behaves differently cold vs warm.
- **No reference** — the OTP waveform isn't pushed; this is
  reverse-engineering with the panel as the test bench.

Today's ~545 ms per keystroke *batch* (not per keystroke) is acceptable for
the one-refresh-per-batch editor model. Revisit only if typing latency
becomes the top user complaint.

## Lessons

- **Registers you can't read back are write-only hazards.** When a factory
  init never touches a register, that silence may itself be load-bearing —
  OTP-loaded state can hide behind any POR value in the datasheet.
- **Cheap refutation is good spike economics.** One driver-level change,
  one flash, and the existing per-refresh timing logs settled a "very
  promising" theory in minutes — including a failure mode (mirroring) that
  proved the writes took effect, making the negative timing result trustworthy.
