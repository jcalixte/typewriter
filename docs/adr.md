# Architecture Decision Records

A running log of the load-bearing technical decisions on this project.
Each record states what was considered, what we chose, and what we accept
as a consequence. Status moves from **Proposed** → **Accepted** →
(eventually) **Superseded** when a later ADR replaces it.

Format inspired by Michael Nygard's ADR template, kept short on purpose.

**Related docs:**
[`../README.md`](../README.md) — project overview, hardware table, macro plan.
[`../CONTEXT.md`](../CONTEXT.md) — project glossary: **Tracked**, **Local**,
**Save**, **Publish**, plus the principles ("writing tool, not sync engine")
that constrain [ADR-010] specifically.
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

| Option                          | Pros                                                                                                                                                                     | Cons                                                                                                                                         |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| **C on ESP-IDF (no Arduino)**   | Reference platform on the bare native SDK; every peripheral has a driver; smallest binary of the C-family options; no C++ runtime / exceptions / RTTI to reason about.   | All memory safety on you; no RAII for resource cleanup; no generics so widget / state code gets repetitive; refactoring at scale is painful. |
| **C++ on ESP-IDF**              | Same peripheral coverage as C; RAII, templates, and `std::` containers ease widget / state code; mature in the ESP-IDF examples.                                         | Exception / RTTI story on embedded is messy; ABI / linker surprises; memory safety still on you; binary larger than plain C.                 |
| **Rust on `esp-idf-rs` (std)**  | First-class Espressif-sponsored Rust support; `std` gives heap / threads / VFS / mbedtls; can use the broader Rust ecosystem (`gitoxide`, `ropey`, `embedded-graphics`). | Larger binary than `no_std`; longer build times; some `unsafe` at FFI seams.                                                                 |
| **Rust on `esp-hal` (no_std)**  | Smallest binary, most "pure" embedded experience.                                                                                                                        | No `std` = no off-the-shelf git, no easy TLS, would re-implement a lot of plumbing.                                                          |
| **Gleam + Shore on AtomVM**     | Beautiful language, the user's stated preference.                                                                                                                        | BEAM on ESP32 is memory-hungry; no bindings for USB host, e-ink, SD, TLS, git in that ecosystem. Two research projects stacked.              |
| **MicroPython / CircuitPython** | Fastest to prototype.                                                                                                                                                    | Too slow for responsive editing at the latencies e-ink already imposes; GC pauses would surface as dropped keys.                             |
| **TinyGo**                      | Modern, ergonomic.                                                                                                                                                       | ESP32-S3 support is thinner than Rust's; smaller ecosystem of embedded crates equivalents.                                                   |

### Decision

**Rust on `esp-idf-rs` (std).** It's the sweet spot: keeps the door open to
the entire Rust ecosystem we need (`gitoxide` especially), gets us threads
and TLS without writing them, and has Espressif as an actual upstream.

### Consequences

- Binary will be in the 1–2 MB range — comfortable in 16 MB flash.
- Build times are real (clean build ~5–10 min). Acceptable.
- Cross-compiling toolchain (`espup`) is one more thing to install.
- We will not use `tokio` or async runtimes in v0.1 — see [ADR-006].
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

| Option                                              | Pros                                                                                                                                             | Cons                                                                                                                                                                                      |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Ratatui** with a custom backend                   | Mature widget set, well-known API, lots of community examples.                                                                                   | Built for char-grid terminals over ANSI; per-cell diff fights e-ink's region-refresh model; backend would re-rasterise glyphs from cell-diffs; ~200 KB of binary and a leaky abstraction. |
| **Raw `embedded-graphics` only**                    | Smallest footprint, full control.                                                                                                                | Every screen built from primitives; no widget reuse; status line / palette would each be ad-hoc.                                                                                          |
| **LVGL via Rust bindings**                          | Full GUI toolkit, themable.                                                                                                                      | Designed for actively-refreshing colour LCDs; e-ink integration is awkward; way more than we need.                                                                                        |
| **Custom thin widget layer on `embedded-graphics`** | Borrow Ratatui's API ideas (`Layout`, `Block`, `Paragraph`) without its rendering model; dirty-rect tracking aligned to e-ink regions; ~500 LoC. | We own and maintain the layer.                                                                                                                                                            |

### Decision

**Custom thin widget layer on `embedded-graphics`.** Steal the widget _API
shape_ from Ratatui (because it's a good shape) but render directly to a
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

## ADR-003: Display medium — e-ink (GDEY0579T93 panel)

**Status:** Accepted — 2026-05-14
**Scope:** v0.1 through v1.0. 10.3" e-ink upgrade remains on the v1.x table; a non-e-ink swap would supersede this ADR.

### Context

The display has the largest downstream blast radius of any hardware choice.
The _medium_ (e-ink vs. LCD vs. memory LCD vs. OLED) — not the specific
panel — is the real architectural decision: it sets the render strategy
([ADR-002]), the per-keystroke latency floor, the idle-power profile (and
so the v0.8 battery story — [ADR-008]), the UX posture, and the BOM shape.
The specific panel (GDEY0579T93 + DESPI-c579 breakout) is already on hand
and documented here as the _instantiation_, not as a freshly weighed option.

This ADR records the medium choice with eyes open. E-ink has well-known
costs at the typing latencies a writing appliance wants — Astrohaus shipped
the Freewrite Alpha in 2023 on a reflective LCD specifically to address
typing-latency complaints from their original e-ink line. We are accepting
costs the category leader retreated from.

### Options considered

| Option                                        | Refresh / persistence                                     | Pros                                                                                                                                                                                    | Cons                                                                                                                                                                                                                                                                         |
| --------------------------------------------- | --------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **E-ink (reflective, image-persistent)**      | ~100–300 ms partial / ~700–1000 ms full / persists at 0 W | Paper aesthetic; persists at zero idle power; no backlight (no glare, no eye strain); category convention (Freewrite, reMarkable, Kindle Scribe, Boox); medium enforces writing posture | Slow per-keystroke feedback; ghosting accumulates → periodic full-refresh flash; scroll is the worst-case refresh op (full edit-area redraw); requires waveform / refresh-cadence tuning; Astrohaus retreated from e-ink in Freewrite Alpha (2023) on typing-latency grounds |
| **FSTN graphical LCD (monochrome)**           | <16 ms, no refresh quirks                                 | Cheap (~$5–15); trivial render code; snappy scroll                                                                                                                                      | Backlit (always-on power), unreadable indoors without it; no image persistence; calculator / feature-phone aesthetic; writing-grade resolution (≥600 px wide at ≥6") effectively unavailable as a hobbyist part                                                              |
| **Sharp Memory LCD (monochrome, reflective)** | ~20 ms, persists at near-0 W                              | Persists _and_ refreshes fast (best technical combo); sun-readable; ghost-free                                                                                                          | Caps around 4.4" before getting rare and expensive; reflective-only feels like a screen, not paper; niche sourcing; lower DPI than e-ink at writing size                                                                                                                     |
| **TFT / OLED (color, self-lit or backlit)**   | <16 ms, persists only at full power                       | Bright, fast, plentiful                                                                                                                                                                 | Backlit / self-lit → screen-feel, not paper; OLED burns in static text (status line, header); defeats the writing-tool posture; not seriously a contender                                                                                                                    |

### Decision

**E-ink as the display medium**, instantiated with the **GDEY0579T93 (5.79",
792×272, SSD1683-class) driven over SPI through the DESPI-c579 breakout** —
which is already on hand. The DESPI-c579 is a passive level-shifter / FPC
adapter, not an active controller — same SPI driver model as any other
e-paper.

The medium is chosen for: paper aesthetic, zero-idle-power persistence
(which makes [ADR-008]'s battery deferral structurally cheap to revisit at
v0.8), the category convention users have a mental model for, and alignment
with the "writing tool, not screen" posture pinned in
[`CONTEXT.md`](../CONTEXT.md). The slow refresh and scroll cost are accepted
as the price of those properties.

### Consequences

- Visible edit area on this panel is ~11 lines. UI must embrace the
  constraint — no multi-pane, no large headers. See
  [v0.1 product → screen layout](v0.1-mvp-product.md#screen-layout).
- Framebuffer is ~27 KB; keeps PSRAM free for git pack data — a top-3
  budget item in [qfd.md §6](qfd.md#6-critical-performance-budget).
- Driver: SSD1683-class. If `epd-waveshare` doesn't already cover this
  panel's controller, ~300 LoC of `embedded-hal` SPI driver. Validated in
  [spike 2](v0.1-mvp-technical.md#hardware-bring-up-order).
- **Per-keystroke latency floor ~100–300 ms** (partial refresh). The render
  module must buffer the active line and flush on a short timer, not redraw
  on every keystroke. Owns the top-ranked H1 latency constraint in
  [qfd.md §3](qfd.md#3-house-of-quality--whats--hows); strategy lives in
  [ADR-002].
- **Scroll is the worst-case refresh operation** — every scroll is a full
  edit-area redraw, either with a visible flash (full refresh) or
  accumulating ghost trails (partial refresh). The concrete scroll strategy
  (continuous-scroll-with-periodic-flush vs. page-down vs. hybrid) is a v0.1
  product decision, not part of this ADR — see
  [v0.1 product → screen layout](v0.1-mvp-product.md#screen-layout). Tuning
  is a render-module concern in
  [v0.1 technical](v0.1-mvp-technical.md#module-breakdown).
- **Industry calibration:** Astrohaus shipped Freewrite Alpha (2023) on a
  reflective LCD specifically to fix typing-latency complaints from their
  e-ink line. The latency cost we're accepting is one the commercial leader
  couldn't fully tune away after a decade. Set expectations accordingly —
  do not promise "instant feedback."
- Idle power on e-ink is structurally ~0, which makes the v0.8 battery
  sizing exercise straightforward — see [ADR-008] and
  [roadmap → v0.8](roadmap.md#v08--power-battery--sleep--).
- 10.3" e-ink upgrade path is preserved by keeping the renderer
  resolution-agnostic. A _non_-e-ink swap (e.g. Sharp Memory LCD) would
  invalidate [ADR-002]'s dirty-rect strategy and force a fresh medium ADR.

---

## ADR-004: Git implementation — `gitoxide` (`gix`)

**Status:** Accepted — 2026-05-14
**Scope:** Whole project, all releases.

### Context

The device must do `add`, `commit`, `push` over the network. Optionally
later: `fetch`, `pull`, `branch`. The library must compile against
`esp-idf-rs` (std, mbedtls available).

### Options considered

| Option                         | Pros                                                                                          | Cons                                                                                                      |
| ------------------------------ | --------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| **`libgit2-sys`** (C bindings) | Battle-tested, comprehensive, well-known.                                                     | C dependency complicates cross-compile to ESP32-S3; needs mbedtls glue; binary size; less Rust-idiomatic. |
| **`gitoxide` (`gix`)**         | Pure Rust, modular crates (we only depend on what we use), idiomatic API, active development. | Smart-HTTP push path is newer than libgit2's; PSRAM allocation patterns less battle-tested on embedded.   |
| **Hand-rolled HTTP + pack**    | Smallest possible footprint.                                                                  | Reinventing git internals; pack delta + ref discovery + index updates are not weekend work.               |
| **Shell out to `git` binary**  | Trivial.                                                                                      | There is no `git` binary on the ESP32-S3.                                                                 |

### Decision

**`gitoxide`.** Modular means we pull only `gix-pack`, `gix-protocol`,
`gix-transport`, etc. — not 200 KB of features we don't use. Pure Rust
removes a class of cross-compile pain. The smart-HTTP path is validated in
spike 7 _before_ we commit to integration; if it fails on the device, we
fall back to `libgit2-sys` for v0.1 (documented as the kill-switch in the
risk table).

### Consequences

- We become an early-ish embedded user of `gitoxide`; bugs reported back
  upstream.
- Auth via PAT in an Authorization header — no SSH (see [ADR-005]).
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

| Option                                 | Pros                                                                                                                 | Cons                                                                                                                                                                                    |
| -------------------------------------- | -------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **HTTPS + PAT**                        | Trivial to implement; PAT is a string the user pastes during captive-portal setup; works with `gitoxide` smart-HTTP. | Long-lived secret on device; PAT rotation is manual.                                                                                                                                    |
| **HTTPS + OAuth device flow**          | No secret typed by hand; user approves on github.com.                                                                | Adds an OAuth client app to maintain; token still has to live on device; more first-run UX work.                                                                                        |
| **SSH**                                | No PAT; per-device deploy keys.                                                                                      | SSH on embedded is heavy (host-key handling, key generation); `gitoxide`'s SSH transport story is less mature than HTTPS; users would have to register the public key on GitHub anyway. |
| **GitHub App with installation token** | Strongest model, rotating credentials.                                                                               | Massive overhead for a single-user device.                                                                                                                                              |

### Decision

**HTTPS + PAT.** In v0.1 the PAT (and all other config) is compiled into the
firmware binary via build-time env vars — the dev's-only-user model makes the
binary-as-secret-store acceptable. From v0.9 onward, the PAT moves to
encrypted LittleFS with a key derived from the chip's eFuse, so a stolen SD
card alone is not enough.

### Consequences

- The user (= dev, in v0.1) must generate a PAT with `repo` scope and supply
  it as a build-time env var. Provisioning is build-time only — see
  [v0.1 product → provisioning](v0.1-mvp-product.md#provisioning-build-time-dev-only).
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

| Option                         | Pros                                                                                                        | Cons                                                                                                                                      |
| ------------------------------ | ----------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| **`std::thread` + channels**   | Boring, debuggable, stack traces work, no executor to tune; ESP-IDF FreeRTOS underneath is well-understood. | Each thread costs 8–32 KB stack depending on workload; not zero-cost like async.                                                          |
| **`embassy` async**            | Trendy, ergonomic, low memory per task.                                                                     | `esp-idf-rs` and `embassy` don't mix cleanly; adopting embassy means dropping `std` and rewriting against `esp-hal` ([ADR-001] reversed). |
| **`tokio` on `esp-idf-rs`**    | Familiar async.                                                                                             | Heavy executor, oversized for ≤ 8 tasks, mbedtls/`gitoxide` integration would need a lot of glue.                                         |
| **Single-threaded event loop** | Smallest memory.                                                                                            | Long-running ops (git push, full refresh) block input.                                                                                    |

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

| Option                                                         | Pros                                                                                                                | Cons                                                                                       |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| **SD (FAT) for working copy + LittleFS (internal) for config** | Plays to each medium's strengths; user can pop the SD to read on desktop; config can't be lost by yanking the card. | Two filesystems to manage.                                                                 |
| **All on SD**                                                  | One filesystem.                                                                                                     | Config disappears if SD is removed; PAT on FAT is harder to protect than on encrypted NVS. |
| **All in internal flash**                                      | Single medium; encrypted.                                                                                           | 16 MB flash limits notes growth; no desktop-side access; SD slot becomes pointless.        |
| **SPIFFS for everything**                                      | Single FS, well-known on ESP32.                                                                                     | SPIFFS isn't great with large files; no removability.                                      |

### Decision

**FAT on SD for `/sd/repo/` and `/sd/local/`. LittleFS on internal flash
for `/nvs/config.toml`.** PAT inside config is encrypted with an eFuse-
derived key.

### Consequences

- User can plug the SD into a laptop and read/edit files there.
  Discouraged but possible.
- Config survives SD reformatting.
- Power-loss safety on FAT is weaker than LittleFS — we mitigate with
  atomic-rename writes (see
  [v0.1 technical → `persistence`](v0.1-mvp-technical.md#module-breakdown)
  and [file layout](v0.1-mvp-technical.md#file-layout)).

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

- v0.1 device is tethered. Not the final aesthetic, but the right MVP —
  scope is in [v0.1 product → out of scope](v0.1-mvp-product.md#out-of-scope-for-v01).
- We can decide cell capacity from real numbers in v0.8, not specs sheets.
- Lid-close detection / deep sleep slips to v0.8 with the battery — see
  [roadmap → v0.8](roadmap.md#v08--power-battery--sleep--).

---

## ADR-009: Keyboard transport — USB host (TinyUSB)

**Status:** Accepted — 2026-05-14
**Scope:** v0.1 through at least v1.0.

### Context

The Nuphy keyboard speaks both wired USB-C (HID) and Bluetooth LE (HID).
The ESP32-S3 has USB OTG (host capable) and BLE 5. Either transport works.

### Options considered

| Option                                       | Pros                                                                                                                                                                      | Cons                                                                                                                                                        |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **USB host (TinyUSB)**                       | Keyboard draws no battery of its own; ESP32-S3 powers it through the host port; standard boot-protocol HID is well-supported; no radio contention with Wi-Fi during push. | One more USB connector on the enclosure; cable between device and keyboard (or shared chassis).                                                             |
| **BLE-HID**                                  | No cable; keyboard can be slightly remote from the device.                                                                                                                | Keyboard has its own battery to manage; BLE shares the 2.4 GHz radio with Wi-Fi, so a `Ctrl-G` push contends with input; pairing UX is more first-run work. |
| **UART receiver (custom keyboard firmware)** | Lowest latency, simplest stack.                                                                                                                                           | Requires reflashing the Nuphy or building a passthrough; not viable as a product choice.                                                                    |

### Decision

**USB host (TinyUSB) for v0.1.** BLE-HID is kept as a documented fallback
if TinyUSB host turns out unstable
([spike 4](v0.1-mvp-technical.md#hardware-bring-up-order) is the gate).

### Consequences

- Enclosure design must include a USB-A or USB-C port for the keyboard.
- The Nuphy's own battery is irrelevant when wired — saves the user a
  charging surface.
- Wi-Fi and keyboard input do not contend for radio time.
- If we ever want a fully wireless build, we revisit with a BLE-HID ADR.

---

## ADR-010: Publish UX — atomic `Ctrl-G`, auto-timestamp commit message, no user prompt

**Status:** Accepted — 2026-05-14
**Scope:** Whole project, all releases.

### Context

The device needs an action that ships writing to the git remote. Most
git-using tools expose `commit` and `push` as distinct user gestures, often
with a commit-message prompt. The device's actual user (= the author of this
firmware) already uses the [`gct` shell alias](../CONTEXT.md#user-facing-actions)
for their own writing: `git add . && git commit -m "<timestamp>" && git push`,
with a `git pull --no-edit` fallback when the push fails non-fast-forward.
`gct` is the established workflow; the typewriter mirrors it.

### Options considered

| Option                                                                                    | Pros                                                                                                            | Cons                                                                                                                                                                                                       |
| ----------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Three separate gestures** (save / commit / push)                                        | Maximally git-native; user has fine control.                                                                    | Three keys to remember, three failure modes to surface, three concepts in the user's head. Wrong shape for an appliance whose job is to remove ceremony.                                                   |
| **One gesture, prompt for message** (`Ctrl-G` → modal asking for message → commit → push) | Conventional "publish" pattern; each commit is named.                                                           | A modal prompt on e-ink is hostile (latency, full refresh); the user's actual workflow (`gct`) explicitly avoids authoring messages; messages would be noise (`"updated notes"` × 1000).                   |
| **One gesture, auto-timestamp message** (`Ctrl-G` mirrors `gct`)                          | Matches the user's real workflow; one key, one outcome; no prompts, no modes, no decisions in the writing path. | Commit history is timestamp-noise (useless for code archaeology); a future reader will wonder where the commit messages went; locks in a UX assumption that's hard to undo without breaking muscle memory. |

### Decision

**One gesture, auto-timestamp message, atomic from the user's view.** `Ctrl-G`
runs the full `gct` sequence (stage all → short-circuit if nothing staged →
commit with ISO-8601 timestamp → push → on push failure, `pull --no-edit` then
retry). Failure surfaces as a single retry-able outcome in the status line.

### Consequences

- The user's vocabulary collapses to **Save** and **Publish**;
  [`CONTEXT.md`](../CONTEXT.md#user-facing-actions) pins this — _commit_ is
  not a user-facing term.
- Commit history is a stream of timestamps. The device is a writing tool, not
  a code repository — the history is here for recoverability, not narrative.
- The pull-merge-retry path means the device may author merge commits on the
  user's behalf, with git's default merge message. Acceptable: the user
  doesn't read commit history from the device anyway.
- The previously-planned "commit message prompt" item in v0.7 has been
  removed from the roadmap.
- Reversing this later (introducing message prompts) would change the
  semantics of `Ctrl-G` and break the user's muscle memory. Hard-to-reverse
  by design.

---

## How to add a new ADR

1. Append a new `## ADR-NNN: <title>` section to this file.
2. Status starts as **Proposed**, with today's date.
3. Once merged + agreed, flip to **Accepted**.
4. When superseded, leave the old ADR in place and add **Superseded by
   ADR-MMM** to its status line. Never delete.
5. Cross-reference from the relevant section of the README or design docs
   if the decision is load-bearing for code review.

[ADR-001]: #adr-001-language-and-runtime--rust-on-esp-idf-rs-std
[ADR-002]: #adr-002-ui-strategy--custom-widgets-on-embedded-graphics-not-ratatui
[ADR-003]: #adr-003-display-medium--e-ink-gdey0579t93-panel
[ADR-004]: #adr-004-git-implementation--gitoxide-gix
[ADR-005]: #adr-005-auth--https--github-personal-access-token
[ADR-006]: #adr-006-concurrency--stdthread--channels-no-async-runtime
[ADR-007]: #adr-007-storage-split--fat-on-sd-for-working-copy-littlefs-on-flash-for-config
[ADR-008]: #adr-008-mvp-power--wall-powered-battery-deferred-to-v08
[ADR-009]: #adr-009-keyboard-transport--usb-host-tinyusb
[ADR-010]: #adr-010-publish-ux--atomic-ctrl-g-auto-timestamp-commit-message-no-user-prompt
