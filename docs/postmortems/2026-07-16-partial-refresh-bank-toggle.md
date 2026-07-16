# Display "buffer toggling" — partial refresh left one RAM bank two frames stale

> Date: 2026-07-16 · Build at time of diagnosis: `07-16 19:59Z @61db4a6-dirty`
> Status: **FIXED, verified on device 2026-07-16** — after every partial
> refresh the band is re-written to *both* controller RAM banks (the GxEPD2
> `writeImageAgain` sequence). Lines no longer flap during fast typing.
>
> Context: partial refresh landed in Spike 5, windowed-Y in Spike 13 (both in
> [`../v0.1-mvp-technical.md`](../v0.1-mvp-technical.md#hardware-bring-up-order)),
> panel + driver port [ADR-003](../adr.md#adr-003-display--gdey0579t93-epd-driven-by-a-thin-ssd1683-port),
> driver: [`../../firmware/src/epd.rs`](../../firmware/src/epd.rs), refresh
> policy: [`../../firmware/src/main.rs`](../../firmware/src/main.rs).

## Summary

Since the v0.1 partial-refresh port, writing fast made the panel "toggle":
lines below the caret flapped up and down around an Enter (the new line
appearing, un-appearing, re-appearing), and characters from the previous
keystroke batches flickered out and back in. Everything always settled correct
at the next pause, which is why it survived every spike and soak as a vague
"e-ink shimmer" until a fast-writing session made it undeniable.

Root cause: the SSD1683 **ping-pongs its two RAM banks on every Display Mode 2
(partial) update**, and the driver's partial path re-synced only the `0x26`
bank afterwards — which post-swap is the bank that is *already correct*. The
other bank was left holding the frame from **two refreshes ago**, and the next
partial drove the whole panel toward it. Every second partial update therefore
repainted recent history: new content reverted, then returned, alternating.

One extra band write per refresh fixes it. The cost is ~5 ms on a windowed
keystroke refresh, ~60 ms on a full-area pass.

## Symptom

- Fast typing, especially across an Enter: the lines below the caret jump to
  their shifted position, jump back, jump forward again — "the lines get
  swapped up with what there was before."
- Characters from the previous 1–2 keystroke batches blink off/on (subtle — a
  10 px glyph next to a 20 px line shift).
- Always self-heals at the next pause or Normal-mode action. Saved text was
  never wrong; this was purely a panel artifact.

## Investigation

### Wrong turn first: refresh cadence

The per-refresh log (`windowed/full-area/FULL refresh #N … rows a..=b`) first
pointed at cadence, and three real problems fell out of that pass:

1. the periodic panel-longevity FULL refresh promoted a *keystroke* repaint
   every 64 updates — and since the counter only advanced while typing, its
   ~2 s multi-flash could **only ever** land mid-sentence;
2. the 750 ms Insert caret debounce fired during ordinary mid-sentence pauses,
   and each show + erase-on-resume pair cost two whole-panel passes;
3. any erase — including just the caret bar — forced a full-area pass.

All three were fixed (debounce → 2000 ms; FULL deferred to the idle branch;
caret-bar erase allowed on the windowed path via an erase-bounding-box check).
They made typing calmer — and made the *real* bug *more* visible, which was
the tell: the frequent caret full-area passes had been accidentally healing
the panel every second or two.

### The trace that pinned it

A second serial trace during a repro session proved the firmware side clean:
every refresh band matched the keystroke stream exactly (the Enter's line
shift landed as one `full-area (rows 40..=259)`, subsequent chars as
`windowed (rows 40..=59)`), and no band ever re-painted a reverted layout. The
frames were right; the panel disagreed — so the divergence had to live in the
controller's RAM banks.

Two timing facts closed in:

- windowed (20 rows) vs full-area (272 rows) partials measured **543 ms vs
  629 ms** — the waveform does *not* scale with the band; only the SPI
  transfer does. Every partial update drives the whole panel, so the whole
  panel is exposed to whatever the banks contain, every time;
- the toggling alternated update-by-update — the signature of *two* states
  taking turns, i.e. two banks taking turns being the reference.

### Root cause: Mode-2 ping-pong vs a one-bank resync

The driver's partial sequence was:

```
write 0x24 (band) → update_part → write 0x26 (band)   # WRONG for this panel
```

Upstream GxEPD2 for this exact panel (`GxEPD2_579_GDEY0579T93`) re-writes the
image to **both** banks *after* every partial refresh (`writeImageAgain`:
`0x26` then `0x24`). The reason: on a Mode-2 display the controller swaps the
roles of the two RAM buffers. Post-swap, `0x26` addresses the just-displayed
(correct) buffer and `0x24` addresses the stale one — so syncing only `0x26`
is a no-op, and the stale buffer stays stale:

```
physical buffers P, Q            P            Q          panel shows
full-area refresh (frame F0)     F0           F0         F0
partial #1 (F1, band b1)
  write 0x24[b1]                 F1           F0
  update  → drives diff(P,Q)                             F1   (swap: 0x24→Q)
  write 0x26[b1]  (lands in P)   F1 (no-op)   F0  ← never updated
partial #2 (F2, band b2)
  write 0x24[b2]  (lands in Q!)  F1           F0+b2
  update  → drives diff(Q,P)                             F0+b2  ← b1 REVERTED
partial #3 (F3, band b3)                                 F3     ← b1 restored,
  …                                                              b2 reverted
```

Every second partial repaints toward a buffer that is mostly two frames old.
Any full-frame pass (full refresh, or a full-area partial, which writes all
272 rows around the update) rewrites both physical buffers and heals
everything — which is exactly why the bug hid behind the old 750 ms caret
cadence, slow deliberate spike testing (Spike 13 verified windowed-Y with
single measured edits), and Normal-mode-heavy sessions.

## The fix

[`epd.rs`](../../firmware/src/epd.rs) `display_frame_partial_window` now ends
with the faithful GxEPD2 sequence — band re-written to both banks, `0x26`
then `0x24`:

```
write 0x24 (band) → update_part → write 0x26 (band) → write 0x24 (band)
```

Verified on device 2026-07-16: fast typing across Enters, lines shift once
and stay put; no char flicker from previous batches.

## Shipped alongside (same session)

- `CURSOR_DEBOUNCE_MS` 750 → 2000 ms (no caret churn on mid-sentence pauses).
- Panel-longevity FULL refresh deferred to the idle branch
  (`partials_since_full` counter; never promotes a keystroke repaint anymore).
- `only_adds_ink` → `erase_bbox`: an erase confined to one character cell with
  the caret on screen (the debounced caret bar being re-suppressed) rides the
  windowed path instead of forcing a full-area pass.

## Lessons

- **Port the *sequence*, not the register writes.** The original port read
  GxEPD2's post-refresh writes as "sync the previous bank" and kept only the
  `0x26` write. The `writeImageAgain` name — *again*, same image, both banks —
  was the contract; the ping-pong is why it exists.
- **A bug that self-heals hides behind anything that heals it.** The caret
  debounce was accidental hygiene; removing it exposed the real fault.
  Cadence fixes that make a display *worse* are a strong hint the remaining
  fault is state, not policy.
- **Prove which layer owns the corruption before touching it.** The refresh
  log's per-band rows made it possible to verify every frame against the
  keystroke stream and exonerate the firmware — after that, the panel RAM was
  the only suspect left.
- **Measured timings encode architecture.** Windowed vs full-area at
  543/629 ms said "the waveform drives all gates regardless" — which both
  scoped the blast radius of stale RAM and flagged the next lever: restricting
  the gate scan so single-line updates get *actually* fast (open, future perf
  pass).
