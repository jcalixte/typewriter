# Quality Function Deployment

Translates what the device must _be_ (user-facing requirements) into what it
must _achieve_ (engineering characteristics) and what we must _build_
(components), cascading through the four classical Houses of Quality:
requirements × characteristics, characteristics × components, components ×
processes, processes × controls. Surfaces the few targets that dominate the
design and the conflicts between them. Every decision cell points back to
[`adr.md`](adr.md). Strength weights everywhere: **9** strong, **3** medium,
**1** weak, blank none.

Scope: the shipped device (v0.1 delivered 2026-07-11, v0.5–v0.7 delivered
2026-07-12/14, v0.9 onboarding in flight; see
[`v0.1-mvp-product.md`](v0.1-mvp-product.md),
[`v0.5-palette-and-multi-file.md`](v0.5-palette-and-multi-file.md),
[`v0.7-search-and-git.md`](v0.7-search-and-git.md),
[`v0.9-onboarding-wizard.md`](v0.9-onboarding-wizard.md)), **plus the
companion products** that now deliver the getting-started outcome: the macOS
installer ([`../installer/DESIGN.md`](../installer/DESIGN.md)), the
typoena.dev site with its `install.sh` one-liner, and the Typoena GitHub App
(device-flow auth shared by installer and on-device wizard). The remaining
v0.8–v1.0 trajectory ([README](../README.md), [macroplan](macroplan.md)) is
kept in mind so we don't paint into a corner. Terminology
(e.g. **Tracked**, **Local**, **Save**, **Push**) follows the project
glossary at [`../CONTEXT.md`](../CONTEXT.md); the WHAT / Function /
Characteristic / Metric / Target ontology is defined in
[`../GLOSSARY.md`](../GLOSSARY.md).

## The pages

Section numbers (§1–§8) are global across the pages; each house diagram
lives on the same page as the tables it mirrors.

- [`qfd-house-1.md`](qfd-house-1.md) — **House 1, WHATs × HOWs**: §1
  requirements + segments, §2 characteristics + measured footnotes, §3
  reading + top priorities, §4 roof conflicts
- [`qfd-perception.md`](qfd-perception.md) — **competitive perception**:
  five products scored 0–5 per WHAT, measured benchmarks, caveats
- [`qfd-house-2.md`](qfd-house-2.md) — **House 2, HOWs × components**: §5
  cascade tree, component catalogue + derived ranking, shared-pool budget
  matrix
- [`qfd-houses-3-4.md`](qfd-houses-3-4.md) — **Houses 3 & 4** under the
  pipeline reading: processes P1–P9 × controls Q1–Q8
- [`qfd-budget.md`](qfd-budget.md) — **§6 critical performance budget**:
  ranked targets, verdicts, and the named fallback per row
- [`qfd-tradeoffs.md`](qfd-tradeoffs.md) — **§7 tradeoffs** T1–T15
  (got / paid / ADR) and the tensions left deliberately unresolved, each
  with its trigger
- [`qfd-changelog.md`](qfd-changelog.md) — **§8 ledger**: every
  inconsistency spotted between the houses and reality, and its fix
- [`quality-house-empty.md`](quality-house-empty.md) — blank practice copy
  of the full four-house cascade
- [`house-vs-product.md`](house-vs-product.md) — standing challenges: when
  the houses and the builder disagree about what the product *is*, the
  dispute is argued there first, not silently re-scored

## What matters now (as of 2026-07-17)

**Top engineering priorities** ([§3](qfd-house-1.md#3-house-of-quality--whats--hows),
by basement Σ): H9 heap during Push (193) · H1 type latency (178) ·
H2 refresh area per keystroke (177) · H12 network reconnect (160) ·
H8 save durability (156).

**Component ranking** ([House 2](qfd-house-2.md), derived): C5 e-ink
panel #1 · C7 widget/editor layer #2 (the headline of the 2026-07-17
W16/flow re-score) · C12 libgit2 #3 · C2 std runtime #4.

**Open gaps** (detail and fallbacks in [§6](qfd-budget.md#6-critical-performance-budget)):

- **H1 erase/caret tier ~630 ms vs ≤ 400 ms** — still unmet. Additive
  typing was bench-clocked 2026-07-21: refresh time is area-*independent*
  (waveform BUSY, not the band-write), so the default windowed factory
  partial is ~495 ms and the earlier ~100–130 ms guess was wrong. The
  custom `0x32` fast-partial LUT (FR `0x08`, `fast_partial` opt-in) reaches
  ~265 ms (meets ≤ 400 ms), pending a longevity + cold soak before it can
  default on:
  [`tradeoff-curves/epd-refresh-latency.md`](tradeoff-curves/epd-refresh-latency.md).
- **H8 power-pull test still owed** (v0.9 gate); dirty journal + boot
  recovery are shipped, the physical test is not run.
- **H17 reach cost and H16 onboarding duration are unmeasured** — the
  two budget rows that have never been clocked (≤ 6 keystrokes median;
  ≤ 10 min blank-card-to-cursor).
- **H7's v1.0 ≤ 10 s Push target is not honest on deep paths**
  (~12–13 s root-level warm): FAT loose-object residual, lever =
  pack-not-loose writes, deferred to a perf pass.

**Live tensions with triggers** ([§7](qfd-tradeoffs.md#conflicts-left-explicitly-unresolved-by-v01)):
keep-alive race (durable fix owed before v1.0 claims ≥ 99 %), token
plaintext at rest (the open [ADR-011]), onboarding reach (SoftAP
companion deferred), FAT rename window ([ADR-007]), typography paths
(v1.0 pass), battery ([ADR-008] — bench current numbers start v0.8 cell
sizing).

## How to keep these documents honest

- When a new ADR lands, add its components to [House 2](qfd-house-2.md)
  and re-score any characteristic row whose dominant component changed.
  **The same applies when an existing ADR gains an Outcome** (a
  kill-switch fires, a decision reverses): cascade it here the same day:
  these pages scored the dead gitoxide option for ten days after the
  swap.
- When a spike returns numbers, update [§6](qfd-budget.md)'s "Target" or
  "Watched on" columns: §6 is the page that _should_ feel out of date if
  measured reality drifts from estimates.
- The companion surfaces (installer, typoena.dev, GitHub App, wizard) are
  in the house as W15 / H16 / C17–C20 but keep their design records in
  [`../installer/DESIGN.md`](../installer/DESIGN.md) and
  [`v0.9-onboarding-wizard.md`](v0.9-onboarding-wizard.md); when those
  ship changes, re-check those rows rather than re-deriving them here.
- The WHATs (§1) change rarely; the HOWs (§2) change with each release.
  When either changes, re-score the matrix and recompute the basement Σ
  in the [House 1](qfd-house-1.md) diagram; then check §3's priority
  list and §4's conflict list still match the new picture — and update
  this hub's "What matters now" if the headlines moved.
- The [House 2](qfd-house-2.md) component Σ/Rank row is **derived**
  (basement Σ × cell strength): recompute it whenever the basement or a
  §5 cell changes, and keep unbuilt components (today C11, C15)
  parenthesised and out of the rank: scored fiction outranks real
  components, as the 2026-07-16 pass showed.
- The shared-pool budget matrix on [House 2](qfd-house-2.md) is the
  source of truth for the pool-mediated roof cells: when a component
  starts allocating from internal DRAM, PSRAM, or the DMA reserve (or a
  telemetry min-ever moves), update the table first, then draw (or
  retire) the roof cell it justifies. The roof was scored from the call
  graph once and missed three crashes; don't score it that way twice.
- Each house diagram mirrors the tables on its own page (House 1 the
  §1/§2 catalogues and [`qfd-perception.md`](qfd-perception.md)'s zone,
  House 2 the §5 matrix, Houses 3–4 the P/Q catalogues): re-score the
  table first, then the drawing, same day. **Diagram and tables stay on
  one page** — the pre-2026-07-11 split is how they drifted apart last
  time. Every house's TikZ preamble is a copy of House 1's: a style
  change to one must be pasted into all four (across
  [`qfd-house-1.md`](qfd-house-1.md), [`qfd-house-2.md`](qfd-house-2.md),
  [`qfd-houses-3-4.md`](qfd-houses-3-4.md)) plus
  [`quality-house-empty.md`](quality-house-empty.md).
- [Houses 3–4](qfd-houses-3-4.md) re-score when the *pipeline* changes
  shape: a new process step (CI, a second-platform installer,
  auto-update) or a new control (a test rig, release automation) gets a
  column and a fresh derivation the day it ships. Their cells are a
  2026-07-16 single-rater first cut; treat the
  P4-has-no-automated-control and Q6-is-the-only-install-path-control
  flags as live until answered.
- A [§6](qfd-budget.md) row is not done when its target is met: the "If
  we miss it" cell must always name a live fallback, and a
  [§7](qfd-tradeoffs.md) tension must always carry a **Trigger to
  revisit**: otherwise it is a decision being avoided, not deferred.
- When the houses and the builder disagree about what the product *is*,
  the dispute goes to [`house-vs-product.md`](house-vs-product.md) first:
  argued with evidence and a trigger, not resolved by a same-day
  re-weight. Weights, rows, or cells change only after the entry there
  says why; and the next House-1 re-score must settle any OPEN entry
  that is waiting on it (none open today: D1/flow resolved 2026-07-17 by
  the W16/H17 re-score).
- Structural passes and every drift caught get a line in the
  [§8 ledger](qfd-changelog.md) the day they land.

[ADR-007]: adr.md#adr-007-storage-split--fat-on-sd-for-working-copy-littlefs-on-flash-for-config
[ADR-008]: adr.md#adr-008-mvp-power--wall-powered-battery-deferred-to-v08
[ADR-011]: adr.md#adr-011-credential-provisioning--how-the-pat-reaches-the-device-and-is-protected-at-rest
