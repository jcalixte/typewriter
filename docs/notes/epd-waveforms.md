# How an e-paper waveform works

Background note on what a waveform *is* on our GDEY0579T93 panel (SSD1683-family
controller), why it's the crux of the fast-partial dead-end, and where the levers
are. For the byte-by-byte decode of the actual `FAST_PARTIAL_LUT` and the shape of a
proper transition, see the [LUT deep dive](./epd-waveform-lut-deep-dive.md).

## 1. What's physically happening in a pixel

Each e-paper pixel is a microcapsule of clear fluid holding two kinds of charged
pigment: black particles and white particles, oppositely charged. There's a
transparent electrode on the viewer side and a pixel electrode underneath. Apply a
field across the capsule and the particles migrate — a positive field pulls the
white ones to the surface, negative pulls the black ones up. Cut the field and they
**stay put**. That bistability is why the panel holds an image at zero power.

The catch: particles have mass and swim through viscous fluid. Moving them takes
tens to hundreds of milliseconds, and where they end up depends on where they
*started*. So you can't apply "black voltage" for an instant — you have to nudge
them over a timed sequence. **That sequence is the waveform.**

## 2. A waveform is a timed voltage recipe

The controller can only put a few discrete voltages on a pixel:

| Level | Role |
|-------|------|
| **VSH** (+, e.g. +15 V) | push toward one color |
| **VSL** (−) | push toward the other |
| **VSS / 0 V** | hold — no movement |

The panel scans the whole array at a fixed frame rate (~50 Hz → ~20 ms/frame). A
waveform is a list of **phases**; each phase applies one of those levels for an
integer number of frames. So:

```
level
+VSH ┤   ┌───┐       ┌───────────────┐
     │   │   │       │               │
 0V ─┤───┘   └───┐   │   (drive to   └───────  ← field off: particles frozen
     │           │   │    BLACK)
-VSL ┤           └───┘
     └─┴───┴───┴───┴───┴───────────────┴──→ frames (time)
       p0  p1  p2  p3        p4
       └── shake/clear ──┘ └── main drive ──┘
```

Two things the recipe has to respect:

- **DC balance** — the net charge over a transition should be ≈ zero. Always pushing
  the same direction builds up bias that ghosts, then permanently damages the panel.
  The "shake" phases (back-and-forth at the start) both dislodge stuck particles
  *and* help balance the charge.
- **Frame count = latency.** Time = frames × frame period. The longest phase
  dominates the BUSY time. That's the whole speed knob — this is literally why
  `FAST_PHASE0_FRAMES = 0x0F` (15 frames) is pulled out as a constant: fewer frames =
  faster refresh but the ink doesn't drive as fully (lighter black).

## 3. Why it's a *table* (the LUT), not one recipe

The drive needed depends on the **transition**, not the destination: white→black is
a different journey than grey→black or black→black. So the controller stores a
**Look-Up Table**: for each source→target transition, which level to apply in each
phase, plus how many frames each phase lasts.

That's exactly the shape of our `FAST_PARTIAL_LUT` bytes (`screen_epd.rs`) — the
full byte-by-byte decode lives in the [LUT deep dive](./epd-waveform-lut-deep-dive.md).
The short version: the first 60 bytes select *which level* per phase, the next 84
set *how many frames* each phase runs, and a trailing 9 bytes are frame-rate /
gate-scan config. The physical drive voltages the level codes assume (`0x03` gate,
`0x04` source, `0x2C` VCOM) live in separate registers — and we deliberately *don't*
write them, because the panel's own OTP already has correct values for its ink and
clobbering them is what the `update_part` comment warns against.

## 4. Full vs partial vs fast — same mechanism, different recipes

- **Full refresh**: long waveform that flashes every pixel through black/white
  clears, fully DC-balanced, kills all ghosting. Seconds. That's the flash you see.
- **Partial refresh** (our `0x22 ← 0xFF`, ~490–540 ms): short, drives only changed
  pixels straight from current→target, no clear flash. Fast and flicker-free, but
  leaves a little residue each time → you periodically do a full refresh to scrub it.
  A single partial isn't perfectly DC-balanced — that's the longevity worry with a
  *custom* one.
- **Fast / "A2"** (reMarkable-style, what the experiment chases): an even shorter
  partial — fewer frames, 1-bit monochrome — trading contrast and ghosting for
  latency.

## 5. Why temperature is in the mix

Cold fluid is more viscous → particles move slower → the same phase count
under-drives the ink. So the manufacturer characterizes a *different* waveform per
temperature and stores them all in OTP, indexed by the temperature register
(`0x1A`). Writing a **high** temperature selects a **shorter** OTP waveform — that's
the "fast full" trick our `init()` already uses (`0x1A` + `0x22 ← 0x91`). It works
for the full waveform; on this panel it turned out *not* to shorten the partial (the
closed `PARTIAL_TEMP` experiment) because the internal sensor overrides it there.

## 6. Why this is the whole dead-end

- **OTP waveforms** are the vendor's, empirically tuned to *this* panel's exact ink
  chemistry, cell gap, and VCOM. `0x22`'s "load LUT from OTP" bits pull them in.
  Correct, but we can't shorten them.
- **Custom LUT** via `0x32` lets you write your own recipe and display it (`0xCF` =
  "use the LUT I just wrote, don't reload OTP"). But the recipe only darkens if its
  level/frame/voltage numbers match the ink. We borrowed Waveshare's 1.54″ partial
  waveform — right *format*, wrong *panel* — so it ran but the particles never fully
  moved. No darkening.

You cannot safely guess LUT bytes: the numbers come from physical characterization
the vendor does on a lab bench, and a badly-balanced one can permanently damage the
panel. That's exactly why the only real unblock is Good Display handing us the tuned
GDEY0579T93 fast/partial waveform — bytes we drop into `FAST_PARTIAL_LUT` — after
which `FAST_PHASE0_FRAMES` finally becomes a meaningful speed knob.
