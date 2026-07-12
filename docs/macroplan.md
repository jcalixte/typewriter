# Macroplan — version details

Frequent releases. Each version is a usable artifact, not a checkpoint.
This file holds the `macroplan` source block (below), a cross-version status
roll-up, and a one-line summary of each release linking to its dedicated page.
The user-facing requirements and engineering targets each release feeds into are
tracked in [`qfd.md`](qfd.md).

## Macro-plan

Macroplan source — paste into the macroplan app to render the week-by-week
view. `original` dates are the June 2026 baseline and never move; slips get
appended as `reestimates`, per-item actuals live in the Status block below.

```macroplan
title = "Typoena — macro plan"

[[feature]]
name = "v0.1 it writes, it pushes"
start = 2026-06-01
original = 2026-06-29
delivered = 2026-07-11
learning = "Shipped 12 days late. The long pole was hardware bring-up risk, not the editor: SD on a shared SPI bus (resolved by moving it to its own SPI3, ADR-012) and on-device git (gix killed, pivoted to libgit2 as an esp-idf CMake component, ADR-004). Splash landed as a vector wordmark, not the planned 1-bit bitmap — the asset-embed/blit path is deferred to v1.0."

[[feature]]
name = "v0.2 navigation"
start = 2026-06-29
original = 2026-07-20
delivered = 2026-07-11
learning = "Delivered 9 days early. Motions/modes, Ctrl-d/u, the UTF-8 buffer, and the absolute line-number gutter all landed 2026-07-11; the last gate, Spike 13's on-panel gutter refresh check, confirmed a single-line edit repaints only rows at/below it with no extra full refresh. Relative line numbering was dropped as an e-ink ghosting cost with no proportionate gain."

[[feature]]
name = "v0.2.5 international input"
start = 2026-07-20
original = 2026-08-03
delivered = 2026-07-11
learning = "Delivered 23 days early — ahead of its own start window. Dead-key accent composer in the keymap crate (US-International, à é ê ë ñ ç), editor buffer made UTF-8-correct, typed on the bench with no panic. The side-panel pending-accent marker was dropped by decision: at typing speed it is stale before the ~630 ms panel repaint, so it conveyed nothing. Bonus: physical Esc (HID 0x29) remapped to backtick/tilde so code fences + grave/tilde accents work on a 60% board without a Fn layer."

[[feature]]
name = "v0.3 editing"
start = 2026-08-03
original = 2026-08-24
delivered = 2026-07-11
learning = "Core complete 44 days early, host-tested and partially smoke-tested on the panel. Register + yank/paste (yy/p/P), snapshot undo/redo (u/Ctrl-r, bounded 100 groups in PSRAM), and keystroke-recorded `.` repeat all landed 2026-07-11; the d/c operator grammar + text objects were already done ahead of schedule. Firmware bumped to 0.3.0. On device dd/yy/Ctrl-r confirmed; the one bug found was a multi-line paste leaving its later lines below the fold (adjust_scroll only tracked the caret) — fixed with a reveal() that scrolls the block end into view."

[[feature]]
name = "v0.4 visual + ex"
start = 2026-08-24
original = 2026-09-07
delivered = 2026-07-11
learning = "Core complete 58 days early, host-tested. Visual (v) and VisualLine (V) selection with y/d/c landed 2026-07-11 (charwise vim-inclusive of the char under the caret; linewise spans whole lines and pastes like yy/dd), plus the recorded v/V→Visual reassignment: the read-only View mode moved to `gr` (go-read). Selection is drawn as reverse-video cells on the 1-bit panel with the caret punched back to normal video so the active end stands out; 18 new editor tests (83 total). The `:` command mechanism and :fmt were already done; `:e <path>` was deliberately deferred to v0.5 where its multi-file/buffer-lifecycle machinery (Spikes 11/14) lives, rather than half-building file-open here. Firmware bumped to 0.4.0. On-device smoke-test of Visual still pending (pure editor-core, low risk)."

[[feature]]
name = "v0.5 palette + multi-file"
start = 2026-09-07
original = 2026-09-28
delivered = 2026-07-12
learning = "Delivered 2026-07-12, well ahead of the 2026-09-28 baseline, and fully on-device confirmed. Four slices: the drained Effect queue + parked-buffer LRU foundation; the Cmd-P fuzzy file palette (Spike 11 — no ghosting on the transient panel); :enew + file delete (Spike 14 caught that add_all alone doesn't stage a deletion on this libgit2 — fixed with update_all, i.e. git add -A); and the git-tracked .typoena.toml prefs with a stay-open palette `>` command mode + :settings. Both directions of the prefs loop are proven on hardware — boot-read (byte-exact parse) and on-device palette edit (a device publish flipped line_numbers on origin). Three decide-before-build calls: the idle auto-save is unformatted, and both the per-device auto_sync override and the `> auto sync` command are deferred to v0.7 where auto_sync gains behaviour. Descoped from v0.5 (not the four slices): explicit buffer close, the grey-Publish-in-Local panel cue, and the multi-file publish count."

[[feature]]
name = "v0.6 markdown"
start = 2026-09-28
original = 2026-10-12
status = "on-track"
note = "Render affordances done early; snippet engine (added 2026-07-08) remains."

[[feature]]
name = "v0.7 search + git"
start = 2026-10-12
original = 2026-11-02

[[feature]]
name = "v0.8 battery + sleep"
start = 2026-11-02
original = 2026-11-30

[[feature]]
name = "v0.9 robustness"
start = 2026-11-30
original = 2026-12-28

[[feature]]
name = "v1.0 polish"
start = 2026-12-28
original = 2027-01-25

[[milestone]]
name = "MVP ships"
week = 2026-06-29
requires = ["v0.1 it writes, it pushes"]
```

## Status — synced 2026-07-12

The editor **core** has been built 2–3 versions ahead of the device
**releases**, and is now **extracted into a host-testable `editor` crate** (plus
a `display` crate for the panel framebuffer) so `cargo test` exercises it off the
xtensa target. **v0.1 shipped 2026-07-11** (late against the 2026-06-29
baseline): SD storage, save, and **git publish are all wired into the app binary
and hardware-verified** (`:sync` commits on the SD `/sd/repo` and pushes to a
test repo), and the **boot splash (Spike 9) is confirmed on the panel** — a
vector `typoena`-in-a-circle shown at startup while the SD mounts, then the
editor comes up. **Cold boot verified at 4258 ms** (power-on → cursor,
2026-07-11; 742 ms under the ≤ 5 s gate). It first measured ~5.5 s; the fix was
to bring the editor up with a full-area partial (~630 ms) instead of a second
full refresh (~1.9 s) — panel confirmed clean, no ghosting. The 1-hour soak is
attested from real use; the remaining post-ship acceptance checks are power-pull
recovery, 1000-word no-drop, and `Ctrl-G`'s not-yet-built pull-then-retry
(→ v0.9). **v0.2 navigation is COMPLETE 2026-07-11** — Spike 13's on-panel gutter
refresh check passed (single-line edit repaints only rows at/below it, no extra
full refresh), closing the last gate. **v0.2.5 international input** is
hardware-verified (2026-07-11), and **v0.3 editing is complete in core** the same
day (register + yank/paste, snapshot undo/redo, `.` repeat — host-tested, and
partially smoke-tested on the panel: `dd`/`yy`/`Ctrl-r` good, a multi-line-paste
scroll bug found + fixed). **v0.4 visual + ex is complete in core** the same day
too — charwise/linewise **Visual** selection (`v`/`V` with `y`/`d`/`c`), the
read-only View mode moved to `gr`, and the selection drawn as reverse-video on
the panel; `:e` was deferred to v0.5. Host-tested (83 editor tests); on-device
smoke-test pending. The firmware crate is bumped to **0.4.0**. Most of v0.6
Markdown also already runs. Version numbers track shippable device releases, not
raw core progress — the 0.4.0 bump reflects the v0.4 feature set being met.
**v0.5 palette + multi-file is DELIVERED 2026-07-12** (firmware **0.5.0**), fully
on-device confirmed: the Cmd-P fuzzy palette, `:e`/`:enew`/delete across the
`/sd/repo` + `/sd/local` scopes, and the git-tracked `.typoena.toml` prefs
(boot-read plus a stay-open palette `>` command mode + `:settings` that edits them
live and syncs the change). Descoped to later: explicit buffer close, the
grey-Publish-in-Local panel cue, and the multi-file publish count.

Marks: `[x]` done in core · `[~]` partially done · `[ ]` not started. An
inline `(✓)` marks the done half of a split item.

Each version below links to its dedicated page, which carries the full scope
checklist and status.

---

## v0.1 — MVP: "it writes, it pushes" — [x]

The minimum thing that justifies the hardware existing — boot, type one file,
`:w` to save, `:sync` to publish to GitHub. **SHIPPED 2026-07-11** (late vs the
2026-06-29 baseline); cold boot verified at 4258 ms.
**Design:** [product](v0.1-mvp-product.md) · [technical](v0.1-mvp-technical.md).

## v0.2 — Vim navigation — [x]

Modal Normal/Insert/View, `h j k l`/`w b e`/`0 $`/`gg G` motions, `Ctrl-d/u`
half-page scroll, the UTF-8-correct buffer, and the absolute line-number gutter.
**COMPLETE 2026-07-11.** Detail: [v0.2-navigation.md](v0.2-navigation.md).

## v0.2.5 — International input — [x]

US-International dead-key accent composition (à é ê ë ñ ç) in the `keymap`
crate, plus the Esc→backtick/tilde remap for a 60% board.
**Hardware-verified 2026-07-11.**
Detail: [v0.2.5-international-input.md](v0.2.5-international-input.md).

## v0.3 — Vim editing — [x]

Register + yank/paste (`yy`/`p`/`P`), snapshot undo/redo (`u`/`Ctrl-r`), `.`
repeat, and the `d`/`c` operator grammar + text objects.
**COMPLETE in core 2026-07-11**, partially smoke-tested on the panel.
Detail: [v0.3-editing.md](v0.3-editing.md).

## v0.4 — Visual mode + ex commands — [x]

Charwise `v` / linewise `V` selection with `y`/`d`/`c`, the `:` command line
(`:w`/`:fmt`/`:sync`/`:gl`), and View mode moved to `gr`.
**COMPLETE in core 2026-07-11**, on-device smoke-test pending.
Detail: [v0.4-visual-and-ex.md](v0.4-visual-and-ex.md).

## v0.5 — File palette + multi-file — [x]

The `Cmd-P` fuzzy file palette, `:e`/`:enew`/delete across `/sd/repo` +
`/sd/local`, the parked-buffer LRU, and the git-tracked `.typoena.toml` prefs
with a palette `>` command mode + `:settings`.
**DELIVERED 2026-07-12** (firmware 0.5.0), fully on-device confirmed.
Detail: [v0.5-palette-and-multi-file.md](v0.5-palette-and-multi-file.md).

## v0.6 — Markdown affordances — [~]

Heading bolding, list continuation, and soft-wrap are done; the trigger-driven
snippet engine (net-new scope, added 2026-07-08) remains.
Detail: [v0.6-markdown.md](v0.6-markdown.md).

## v0.7 — Search + better git — [~]

`/` forward search (`n`/`N`) and `:gl` pull (fetch + fast-forward only). The
`:gl` editor command landed 2026-07-11; the on-device fetch and search are still
to build. Detail: [v0.7-search-and-git.md](v0.7-search-and-git.md).

## v0.8 — Power: battery + sleep — [ ]

Bench current-draw measurement, 18650 + charge board, light/deep sleep, and a
battery indicator. **Not started.**
Detail: [v0.8-battery-and-sleep.md](v0.8-battery-and-sleep.md).

## v0.9 — Robustness — [ ]

Crash-safe writes, interrupted-push recovery, SD removal handling, Wi-Fi
reconnect, and on-device provisioning (the first release usable by a non-author).
**Not started.** Detail: [v0.9-robustness.md](v0.9-robustness.md).

## v1.0 — Polish — [ ]

≤ 3 s boot, runtime-switchable fonts and light/dark theme, enclosure files, and
a user guide. **Not started.** Detail: [v1.0-polish.md](v1.0-polish.md).

## v1.x — Stretch / nice-to-have

Post-1.0 ideas, not committed to any release (10.3" panel, multiple remotes,
writing stats, BLE-HID fallback). Detail: [v1.x-stretch.md](v1.x-stretch.md).
