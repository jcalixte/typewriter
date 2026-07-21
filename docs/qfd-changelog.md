# 8. Inconsistencies spotted and fixed

The QFD ledger: every drift caught between the houses and reality, and
what was done about it. Entries dated before 2026-07-17 describe the
single-file `qfd.md` layout; its §1–§8 sections now live across the
`qfd-*.md` pages indexed in [`qfd.md`](qfd.md), section numbers and
anchors unchanged.

- **[ADR-006] stack figure.** [ADR-006] previously said "~40 KB of stack
  space for task stacks", but the v0.1 technical design's task table
  (`usb 8 + wifi 8 + ui 16 + render 12 + git 32`) sums to **76 KB**.
  Updated [ADR-006]'s Consequences section to reflect the actual budget
  and cross-reference the tech doc. The 76 KB figure still fits
  comfortably in the ESP32-S3's 512 KB internal SRAM, so no design
  change, just documentation accuracy.
- **Commit-message format triple-mismatch.** README said `git commit -m
"wip"`, the v0.1 product doc said `"wip <timestamp>"`, and the user's
  actual shell alias (`gct` / `git-commit-timestamp`) uses a pure ISO-8601
  timestamp with no `wip` prefix. Resolved by aligning all docs on `gct`
  and recording the decision as
  [ADR-010].
  Pulled the v0.7 roadmap item "Commit message prompt instead of hard-coded
  `wip`": it's now contradicted by [ADR-010] and removed.
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
  single user-facing **Push**. Resolved by introducing
  [`CONTEXT.md`](../CONTEXT.md) as the canonical glossary; user-facing text
  now uses **Save** and **Push** only.
- **House of Quality column sums recomputed.** Earlier Σ row drifted from
  the matrix arithmetic: H1 listed 138 but sums to 148; H8 147 vs 132;
  H9 162 vs 172; H13 74 vs 65; smaller deltas elsewhere. Recomputed all
  sums from the cells. H8 dropped from #3 to #6: a "fewer WHAT voters"
  artifact, not a signal that durability matters less to the design.
- **W13 reframed, W14 removed.** Earlier W13/W14 rows named solutions
  ("beautiful monospace", "beautiful serif") inside the requirements
  column, conflating _what the user values_ with _which asset delivers it_.
  Replaced with one outcome WHAT (typography sets a writing-tool tone),
  and moved the mono+serif option to §7 as a v1.0 unresolved tension.
  Σ shifted (H9 205 to 193, H2 198 to 177, H1 155 to 148) because the prior
  W13/W14 cells were scoring solution-fit rather than outcome-fit.
- **WHATs swept for solution-shape phrasing.** Following the W13 reframe,
  the same drift was found in W2 (named the key `Ctrl-G`), W4 (named the
  process shape "one-shot"), W7 (named the hardware "surface"), W8 (named
  the medium "e-ink"), W10 (named the deliverable "BOM"), and W9 ("nine
  releases", brittle vs roadmap reshuffles). All rephrased as outcomes;
  the named solutions remain documented in §7 tradeoffs and the relevant
  ADRs where they belong. Matrix cell strengths held (each cell scored
  the characteristic against the underlying outcome, not the surface
  phrasing), so no Σ recompute.
- **§3 vs §6 priority lists clarified.** The two were giving different
  orderings without saying why. §6 now states explicitly that it is a
  curated rank with two named overrides over §3's pure arithmetic:
  acceptance-criteria critical paths (H4, H5) and table-stakes correctness
  (H8) get manual lifts. §3 now names the HoQ structural bias that makes
  the curation necessary (reward for spread, penalty for narrow-but-
  critical characteristics), using H8/W3 as the canonical example.
- **W14 added: portability outcome.** Captures "I can carry the device
  and write away from a desk" as a distinct WHAT from W11 (multi-day
  battery), weight 8. Recomputed basement Σ; H8 lifted from #6 to #3 in
  the §3 priority list as its voter base widened from W3+W6+W12 to also
  include W14, and H12 entered the top six at #4; H6 dropped out. The
  ID "W14" was previously held by a deprecated typography row (see the
  "W13 reframed, W14 removed" bullet above); the slot is now repurposed.
  §6's "(b) narrow voter base" override for H8 no longer applies and
  has been retired in the §6 preamble.
- **H14 retired: outside §2's scope.** §2 covers measurable engineering
  characteristics: performance metrics of the device's functions, or
  properties of its firmware artifact, memory layout, and build process.
  H14 ("Module count / public-API surface (refactor proxy)") is a
  property of source-code organisation, none of those. The refactor-
  leverage idea survives in §5's component structure and the ADRs that
  decide architectural discipline; it does not need a HoQ matrix slot.
  Removed from §2, the §5 matrix row, the C12 overloaded-list mention,
  and the §4 H14↔H15 conflict bullet. W9's matrix vote shrinks from
  `H10 W + H11 W + H14 S + H15 M` to `H10 W + H11 W + H15 M`: an
  honest reading that "codebase absorbs the planned roadmap" is
  delivered by ADRs, not by a measurable characteristic. ID "H14" left
  as a gap (cross-doc HOW references survive without renumbering H15).
  Total basement Σ drops 1674 to 1557, so rel% recomputed in the §3
  basement.
- **HOWs renamed "characteristics," not "functions."** A function is a
  transformation (input → output); HOWs like H6 "success rate" and
  H10 "binary size" are _measures_ of functions or properties of
  artifacts, not transformations themselves. §2's header, §4's
  ("HOW-vs-HOW tradeoffs"), §5's ("HOW → Component mapping") and
  caption, and §6's column header all cascaded: wherever "function"
  meant HOW. Classical QFD uses "engineering characteristics" (or
  "substitute quality characteristics") for exactly this slot. The
  methodology name in the title (Quality Function Deployment) stays:
  it is the framework's proper noun, not a claim about this doc's
  vocabulary.
- **H6/H7/H8/H12 swept for solution-shape phrasing and measure-vs-
  attribute.** Two drifts in one pass. (a) Solution names inside
  characteristic names: H6 was "`Ctrl-G` push success rate on healthy
  Wi-Fi": three solutions inside one name (the key, the git verb, the
  transport); H7 was "Push end-to-end (one-file commit)": git verb and
  its unit; H12 was "Wi-Fi reconnect on transient outage": transport
  in the name. (b) Measure or behaviour assertion instead of attribute:
  H6's "success rate" is a metric; H8 "Save survives power loss after
  status confirms" is a behaviour assertion. Renamed to pure attributes
  under outcome-shaped conditions: H6 = "Push reliability (network
  up)", H7 = "Push latency (one file)", H8 = "Save durability
  (post-confirm power loss)", H12 = "Network reconnect time (transient
  outage)". H7's "latency" pairs with H1's "Type latency".
  Matrix cell strengths held; no Σ recompute.
- **Functions surfaced as their own ontology layer.** Earlier, the
  HOW names packed both a function reference and an attribute
  ("Push reliability" = Push [function] + reliability
  [attribute]) without Functions being defined anywhere. §2 now
  opens with a Functions inventory (Type, Save, Push, Recover,
  Boot, Provision) so the function names HOWs reference have a
  single source of truth. Render and Reconnect remain sub-functions
  referenced inside HOW names; they did not earn top-level slots in
  v0.1. The five-layer ontology stack (WHAT / Function /
  Characteristic / Metric+Unit / Target) is documented in
  [`../GLOSSARY.md`](../GLOSSARY.md), peer to `CONTEXT.md`
  (device vocabulary). With Functions explicit, two arrow-style HOW
  names collapsed for parallelism: H1 "Keypress → glyph latency" →
  "Type latency (keypress → glyph)", H4 "Cold boot → cursor ready" →
  "Boot latency (cold)". The arrow text moved to the parenthetical
  context where it belongs once the function name carries the
  transformation; H4's "to cursor" is implicit in Boot's definition.
  Matrix cell strengths held; no Σ recompute.
- **H4 boot measured; H3 cadence corrected; boot-time docs added
  (2026-07-11).** Cold boot instrumented at **4258 ms**: the ≤ 5 s v0.1 target
  is met; §6's H4 row now carries that measured result and the real mitigation
  (editor rides a full-area partial over the splash, −1.25 s) in place of the
  pre-integration guesses (trim logging / lazy-mount SD). §2's ≤ 3 s v1.0 target
  gained a footnote flagging it **marginal-to-unreachable**: one ~1.9 s full
  refresh is an unavoidable e-ink cold-boot floor. Separately, §2 + §6 H3
  full-refresh cadence corrected from "1 per 20 partials" to **1 per 64**: the
  firmware stretched it (`FULL_REFRESH_EVERY = 64`) once windowed-Y refresh made
  ghosting rare: a drift that predated this pass. New supporting docs:
  [`notes/boot-time-budget.md`](notes/boot-time-budget.md) (waterfall + v1.0
  feasibility) and
  [`tradeoff-curves/epd-refresh-latency.md`](tradeoff-curves/epd-refresh-latency.md)
  (rows-vs-latency model), cross-linked from §4's H1↔H3 bullet.
- **Typoena perception column rebased from target to measured (2026-07-11).**
  With v0.1 delivered and hardware-verified, the §3 right-hand zone's Typoena
  profile is the shipped v0.1 result, not a §2 target projection: legend +
  caption relabelled "v0.1 measured", W5 rationale now cites the 4.26 s cold
  boot, W3 notes the verified atomic round-trip (power-pull test still deferred
  to v0.9), **W6 rose 3 to 4** on the attested 1 h soak, and **W1 dropped 4 to 2**
  once type latency was measured at ~630 ms (over the revised ≤400 ms target).
  Net Typoena total 52 to 51, trimming its lead over Pomera to a single point.
  Competitor scores untouched (no new external release). Two drifts caught in the same pass: the
  TikZ W14 row scored Pomera/Smart 2/5 while the authoritative table and totals
  use 5/1 (Smart ~5 lb desk-bound = 1, Pomera pocketable = 5), TikZ corrected
  to match; and the Caveats "thirteen rows" corrected to "fourteen" (§1 has 14
  WHATs).
- **H1 type-latency target relaxed ≤200 to ≤400 ms; v1.0 reset to ≤300 ms
  (2026-07-11).** Cold per-keystroke render measures ~630 ms, so §2's v0.1 H1
  target moved from ≤ 200 ms to ≤ 400 ms and gained a footnote; §3's basement
  target text and §6's rank-4 row followed. The relaxed target is unmet: ~630 ms
  still exceeds ≤ 400 ms (the open v0.1 latency gap), though a longer wait is
  acceptable for now; next-version usage will settle it. The perception W1
  score dropped 4 to 2 to match. The v1.0 figure was reset from ≤ 150 ms to
  ≤ 300 ms ([ADR-003]'s ~200–300 ms floor); ≤ 150 ms sat below what the panel
  can deliver.

- **This file lagged [ADR-004]'s fired kill-switch by ten days (fixed
  2026-07-16).** Spike 7 fired the kill-switch on 2026-07-06 (gix has no
  HTTPS push; the shipped git engine is `libgit2`/`git2` as an esp-idf CMake
  component), and adr.md recorded it in an "Outcome" section, but this doc
  kept scoring C12 as `gitoxide` and §7 kept "gitoxide over libgit2-sys" as
  the standing decision. §3 narrative, §4 roof bullets, §5 C12 + read-across,
  §6 rank-2 fallback, and the §7 row all rewritten to the libgit2 reality.
  Lesson for the "keep this honest" list: an ADR outcome edit must cascade
  here the same day.
- **`Ctrl-G` → `:gp` swept (2026-07-16).** Push moved off Ctrl-G to the
  `:gp` ex command (`:sync` → `:gp` rename 2026-07-14); the keymap has no
  Ctrl-G binding at all. W2's rationale and §7's [ADR-010] row updated, and
  [ADR-010] itself was amended the same day with an as-shipped Outcome
  section covering all three of its drifts: the `:gp` trigger, the
  `Typoena push — unix <epoch>` message (not ISO-8601), and
  replay-not-merge on rejected pushes (the "device may author merge
  commits" consequence never materialised).
- **Config landed on the card, not in encrypted internal flash
  (2026-07-16).** [ADR-005]/[ADR-007] planned "v0.9 moves the secret to
  encrypted LittleFS/NVS with an eFuse key"; v0.9 actually shipped plaintext
  `/sd/typoena.conf`, deliberately, so the wizard and the macOS installer
  produce one identical, desktop-inspectable artifact. C11/C15 are therefore
  still unused, and at-rest protection is the open [ADR-011]. §5's C11
  bullet and §7's auth row updated; the tension is now explicit in §7's
  unresolved list instead of being mis-described as done-in-v0.9.
- **H11 stack budget was fiction twice over (2026-07-16).** The ≤ 80 KB
  target priced a five-thread model (usb/wifi/ui/render/git, 76 KB) that no
  longer exists: UI and render run on the main task, Wi-Fi is owned by the
  git thread, and the shipped explicit stacks are git 96 KB + walk 16 KB +
  USB 4+8 KB = **124 KB**. Target revised to ≤ 128 KB (§2 ∥); §6's row now
  carries the measured breakdown. The 96 KB git stack is an [ADR-004]
  consequence the old budget predates.
- **H1's ~630 ms was the wrong tier (2026-07-16).** The 2026-07-11 footnote
  presented ~630 ms as "per-keystroke render", but the refresh-latency curve
  doc shows that figure is the **full-area partial** (deletes, caret moves,
  splash swap); additive typing rides the windowed-Y partial at ~100–130 ms
  (projected: bench confirmation still owed from the on-device refresh
  log). §2 §-footnote rewritten as a two-tier story; perception W1 raised
  2 to 3, not higher, until the bench number lands and the erase tier gets a
  lever.
- **W15 + H16 added: the companions enter the house (2026-07-16).** The
  product now includes surfaces that are not the device: the macOS installer
  (card provisioner), typoena.dev + `install.sh`, the Typoena GitHub App,
  and the on-device wizard. Their shared user outcome landed as W15 ("a
  first-time user reaches writing without developer tools", weight 7), their
  shared characteristic as H16 (onboarding duration, ≤ 10 min, unmeasured),
  and their parts as C17–C20. Basement Σ recomputed 1557 to 1627 (H12 picks
  up W15's weak vote, 153 to 160); rel% re-derived. The house deliberately
  reads H16 as bottom-tier for the daily writing loop: its weight is about
  product reach, and §6 carries its (unmeasured) budget row.
- **W14's "no enclosure spec yet" was stale (2026-07-16).** The parametric
  OpenSCAD case exists (`hardware/case/`, scad + stl + renders); the score
  stays 2 because portability hinges on [ADR-008]'s battery, not the shell.
  Rationale corrected.
- **[ADR-009] TinyUSB tension retired (2026-07-16).** "If TinyUSB turns out
  unstable, BLE-HID is the fallback" sat in §7's unresolved list since
  before spike 4; the USB host path has since carried every hardware session
  for two weeks. Removed from the live-tension list: reopening it would
  take new evidence, not vigilance.
- **Companion-side doc drift, flagged and then fixed the same day
  (2026-07-16).** The site repo's README called `install.sh` a placeholder
  that "flashes the firmware": rewritten to the live, checksum-verified,
  never-flashes reality (and its repo pointer corrected to the
  `typoena` org). The installer's `DESIGN.md` still cited
  `installer-v0.1.0` as the release: trued up to the tag-per-release
  model, latest `installer-v0.4.0`; the GitHub release itself was never
  lagging (latest-release already served 0.4.0), only the prose.
  `v0.5-palette-and-multi-file.md`'s header still said "slice 1 of 4" and
  `v0.6-markdown.md`'s still said slice 5 was remaining: both stamped
  **DELIVERED 2026-07-12** to match macroplan and the on-device record.
- **Format-alignment pass against the QFD skill's canonical DESIGN shape
  (2026-07-16)**, and the two inconsistencies it flushed out. Additions:
  the §5 cascade tree (WHAT → Function → How → Components, rejected
  alternatives kept visible), the derived component Σ/Rank row (component
  priorities now arithmetic, not asserted), a mandatory "If we miss it"
  fallback per §6 row, an explicit **Trigger to revisit** per §7 tension,
  T-IDs on the tradeoff rows, the §3 characteristic-benchmarks table
  (numbers beat ratings; blanks beat guesses), theme grouping on the §1/§2
  catalogues, and the U1/U2 segment table making the single-rater bias
  structural. The derivation immediately earned its keep twice: (a)
  **H12 × C12 was blank** although the shipped H12 lever, TLS session
  resumption, lives in C12's vendored `esp_mbedtls_stream.c`; cell added
  at 3 (basement unaffected; it scores WHAT × HOW). (b) **C11's matrix
  votes were fiction**: unbuilt LittleFS would have ranked #13, above
  actually-shipped C14; its Σ is now parenthesised and unranked until
  [ADR-007]'s future shape ships. Kept deliberately against the skill's
  letter: "engineering characteristics" over its "Functions" naming (the
  GLOSSARY.md ontology is sharper: H10 "binary size" is no verb), the
  `docs/qfd.md` + single `adr.md` layout (anchors are load-bearing;
  renaming buys convention, pays link churn), and the `++/−−` roof glyphs
  (mapped to the classical `◎○×⊗` in §4's legend).
- **House 2 drawn (2026-07-16).** §5 was titled "Phase 2" but its
  HOW → component matrix had never been rendered as a house: the doc
  showed one house and called itself a QFD cascade. The Phase-2 house now
  sits in §5 (same `qfdhouse` preamble as House 1, 15 HOW rows × 20
  component columns, Phase-1 Σ as row importance, derived Σ/Rank as
  basement, and a roof carrying only the component correlations already
  documented in this file: the C10↔C12 `−−` cell is the FAT-vs-loose-
  objects residual made visible).
- **Houses 3–4 drawn under the pipeline reading (2026-07-16, same day).**
  First recorded as "deliberately undrawn, no manufacturing process";
  superseded within hours: the project *does* have a production system,
  the toolchain + release pipeline (P1–P9) guarded by its verification
  practices (Q1–Q8), and reading "process" that way makes both houses
  informative rather than scaffolding. The cascade now runs all four
  houses with Σ carried down each basement. Two findings on first
  derivation: **P4 bench assembly is the #2 process (22 %) with only
  manual controls**: the CS-jumper and SDXC lessons were both paid
  there; and **Q6 (checksum chain) ranks #8 by breadth while being the
  sole control on the public install path**: the same
  narrow-voter-vs-absolute-stakes bias H8 exposed in House 1. Cells are
  a single-rater first cut from the documented pipeline, flagged as such
  in §5.
- **All four houses stacked at the top; legend weight fixed
  (2026-07-16).** The doc is read from Remanso, where the diagrams are
  the summary: Houses 2–4 moved from §5 up beside House 1, each with a
  headline caption; §5 keeps the matrices, catalogues, and narrative as
  the source of truth, with pointers up. Same pass: the House-1 legend's
  "Typoena (shipped, measured)" label was set `font=\bfseries`, which
  *replaces* the picture's `\scriptsize`: it rendered bold at default
  size and overflowed the legend box. Now `\scriptsize\bfseries` (bold
  only, same size), fixed in every preamble copy here and in
  `quality-house-empty.md`.
- **The flow challenge: [`house-vs-product.md`](house-vs-product.md)
  opened (2026-07-17).** The author rejected the houses' reading of the
  product ("your keystroke appears instantly and your words are never
  lost") in favour of **flow** (the first 2S of 5S applied at every
  layer) and the July effort record backs the claim as revealed
  preference: the rank-vs-effort divergence (§5) reads as stale weights,
  not drift, and the shipped editing grammar (palette, vim modes, search)
  turns out to have **no WHAT row voting for it at all**. Not fixable by
  a same-day re-weight without baking the assertion in, so this became
  the first entry (D1) of a new standing-challenges page where the model
  is argued with instead of silently re-scored. Nothing in the matrices
  changed; §1 gained the "WHAT that has no row" note and the §5 flag now
  carries D1's counter-reading.
- **W16 + H17 scored: D1 resolved by re-derivation, same day
  (2026-07-17).** The user took the challenge's strongest fix: a reach
  *outcome* WHAT (**W16** "any file, any action, any edit point is one
  motion away", weight 10) with a measurable companion characteristic
  (**H17** reach cost in keystrokes, ≤ 6 median, unmeasured), plus a
  **Navigate** function row: not a holistic "flow" row, which would have
  touched everything weakly. Cells kept sparse (W16 → H1/H16/H17; H17
  voters W16 + W2; H17 → C7/C8/C9/C10) and the full cascade re-derived:
  House 1 total 1627 to 1804, **H1 climbs #5 to #2** (178, past H2's 177),
  H17 enters at #9 above H5, H16 63 to 93; House 2 headline **C7 #5 to #2**
  (5 667) past libgit2: the derived ranking now agrees with the July
  effort record, dissolving the §5 rank-vs-effort flag; Houses 3–4 ranks
  unchanged (P1 52.4 %, P4 21.4 %), a robustness check passed. Perception
  gained the W16 row (Typoena's five is self-scored on home turf and
  flagged as such). Re-verifying every number caught two pre-existing
  slips, both fixed: **House 4's row-importance column carried the Q
  basement values instead of the P process weights** (eight entries for
  nine rows: a paste of its own basement), and **§3's priority list had
  H8 (156) at #3 above H12 (160)**, an ordering the arithmetic never
  supported.
- **House 2's roof was scored from the wrong graph: three pool-mediated
  `−−` added, shared-pool budget matrix opened (2026-07-17).** The roof
  carried one conflict (C10↔C12) while the July crash record held three
  more, each already paid for on the bench: **C7↔C12** (push exhausted
  PSRAM, `Frame::new_white` died, UI thread OOM-aborted, run 4),
  **C7↔C13** (palette's file list held internal DRAM, `ssl_setup`'s
  ~33 KB failed, TLS refused to start), **C6↔C12** (checkout exhausted
  internal, `spi_master` NULL-dereffed a failed DMA alloc). All three
  were invisible because the roof was read off the call graph while the
  conflicts run through shared memory pools: N-way contention a
  pairwise roof fragments, the House-2 sibling of D1's fragmented flow.
  Making a pool a component *column* was considered and rejected
  (columns rank effort targets; a pool would vote itself to #1 and
  distort the cascade), so §5 gained the transpose instead: a
  **consumers × pools budget matrix** (cells = worst-observed draw,
  bottom row = per-pool min-ever free, internal DRAM's is 2 099 B),
  now the source of truth for the pool-mediated roof cells. No Σ
  changes: the roof and the new table sit outside the importance
  arithmetic. Same pass caught §4's roof intro still saying "14×14"
  (stale across two HOW-catalogue changes; the roof has been 15- then
  16-wide): corrected to 16×16.

- **The file was 5S'd into per-house pages (2026-07-17).** At ~3,100
  lines (four TikZ houses each carrying a copy of the ~250-line
  preamble, plus this ledger), `qfd.md` had outgrown single-file
  navigation. Split by house so each diagram stays on the same page as
  its source tables — the 2026-07-11 merge lesson, kept:
  [`qfd-house-1.md`](qfd-house-1.md) (§1–§4),
  [`qfd-perception.md`](qfd-perception.md),
  [`qfd-house-2.md`](qfd-house-2.md) (§5),
  [`qfd-houses-3-4.md`](qfd-houses-3-4.md),
  [`qfd-budget.md`](qfd-budget.md) (§6),
  [`qfd-tradeoffs.md`](qfd-tradeoffs.md) (§7), and this ledger (§8).
  `qfd.md` itself is now the hub: the what-matters-now headlines, the
  page index, and the keep-honest rules. Section numbers and heading
  anchors were kept, so cross-doc references only changed filenames.
  Deleted rather than moved: the intro's own layout history (merged
  quality-house 2026-07-11, hoisted diagrams 2026-07-16 — both
  superseded by this layout) and the "weighted totals left as
  exercise" aside.


The earlier variance between README's "~12 lines" and product/[ADR-003]'s
"~11 lines" of "edit area" is now superseded: the side-panel redesign removed
the top header and bottom status bars (metadata moved into the **side panel**),
so the **writing column** spans the full panel height: ~13 lines at the
editor's 20 px font (`FONT_10X20`, `editor.rs` `ROWS = HEIGHT / 20 = 13`).
README, the product/technical docs, and [ADR-003] are all updated to ~13 lines
(writing column).

- **H1 typing-latency target was a guess; bench corrected it (2026-07-21).**
  §6's H1 row marked the typing tier met (✓) on a ~100–130 ms *projection* that
  assumed refresh time scales with refresh area. The custom `0x32` fast-partial
  LUT work (now on `main`) measured it: refresh time is area-**independent** —
  waveform BUSY dominates, windowing the Y-band saves only ~35 ms — so the default
  windowed factory partial is ~495 ms, never the projected 100–130 ms, and never
  met ≤ 400 ms. The custom LUT (FR `0x08`, behind the default-off `fast_partial`
  flag) does reach ~265 ms, pending a longevity + cold soak. Fixed §6 row 4
  (Watched/Verdict/fallback), the hub's H1 open-gap bullet, the House-1
  §Type-latency narrative, and perception's H1 table row + intro/W1 rationale
  (the sweep first missed the last two, leaving the stale ~100–130 ms as a live
  claim); data in
  [`tradeoff-curves/epd-refresh-latency.md`](tradeoff-curves/epd-refresh-latency.md).
  The dead "windowed erase" fallback was retired with it. **Open follow-on:** this
  undercuts the premise that H2 (refresh area) is a latency driver — now opened as
  [house-vs-product](house-vs-product.md#d2--refresh-area-is-not-a-latency-lever) **D2**.

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
