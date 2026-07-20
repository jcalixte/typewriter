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
