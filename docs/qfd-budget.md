# 6. Critical performance budget

A curated rank, drawing from [§3 importance](qfd-house-1.md#3-house-of-quality--whats--hows) and [§4 conflicts](qfd-house-1.md#4-roof--how-vs-how-tradeoffs), with one
deliberate override: acceptance-criteria critical paths (H4 boot,
H5 soak) move up regardless of weighted-vote spread. (Pre-W14 this list
also lifted H8 durability over its narrow voter base; W14 has widened
that base, so H8's top-five spot is now arithmetic; see [§3](qfd-house-1.md#3-house-of-quality--whats--hows).) These started as
the numbers spikes 2–7 had to validate; most are now measured on the
shipped device. The Verdict column carries the result, and every row
names its fallback in "If we miss it": a target without a fallback is a
wish, not a budget. The fallback column also covers regression on
already-met targets.

| Rank | Characteristic         | Target                           | Watched on                          | Verdict                                                                                                                                                              | If we miss it                                                                                                                                                            |
| ---- | ---------------------- | -------------------------------- | ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1    | H2 region area         | ≤ 1 line per keypress            | on-device refresh log               | ✓ windowed-Y drives only the touched line's band — but its *latency* framing is disputed (2026-07-21 bench: refresh time is area-independent), see [D2](house-vs-product.md#d2--refresh-area-is-not-a-latency-lever)                                                                                                                     | Larger font / coarser refresh region: the fallback that was never needed, kept named                                                                                    |
| 2    | H9 heap (Push)      | ≥ 1 MB PSRAM free at push peak   | `log_push_heap` telemetry           | ✓ run 9: min-ever 4.5 MB after mwindow 64 KB/1.5 MB + odb 1 MB caps; **new watch = internal DRAM** (min-ever ~2.1 KB during TLS send); [§2 ¶](qfd-house-1.md#2-engineering-characteristics-the-hows)                           | Re-tighten the mwindow/odb caps; move remaining internal-DRAM allocs to PSRAM (the `EXTERNAL_MEM_ALLOC` pattern); last resort = gate repo shape, as onboarding's 30 MB gate already does |
| 3    | H8 durability          | 100 % (post-confirm power loss)  | dirty journal + boot recovery       | Journal (`/sd/.typoena-dirty`) + `*.tmp` boot-recovery + stranded-commit replay shipped; the physical power-pull test is still owed (v0.9)                             | A failed pull test blocks v0.9 sign-off: fsync the directory handle after rename, then redesign the journal if that is not enough                                        |
| 4    | H1 Type latency        | ≤ 400 ms (revised from ≤ 200 ms) | refresh log; bench 2026-07-21 ([tradeoff](tradeoff-curves/epd-refresh-latency.md)) | Bench killed the ~100–130 ms projection — refresh time is area-**independent** (waveform BUSY, not the band-write), so the default windowed factory partial is **~495 ms ✗**. Custom `0x32` LUT (FR `0x08`, `fast_partial` opt-in) → **~265 ms ✓**, pending longevity + cold soak before default-on; **erase/caret tier ~630 ms ✗**                                                                                                   | Windowed erase is dead as a lever (time is area-independent); the proven path is the same custom fast waveform on the erase/caret tier, or graduate `fast_partial` to default after its soak — else re-price [ADR-003] and move the target openly, never quietly                                     |
| 5    | H6 Push reliability | ≥ 95 % (network up)              | daily `:gp` use                     | Rejected-push → reconcile → replay → push cycle verified on device 2026-07-14; residual risk = stale keep-alive on long marking gaps (avoided via repack, not fixed)  | Reconnect-on-stale in the http layer: the named durable fix, owed before v1.0 claims ≥ 99 %                                                                             |
| 6    | H3 cadence             | full every ~64 partials          | `FULL_REFRESH_EVERY = 64`           | ✓ holding; flashes deferred to idle ≥ 1 s                                                                                                                             | If ghosting returns: lower `FULL_REFRESH_EVERY`, temperature-tune per panel                                                                                              |
| 7    | H4 Boot latency        | ≤ 5 s (cold, to cursor)          | 4258 ms 2026-07-11 ✓                | Held ~4.2 s through the 2026-07-14 restructure (async splash, background walk); [boot-time-budget](notes/boot-time-budget.md)                                        | For v1.0's ≤ 3 s: memtest off (−0.74 s); the ~630 ms editor-bring-up **partial** is now a boot-path lever too — the custom `0x32` fast LUT ([tradeoff](tradeoff-curves/epd-refresh-latency.md)) does full-area in ~300 ms at the bench, ~330 ms off boot if it validates cold; the ~1.9 s cold full refresh remains the e-ink floor the fast partial can't touch                                |
| 8    | H5 soak                | 1 h no leak / no drop            | 1 h real-use soak ✓ 2026-07-11      | Attested                                                                                                                                                              | Bisect the heap-touching change (the run-4 per-draw-alloc OOM was exactly this class) and re-soak before shipping it                                                     |
| 9    | H17 reach cost         | ≤ 6 keystrokes median (file / command / edit point) | **unmeasured**: count a real session | 4-keystroke file reach by construction (Cmd-P + 2-char query + Enter; MRU recents under 2 chars); the grammar is host-tested but a session median has never been counted | MRU depth + `PALETTE_MIN_QUERY` tuning, pinned files; if the *grammar itself* is what costs motions, that is a design question for [house-vs-product.md](house-vs-product.md), not a tuning knob |
| 10   | H16 onboarding         | ≤ 10 min (blank card → cursor)   | **unmeasured**: time a fresh run   | Wizard slices 0–5a verified on hardware but never wall-clocked                                                                                                        | Shallow-clone tuning, device-flow poll cadence; structurally, the deferred SoftAP companion (a phone keyboard beats the device keyboard for entry speed)                 |

The two not-in-MVP rows but already-shaped-by-design:

| — | H13 current | Measured only in v0.1 | bench multimeter | Cell sizing for v0.8 is data-driven, not spec-sheet; the Wi-Fi/auto-sync energy half is modelled in [wifi-auto-sync](tradeoff-curves/wifi-auto-sync.md) | If measurements say > 2-day life is unreachable: revisit [ADR-008]'s cell class or W11's weight, on numbers, not hope |
| — | H11 stacks | Sum ≤ 128 KB (was ≤ 80 KB) | measured: 124 KB explicit (git 96 + walk 16 + USB 4+8) | Target followed the shipped architecture; [§2 ∥](qfd-house-1.md#2-engineering-characteristics-the-hows) | Re-price before adding any thread; if a new one breaks the sum, shrink or merge an existing stack first |

---

[ADR-001]: adr.md#adr-001-language-and-runtime--rust-on-esp-idf-rs-std
[ADR-002]: adr.md#adr-002-ui-strategy--custom-widgets-on-embedded-graphics-not-ratatui
[ADR-003]: adr.md#adr-003-display-medium--e-ink-gdey0579t93-panel
[ADR-004]: adr.md#adr-004-git-implementation--gitoxide-gix
[ADR-005]: adr.md#adr-005-auth--https--github-personal-access-token
[ADR-006]: adr.md#adr-006-concurrency--stdthread--channels-no-async-runtime
[ADR-007]: adr.md#adr-007-storage-split--fat-on-sd-for-working-copy-littlefs-on-flash-for-config
[ADR-008]: adr.md#adr-008-mvp-power--wall-powered-battery-deferred-to-v08
[ADR-009]: adr.md#adr-009-keyboard-transport--usb-host-tinyusb
[ADR-010]: adr.md#adr-010-push-ux--atomic-ctrl-g-auto-timestamp-commit-message-no-user-prompt
[ADR-011]: adr.md#adr-011-credential-provisioning--how-the-pat-reaches-the-device-and-is-protected-at-rest
[ADR-012]: adr.md#adr-012-sd-on-its-own-spi3-host-not-shared-with-the-epd-on-spi2
