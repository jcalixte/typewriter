# The house vs. the product

Standing challenges between the scored QFD houses ([`qfd.md`](qfd.md)) and
the product actually being built: places where the model and reality tell
different stories. [`qfd-changelog.md`](qfd-changelog.md#8-inconsistencies-spotted-and-fixed)
is the ledger of inconsistencies *fixed*; this page holds the disputes still
*open*, claims about what the product is that the houses cannot yet express,
argued with evidence on both sides rather than settled by fiat.

**Rule of engagement.** When the house and the builder disagree, neither is
silently corrected. The challenge lands here with the claim, the house's
counter-reading, the evidence, a reconciliation if one exists, and a
**trigger** naming what would resolve it. On resolution, the outcome
cascades into the `qfd-*.md` houses (weights, cells, or commentary,
recomputed per [`qfd.md`](qfd.md)'s honesty rules) and the entry here is stamped resolved with a pointer to
the §8 ledger line. An entry that never resolves is fine; an entry that
resolves silently is not.

---

## D1 — Flow is the product's center, and the house can't see it

**Opened 2026-07-17 · RESOLVED 2026-07-17.** The author took the
second candidate fix (a reach outcome WHAT); see **Outcome** below.

**The claim** (the author): the product's most important quality is not
the two weight-10 WHATs (W1 sub-second typing response, W3 power-loss
safety) but **the way the device puts you in flow**: writing, vim modes,
the palette, an installer that asks almost nothing of the user. Everything
is one command away. In 5S terms the product is the first 2S applied at
every layer: **seiri** (remove what doesn't belong: no notifications, no
apps, no browser; W7 made physical) and **seiton** (a place for everything,
everything within one motion: every file one Cmd-P away, every action one
`:command` away, every edit one home-row motion away, the whole product one
`curl | sh` away).

**What the house says instead:** House 1 ranks heap-during-Push,
partial-refresh area, reconnect time, and save durability as the top
characteristics; House 2 sends the next unit of effort to C5/C12; the
onboarding-and-editing surface (C17–C20) ranks #15–18.

**Evidence for the claim: revealed preference.** The July effort record
is the palette, visual mode, `.` repeat, smartcase + accent-folded search,
Cmd+S, scroll margin, the one-command installer, the zero-computer wizard.
The house flagged this as rank-vs-effort divergence and explained it as a
one-off reach purchase (W15). The claim reads the same record the other
way: the builder's hands kept returning to the seiton layer because that
*is* the product, and the weights are what lagged. When effort persistently
disagrees with a priority matrix, either discipline is failing or the
WHATs are stale.

**Sharper still: most of the seiton layer has no WHAT at all.** Vim modes,
the palette, search, `.` repeat (the shipped editing grammar) map to no
W-row. They shelter under W7 ("nothing on the device competes with prose",
weight 8) at best, a row that voices *absence of distraction*, not
*presence of reach*. The house never voted for the features that most
distinguish the product, and they got built anyway.

**The reconciliation (both readings survive).** W1 and W3 don't become
unimportant under the claim; they change role, from identity to
**preconditions**. A 630 ms repaint on every deletion breaks flow
mechanically; one lost paragraph breaks it psychologically and permanently
(you start hedging, copying text out, distrusting `:w`). The floor is
real. But the floor is also table stakes: a Freewrite has instant keys and
durable saves too. What it lacks is seiton: no modal editing, no palette,
no one-motion reach to anything. The two 10s are what you stand on; the
ordering is what you bought the device for.

**Why the house is structurally blind here.** Flow is a holistic WHAT:
it exists in the *composition* of W7 + W8 + W2 + W13 + W15 plus the un-rowed
editing grammar, not in any one row. Column sums fragment it, so the house
underprices it by construction. This is the inverse of the narrow-voter
bias that once hid H8 (one absolute voter read as unimportant): here a
broad, emergent quality has many weak voices and no loud one. Adding a
"flow" row would not fix it: a row that touches every HOW weakly adds
noise, not signal, and re-introduces the solution-in-the-requirements
smell that got the old W13/W14 rewritten.

**The 5S reading** (the product frame behind the claim, kept for the
roadmap):

| S | Meaning | Where Typoena stands |
| --- | --- | --- |
| Seiri (sort) | Remove what doesn't belong | Done in hardware: no notifications, no apps, nothing competes with prose |
| Seiton (set in order) | Everything within one motion | The shipped software layer: palette, vim grammar, `:commands`, one-line install |
| Seiso (shine) | Clean as you go | Emerging: full-refresh cadence wipes ghosting, auto-repack folds packs at load |
| Seiketsu (standardize) | The order is the same everywhere | Partial: `.typoena.toml` carries prefs; a re-flashed or second device should feel identical |
| Shitsuke (sustain) | The discipline keeps itself | Unbuilt: whatever makes the device sustain a writing *practice*, not just a session |

**What acting on the claim would change.** Candidate moves not taken on
the day of the claim (re-deriving four houses on an assertion minutes old
would bake it in rather than test it):

- **Re-weight the flow cluster** (W7 and W13 up, possibly W2), then
  recompute the full cascade. Honest but heavy; wants a second look at
  *all* weights, not a spot-raise.
- **Give the editing grammar a WHAT**: an outcome row for reach
  ("any file, any edit, any action is one motion away"), which is a
  requirement, not a solution, and would finally give C-rows like the
  palette a voter. The most likely concrete fix. **Taken, same day.**
- **Accept flow as an umbrella**: name it above the table the way W13's
  typography note works, and keep the arithmetic as the floor-model it is.
  Cheapest; risks being a caption that changes nothing.

**Trigger to resolve:** the next House-1 re-score taken for its own
reasons (a WHAT or HOW changes) must decide this rather than carry it;
the weights question rides along for free. Early trigger: the
rank-vs-effort flag fires a **second** time with the effort again in the
seiton layer. Once is a reach purchase; twice is the weights being wrong.
*(Both triggers discharged by the resolution below; the flag itself is
retired in [`qfd-house-2.md`](qfd-house-2.md) §5.)*

**Outcome (2026-07-17).** The author chose the reach-WHAT fix and the
cascade was re-derived the same day
([`qfd-changelog.md`](qfd-changelog.md#8-inconsistencies-spotted-and-fixed) has the full
ledger entry): **W16** "any file, any action, any edit point is one
motion away" (weight 10, joining W1/W3, identity alongside its
preconditions), **H17** reach cost (≤ 6 keystrokes median, unmeasured;
now §6 budget row 9), and a **Navigate** function row. The holistic
"flow" row stayed rejected, as argued above. The re-derivation confirmed
the claim quantitatively: **H1 type latency rose #5 to #2** and **C7, the
widget/editor layer where the palette and modal grammar live, rose
#5 to #2 past libgit2**. The derived ranking now points where the July
effort went, which is what this entry predicted a correct re-score would
show. Residual worth keeping: Typoena's 5 on the new W16 perception row
is self-scored on the product's home turf (flagged in §3); and the
re-verification pass caught two unrelated pre-existing slips (House 4's
importance column, §3's H8/H12 ordering), both fixed and ledgered.

---

## D2 — Refresh area is not a latency lever

**Opened 2026-07-21 · OPEN.**

**The claim** (the bench): H2 "refresh area per keystroke" is priced as a top
*latency* characteristic — §6 ranks it #1, and its basement Σ (177) sits second
only to H1 — on the premise that a smaller refreshed region is faster. The
2026-07-21 custom-LUT bench shows refresh time is **area-independent**: a
full-area partial (~272 rows) and a one-line windowed band (~20 rows) land within
~35 ms of each other, because the SSD1683 runs the *whole* waveform regardless of
how many rows carry new pixels. Windowing the Y-band saves only the SPI-transfer
term (~0.34 ms/row); the latency lever is the waveform LUT (its FR frame-rate
byte, 420 → 265 ms), not the area. So H2's rank rests on a premise that is false
on this panel.

**What the house says instead:** H2 is the #1 §6 budget row and a top-Σ
characteristic, scored strong against the sub-second-typing WHATs as if shrinking
the refreshed region is what buys responsiveness.

**Evidence:** [`tradeoff-curves/epd-refresh-latency.md`](tradeoff-curves/epd-refresh-latency.md)
— windowed one-line band vs full-area within ~35 ms across the whole sweep; the FR
byte alone moves latency 420 → 265 ms; phase-count trims and the charge-pump
keep-hot lever were both near-noops. This only confirms the area-independent cost
model already in that doc's header (the 2026-07-16 gate-scan spike had refuted
MUX-proportional timing; the custom-LUT work refutes area-proportional timing the
same way).

**The reconciliation (H2 survives; its *reason* changes).** Windowing is still
worth doing — but for **visual disturbance**, not speed: a one-line band flickers
only the touched line instead of strobing the whole page, a real flow/ghosting
quality, and it bounds the SPI term and eases the H3 full-refresh cadence. H2 is a
legitimate, *met* target; it is simply mis-filed as a latency driver. The honest
fix is to re-read H2 as a disturbance-containment characteristic and let the
latency WHATs lean on the waveform (H1) instead — which drops H2's
latency-weighted rank while keeping the row.

**Trigger to resolve:** the next House-1 re-score taken for its own reasons must
decide whether H2's cells point at latency or at disturbance, and recompute the
basement Σ / §6 rank accordingly. Early trigger: `fast_partial` graduating to
default — at that point typing latency is provably owned by the custom waveform,
and any residual H2 latency weighting is dead. Recorded from the
[§8 ledger](qfd-changelog.md#8-inconsistencies-spotted-and-fixed) entry that
flagged it.

---

## D3 — Focus mode acts on its own, and W8 says the UI must not

**Opened 2026-07-22 · OPEN.**

**The claim** (this audit): focus mode — the silent Pomodoro behind `:focus`
(v0.7.5) — is a shipped feature that **no WHAT names**, and its behaviour runs
against one that exists. `Panel::rest_if_due` ([`app/src/render.rs`](../app/src/render.rs),
called from the idle/pause branch of the run loop, `runtime.rs:244`) drops a
full-screen Rest card **on a timer**: at the first typing pause after a block
completes, with no keystroke from the writer. W8 is "the UI never moves except
when I move it" (weight 7, scores **4** on the perception table). A card that
appears on a clock is the UI moving when the writer didn't move it. W7 ("nothing
on the device competes with prose", weight 8) reads a non-prose full-screen card
as a competing surface.

**What the house says instead:** the house has no row for timeboxing, rest, or
writing *practice*. The nearest WHATs — W7 (absence of distraction) and W8 (UI
stillness) — both point *against* an autonomous card, not for it. Focus mode was
never scored, for or against; it shipped anyway.

**Evidence:** `rest_if_due` is gated only on the focus block length plus a typing
pause, never on a key; the card is `Mode::Rest`, a full-screen surface like
`:about`. It is opt-in via `:focus`, so the writer consents to the *mode* — but
not to each individual card, and the card is the W8-relevant event.

**The reconciliation (focus mode is Shitsuke; W8 needs a carve-out or the card
must soften).** [D1](#d1--flow-is-the-products-center-and-the-house-cant-see-it)
already found this category and marked it unbuilt: the 5S table's **Shitsuke —
"whatever makes the device sustain a writing *practice*, not just a session"** was
the single row with no product behind it. Focus mode is the first concrete
Shitsuke. So the honest reading is not "focus mode violates W8" but "the house is
missing the WHAT focus mode serves, and W8 was written before any
sustain-the-practice feature existed." Two exits: **(a)** add a practice/Shitsuke
WHAT ("the device sustains a session's rhythm, not just its keystrokes"), against
which the autonomous card is a *feature*, and give W8 an explicit carve-out for
user-armed timers; or **(b)** if no such WHAT is wanted, change the card — arm it
but let it wait for the *next* pause the writer takes on their own, so it never
interrupts and W8 stays literal.

**Trigger to resolve:** the next House-1 re-score (same trigger as
[D2](#d2--refresh-area-is-not-a-latency-lever); take them together) must decide
whether a practice WHAT joins the house and whether W8's "except when I move it"
carves out user-armed autonomy. Early trigger: any **second** autonomous surface
(a break reminder, a daily-goal nudge) — one is a mode, two is a pattern the house
must price before it accretes. Recorded from this audit; connects to D1's unbuilt
Shitsuke row.

---

## D4 — The font picker may over-serve W13

**Opened 2026-07-22 · OPEN.**

**The claim** (this audit): the baked-font feature (`font` pref + a Font row in
the `>` palette cycling six options — the built-in default plus five baked
families; landed 2026-07-21) is the **first concrete delivery of W13 "Typography
sets a writing-tool tone"**, a WHAT that until now had no HOW behind it (every
H-column is a performance characteristic; none rendered type). So it is
need-driven, not feature-factory output. But W13 asks for a *tone*, and tone is a
quality a designer *sets*, not a knob the user cycles. The product shipped a
six-way live picker where W13 arguably wanted one well-chosen default.

**What the house says instead:** W13 is a *tone* WHAT — a felt quality — not a
*customization* WHAT. Nothing in the house asks for user-selectable typography;
the picker answers a question the house never posed.

**Evidence:** grid-invariant (10×20), so the engineering cost is near-nil — ~5 KB
atlas × 5 families ≈ 25 KB flash (H10, against a 2 MB budget), no H1/H2/H3,
user-initiated so no W8. The only real cost is the extra settings surface itself
(a nick against W7). `display/src/fonts/mod.rs` `FONT_OPTIONS`, cycled from the
`>` palette.

**The reconciliation (support yes, picker maybe).** Font *support* clearly earns
its place under W13. The open question is scope: does W13 want the user cycling
six families, or the designer choosing the one that sets the tone (JBM Medium
today) and shipping that? The former is a preference product; the latter is a
curation product — and Typoena's identity (D1's seiri, W7) leans curation. Likely
resolution: keep the baked-font machinery, but reconsider whether the *cycling
picker* stays a shipped surface or collapses to a single opinionated default, the
alternates reachable only via a hand-edited `.typoena.toml`.

**Trigger to resolve:** the same House-1 re-score that weighs
[D1](#d1--flow-is-the-products-center-and-the-house-cant-see-it)'s flow cluster
(W7/W13) — decide there whether W13 stays a pure tone WHAT (→ curated default) or
gains a customization reading (→ picker). No standalone trigger; it rides the W13
weight question D1 already parked.

---

## How to keep this page honest

- One entry per challenge, D-numbered, dated, stamped OPEN or RESOLVED,
  never deleted. A resolved entry keeps its argument and gains the outcome
  plus a pointer to the [`qfd-changelog.md`](qfd-changelog.md) ledger line that recorded the cascade.
- Every OPEN entry carries a **trigger**. An entry without one is an
  opinion parked where a decision should be.
- Claims here are single-rater until noted otherwise: same U1 chair as
  the weights they challenge. A real second user (U2 observed, not bet)
  re-opens every entry that leans on revealed preference.
- When `qfd.md` weights, rows, or cells change *because of* an entry here,
  the change is argued here first: this page is where the model is
  challenged, so the model must not quietly update to dodge the challenge.
