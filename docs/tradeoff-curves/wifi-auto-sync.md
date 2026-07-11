# Wi-Fi energy vs auto-sync interval

> **Decision:** `auto_sync` defaults to **10 min**, and is an *opportunistic,
> rate-limited* push — not a wall-clock timer that wakes the device. See
> [Policy](#policy). Backs the `.typoena.toml` `auto_sync` key in
> [`../macroplan.md`](../macroplan.md) (v0.5), whose runtime timer lands in v0.7 and
> must respect sleep (v0.8).
>
> Tradeoff-curves index: [`README.md`](README.md). Docs index:
> [`../README.md`](../README.md).

## The model

For a **text** commit the git payload is a few KB — negligible. Almost all the
energy of one sync is a *fixed* radio burst that costs the same no matter how
little changed:

```
radio wake  →  AP association  →  TLS handshake  →  tiny push  →  teardown
```

So energy per unit time scales as **(fixed cost per sync) × (syncs per hour)**:

```
E(T) = K / T          T = interval in minutes,  K = one burst's worth of energy
```

A hyperbola. Doubling the frequency doubles the cost; the words you actually
wrote barely move it.

Placeholder constants (pending the v0.8 bench measurement — "measure idle /
typing / push current draw"): an ~8 s radio burst at ~150 mA average ⇒
**0.33 mAh per sync**, so `K ≈ 20 mAh·min/hr`. The vertical scale below moves
with the real measurement; the *shape* and the knee do not.

**One assumption is baked into that burst: the radio is fully off between
syncs**, not parked in modem-sleep. Holding the association awake to skip the
per-sync handshake costs ~15–20 mAh/hr on the WROOM — more than a 1-min interval
and ~10× the 10-min default — and only pays back above ~150 syncs/hr (one sync
every ~24 s), which a writing appliance never reaches. So each sync legitimately
pays a full fresh `wake → associate → handshake` burst, and "off" everywhere
below means radio **de-init**, not beacon-listening. Tear the connection down
immediately after each push, too: with syncs ≥2 min apart a keep-alive window
saves nothing, and Typoena only ever *pushes* — there's no inbound traffic that
would justify staying reachable.

> **Status (v0.1) — the shipped firmware does *not* cycle the radio yet.** It
> brings Wi-Fi up lazily on the first `:sync` and then keeps it up for the rest
> of the session: `run_git_service` in
> [`../../firmware/src/git_sync.rs`](../../firmware/src/git_sync.rs) holds the
> `wifi` handle across its whole request loop and never stops, disconnects, or
> drops it (grep the module for `stop`/`disconnect`/`drop` — zero hits). So
> today's device runs the *stay-associated* strategy this section argues
> against, at ~15–20 mAh/hr after the first push. The off-between-syncs
> assumption above is the **target**, not current behaviour: the modem is
> `.take()`n exactly once, so per-sync teardown is a v0.8 refactor of that
> ownership, not a config flip — and a prerequisite before any sleep mode ships.

## The curve

```
  Wi-Fi energy vs auto-sync interval          E(T) ≈ K / T

  mAh/hr
   20 | *                      each sync ≈ one fixed radio burst,
      |  *                     independent of how much text changed
      |  *
   15 |  *   ← STEEP: every extra sync/min costs a full burst
      |  *      for zero payload benefit
      |  *
      |  *
   10 |  *
      |  *
      |   *
      |    *  ← knee
    5 |    *·.___  (5 min)
      |     `·-·__ ______
      |          `·-·__·--·______ (15)        diminishing returns:
    0 |                 `·--·----·----·----·----·--- the tail is ~flat
      +----+----+----+----+----+----+----+----+----+----+----+----+
      0    5    10   15   20   25   30   35   40   45   50   55  min
           └── knee: 5–10 min. Left of here you pay a lot;
               right of here you save almost nothing.
```

| interval | syncs/hr | Wi-Fi mAh/hr | vs 5-min | per 8 h day |
| ---: | ---: | ---: | ---: | ---: |
| 1 min | 60 | 20.0 | 5.0× | 160 mAh |
| 2 min | 30 | 10.0 | 2.5× | 80 mAh |
| 5 min | 12 | 4.0 | 1.0× | 32 mAh |
| **10 min** | 6 | **2.0** | **0.5×** | 16 mAh |
| 15 min | 4 | 1.33 | 0.33× | 10.7 mAh |
| 30 min | 2 | 0.67 | 0.17× | 5.3 mAh |
| 60 min | 1 | 0.33 | 0.08× | 2.7 mAh |

## Two things that move where "best" sits

**`save_on_idle` already prevents data loss — auto-sync is only remote-mirror
freshness.** The durable local copy is the SD write on the idle pause. A longer
sync interval never risks *losing work*; it only means the GitHub mirror is a
few minutes staler. That's a weak cost, and it pushes the optimum toward
*longer* intervals.

**The real battery risk is the sleep interaction, not the awake case.** While
you're typing, the CPU/e-ink baseline dwarfs the sync cost — 5 vs 15 min is
noise. The damage happens when the device is idle or asleep and a wall-clock
timer wakes it *just to push*: each wake pays the radio burst plus the wake/boot
cost and blocks the low-power state. That turns "closed on the desk overnight"
from weeks of standby into dead-by-morning.

## Policy

Ship `auto_sync` as an opportunistic, rate-limited push, with the config value
read as a *max-staleness cap* rather than a timer period:

- **Push when already awake + dirty**, coalesced into the existing idle-pause,
  rate-limited to at most once per `auto_sync` — so a fast typist pausing every
  20 s doesn't sync 100×/hr.
- **Push once on the way into sleep** (idle → light sleep, and especially
  lid-close → deep sleep) if dirty. This is the highest-value sync: nearly free
  (the device is spinning up anyway) and it's the freshness guarantee.
- **Never wake from deep sleep purely to sync.** The one behavior that wrecks
  standby life.

On the single number: **10 min** halves the sync energy versus a 5-min default
for essentially no real cost, because `save_on_idle` already owns data safety.
Clamp the minimum to **~2 min** so a palette command (`> auto sync: 10s`) can't
quietly drain the battery.
