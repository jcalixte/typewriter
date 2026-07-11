# Rendering & UX spikes

> Scope: bench experiments that retire **display/UX** risk, run after the v0.1
> hardware bring-up spikes (1–7) in
> [`v0.1-mvp-technical.md`](v0.1-mvp-technical.md#hardware-bring-up-order).
> Unlike 1–7, these are **not** a v0.1 integration gate, and each feeds a
> different release (noted per spike).
>
> They ride on the `epd::Frame` `DrawTarget` and the Spike 5 partial-refresh
> path. Project overview: [`../README.md`](../README.md). Vocabulary:
> [`../CONTEXT.md`](../CONTEXT.md). Release sequence: [`roadmap.md`](roadmap.md).

These prove display/UX risks, not stack risks, so they sit outside the 1–7
"prove before integration" gate. **Run Spike 8 first:** it partitions the panel
into the writing column and the side panel that the later spikes (scrollbar,
line-number gutter, status readouts) draw within and around. Each feeds a
different release, but the bench experiment is worth running now to retire the
risk early.

| Spike                                     | Feeds                                                |
| ----------------------------------------- | ---------------------------------------------------- |
| 8 — Layout: side panel + writing column   | v0.1                                                 |
| 9 — Boot splash bitmap                    | v0.1                                                 |
| 10 — Dark / light theme                   | v1.0                                                 |
| 11 — Transient panel (help / config)      | v0.4 `:` · v0.5 palette · v0.9 settings (mechanism)  |
| 12 — Scroll position indicator            | reading / `View`-mode UX                             |
| 13 — Line-number gutter                   | v0.2                                                 |
| 14 — Multi-file navigation                | v0.5                                                 |

8. **Spike 8 — Layout: side panel + writing column.** Split the 792×272 landscape
   into a full-height **writing column** (~60 cols, left) and an always-visible
   **side panel** (~150 px / ~20 cols, right), per the
   [Screen layout](v0.1-mvp-product.md#screen-layout). Today the editor fills the
   full 79-col width with text and a cramped 12 px bottom status
   ([`firmware/src/editor.rs`](../firmware/src/editor.rs), `COLS`/`ROWS` +
   `draw_status`); this spike narrows the text to a legible measure and moves all
   metadata into the panel, retiring the header/status bands entirely. Two driver
   facts shape it. First, narrowing does *not* speed up typing: `update_part`
   already drives both controllers full width, windowed only in Y, because "the
   waveform time dominates, not the data clock-out"
   ([`firmware/src/epd.rs`](../firmware/src/epd.rs)) — the win is line length and
   persistent info, not latency. Second, since every partial refresh spans the full
   width, a keystroke's windowed-Y band repaints the panel's pixels on that row
   *for free* (redrawn identically), but a panel field that changes on a *different*
   row than the cursor costs a **second** windowed-Y band — burning the "20 partials
   → forced full refresh" ghosting counter twice as fast (render module). That sorts
   what the panel may hold: static (filename, dirty), event-driven (mode, Wi-Fi,
   keyboard-disconnect, publish state), and throttled (clock, word count, session)
   fields only — nothing that repaints per keystroke, so no live cursor *column*.
   The panel sits entirely in the master half (right of the `x = 396` seam), so its
   glyphs never split the seam; the writing column still straddles it, as today.
   Prove: render column + panel; type and confirm the windowed-Y refresh stays a
   single band (panel redraws in place, no second region); then update a throttled
   field (word count on a typing pause) and a discrete one (mode flip) and confirm
   each is one isolated band that forces no extra full refresh. Decides the
   column/panel split in pixels, the panel's field layout and update cadence, and
   whether the ghosting counter needs per-region accounting. (Feeds v0.1's screen
   layout — the writing column + side panel the product doc now draws.)

9. **Spike 9 — Boot splash bitmap.** Embed the Typoena logo as a 1-bit asset and
   blit it, with the build tag, at boot before the editor opens. Spike 2 already
   proved vector + font rendering, so the only new risk is the image-asset
   pipeline (embedding + blit) and that a large near-full-frame image takes a
   clean full refresh. Mostly a feature — kept as a spike only to prove the
   asset path. (Feeds v0.1's "e-ink shows Typoena splash + boot log".)

   **Built 2026-07-11 as a *vector* splash**, not a bitmap. The frame is
   [`display::Frame::splash`](../display/src/lib.rs) — the `typoena` wordmark
   centred inside a stroked `Circle`, drawn with `embedded-graphics`, one clean
   full refresh. It is shared by two callers: the
   [`splash`](../firmware/src/bin/splash.rs) bench binary (`just flash-splash`)
   and **`main.rs`'s boot path**, which shows it right after EPD init — replacing
   the old white-clear baseline — while the SD mounts and the note loads, then a
   second full refresh brings up the editor. Nothing is embedded, so the
   image-asset pipeline named above is deliberately **left unproven** — an
   acceptable trade because Spike 2 already covered vector + font rendering.
   **Proposition, deferred to end-of-project polish (v1.0):** replace the vector
   mark with an embedded 1-bit raster logo, which is when the asset-embed/blit
   path would finally be exercised. **Confirmed on the panel 2026-07-11** — the
   splash renders cleanly at boot, then the editor comes up. This closed the last
   v0.1 display gate; v0.1 shipped the same day.

10. **Spike 10 — Dark / light theme.** Invert the 1-bit framebuffer (white on
    black) and refresh. The invert is trivial (XOR at blit); the real unknowns
    are (a) ghosting and refresh time on a predominantly-black panel — full-black
    waveforms stress the panel differently — and (b) whether partial-refresh
    ghosting accumulates faster in dark mode, forcing a more frequent full
    refresh. Prove: legible inverted text, acceptable ghosting, and
    partial-refresh latency unchanged from Spike 5. (Feeds v1.0's light/dark
    theme.)

11. **Spike 11 — Transient panel (help / config).** Prove a modal full-screen
    view that swaps in over the editor and restores the buffer exactly on exit —
    the shared primitive behind a help screen, the v0.9 settings screen, the v0.5
    file palette (`Ctrl-P`), and the v0.4 `:` command line. The risk is
    e-ink-specific: entering/leaving with partial refresh leaves ghosting, and
    the editor viewport underneath must return artifact-free. Prove with a static
    help screen (trivial content): a full refresh on enter and on exit restores
    the prior text exactly; measure in/out latency. Decides overlay-box vs.
    full-screen swap — recommend the swap, since a dimmed partial overlay is both
    slow and ghost-prone at ~630 ms. Panel *content* (which config keys, help
    text) is feature work deferred to the owning release; this spike proves only
    the mechanism.

12. **Spike 12 — Scroll position indicator.** The buffer already scrolls (`View`
    mode `j`/`k`/`space`); this spike proves how to *show* position without
    wrecking latency. Measure two affordances on the bench: (a) a right-edge
    scrollbar repainted each scroll step, and (b) a compact side-panel readout
    (`34/128` or `27%`). Risk: the scrollbar is a tall pixel column repainted on
    every scroll — measure its partial-refresh cost against the small
    side-panel field, on the windowed-Y path, during held-key scroll. Prove:
    pick the affordance whose refresh cost is acceptable and confirm it forces no
    extra full refreshes. (Feeds the reading/`View`-mode UX; the a-vs-b choice is
    a design decision this spike hands data to.)

13. **Spike 13 — Line-number gutter.** Draw a fixed-width digit column left of
    the text area. The v0.2 spec is the hard case: **relative** numbers in
    Normal mode (current line as its absolute number), **absolute** in Insert.
    The naive part — reserving columns and drawing digits — is trivial; the
    e-ink risk is churn. Relative numbering renumbers the *entire visible gutter
    on every `j`/`k`*, so a single cursor move becomes a partial refresh of a
    tall digit column, straight into the Spike 5 windowed-Y path and the "20
    partials → forced full refresh" ghosting counter
    ([`firmware/src/epd.rs`](../firmware/src/epd.rs), render module). Absolute
    numbering is cheap by comparison — only the rows below an inserted/deleted
    line renumber. Two more interactions: the gutter steals horizontal columns
    from the render-time soft-wrap, and wrapped continuation rows have no number
    (blank vs. tilde — a layout decision). Prove: measure gutter partial-refresh
    cost for (a) a held-`j` scan that renumbers the whole relative gutter vs.
    (b) a single-line absolute edit, and confirm neither blows the ghosting
    budget or forces extra full refreshes. Decides whether relative numbering is
    viable on this panel or must be gated (absolute-only, or a batched/coalesced
    gutter repaint). (Feeds v0.2's line-number gutter; genuine new e-ink risk.)

14. **Spike 14 — Multi-file navigation (open / switch / new / delete).** The
    panel *mechanism* is already Spike 11 (which names this v0.5 file palette),
    and filename entry reuses the v0.4 `:` command line — so this spike proves
    neither, and depends on both. What is genuinely new is **persistence and
    buffer lifecycle**, not rendering: enumerate a FAT directory across *both*
    `/sd/repo` and `/sd/local`, keep ≤ 3 ropes resident, and on switch swap the
    active rope + cursor + soft-wrap after an atomic save-of-current
    (persistence module). Two sharp edges. **New file** (`:enew`) must prompt
    for scope — tracked (`/sd/repo`, pushed) vs. local (`/sd/local`, `Ctrl-G`
    disabled). **Delete** extends v0.5 scope (the roadmap lists `:enew` but not
    delete): removing the currently-open buffer must close it and fall back to
    another resident buffer or an empty one, and — the load-bearing check — the
    FAT unlink must reach the next Publish's *staged* set, which the git
    module's `git add .`-equivalent staging may not catch for removals (needs
    `git rm` / `add -A` semantics —
    [`git` module](v0.1-mvp-technical.md#git--commit--push)). Prove:
    open/switch/new/delete across both roots with a correct ≤ 3-buffer
    lifecycle, a delete that propagates into the following commit, and an
    artifact-free full-refresh swap on every buffer change (leaning on Spike
    11). Latency is not the risk here; buffer-state and git-staging correctness
    are. (Feeds v0.5 multi-file, delete included — now authorized in
    [roadmap](roadmap.md#v05--file-palette--multi-file--) v0.5.)
