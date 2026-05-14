# Architecture Decision Records

A running log of the load-bearing technical decisions on this project.
Each record states what was considered, what we chose, and what we accept
as a consequence. Status moves from **Proposed** → **Accepted** →
(eventually) **Superseded** when a later ADR replaces it.

Format inspired by Michael Nygard's ADR template, kept short on purpose.

**Related docs:**
[`../README.md`](../README.md) — project overview, hardware table, macro plan.
[`roadmap.md`](roadmap.md) — per-version scope (v0.1 → v1.x).
[`v0.1-mvp-product.md`](v0.1-mvp-product.md) — what the v0.1 device must do.
[`v0.1-mvp-technical.md`](v0.1-mvp-technical.md) — how v0.1 is built.
[`qfd.md`](qfd.md) — Quality Function Deployment: requirements → functions →
components, with the tradeoffs from this file ranked by user-facing weight.

---

## ADR-001: Language and runtime — Rust on `esp-idf-rs` (std)

**Status:** Accepted — 2026-05-14
**Scope:** Whole project.

### Context
The firmware needs: USB host, Wi-Fi + TLS, SPI peripherals, a SD filesystem,
and a working git implementation that can push over HTTPS. All on an ESP32-S3
with 8 MB PSRAM. We also want the code to stay refactorable as features pile
up across nine downstream releases.

### Options considered

| Option | Pros | Cons |
|---|---|---|
| **C / C++ on ESP-IDF** | Reference platform, every peripheral has a driver, fastest path to first pixel. | Refactoring at scale is painful; memory safety is on you. |
| **Rust on `esp-idf-rs` (std)** | First-class Espressif-sponsored Rust support; `std` gives heap / threads / VFS / mbedtls; can use the broader Rust ecosystem (`gitoxide`, `ropey`, `embedded-graphics`). | Larger binary than `no_std`; longer build times; some `unsafe` at FFI seams. |
| **Rust on `esp-hal` (no_std)** | Smallest binary, most "pure" embedded experience. | No `std` = no off-the-shelf git, no easy TLS, would re-implement a lot of plumbing. |
| **Gleam + Shore on AtomVM** | Beautiful language, the user's stated preference. | BEAM on ESP32 is memory-hungry; no bindings for USB host, e-ink, SD, TLS, git in that ecosystem. Two research projects stacked. |
| **MicroPython / CircuitPython** | Fastest to prototype. | Too slow for responsive editing at the latencies e-ink already imposes; GC pauses would surface as dropped keys. |
| **TinyGo** | Modern, ergonomic. | ESP32-S3 support is thinner than Rust's; smaller ecosystem of embedded crates equivalents. |

### Decision
**Rust on `esp-idf-rs` (std).** It's the sweet spot: keeps the door open to
the entire Rust ecosystem we need (`gitoxide` especially), gets us threads
and TLS without writing them, and has Espressif as an actual upstream.

### Consequences
- Binary will be in the 1–2 MB range — comfortable in 16 MB flash.
- Build times are real (clean build ~5–10 min). Acceptable.
- Cross-compiling toolchain (`espup`) is one more thing to install.
- We will not use `tokio` or async runtimes in v0.1 — see ADR-006.
- Revisit if `esp-idf-rs` upstream stalls or if `gitoxide` doesn't compile
  cleanly against it (spike 7 is the kill-switch — see
  [v0.1 technical: hardware bring-up order](v0.1-mvp-technical.md#hardware-bring-up-order)).

See also: [qfd.md §7](qfd.md#7-tradeoffs-and-their-why-linked-to-adrs) for
the binary-size / build-time costs traded against ecosystem access.

---

## ADR-002: UI strategy — custom widgets on `embedded-graphics`, not Ratatui

**Status:** Accepted — 2026-05-14
**Scope:** Whole project.

### Context
We need a TUI-like editor (header, edit area, status, palettes later). The
output medium is e-ink: pixel framebuffer with **partial-refresh windows**
aligned to panel-internal regions, ~10× slower than an LCD per region.

### Options considered

| Option | Pros | Cons |
|---|---|---|
| **Ratatui** with a custom backend | Mature widget set, well-known API, lots of community examples. | Built for char-grid terminals over ANSI; per-cell diff fights e-ink's region-refresh model; backend would re-rasterise glyphs from cell-diffs; ~200 KB of binary and a leaky abstraction. |
| **Raw `embedded-graphics` only** | Smallest footprint, full control. | Every screen built from primitives; no widget reuse; status line / palette would each be ad-hoc. |
| **LVGL via Rust bindings** | Full GUI toolkit, themable. | Designed for actively-refreshing colour LCDs; e-ink integration is awkward; way more than we need. |
| **Custom thin widget layer on `embedded-graphics`** | Borrow Ratatui's API ideas (`Layout`, `Block`, `Paragraph`) without its rendering model; dirty-rect tracking aligned to e-ink regions; ~500 LoC. | We own and maintain the layer. |

### Decision
**Custom thin widget layer on `embedded-graphics`.** Steal the widget *API
shape* from Ratatui (because it's a good shape) but render directly to a
pixel framebuffer with our own dirty-rectangle tracking sized to the panel's
refresh regions.

### Consequences
- ~500 LoC of widget/layout code we maintain. Worth it.
- We can tune refresh cadence (partial vs full) at the widget level.
- If we later want to render to a terminal for desktop testing, we add a
  second backend; the widget API stays.

Implementation: [v0.1 technical → render module](v0.1-mvp-technical.md#module-breakdown).
Owns the two top-ranked functions (H1 latency, H2 region area) in
[qfd.md §3](qfd.md#3-house-of-quality--whats--hows).

---

## ADR-003: Display — GDEY0579T93 + DESPI-c579 breakout

**Status:** Accepted — 2026-05-14
**Scope:** v0.1 through v1.0. 10.3" upgrade remains on the v1.x table.

### Context
The screen is the most user-facing hardware choice. It sets the aspect of
the writing experience, the BOM cost, the GPIO budget, the framebuffer size,
and the refresh feel.

### Options considered

| Option | Size / Res | Aspect | Pros | Cons |
|---|---|---|---|---|
| **GDEY0579T93 + DESPI-c579** | 5.79" / 792×272 | 2.9:1 strip | SPI, partial refresh, small framebuffer (~27 KB), Freewrite-style narrow viewport, low power, low GPIO use. | Only ~11 visible lines of edit area; less context on screen. |
| **Waveshare 7.5" V2** | 7.5" / 800×480 | 5:3 page | More lines visible, well-supported by `epd-waveshare` out of the box. | Bigger BOM, bigger framebuffer (~48 KB), more conventional / less typewriter-feeling. |
| **Waveshare 10.3" + IT8951** | 10.3" / 1872×1404 | 4:3 | Real "page" experience; great for long-form. | +$80 BOM; parallel bus eats GPIO; IT8951 adds a controller board; overkill for v0.1. |
| **2.9" / 4.2" smaller panels** | varied | varied | Cheap, common. | Too cramped for a typewriter; status bars eat the screen. |

### Decision
**GDEY0579T93 driven over SPI via the DESPI-c579 breakout.** The strip
aspect biases UX toward "current line + recent context" — the writing
posture we actually want. Small framebuffer keeps PSRAM free for git pack
data. The DESPI-c579 is a passive level-shifter / FPC adapter, not an active
controller — same SPI driver model as any other e-paper.

### Consequences
- Visible edit area is ~11 lines. UI design must embrace this (no
  multi-pane, no large headers). See
  [v0.1 product → screen layout](v0.1-mvp-product.md#screen-layout).
- Driver: if `epd-waveshare` doesn't already support this panel's
  controller (SSD1683-class), we write ~300 LoC of `embedded-hal` SPI
  driver. Validated in spike 2 — see
  [v0.1 technical → hardware bring-up order](v0.1-mvp-technical.md#hardware-bring-up-order).
- 10.3" upgrade path is preserved by keeping the renderer resolution-agnostic.
  See [roadmap → v1.x](roadmap.md#v1x--stretch--nice-to-have).

---

## ADR-004: Git implementation — `gitoxide` (`gix`)

**Status:** Accepted — 2026-05-14
**Scope:** Whole project, all releases.

### Context
The device must do `add`, `commit`, `push` over the network. Optionally
later: `fetch`, `pull`, `branch`. The library must compile against
`esp-idf-rs` (std, mbedtls available).

### Options considered

| Option | Pros | Cons |
|---|---|---|
| **`libgit2-sys`** (C bindings) | Battle-tested, comprehensive, well-known. | C dependency complicates cross-compile to ESP32-S3; needs mbedtls glue; binary size; less Rust-idiomatic. |
| **`gitoxide` (`gix`)** | Pure Rust, modular crates (we only depend on what we use), idiomatic API, active development. | Smart-HTTP push path is newer than libgit2's; PSRAM allocation patterns less battle-tested on embedded. |
| **Hand-rolled HTTP + pack** | Smallest possible footprint. | Reinventing git internals; pack delta + ref discovery + index updates are not weekend work. |
| **Shell out to `git` binary** | Trivial. | There is no `git` binary on the ESP32-S3. |

### Decision
**`gitoxide`.** Modular means we pull only `gix-pack`, `gix-protocol`,
`gix-transport`, etc. — not 200 KB of features we don't use. Pure Rust
removes a class of cross-compile pain. The smart-HTTP path is validated in
spike 7 *before* we commit to integration; if it fails on the device, we
fall back to `libgit2-sys` for v0.1 (documented as the kill-switch in the
risk table).

### Consequences
- We become an early-ish embedded user of `gitoxide`; bugs reported back
  upstream.
- Auth via PAT in an Authorization header — no SSH (see ADR-005).
- Performance on PSRAM during pack operations is a watched metric — top-3
  priority in [qfd.md §6](qfd.md#6-critical-performance-budget).

Implementation: [v0.1 technical → `git` module](v0.1-mvp-technical.md#module-breakdown)
and [risks table](v0.1-mvp-technical.md#risks-and-how-well-know-they-bit-us).

---

## ADR-005: Auth — HTTPS + GitHub Personal Access Token

**Status:** Accepted — 2026-05-14
**Scope:** v0.1 through at least v0.9.

### Context
The device must authenticate to GitHub (or other git remotes) to push.
Auth has to be: enterable on a tiny screen-less first-run flow, storable
on-device, and reasonably secure for a personal appliance.

### Options considered

| Option | Pros | Cons |
|---|---|---|
| **HTTPS + PAT** | Trivial to implement; PAT is a string the user pastes during captive-portal setup; works with `gitoxide` smart-HTTP. | Long-lived secret on device; PAT rotation is manual. |
| **HTTPS + OAuth device flow** | No secret typed by hand; user approves on github.com. | Adds an OAuth client app to maintain; token still has to live on device; more first-run UX work. |
| **SSH** | No PAT; per-device deploy keys. | SSH on embedded is heavy (host-key handling, key generation); `gitoxide`'s SSH transport story is less mature than HTTPS; users would have to register the public key on GitHub anyway. |
| **GitHub App with installation token** | Strongest model, rotating credentials. | Massive overhead for a single-user device. |

### Decision
**HTTPS + PAT.** Stored in internal LittleFS, encrypted with a key derived
from the chip's eFuse so a stolen SD card alone is not enough. Captive
portal accepts the PAT during first-run setup.

### Consequences
- The user must generate a PAT with `repo` scope. Documented in
  [v0.1 product → first-run flow](v0.1-mvp-product.md#first-run-provisioning-flow).
- PAT is never logged. Validated in code review.
- Rotation in v0.1 = wipe NVS and re-run setup. Proper rotation UI is v0.9
  — see [roadmap → v0.9](roadmap.md#v09--robustness--).
- Revisit if we ever want to support multiple remotes per device with
  different credentials.

---

## ADR-006: Concurrency — `std::thread` + channels, no async runtime

**Status:** Accepted — 2026-05-14
**Scope:** v0.1 through at least v1.0.

### Context
The firmware has several concurrent concerns: USB input, Wi-Fi maintenance,
screen rendering, occasional git operations. None of them are I/O-bound at
the scale where async wins. The number of "tasks" is bounded and small (≤ 8).

### Options considered

| Option | Pros | Cons |
|---|---|---|
| **`std::thread` + channels** | Boring, debuggable, stack traces work, no executor to tune; ESP-IDF FreeRTOS underneath is well-understood. | Each thread costs 8–32 KB stack depending on workload; not zero-cost like async. |
| **`embassy` async** | Trendy, ergonomic, low memory per task. | `esp-idf-rs` and `embassy` don't mix cleanly; adopting embassy means dropping `std` and rewriting against `esp-hal` (ADR-001 reversed). |
| **`tokio` on `esp-idf-rs`** | Familiar async. | Heavy executor, oversized for ≤ 8 tasks, mbedtls/`gitoxide` integration would need a lot of glue. |
| **Single-threaded event loop** | Smallest memory. | Long-running ops (git push, full refresh) block input. |

### Decision
**`std::thread` + `crossbeam-channel`.** Five tasks (`usb`, `wifi`, `ui`,
`render`, `git`). Editor state behind a single `Mutex`. No `await`, no
runtime to tune, no colour-of-functions problem.

### Consequences
- ~76 KB of stack space across the five task stacks (8 + 8 + 16 + 12 + 32
  KB — see [v0.1 technical → threads / tasks](v0.1-mvp-technical.md#threads--tasks)
  for the breakdown). Comfortable in the ESP32-S3's 512 KB internal SRAM.
- Refresh / git / Wi-Fi each get their own thread, so a slow push doesn't
  freeze typing.
- If task count balloons past ~10 (unlikely), revisit.

---

## ADR-007: Storage split — FAT-on-SD for working copy, LittleFS-on-flash for config

**Status:** Accepted — 2026-05-14
**Scope:** Whole project.

### Context
Two storage needs: a large, removable, growable area for the git working
copy and notes; and a small, durable, never-removed area for device config
(Wi-Fi credentials, PAT, remote URL).

### Options considered

| Option | Pros | Cons |
|---|---|---|
| **SD (FAT) for working copy + LittleFS (internal) for config** | Plays to each medium's strengths; user can pop the SD to read on desktop; config can't be lost by yanking the card. | Two filesystems to manage. |
| **All on SD** | One filesystem. | Config disappears if SD is removed; PAT on FAT is harder to protect than on encrypted NVS. |
| **All in internal flash** | Single medium; encrypted. | 16 MB flash limits notes growth; no desktop-side access; SD slot becomes pointless. |
| **SPIFFS for everything** | Single FS, well-known on ESP32. | SPIFFS isn't great with large files; no removability. |

### Decision
**FAT on SD for `/sd/repo/` and `/sd/local/`. LittleFS on internal flash
for `/nvs/config.toml`.** PAT inside config is encrypted with an eFuse-
derived key.

### Consequences
- User can plug the SD into a laptop and read/edit files there.
  Discouraged but possible.
- Config survives SD reformatting.
- Power-loss safety on FAT is weaker than LittleFS — we mitigate with
  atomic-rename writes (see technical design).

---

## ADR-008: MVP power — wall-powered, battery deferred to v0.8

**Status:** Accepted — 2026-05-14
**Scope:** v0.1 only. Revisited in ADR-future at v0.8.

### Context
"DIY typewriter" suggests portability, which suggests battery. But battery
adds: charging circuit, BMS, thermal margin, soft power switch, lid-close
detection, sleep states. Each of those has its own bring-up cost.

### Options considered

- **USB-C wall power, no battery.** Simple, safe, lets us measure real
  draw before sizing a cell.
- **18650 + IP5306 from day one.** Pretty close to a known-good pattern;
  IP5306 handles charge + 5 V boost.
- **LiPo + dedicated charger IC + buck/boost.** More control, more parts.

### Decision
**Wall power only for v0.1.** Battery is its own phase (v0.8) once the
power profile of "boot + type + idle + push" is measured on real hardware.
Sizing a battery before measuring is guessing.

### Consequences
- v0.1 device is tethered. Not the final aesthetic, but the right MVP.
- We can decide cell capacity from real numbers in v0.8, not specs sheets.
- Lid-close detection / deep sleep slips to v0.8 with the battery.

---

## ADR-009: Keyboard transport — USB host (TinyUSB)

**Status:** Accepted — 2026-05-14
**Scope:** v0.1 through at least v1.0.

### Context
The Nuphy keyboard speaks both wired USB-C (HID) and Bluetooth LE (HID).
The ESP32-S3 has USB OTG (host capable) and BLE 5. Either transport works.

### Options considered

| Option | Pros | Cons |
|---|---|---|
| **USB host (TinyUSB)** | Keyboard draws no battery of its own; ESP32-S3 powers it through the host port; standard boot-protocol HID is well-supported; no radio contention with Wi-Fi during push. | One more USB connector on the enclosure; cable between device and keyboard (or shared chassis). |
| **BLE-HID** | No cable; keyboard can be slightly remote from the device. | Keyboard has its own battery to manage; BLE shares the 2.4 GHz radio with Wi-Fi, so a `Ctrl-G` push contends with input; pairing UX is more first-run work. |
| **UART receiver (custom keyboard firmware)** | Lowest latency, simplest stack. | Requires reflashing the Nuphy or building a passthrough; not viable as a product choice. |

### Decision
**USB host (TinyUSB) for v0.1.** BLE-HID is kept as a documented fallback
if TinyUSB host turns out unstable (spike 4 is the gate).

### Consequences
- Enclosure design must include a USB-A or USB-C port for the keyboard.
- The Nuphy's own battery is irrelevant when wired — saves the user a
  charging surface.
- Wi-Fi and keyboard input do not contend for radio time.
- If we ever want a fully wireless build, we revisit with a BLE-HID ADR.

---

## How to add a new ADR

1. Append a new `## ADR-NNN: <title>` section to this file.
2. Status starts as **Proposed**, with today's date.
3. Once merged + agreed, flip to **Accepted**.
4. When superseded, leave the old ADR in place and add **Superseded by
   ADR-MMM** to its status line. Never delete.
5. Cross-reference from the relevant section of the README or design docs
   if the decision is load-bearing for code review.
