# Quality Function Deployment

Translates what the device must _be_ (user-facing requirements) into what it
must _do_ (engineering functions) and what we must _build_ (components).
Surfaces the few targets that dominate the design and the conflicts between
them. Every decision cell points back to [`adr.md`](adr.md).

Scope: v0.1 MVP — see
[`v0.1-mvp-product.md`](v0.1-mvp-product.md) for user-facing scope and
[`v0.1-mvp-technical.md`](v0.1-mvp-technical.md) for implementation —
with the v0.2–v1.0 trajectory ([README](../README.md),
[roadmap](roadmap.md)) in mind so we don't paint into a corner. Terminology
(e.g. **Tracked**, **Local**, **Save**, **Publish**) follows the project
glossary at [`../CONTEXT.md`](../CONTEXT.md).

Format inspired by the classic House of Quality, kept compact. Strength
weights: **9** strong, **3** medium, **1** weak, blank none.

---

## 1. Customer requirements (the WHATs)

What a user (= me) values about the device, with importance weights on a
1–10 scale. Source columns point at the doc the requirement comes from.

| ID  | Requirement                                             | Weight | Source                                                                                                             |
| --- | ------------------------------------------------------- | :----: | ------------------------------------------------------------------------------------------------------------------ |
| W1  | Sub-second visible response to typing                   |   10   | [product → Write](v0.1-mvp-product.md#user-stories), [README → UX](../README.md#ux-boundaries-set-by-the-medium)   |
| W2  | `Ctrl-G` reliably **Publishes** to the remote           |   9    | [product → Publish](v0.1-mvp-product.md#user-stories), [ADR-010], [CONTEXT → Publish](../CONTEXT.md#user-facing-actions) |
| W3  | Pulling power never corrupts the file                   |   10   | [product → Recover](v0.1-mvp-product.md#user-stories), [acceptance](v0.1-mvp-product.md#acceptance-criteria)       |
| W4  | One-shot provisioning, never repeated mid-session       |   7    | [product → Provisioning](v0.1-mvp-product.md#provisioning-build-time-dev-only), [roadmap → v0.9](roadmap.md#v09--robustness--) |
| W5  | Quick boot to a writing cursor                          |   6    | [product → acceptance](v0.1-mvp-product.md#acceptance-criteria) (≤ 5 s)                                            |
| W6  | Long sessions without crash / lag / drift               |   9    | [product → acceptance](v0.1-mvp-product.md#acceptance-criteria) (1 h soak)                                         |
| W7  | Distraction-free, single-purpose surface                |   8    | [README → vision](../README.md#vision)                                                                             |
| W8  | E-ink-honest UI (no blink, no animation, no flash spam) |   7    | [README → UX](../README.md#ux-boundaries-set-by-the-medium)                                                        |
| W9  | Refactorable across nine downstream releases            |   8    | [roadmap](roadmap.md)                                                                                              |
| W10 | Hackable / DIY-shaped BOM and code                      |   5    | [README → vision](../README.md#vision)                                                                             |
| W11 | Multi-day battery life (v0.8 onward)                    |   4    | [roadmap → v0.8](roadmap.md#v08--power-battery--sleep--)                                                           |
| W12 | Local-only file scope coexists with git scope (v0.5+)   |   5    | [README → scopes](../README.md#vision), [roadmap → v0.5](roadmap.md#v05--file-palette--multi-file--)               |
| W13 | Beautiful monospace font on the writing surface         |   7    | [roadmap → v1.0](roadmap.md), [README → UX](../README.md#ux-boundaries-set-by-the-medium)                          |
| W14 | Beautiful serif font option for reading / published view |  4    | [roadmap → v1.0](roadmap.md)                                                                                        |

---

## 2. Engineering functions (the HOWs)

Measurable characteristics. Targets are v0.1 unless noted. Direction column
shows what "better" looks like (↑ higher, ↓ lower, → fixed).

| ID  | Function                                           | Dir | v0.1 target              | v1.0 target         |
| --- | -------------------------------------------------- | :-: | ------------------------ | ------------------- |
| H1  | Keypress → glyph latency                           |  ↓  | ≤ 200 ms                 | ≤ 150 ms            |
| H2  | Partial-refresh region area per keystroke          |  ↓  | ≤ 1 text line (~22 px h) | same                |
| H3  | Full-refresh cadence (clears ghosting)             |  →  | 1 per 20 partials        | tuned by panel temp |
| H4  | Cold boot → cursor ready                           |  ↓  | ≤ 5 s                    | ≤ 3 s               |
| H5  | Continuous-typing endurance (no drop, no leak)     |  ↑  | ≥ 1 h                    | ≥ 8 h               |
| H6  | `Ctrl-G` push success rate on healthy Wi-Fi        |  ↑  | ≥ 95 %                   | ≥ 99 %              |
| H7  | Push end-to-end (one-file commit)                  |  ↓  | ≤ 30 s                   | ≤ 10 s              |
| H8  | Save survives power loss after status confirms     |  →  | 100 %                    | 100 %               |
| H9  | PSRAM heap headroom during push                    |  ↑  | ≥ 1 MB free at peak      | same                |
| H10 | Firmware binary size                               |  ↓  | ≤ 2 MB                   | ≤ 1.5 MB            |
| H11 | Stack budget across all tasks                      |  ↓  | ≤ 80 KB (sum)            | same                |
| H12 | Wi-Fi reconnect on transient outage                |  ↓  | ≤ 30 s                   | ≤ 10 s              |
| H13 | Idle / typing / push current draw                  |  ↓  | measured only            | sized for >2 days   |
| H14 | Module count / public-API surface (refactor proxy) |  →  | ≤ 8 modules              | same                |
| H15 | Build time (clean, release)                        |  ↓  | ≤ 7 min                  | ≤ 5 min             |

---

## 3. House of Quality — WHATs × HOWs

Reading: row × column cell is how strongly the function (H) advances the
requirement (W). Importance at the bottom is `Σ(weight × strength)` — the
weighted vote on which functions deserve the most engineering attention.

|         | H1 lat  | H2 area | H3 cad  | H4 boot | H5 soak | H6 push% | H7 push s | H8 dura | H9 heap | H10 bin | H11 stk | H12 wifi | H13 mA | H14 mod | H15 build |
| ------- | :-----: | :-----: | :-----: | :-----: | :-----: | :------: | :-------: | :-----: | :-----: | :-----: | :-----: | :------: | :----: | :-----: | :-------: |
| W1 (10) |  **9**  |    9    |    3    |         |    3    |          |           |         |    1    |         |    1    |          |        |         |           |
| W2 (9)  |         |         |         |         |         |  **9**   |     3     |         |    9    |         |         |    9     |        |         |           |
| W3 (10) |         |         |         |         |         |          |           |  **9**  |         |         |         |          |        |         |           |
| W4 (7)  |         |         |         |         |         |    3     |           |         |         |         |         |    3     |        |    1    |           |
| W5 (6)  |         |         |         |  **9**  |         |          |           |         |         |    3    |         |          |        |         |           |
| W6 (9)  |    3    |         |    3    |         |  **9**  |    3     |           |    3    |    9    |         |    3    |    3     |        |         |           |
| W7 (8)  |    3    |    3    |    3    |         |         |          |           |         |         |         |         |          |   3    |    1    |           |
| W8 (7)  |    1    |    9    |  **9**  |         |         |          |           |         |         |         |         |          |        |         |           |
| W9 (8)  |         |         |         |         |         |          |           |         |         |    1    |    1    |          |        |  **9**  |     3     |
| W10 (5) |         |         |         |         |         |          |           |         |         |    3    |         |          |   1    |    3    |     1     |
| W11 (4) |         |         |         |         |         |          |           |         |         |         |         |          | **9**  |         |           |
| W12 (5) |         |         |         |         |         |    1     |           |    3    |         |         |         |          |        |    3    |           |
| W13 (7) |    1    |    3    |         |         |         |          |           |         |    3    |         |         |          |        |         |           |
| W14 (4) |         |         |         |         |         |          |           |         |    3    |         |         |          |        |         |           |
| **Σ**   | **155** | **198** | **144** | **54**  | **111** | **134**  |  **27**   | **132** | **205** | **41**  | **45**  | **129**  | **65** | **117** |  **29**   |

### Top engineering priorities (from importance)

1. **H9 — PSRAM heap during push** (205). gitoxide pack + rope + TLS all
   share the same arena; [ADR-001] and [ADR-004] trade binary size for ecosystem
   so this becomes the watched metric. Two embedded fonts (W13, W14) each
   keep their own glyph cache, adding to the pressure.
2. **H2 — partial-refresh region area** (198). Bound how many pixels the
   panel has to flip per keypress; [ADR-003] is the hardware-side answer.
   A mono writing surface (W13) bounds it predictably; a serif option (W14)
   widens it.
3. **H1 — keypress latency** (155). The single most user-visible number;
   [ADR-002] and [ADR-003] are co-conspirators.
4. **H3 — full-refresh cadence** (144). The ghosting/flash tradeoff; lives
   in the render layer.
5. **H6 — push success rate** (134). [ADR-004] (gitoxide) and [ADR-005] (PAT
   over HTTPS) own this jointly; spike 7 is the kill-switch.
6. **H8 — save durability** (132). Atomic-rename + fsync; FAT's weakness is
   acknowledged in [ADR-007] and mitigated, not designed around. H8 sits
   below the latency cluster because only three WHATs touch it (W3, W6,
   W12) — fewer voters, not weaker requirement.

The bottom three (H7 push time, H15 build time, H10 binary size) are real
costs but ones we knowingly took on ([ADR-001]) and are not in the critical
path of user experience. The tightened H15 v0.1 target (≤ 7 min) reflects
user preference for faster iteration, not matrix-derived priority; if it
pushes back against [ADR-001]'s "+5–10 min" pricing, the target moves
before the runtime decision does.

---

## 4. Roof — function-vs-function tradeoffs

The roof shows where pushing one function pushes another the wrong way.
**`++`** strong reinforcement, **`+`** mild, **`–`** mild conflict,
**`– –`** strong conflict.

|         | H1  | H2  | H3  | H4  | H5  | H6  | H7  | H8  | H9  | H10 | H11 | H12 | H13 | H14 | H15 |
| ------- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **H1**  | —   | ++  | –   |     | +   |     |     |     |     |     |     |     | –   |     |     |
| **H2**  |     | —   | ++  |     |     |     |     |     |     |     |     |     | +   |     |     |
| **H3**  |     |     | —   |     |     |     |     |     |     |     |     |     | +   |     |     |
| **H4**  |     |     |     | —   |     |     |     |     |     | –   |     |     |     |     |     |
| **H5**  |     |     |     |     | —   | +   |     | +   | – – |     |     |     |     |     |     |
| **H6**  |     |     |     |     |     | —   | +   |     | – – |     |     | ++  |     |     |     |
| **H7**  |     |     |     |     |     |     | —   |     | –   |     |     | ++  |     |     |     |
| **H8**  |     |     |     |     |     |     |     | —   |     |     |     |     |     |     |     |
| **H9**  |     |     |     |     |     |     |     |     | —   | – – |     |     |     |     |     |
| **H10** |     |     |     |     |     |     |     |     |     | —   |     |     |     |     | – – |
| **H11** |     |     |     |     |     |     |     |     |     |     | —   |     | –   |     |     |
| **H12** |     |     |     |     |     |     |     |     |     |     |     | —   |     |     |     |
| **H13** |     |     |     |     |     |     |     |     |     |     |     |     | —   |     |     |
| **H14** |     |     |     |     |     |     |     |     |     |     |     |     |     | —   | –   |
| **H15** |     |     |     |     |     |     |     |     |     |     |     |     |     |     | —   |

### Conflicts that actually shape the design

- **H1 latency ↔ H3 refresh cadence** (mild). More partial refreshes per
  second pile up ghosting faster, demanding earlier full refreshes —
  visible flashes that hurt H8 perception and H1 burst behaviour. The
  [ADR-003] strip aspect is the structural answer: a small framebuffer makes
  _both_ cheaper, not one at the expense of the other. The runtime answer
  is render §H3: schedule full refreshes on idle ≥ 1 s (v0.1 tech doc).
- **H9 heap ↔ H10 binary size** (strong). std + gitoxide + mbedtls inflate
  both. We chose to spend on these ([ADR-001], [ADR-004]) because 16 MB flash
  and 8 MB PSRAM make them affordable; the kill-switch is spike 7. If
  heap during push refuses to come under 1 MB free, [ADR-004] flips to
  libgit2-sys for v0.1.
- **H9 heap ↔ H5 soak** (strong). A long writing session grows the rope
  and the glyph cache; pushing on top can OOM. Mitigation: 256 KB file cap
  (v0.1 tech doc) + glyph cache eviction before push + watching the spike
  in spike 7.
- **H6 push success ↔ H12 Wi-Fi reconnect** (reinforcing). Both come from
  the same network stack; investing in reconnect backoff helps both.
- **H10 binary ↔ H15 build time** (strong). std builds are slow. Accepted
  in [ADR-001] — refactor leverage (H14) is the long-term payoff, not the
  per-build seconds.
- **H4 boot ↔ H10 binary** (mild). Larger binary = slower flash load.
  Affordable at our size class but worth watching as features land.
- **H11 stacks ↔ H13 current draw** (mild, future). Idle threads draw
  little but never zero; a future light-sleep policy (v0.8) wants them
  parked.
- **H14 modularity ↔ H15 build time** (mild). More small crates = more
  link work. Boring vs valuable; we lean toward modularity.
- **W13/W14 fonts ↔ H9 heap + H10 binary** (mild, future). Embedding both
  a mono and a serif typeface inflates the binary and adds a second glyph
  cache. Not load-bearing in v0.1 (one font), but the v1.0 typography goal
  is the reason H9 and H10 need slack rather than being squeezed to the
  minimum.
- **Tightened H15 ↔ [ADR-001]** (mild). Pulling v0.1 build time from
  ≤ 10 min to ≤ 7 min eats into [ADR-001]'s accepted "+5–10 min" cost.
  Worth aiming at via cargo profile / vendor LTO / crate-graph trims;
  worth giving up before reversing [ADR-001].

---

## 5. Function → Component mapping (Phase 2)

Which subsystem owns the delivery of each function. Cells are which ADR
constrains the choice.

Components (with anchoring ADR):

| ID  | Component                            | ADR                    |
| --- | ------------------------------------ | ---------------------- |
| C1  | ESP32-S3-N16R8 SoC                   | [ADR-001], [ADR-008]   |
| C2  | `esp-idf-rs` (std) + ESP-IDF         | [ADR-001]              |
| C3  | `std::thread` + `crossbeam-channel`  | [ADR-006]              |
| C4  | PSRAM allocator wrapper              | [ADR-001]              |
| C5  | GDEY0579T93 + DESPI-c579 panel       | [ADR-003]              |
| C6  | `embedded-graphics` + e-paper driver | [ADR-002], [ADR-003]   |
| C7  | Custom widget / dirty-rect layer     | [ADR-002]              |
| C8  | `ropey` rope buffer                  | [ADR-001] (ecosystem)  |
| C9  | TinyUSB host (`esp-idf` bindings)    | [ADR-009]              |
| C10 | FAT on microSD                       | [ADR-007]              |
| C11 | LittleFS on internal flash           | [ADR-007]              |
| C12 | `gitoxide` (`gix-*`)                 | [ADR-004]              |
| C13 | mbedtls TLS (via ESP-IDF)            | [ADR-005]              |
| C14 | HTTPS + GitHub PAT auth              | [ADR-005]              |
| C15 | eFuse-derived encryption key         | [ADR-005], [ADR-007]   |
| C16 | USB-C wall PSU                       | [ADR-008]              |

Function-to-component matrix (9 strong / 3 medium / 1 weak):

|           | C1 SoC | C2 std | C3 thr | C4 PSR | C5 EPD | C6 eg | C7 wid | C8 rope | C9 USB | C10 SD | C11 LFS | C12 gix | C13 TLS | C14 PAT | C15 efs | C16 PSU |
| --------- | :----: | :----: | :----: | :----: | :----: | :---: | :----: | :-----: | :----: | :----: | :-----: | :-----: | :-----: | :-----: | :-----: | :-----: |
| H1 lat    |   3    |   1    |   9    |   3    |   9    |   9   |   9    |    3    |   9    |        |         |         |         |         |         |         |
| H2 area   |        |        |        |        |   9    |   9   |   9    |         |        |        |         |         |         |         |         |         |
| H3 cad    |        |        |        |        |   9    |   3   |   9    |         |        |        |         |         |         |         |         |         |
| H4 boot   |   3    |   9    |   3    |   1    |   3    |       |        |         |        |   9    |    3    |         |         |         |         |         |
| H5 soak   |   3    |   3    |   3    |   9    |   1    |       |        |    9    |   9    |   3    |         |    3    |    3    |         |         |         |
| H6 push%  |        |   3    |        |        |        |       |        |         |        |        |         |    9    |    9    |    9    |         |         |
| H7 push s |        |        |   3    |   1    |        |       |        |         |        |   3    |         |    9    |    9    |         |         |         |
| H8 dura   |        |   3    |        |        |        |       |        |         |        |   9    |    9    |         |         |         |         |         |
| H9 heap   |   3    |   3    |        |   9    |        |       |        |    3    |        |        |         |    9    |    9    |         |         |         |
| H10 bin   |        |   9    |   1    |        |        |   3   |   3    |    3    |   3    |        |         |    9    |    3    |         |         |         |
| H11 stk   |        |        |   9    |        |        |       |        |         |   3    |        |         |    3    |         |         |         |         |
| H12 wifi  |   3    |   9    |        |        |        |       |        |         |        |        |         |         |    3    |         |         |         |
| H13 mA    |   9    |        |   1    |        |   9    |       |        |         |   3    |   3    |         |         |         |         |         |    9    |
| H14 mod   |        |   3    |   3    |        |        |   3   |   9    |    3    |        |        |         |    9    |         |         |         |         |
| H15 build |        |   9    |        |        |        |       |        |         |        |        |         |    9    |    3    |         |         |         |

### Read across, not down

- **C5/C6/C7** (panel + graphics + widget) are the single most leveraged
  cluster — they own H1, H2, H3 (the top of the priority list). [ADR-002]
  and [ADR-003] are the ADRs to keep most honest as v0.x progresses.
- **C12** (`gitoxide`) is overloaded: H6, H7, H9, H10, H11, H14, H15 all
  touch it. That's why [ADR-004] includes a kill-switch (fall back to
  `libgit2-sys` if spike 7 fails). It's also why H9 sits in the top three
  priorities — `gitoxide`'s memory profile is the unknown.
  [ADR-010] pins the *shape* of the publish sequence (the `gct` flow); C12
  is just the library that implements it. Changing [ADR-010] doesn't change
  C12's column, but changing C12 (the kill-switch) does not change
  [ADR-010]'s user contract.
- **C11** (LittleFS) is unused in v0.1 — config is build-time. Its non-zero
  cells in the matrix describe the v0.9+ shape per [ADR-007], not v0.1
  reality.
- **C2** (std runtime) sits underneath almost everything, but it's the
  _enabler_ (H4 boot, H10 binary, H12 Wi-Fi) rather than the bottleneck.
  Reversing [ADR-001] would force re-deciding [ADR-004], [ADR-005],
  [ADR-006], [ADR-007] all at once — they're a single decision in three
  drawers.

---

## 6. Critical performance budget

Pulled from §3 importance and §4 conflicts, in priority order. These are
the numbers spikes 2–7 must validate before integration starts.

| Rank | Function       | Target                                | Watched on        | If we miss it                                                           |
| ---- | -------------- | ------------------------------------- | ----------------- | ----------------------------------------------------------------------- |
| 1    | H2 region area | ≤ 1 line per keypress                 | spike 2 + spike 5 | Increase font size to shrink per-glyph dirty rect ([ADR-003] consequence) |
| 2    | H9 PSRAM heap  | ≥ 1 MB free at push peak              | spike 7           | [ADR-004] kill-switch → `libgit2-sys`; cap rope at 128 KB                 |
| 3    | H8 durability  | 100 % survive power yank after status | bench HIL         | Re-evaluate [ADR-007] (move config to internal NVS only)                  |
| 4    | H1 latency     | ≤ 200 ms keypress→glyph               | spike 5           | Larger partial-refresh region; render multi-char bursts                 |
| 5    | H6 push %      | ≥ 95 % on healthy Wi-Fi               | spike 6 + spike 7 | TLS cipher trim; reconnect backoff tuning                               |
| 6    | H3 cadence     | full every ~20 partials               | spike 2           | Adjust per panel temperature; defer flash to idle ≥ 1 s                 |
| 7    | H4 boot        | ≤ 5 s to cursor                       | integration smoke | Trim startup logging; lazy-mount SD after splash                        |
| 8    | H5 soak        | 1 h no leak / no drop                 | 1 h bench soak    | Glyph-cache eviction; PSRAM heap-fragmentation review                   |

The two not-in-MVP rows but already-shaped-by-design:

| — | H13 current | Measured only in v0.1 | bench multimeter | Cell sizing for v0.8 is data-driven, not spec-sheet |
| — | H11 stacks | Sum ≤ 80 KB | static analysis | Was off-by-2x in [ADR-006] pre-fix — corrected in §7 |

---

## 7. Tradeoffs and their why, linked to ADRs

Plain-language summary of what we accepted in exchange for what.

| Tradeoff                                         | Got                                                                          | Paid                                                                            | ADR       |
| ------------------------------------------------ | ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | --------- |
| std (esp-idf-rs) over no_std (esp-hal)           | Heap, threads, VFS, mbedtls, gitoxide-compatible                             | +1 MB binary, +5–10 min builds                                                  | [ADR-001] |
| Custom widget layer over Ratatui                 | Dirty-rects aligned to e-ink regions; 200 KB binary back                     | 500 LoC we own and maintain                                                     | [ADR-002] |
| e-ink medium over FSTN / memory LCD / OLED       | Paper aesthetic; 0 W idle persistence; medium enforces writing posture       | ~200–300 ms typing latency; periodic full-refresh flash (scroll worst-case)     | [ADR-003] |
| `gitoxide` over `libgit2-sys`                    | Pure Rust, modular, no FFI cross-compile pain                                | Smart-HTTP path is newer; PSRAM profile unproven (spike 7)                      | [ADR-004] |
| HTTPS + PAT over OAuth device-flow or SSH        | Simplest auth that `gitoxide` smart-HTTP already supports                    | Long-lived secret on device; in v0.1 the PAT is compiled into the binary (dev-only target user makes this acceptable); v0.9 moves it to encrypted NVS | [ADR-005] |
| `std::thread` over `embassy` or `tokio`          | Boring, debuggable, real stack traces; no exec to tune                       | ~76 KB total stack across 5 tasks                                               | [ADR-006] |
| FAT-on-SD + LittleFS-on-flash split              | Desktop can read SD; config survives SD reformat                             | Two filesystems to manage; FAT's power-loss weakness mitigated by atomic-rename | [ADR-007] |
| Wall power for v0.1, battery deferred            | Measure real draw before sizing the cell                                     | Tethered MVP; not the final aesthetic                                           | [ADR-008] |
| USB host (TinyUSB) over BLE-HID                  | No radio contention with Wi-Fi during push; keyboard powered from the device | One more USB connector on enclosure                                             | [ADR-009] |
| Atomic `Ctrl-G` + auto-timestamp commit message  | One key, one outcome; matches the user's existing `gct` workflow; no modal prompt to slow H1 latency | Commit history is timestamp noise; the device may author merge commits the user never sees; reversal would break muscle memory | [ADR-010] |

### Conflicts left explicitly _unresolved_ by v0.1

These are the live tensions we are watching, not deciding harder:

- **[ADR-004] vs H9.** If `gitoxide` cannot keep ≥ 1 MB PSRAM free at push
  peak, we are committed to switching transports for v0.1, not absorbing
  the OOM risk.
- **[ADR-009] vs H6/H13.** If TinyUSB host turns out unstable (spike 4),
  BLE-HID is the documented fallback — at the cost of Wi-Fi radio
  contention during push (re-checking H6).
- **[ADR-007] vs H8.** Power loss between FAT rename and dir flush yields
  the previous saved version. We document this as expected behavior; it
  becomes a real bug only if soak testing shows it triggering on routine
  saves.

---

## 8. Inconsistencies spotted and fixed

- **[ADR-006] stack figure.** [ADR-006] previously said "~40 KB of stack
  space for task stacks" — but the v0.1 technical design's task table
  (`usb 8 + wifi 8 + ui 16 + render 12 + git 32`) sums to **76 KB**.
  Updated [ADR-006]'s Consequences section to reflect the actual budget
  and cross-reference the tech doc. The 76 KB figure still fits
  comfortably in the ESP32-S3's 512 KB internal SRAM, so no design
  change — just documentation accuracy.
- **Commit-message format triple-mismatch.** README said `git commit -m
  "wip"`, the v0.1 product doc said `"wip <timestamp>"`, and the user's
  actual shell alias (`gct` / `git-commit-timestamp`) uses a pure ISO-8601
  timestamp with no `wip` prefix. Resolved by aligning all docs on `gct`
  and recording the decision as
  [ADR-010].
  Pulled the v0.7 roadmap item "Commit message prompt instead of hard-coded
  `wip`" — it's now contradicted by [ADR-010] and removed.
- **First-run flow vs. target user.** The v0.1 product doc described a
  captive-portal first-run, but the same doc names the v0.1 target user as
  the dev themselves ("Me. Solo."). Provisioning a solo-dev device through
  a captive portal is ceremony without a user. Resolved by switching v0.1
  to build-time env-var config (no NVS, no LittleFS, no AP mode); on-device
  provisioning is the v0.9 release that introduces non-dev users. Touches
  [ADR-005], [ADR-007], the v0.1 product + technical docs, and the v0.9
  roadmap entry.
- **Vocabulary leak.** Earlier docs used "commit" and "push" as if they
  were distinct user actions; the gct/[ADR-010] model collapses them into a
  single user-facing **Publish**. Resolved by introducing
  [`CONTEXT.md`](../CONTEXT.md) as the canonical glossary; user-facing text
  now uses **Save** and **Publish** only.
- **House of Quality column sums recomputed.** Earlier Σ row drifted from
  the matrix arithmetic — H1 listed 138 but sums to 148; H8 147 vs 132;
  H9 162 vs 172; H13 74 vs 65; smaller deltas elsewhere. Recomputed all
  sums from the cells. Folded in W13/W14 at the same pass. The reordering
  moved H9 to #1 (205), H2 to #2 (198), H1 to #3 (155); H8 dropped from
  #3 to #6 (132). H8's drop is a "fewer WHAT voters" artifact, not a
  signal that durability matters less to the design.

The minor variance between README's "~12 lines" and product/[ADR-003]'s
"~11 lines" of edit area is within rounding for a 14 px glyph in a 240 px
tall edit region and is not load-bearing.

---

## How to keep this document honest

- When a new ADR lands, add its components to §5 and re-score any
  function-row whose dominant component changed.
- When a spike returns numbers, update §6's "Target" or "Watched on"
  columns — this is the doc that _should_ feel out of date if measured
  reality drifts from estimates.
- The WHATs change rarely; the HOWs change with each release; the
  matrices are recomputed when either side changes.

[ADR-001]: adr.md#adr-001-language-and-runtime--rust-on-esp-idf-rs-std
[ADR-002]: adr.md#adr-002-ui-strategy--custom-widgets-on-embedded-graphics-not-ratatui
[ADR-003]: adr.md#adr-003-display-medium--e-ink-gdey0579t93-panel
[ADR-004]: adr.md#adr-004-git-implementation--gitoxide-gix
[ADR-005]: adr.md#adr-005-auth--https--github-personal-access-token
[ADR-006]: adr.md#adr-006-concurrency--stdthread--channels-no-async-runtime
[ADR-007]: adr.md#adr-007-storage-split--fat-on-sd-for-working-copy-littlefs-on-flash-for-config
[ADR-008]: adr.md#adr-008-mvp-power--wall-powered-battery-deferred-to-v08
[ADR-009]: adr.md#adr-009-keyboard-transport--usb-host-tinyusb
[ADR-010]: adr.md#adr-010-publish-ux--atomic-ctrl-g-auto-timestamp-commit-message-no-user-prompt
