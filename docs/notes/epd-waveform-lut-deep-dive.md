# LUT deep dive: SSD1683 bit-packing & the white→black partial

Companion to [How an e-paper waveform works](./epd-waveforms.md). This is the
byte-by-byte reading of our `FAST_PARTIAL_LUT` (`firmware/src/drivers/screen_epd.rs`)
and the shape a *correctly tuned* white→black partial transition should take.

## Part A — The SSD1683 LUT bit-packing, byte by byte

Our `FAST_PARTIAL_LUT` is exactly 153 bytes, and that number isn't arbitrary — it's
the fixed size the `0x32` register expects on this controller family. It splits into
three regions:

```
 bytes  0 – 59   (60)  VS   — voltage-selection: which level, per phase
 bytes 60 –143   (84)  TP   — timing: how many frames each phase runs
 bytes144 –152   ( 9)  cfg  — frame-rate / gate-scan / spare
 ─────────────────────
 total          153
```

### The VS region (60 bytes) = 5 LUTs × 12 phases

The 60 bytes are **5 rows of 12** — matching how the array is literally laid out.
Each *row* is one LUT; each *column* is one of 12 possible phases:

- **4 of the rows** are the pixel-transition LUTs, indexed by (old pixel, new pixel)
  read from the two RAM banks — `(W→W), (W→B), (B→W), (B→B)`.
- **The 5th row is VCOM** — the common-electrode's own level sequence (the reference
  the pixel voltage is measured against).

Each byte holds **four 2-bit level codes**, one per sub-step (A/B/C/D) inside that
phase. The codes:

| code | level | effect |
|------|-------|--------|
| `00` | VSS (0 V) | hold — no movement |
| `01` | VSH1 (+) | push one way |
| `10` | VSL (−) | push the other |
| `11` | VSH2 (+, 2nd) | gray-level push |

So `0x40` = `01·00·00·00` = "VSH1 in sub-step A, hold otherwise." `0x80` =
`10·00·00·00` = "VSL in sub-step A."

### Decoding *our actual* bytes

Only the first two phase-columns are non-zero. Here's the whole waveform, decoded:

```
              phase 0        phase 1
LUT0  W→W?    --             VSH1(+)         (0x40)
LUT1  ...     VSL(-) 0x80    VSL(-)  0x80
LUT2  ...     VSH1(+) 0x40   VSH1(+) 0x40
LUT3  ...     --             VSL(-)  0x80
LUT4  VCOM    --             --              (flat at 0 V)

timing:  phase0 = FAST_PHASE0_FRAMES (15) frames
         phase1 = 1 frame + 1 frame  (the "two touch-up phases")
         phases 2–11 = all zero (unused)
```

That's the entire recipe: **one 15-frame main drive, one 2-frame touch-up, nothing
else, VCOM held flat.** Each timing group is 7 bytes — the leading bytes are the
sub-step frame counts (A/B/C/D), followed by a repeat count and a couple of
spare/gate-scan bytes.

> Honest caveat: the *exact* semantics of the last two timing bytes and the precise
> LUT↔transition ordering drift between SSD168x datasheet revisions. The VS decode
> above is confident because it round-trips against the bytes; treat the tail-byte
> labels as "shape, not gospel."

The 9 trailing config bytes (`0x22×6, 0x00×3`) set the frame-rate divider and
gate-scan options — panel-wide, not per-pixel.

## Part B — What a *proper* white→black partial should look like

Contrast the flat, one-shot recipe above with what a correctly-tuned transition
needs. A good white→black partial is roughly three acts:

```
level
+VSH ┤                    ┌───────────────────────┐
     │   ┌──┐             │                       │
 0V ─┤───┘  └──┐  ┌───────┘   MAIN DRIVE → BLACK  └──┐──── done
     │         │  │       (long enough to saturate)  │
-VSL ┤         └──┘                                   └─(short reverse: settle/balance)
     └──┴──┴───┴──┴───────────────────────────────┴──┴──→ frames
        └ shake ┘         └────── the money phase ──────┘
```

1. **Shake / activate** (a few frames alternating ±) — jolts particles loose from
   wherever the last image left them, so the main drive starts from a repeatable
   state. Partials often keep this tiny or skip it.
2. **Main drive** — sustained push in the black direction, for *enough* frames at
   *enough* voltage that the black particles fully reach the surface. This is the
   dominant term and where saturation (true black vs. grey) is won or lost.
3. **Settle / balance** — a short opposite or zero segment to bleed off residual
   charge and stop overshoot, keeping the transition roughly DC-neutral.

Now put our decoded LUT next to that and the failure is obvious:

- **No real shake** — phases 2–11 are empty, so the ink starts from an unknown state.
- **One 15-frame main phase, single polarity** — and crucially, the level codes
  assume VSH/VSL voltages that we *deliberately don't write* (the panel keeps its OTP
  voltages). Waveshare tuned 15 frames for a **200×200 1.54″** cell; on *this*
  ink/gap, 15 frames of push under the OTP voltages doesn't fully migrate the
  particles.
- **VCOM flat** — no VCOM swing to reinforce the field.

Result: the particles move *part way* — grey, not black — which is exactly the bench
symptom ("ran ~490 ms, ink didn't darken"). The recipe is structurally valid
(`0x32`+`0xCF` genuinely runs it) but its frame counts and level codes are calibrated
for a different panel's physics.

That's the whole reason a hand-guessed LUT can't win here: act 2's frame count and
the drive voltages it assumes have to be measured against *this* ink. Good Display's
tuned bytes are the missing act-2 calibration — drop them into `FAST_PARTIAL_LUT`,
and only then does trimming `FAST_PHASE0_FRAMES` trade contrast for latency along a
curve that actually reaches black.
