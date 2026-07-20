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
learning = "Delivered 2026-07-12, well ahead of the 2026-09-28 baseline, and fully on-device confirmed. Four slices: the drained Effect queue + parked-buffer LRU foundation; the Cmd-P fuzzy file palette (Spike 11 — no ghosting on the transient panel); :enew + file delete (Spike 14 caught that add_all alone doesn't stage a deletion on this libgit2 — fixed with update_all, i.e. git add -A); and the git-tracked .typoena.toml prefs with a stay-open palette `>` command mode + :settings. Both directions of the prefs loop are proven on hardware — boot-read (byte-exact parse) and on-device palette edit (a device push flipped line_numbers on origin). Three decide-before-build calls: the idle auto-save is unformatted, and both the per-device auto_sync override and the `> auto sync` command are deferred to v0.7 where auto_sync gains behaviour. Amended 2026-07-12: a light/dark `theme` key and a set-ahead `> auto sync` preset command (2m/5m/10m/15m/30m) were added on top — the palette generalised so Enter rotates any pref to its next value (a bool is the two-option case); auto_sync is still read by nothing until v0.7. Descoped from v0.5 (not the four slices): explicit buffer close, the grey-Push-in-Local panel cue, and the multi-file push count."

[[feature]]
name = "v0.6 markdown"
start = 2026-09-28
original = 2026-10-12
delivered = 2026-07-12
learning = "Core complete 2026-07-12, ~92 days ahead of the 2026-10-12 baseline, host-tested (187 editor tests). The snippet feature was reshaped 2026-07-08→07-12 from a hard-coded table into a git-synced, Zed-compatible .typoena.snippets.json library: a forward-only tab-stop session ($1..$n/$0, ${n:label} stripped to $n) driven by two surfaces — inline Tab-expansion in Insert and a $ palette launcher — plus a quiet pause hint in the side panel. The Cmd-P palette generalised into a verb split: bare = files, > = a real command registry (toggles stay open, one-shots format/push close, the parameterised `new file` two-step), $ = snippets — retiring :e. Firmware bumped 0.5.0→0.6.0; the boot-read of the library was confirmed to build for xtensa (serde_json, the one new dep — cargo check passes). `just init` now seeds a curated 17-snippet catalog (three opt-in groups). On-device smoke-test still pending (pure editor-core + a mirror of the proven prefs boot-read, low risk). Known caveat: two symbols the catalog inserts (arrow →, neq ≠) are outside ISO-8859-15, so they store/sync correctly but need a display-layer glyph overlay (in flight) to draw on the panel; the other 15 render on the stock font."

[[feature]]
name = "v0.7 search + git"
start = 2026-10-12
original = 2026-11-02
delivered = 2026-07-14
learning = "Delivered 2026-07-14, ~16 weeks ahead of the 2026-11-02 baseline, and closed on-device across three bench runs in three days. `/` search shipped smartcase + accent-folded (a user decision that superseded the same-day plain-insensitive version; /ete finds été) with n/N, Enter-only jump, and an editor-global pattern. `:gl` pull landed fetch + fast-forward-only in all four shapes; the fast-forward is an O(changed) tree-diff apply (apply_tree_diff) built after run 2 crashed in libgit2's O(tree) checkout_tree — internal-DRAM exhaustion plus an esp-idf spi_master NULL-deref on its own failed-alloc path. Three memory/transport fixes rode along: file-list interning to one PSRAM blob (was 182 KB internal), a 64 KB DMA reserve, and TLS session resumption (third vendor delta), which cut the rejected-push reconcile cycle from 59 s to 24 s. Bonus: the first on-device rejected-push → reconcile → replay → push success, and the sd_bench dir-scaling run convicted FAT linear directory scans as the ~400 ms/loose-write residual (bounded, accepted). :sync was renamed :gp to pair with :gl."

[[feature]]
name = "v0.7.5 focus mode"
start = 2026-07-17
original = 2026-07-17
delivered = 2026-07-17
learning = "Delivered same-day — an unplanned insert after v0.7, specced/built/host-tested/on-device-verified in one session (firmware 0.7.5, 5 focus + 245 editor + 29 keymap tests). Silent 25-min block on a monotonic clock with no live countdown (e-ink can't show one cheaply); the rest card drops at the next typing pause, or a +2 min grace cap — proven on device when a continuous-typing block force-broke at 27 s (25 + 2). Resume/quit moved from a bare c / q+Esc to the Ctrl-C / Ctrl-Q chords after a bench run judged a single key too easy to fumble behind the full-screen curtain; the host also drops the rest of the key batch on exit so a bump can't reach the buffer. :focusdebug (25-second clock) made the same-day on-device check practical."

[[feature]]
name = "v0.7.7 OTA firmware update"
start = 2026-07-19
original = 2026-07-19
delivered = 2026-07-19
learning = "Delivered 2026-07-19 — an unplanned insert that RESOLVES the v1.x 'firmware auto-update' open question (raised 2026-07-14) well ahead of its pre-v1.0 deadline. `:update` GETs typoena.dev/firmware/latest.txt and, if newer, streams typoena-<ver>.bin into the inactive slot of an A/B layout (partitions-ota.csv: factory + ota_0 + ota_1 + otadata) via esp-idf OTA, then reboots into it. Proven on hardware across two back-to-back installs (0.7.7→0.7.8→0.7.9, exercising both slot directions). The load-bearing risk was device-side TLS, settled by git.apoena.dev's LE→ISRG Root X1 being in the esp-idf FULL CA bundle (validated on-device, twice). Release hosting was SPLIT after weighing one-platform: the installer stays on GitHub (its /releases/latest/download shortcut, no token), firmware releases live on Gitea git.apoena.dev — the host the device's TLS must trust; nginx on typoena.dev 302-redirects the .bin to the Gitea release asset so binaries never enter the site repo, and `just publish-firmware` cuts the release + writes latest.txt (commit-first for a reproducible tag). A/B rollback (CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE) is enforced on customer units by `just ship`. The 0.7.9 payload that proved it also shipped :about (a version splash), :update naming the running version, and the active filename in the side panel — firmware now 0.7.9."

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

## Status — synced 2026-07-19

The editor **core** has been built 2–3 versions ahead of the device
**releases**, and is now **extracted into a host-testable `editor` crate** (plus
a `display` crate for the panel framebuffer) so `cargo test` exercises it off the
xtensa target. **v0.1 shipped 2026-07-11** (late against the 2026-06-29
baseline): SD storage, save, and **git push are all wired into the app binary
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
grey-Push-in-Local panel cue, and the multi-file push count.
**v0.6 Markdown is COMPLETE in core 2026-07-12** (firmware **0.6.0**), host-tested
(187 editor tests), on-device smoke-test pending. The render affordances (heading
bold, list continuation, soft-wrap) were done early; the headline is the
**snippet engine** — a forward-only tab-stop session reached both inline (type a
prefix + Tab in Insert) and from a **`$` palette** launcher, fed by a git-synced,
Zed-compatible `.typoena.snippets.json` read at boot (serde_json, confirmed to
build for xtensa). The `Cmd-P` palette **generalised** into a verb split — bare =
files, `>` = a command registry (toggles stay open, `format`/`push` one-shots
close, a two-step `new file`), `$` = snippets — which **retired `:e`**. `just init`
seeds a curated 17-snippet catalog (Symbols · Structure · Prose, opt-in). One
caveat: `→`/`≠` sit outside ISO-8859-15 and need a display-layer glyph overlay
(in flight) to render on the panel; the other 15 draw on the stock font.
**v0.7 search + better git is CLOSED 2026-07-14** (firmware **0.7.0**), verified
on-device over three bench runs: `/` search (smartcase + accent-folded, `n`/`N`)
panel-confirmed, and `:gl` pull proven in all four shapes — the fast-forward
closing gate passed with an O(changed) `apply_tree_diff` written after libgit2's
`checkout_tree` crashed the device on run 2. TLS session resumption cut the
rejected-push reconcile cycle from 59 s to 24 s, and `:sync` was renamed `:gp`.
Still open post-v0.7: the warm clean-push measurement, the images-off-card
decision, and the empty-note trailing-newline watch item.
**v0.7.5 focus mode is DELIVERED 2026-07-17** (firmware **0.7.5**), an unplanned
same-day insert specced/built/verified in one session: a silent-timer Pomodoro
(25-min block, no live countdown) that drops a full-screen masking rest card at
the next typing pause, dismissed by the `Ctrl-C` (continue) / `Ctrl-Q` (quit)
chords — the grace cap force-broke a continuous-typing block at 27 s on device.
`:focusdebug` gives a 25-second clock for testing.
**v0.7.7 OTA firmware update is DELIVERED 2026-07-19** (firmware now **0.7.9**),
verified on-device across two back-to-back over-the-air installs
(0.7.7→0.7.8→0.7.9). `:update` pulls a newer image from `typoena.dev/firmware`
(nginx 302 → a Gitea release asset) into the inactive A/B slot and reboots into
it — resolving the v1.x "firmware auto-update" open question. Release hosting is
split: the installer stays on GitHub, firmware releases live on Gitea (the host
the device's TLS trusts, via ISRG Root X1 in the esp-idf CA bundle). `just ship`
enforces the rollback bootloader for customer units. Shipped alongside: the
`:about` version splash, the version in the up-to-date notice, and the active
filename in the side panel.

Marks: `[x]` done in core · `[~]` partially done · `[ ]` not started. An
inline `(✓)` marks the done half of a split item.

Each version below links to its dedicated page, which carries the full scope
checklist and status.

---

## v0.1 — MVP: "it writes, it pushes" — [x]

The minimum thing that justifies the hardware existing — boot, type one file,
`:w` to save, `:sync` to push to GitHub. **SHIPPED 2026-07-11** (late vs the
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

## v0.6 — Markdown affordances — [x]

Heading bolding, list continuation, and soft-wrap, plus the trigger-driven
snippet engine (net-new scope, added 2026-07-08): a tab-stop session reached
inline (prefix + Tab) and from the `$` palette, fed by a git-synced,
Zed-compatible `.typoena.snippets.json`; the `Cmd-P` palette generalised into a
`files`/`>` commands/`$` snippets split (retiring `:e`); and a `just init`
catalog. **COMPLETE in core 2026-07-12** (firmware 0.6.0), on-device smoke-test
pending. Detail: [v0.6-markdown.md](v0.6-markdown.md).

## v0.7 — Search + better git — [x]

`/` forward search (`n`/`N`, smartcase + accent-folded) and `:gl` pull (fetch +
fast-forward only, an O(changed) tree-diff apply); `:sync` renamed `:gp`.
**CLOSED 2026-07-14** (firmware 0.7.0), panel- and git-path-verified on-device.
Detail: [v0.7-search-and-git.md](v0.7-search-and-git.md).

## v0.7.5 — Focus mode (Pomodoro) — [x]

A silent-timer Pomodoro cycle: a 25-minute **focus** block with no visible
countdown (the device tracks it silently and imposes the break), then a
full-screen **rest** card that masks the text. `Ctrl-C` starts the next block,
`Ctrl-Q` quits — both deliberate chords so a stray key can't end a break behind
the curtain (a bench run showed a bare `c` was too easy). Rest is themed (white
card / black in dark theme), untimed, and shows the block's `words · minutes`.
Surfaced as "focus" (the Pomodoro name is trademarked). Ephemeral — RAM-only,
off on reboot. A hidden `:focusdebug` runs the block on a 25-**second** clock
for testing. **DELIVERED 2026-07-17** (firmware 0.7.5), verified on the panel.
Unplanned same-day insert after v0.7, on the
[v0.2.5](v0.2.5-international-input.md) `.5` precedent.
Detail: [v0.7.5-focus-mode.md](v0.7.5-focus-mode.md).

## v0.7.7 — OTA firmware update — [x]

Over-the-air update: `:update` pulls a newer image from `typoena.dev/firmware`
(nginx 302 → a Gitea release asset) into the inactive slot of an A/B partition
layout (`partitions-ota.csv`) and reboots into it; `just publish-firmware` cuts
the release + version pointer, `just ship` bakes the rollback bootloader for
customer units. Installer releases stay on GitHub, firmware releases live on
Gitea (the host the device's TLS trusts). Shipped with the `:about` version
splash, the version named in the up-to-date notice, and the active filename in
the side panel. **DELIVERED 2026-07-19** (firmware 0.7.9), verified on hardware
across two back-to-back installs. Resolves the v1.x "firmware auto-update" open
question (below); rationale there.

## v0.8 — Power: battery + sleep — [ ]

Bench current-draw measurement, 18650 + charge board, per-sync Wi-Fi teardown,
light/deep sleep, the `auto_sync` runtime (re-homed from v0.7), and a battery
indicator. **Not started.**
Detail: [v0.8-battery-and-sleep.md](v0.8-battery-and-sleep.md).

## v0.9 — Robustness — [ ]

Crash-safe writes, interrupted-push recovery, SD removal handling, Wi-Fi
reconnect, and on-device provisioning (the first release usable by a non-author).
**Not started.** Detail: [v0.9-robustness.md](v0.9-robustness.md).

## v1.0 — Polish — [ ]

≤ 3 s boot, runtime-switchable fonts, enclosure files, and a user guide
(light/dark theme landed early, in v0.5). **Not started.** Detail:
[v1.0-polish.md](v1.0-polish.md).

## v1.x — Stretch / nice-to-have

Post-1.0 ideas, not committed to any release (10.3" panel, multiple remotes,
writing stats, BLE-HID fallback, firmware auto-update). Detail:
[v1.x-stretch.md](v1.x-stretch.md).

**Firmware auto-update — RESOLVED + DELIVERED 2026-07-19** (shipped as **v0.7.7**,
well ahead of the pre-v1.0 deadline). Chose **OTA over Wi-Fi**: `:update` fetches a
version manifest (`typoena.dev/firmware/latest.txt`) and, if newer, streams
`typoena-<ver>.bin` into the inactive slot of an A/B layout (`partitions-ota.csv`:
factory + `ota_0` + `ota_1` + `otadata`) via esp-idf OTA and reboots into it.
`CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` gives rollback, enforced on customer units
by `just ship` (which refuses to flash without the rollback bootloader). Images are
hosted on the **Gitea** release for `typoena/typewriter` (git.apoena.dev — the
device validates its LE→ISRG Root X1 chain against the esp-idf FULL CA bundle);
nginx on typoena.dev 302-redirects the `.bin` there, so binaries never touch the
site repo. The **SD-drop alternative wasn't needed.** Not yet done: image signing
(the Gitea release + HTTPS is the trust boundary for now) — a v1.0/v1.x hardening
item. Verified on hardware across two back-to-back installs; see the v0.7.7 entry
above.
