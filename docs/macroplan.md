# Macroplan — version details

Frequent releases. Each version is a usable artifact, not a checkpoint.
This file holds the `macroplan` source block (below) and the per-version scope.
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
note = "Also adds the git-tracked .typoena.toml preferences file (save_on_idle, format_on_save, auto_sync cadence, line_numbers) and the palette `>` command mode that edits it live."

[[feature]]
name = "v0.6 markdown"
start = 2026-09-28
original = 2026-10-12
status = "on-track"
note = "Render affordances done early; 80-col ruler + snippet engine (added 2026-07-08) remain."

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

## Status — synced 2026-07-11

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

Marks: `[x]` done in core · `[~]` partially done · `[ ]` not started. An
inline `(✓)` marks the done half of a split item.

---

## v0.1 — MVP: "it writes, it pushes" — [x]

The minimum thing that justifies the hardware existing. Full design:
[product](v0.1-mvp-product.md) · [technical](v0.1-mvp-technical.md).

**Status:** SHIPPED 2026-07-11 (late vs the 2026-06-29 baseline). Core editing +
partial refresh run on device; **SD mount + save are wired into `main.rs`**
(Spike 3 resolved — a genuine ≤32 GB card mounts, verified on its own SPI3 host
per ADR-012); **git publish is wired** (`:sync` → commit + fast-forward push on
the SD `/sd/repo`, hardware-verified against a test repo); and the **boot splash
(Spike 9) is confirmed on the panel** — [`Frame::splash`](../display/src/lib.rs)
shows a vector `typoena`-in-a-circle at startup while the SD mounts, then the
editor comes up. Cold boot **verified at 4258 ms** (power-on → cursor, 2026-07-11; 742 ms under
the ≤ 5 s gate). It first measured ~5.5 s; the fix was to bring the editor up
with a full-area partial (~630 ms) instead of a second full refresh (~1.9 s) —
panel confirmed clean. The 1-hour soak is attested from real use; the remaining
post-ship acceptance checks are power-pull recovery, 1000-word no-drop, and
`Ctrl-G` pull-then-retry (→ v0.9) — see
[product → acceptance](v0.1-mvp-product.md#acceptance-criteria).

- [x] ESP32-S3 boots (✓); e-ink shows Typoena splash (✓ Spike 9, confirmed on
      panel 2026-07-11); boot status surfaces via the panel snackbar (no serial on device)
- [x] USB host enumerates the Nuphy, key events reach the editor (Spike 4)
- [x] One hard-coded file (`/sd/repo/notes.md`) opens on boot — **wired in
      `main.rs`** (`boot_storage` mounts the SD and loads the note; a missing
      card / repo / unreadable note halts with a panel message). The card is
      pre-seeded from a computer (`just init` copies a full clone to `/sd/repo` +
      writes config), never cold-cloned on device — see
      [note](notes/git-sync-images-and-repo-size.md).
- [x] Insert-only editing, backspace, enter, arrow keys — modal editor overshot this early (see v0.2)
- [x] Line wrap, no line numbers yet — soft-wrap done early (see v0.6)
- [x] Save to SD via `:w` (and `:sync`) — **wired in `main.rs`** through the
      `persistence` module's atomic write (unlink-then-rename + `*.tmp`
      boot-recovery)
- [~] Wi-Fi credentials + remote URL + PAT + author: today baked into the binary
      via `env!()` (no NVS, no on-device provisioning UI in v0.1). Migrating to
      `/sd/typoena.conf` on the card, provisioned by `just provision` (or
      `just init` for a fresh card) from the same `firmware/.env` the build uses
      (minimum input — rotate the PAT or switch networks without a reflash, no
      card re-copy). Firmware to read it at boot instead of
      `env!()` — the git-publish wiring landed with baked config (2026-07-11);
      the `typoena.conf` migration itself is deferred to v0.9 (on-device
      provisioning).
- [x] Publish on **`:sync`** (the editor's command; originally planned as
  `Ctrl-G`): format (`:fmt`, when `format_on_save`) → save → stage `notes.md`
  → commit with a timestamp message →
  fast-forward `push`; on a rejected push, fetch + reconcile then retry once
  (no-op short-circuit when the tree is unchanged). **Wired into the editor and
  hardware-verified 2026-07-11** — `firmware::git_sync` opens the SD `/sd/repo`,
  runs on a dedicated 96 KB git thread with lazy Wi-Fi, and pushes over mbedTLS
  HTTPS+PAT; the panel snackbar shows `synced <oid>` / `up to date` /
  `sync failed`. (Interrupted-push auto-retry deferred to v0.9.)
- [x] Split the display into a **writing column** (60 cols) + a **side panel**
      (~30 cols at FONT_6X10) for metadata — the surface every later panel
      feature writes to. **Built** in the `editor` crate (`draw_panel`): a
      full-height divider at x=600, with the panel currently showing the word
      count, the mode indicator, a NO-KBD flag, and a transient save/publish
      **snackbar** (below). Later fields (filename, clock, Wi-Fi, battery) add to
      the same surface. Defined in
      [`CONTEXT.md` § Screen regions](../CONTEXT.md#screen-regions) and
      [product § Screen layout](v0.1-mvp-product.md#screen-layout).
- [x] **Snackbar** — a transient side-panel notice for host events (added
      2026-07-11). On-device there is no serial log, so boot posts `loaded
      <name>` (the note's filename without suffix), `:w` posts `saved` /
      `save FAILED - retry :w`, and `:sync` posts `syncing...` then the push
      result (`synced <oid>` / `up to date` / `sync failed`). Set via
      `Editor::set_notice`; cleared on the next keystroke
      rather than a timer — a timed auto-dismiss would cost a ~630 ms full-area
      e-ink flash purely to erase text, which the panel deliberately avoids (cf.
      the dropped pending-accent marker in v0.2.5).
- [x] Partial refresh on edits (✓ Spike 5); save wired (full-area partial
      repaint on `:w`)

Out of scope: Vim, palette, multiple files, branches, conflict handling.

## v0.2 — Vim navigation — [x]

**Status:** COMPLETE 2026-07-11. Navigation done in core; the **UTF-8-correct
buffer** and **`Ctrl-d/u` half-page scroll** landed and are hardware-verified,
and the **absolute line-number gutter** is built, host-tested, and **confirmed
on the panel (Spike 13) 2026-07-11** — a single-line edit repaints only the rows
at/below the change and forces no extra full refresh. Shipped early beyond scope:
a read-only **View** mode and the full `d`/`c` operator + text-object grammar
(see v0.3 / v0.4).

- [x] Mode state machine (Normal / Insert / View), mode indicator in the status strip
- [x] Movement: `h j k l`, `w b e`, `0 $`, `gg G`, `Ctrl-d Ctrl-u`. `Ctrl-d/u`
      step **display** (soft-wrapped) rows, not logical lines — half a page is
      half the visible window however prose wraps; decoded as `HalfPageDown/Up`
      intents in the keymap, caret moves and the viewport follows.
- [x] `i a o O A` to enter Insert
- [x] `Esc` returns to Normal
- [x] Line numbers in the left gutter: **absolute**, built + host-tested
      2026-07-11, **confirmed on the panel (Spike 13) 2026-07-11** — numbered on a
      logical line's first display row, blank on wrapped continuation rows; the
      gutter width tracks the buffer's line count (2 digits + separator, widening
      past 99 lines) and steals its columns from the soft-wrap. **Always on** in
      v0.2; the on/off toggle rides the v0.5 `.typoena.toml` prefs (below).
      Relative numbering was dropped (2026-07-11): renumbering the whole gutter on
      every `j`/`k` burns the e-ink ghosting budget for no proportionate gain,
      whereas absolute renumbers only the rows below an edit — the on-panel check
      confirmed a single-line edit repaints only rows at/below it with no extra
      full refresh.
- [x] Groundwork — UTF-8-correct buffer: caret motions and edits step by
      character, not byte (dropped the ASCII == byte-offset assumption), so every
      motion stays correct with accented input. **Done 2026-07-11** alongside
      extracting the editor into a host-testable crate — char-step
      motions/deletes, byte-vs-char split in `layout`/`caret_rc`, `word_end`/`de`
      fixed; 15 host tests. Render font is ISO-8859-15 (Latin-9), so accented
      glyphs display.

## v0.2.5 — International input — [x]

**Status:** DONE in core, **hardware-verified 2026-07-11** (typed ç é è ñ on the
bench, no crash). US-International dead-key accent composition lives in the
`keymap` crate — a `Composer` downstream of the decoder — wired into
`usb_kbd.rs` so the editor still receives a single `Key::Char`. Builds on the
v0.2 UTF-8-correct buffer and the ISO-8859-15 render font. Host-tested.

- [x] Dead keys — grave, acute, circumflex, diaeresis, tilde — compose with
      the next letter: à é ê ë ñ, ç (via `'`+c), both cases
- [x] `'`+space emits a literal apostrophe (the everyday apostrophe path); a
      dead key followed by a non-composing letter emits the accent then the
      letter
- [x] A non-character event (Enter, Backspace, arrows) flushes any pending
      accent as its literal first
- [ ] ~~Pending-accent indicator in the side-panel status strip~~ — **DROPPED
      (2026-07-11 decision):** at typing speed it would be stale before the
      ~630 ms panel repaint, so it conveys nothing. Left unbuilt on purpose.
- [x] Bonus (2026-07-11): the physical **Esc key** (HID 0x29) now types
      `` ` ``/`~` — Esc comes from the Caps tap — so grave/tilde accents and
      Markdown code fences are reachable on a 60% board without a Fn layer.

## v0.3 — Vim editing — [x]

**Status:** COMPLETE in core 2026-07-11, host-tested (65 editor + 28 keymap
tests) and **partially smoke-tested on the panel 2026-07-11**. The three
remaining pieces landed together: a single unnamed **register** with
`y`/`yy`/`p`/`P` (and `x`/`d`/`c` filling it, so `dd`…`p` moves a line),
**undo/redo** (`u`/`Ctrl-r`, snapshot-based, bounded to 100 groups in PSRAM — a
whole Insert session undoes as one group), and **`.` repeat** (keystroke-recorded,
so it replays an insert session like `ciwfoo<Esc>`). The `d`/`c` operator grammar
and text objects had already landed ahead of schedule. On device, `dd`, `yy`, and
`Ctrl-r` confirmed good; the one issue found was that a **multi-line paste near
the bottom left its later lines below the fold** — `adjust_scroll` only kept the
caret's (first) pasted line visible. Fixed by a `reveal()` that scrolls the end of
the pasted block into view while the caret stays on its first line (reflash to
re-confirm on panel).

- [x] `x dd`, `dw dd d$` (✓); `yy p P` (✓) and `.` repeat (✓) — register + a
      keystroke-recorded last-change both landed 2026-07-11
- [x] Undo / redo (`u`, `Ctrl-r`) — snapshot history bounded to 100 groups in
      PSRAM; one Insert session = one undo group
- [x] Numeric prefixes (`3dd`, `5j`)
- [x] Ahead of schedule: `c` change operator + text objects
      (`ciw`, `di(`, `ca"`, … — inner/around, nesting-aware)

Known limits (deferred): `.` drops a *leading* count (`3x` then `.` deletes one;
a count inside an operator like `d2w` is kept); no named registers; `.` after an
aborted operator (`d<Esc>`) is a no-op.

## v0.4 — Visual mode + ex commands — [x]

**Status:** COMPLETE in core 2026-07-11, host-tested (83 editor tests), on-device
smoke-test pending. Charwise **Visual** (`v`) and linewise **VisualLine** (`V`)
selection landed with `y`/`d`/`c` on the span: charwise is vim-inclusive of the
char under the further caret, linewise spans whole logical lines and fills the
register linewise (so `Vy`…`p` copies a line, `Vd` deletes it like `dd`). Motions
(`h j k l`, `w b e`, `0 $`, `gg G`, `Ctrl-d/u`) and counts extend the selection;
`v`/`V` toggle/switch submode, `Esc` cancels. The selection renders as
reverse-video cells (black fill, glyphs redrawn white) — the only selection
affordance on a 1-bit panel — with the caret cell punched back to *normal* video
so the active end stands out. The Normal-mode motions were factored into a shared
`move_by` helper so Normal and Visual can't drift.

**DECISION (2026-07-07, resolved 2026-07-11):** `v`/`V` = **Visual** selection
(vim-standard). The read-only **View** (reading/scroll) mode that used to sit on
`v`/`V` moved to **`gr`** (go-read) — a `g`-prefixed gesture reusing the existing
pending-`g` machinery, no vim clash. View mode stays; `v`/`V` are now Visual.

- [x] Visual char (`v`) and line (`V`) modes, `y d c` on selections — landed
      2026-07-11 (18 new tests). Known limits (deferred): no `o` swap-ends, no
      `x`/`s` operator aliases, no Visual `.` repeat, no `:'<,'>` range commands.
- [~] `:` command line (mechanism ✓; `:w`/`:wq`/`:x` save, `:fmt`/`:sync`/`:gl`
      wired; `:q` deliberately dropped — nothing to quit to). Command-line
      editing added 2026-07-11: Ctrl-W deletes the previous word, Cmd-Backspace
      clears the line. **`:e <path>` deferred to v0.5** — opening another file
      needs host file-IO + buffer switching, which is v0.5's multi-file work
      (gated behind Spikes 11/14); half-building it here ahead of its
      dirty-buffer handling wasn't worth it.
- [x] Ahead of schedule / unscheduled: `:fmt` Markdown formatter
      (table alignment, blank-line collapse, trailing-whitespace strip)

## v0.5 — File palette + multi-file — [~]

**Status:** buffer **foundation** landed in core 2026-07-11 (slice 1 of 4),
host-tested; the palette + transient panel (Spike 11) and delete → git-staging
(Spike 14) remain the on-device gates. The single-file `Effect` return became a
drained **effect queue** (`Save{path,contents}` / `Load{path}` / `Publish` /
`Pull`), so one action can ask the host for several steps in order — opening a
non-resident file queues a `Save` of the outgoing dirty buffer *then* a `Load` of
the target. The multi-buffer state deliberately avoids a rope-per-buffer rewrite:
the active buffer keeps its fields inline on `Editor`, inactive buffers park in a
small LRU `Vec<Buffer>` (≤ 3 resident = active + 2), and a switch marshals fields
in/out so the ~3k-line editing engine is untouched. A dirty parked buffer is
saved before it is evicted (nothing leaves RAM unsaved); `:e <path>` opens by
prefix (`/sd/repo` → Tracked, `/sd/local` → Local); `:sync` is refused in-core in
a Local buffer. Firmware drains the queue **to empty** each batch (a `Load` can
cascade an eviction `Save`), and `persistence::{load_path,save_path}` generalise
the atomic save off the hard-coded `notes.md`.

**Slice 2 of 4 landed in core 2026-07-11**, host-tested: the `Cmd-P` file
**palette** — a modal transient panel over the writing column with a bare
fuzzy-search input (no `>` prefix: `>` is reserved for the command palette,
slice 4 — VS Code semantics), the ranked list, and the selected row in reverse
video. A pure host-testable fuzzy matcher (`fuzzy_score`: subsequence match,
boundary + consecutive-run bonuses, no penalties) ranks results; an in-core MRU
floats recently-opened files to the top on an empty query and is **shared with
`:e`** (both flow through `open_path`). The host feeds the file list once at boot
(`set_file_list`, enumerating `/sd/repo` + `/sd/local`, dotfiles skipped);
`Ctrl-n`/`Ctrl-p` (fzf-style; `Ctrl-d`/`Ctrl-u` too) move the selection — the
60 % board has no arrow keys — Enter opens via the same park/evict path as `:e`,
Esc (or `Cmd-P` again) closes. Same slice: **`Ctrl-n`/`Ctrl-p` also work as
down/up line motions in Normal mode** (vim `CTRL-N`≡`j`, `CTRL-P`≡`k`,
count-aware), which is why the palette opener moved to `Cmd-P` alone. Scope
shows as the inline `repo/…` vs `local/…` label rather than the planned
`[git]`/`[local]` badge — it also disambiguates subpaths, not just scope. 111
editor tests + 28 keymap tests pass; the no-git firmware binary builds clean.
The transient-panel refresh (**Spike 11**) is **CONFIRMED ON DEVICE 2026-07-12 —
no ghosting** (user flashed it and eyeballed the full-area partial the palette
forces); Cmd-P opens it on-device too. Remaining v0.5 slice: 4 prefs +
palette command mode.

**Slice 3 (`:enew` + delete) COMPLETE + CONFIRMED ON DEVICE 2026-07-12**
(committed `c9c0716`). `:enew <name>` creates a new file: empty, active, marked **dirty**
so eviction/`:w` persists it, and added to the in-core file list so the palette
finds it without a disk re-enumeration — no card IO until it is saved. `:delete`
unlinks the **current** file (a new `Effect::Delete` the host services), then
switches to the most-recently-parked buffer or an empty scratch; the discarded
buffer is never saved even when dirty. **Scope for a new file is read from the
path, not a modal prompt** — `local/x` / `repo/x` (the palette label form) select
the scope, a bare name uses the current buffer's scope. Same change made the
**`/sd` prefix optional everywhere** in `resolve_path`: `/sd/repo/x`, `/repo/x`,
and `repo/x` all name one file and nothing resolves outside `/sd` (the writer
can't reach anything else). **Spike 14 (delete → git-staging) DID need a firmware
fix.** The first on-device test found `add_all(["*"])` alone does **not** stage a
deletion on this libgit2 build (the tree came back unchanged, so the second push
was a silent "up to date" no-op — the "delete didn't work" report). Fix:
`stage_and_commit` now runs `add_all` **then `update_all(["*"])`** (`git add -u`),
which removes index entries whose working-tree file is gone — together they are
`git add -A`. Also, `:delete` gave no clear feedback, so the snackbar now names
the scoped file and, for a Tracked file, that it is local until `:sync`
(`deleted repo/notes.md - :sync to publish`). Deferred to later: greying the
Publish affordance for a Local buffer, and the multi-file publish count. 123
editor tests + 28 keymap tests pass; the no-git firmware binary builds clean. The
`update_all` fix (behind `--features git`, unbuildable locally) was **verified on
device 2026-07-12** — `:enew test.txt` → `:sync` → `:delete` → `:sync` removed
test.txt from origin.

**Slice 4 (`.typoena.toml` prefs + palette `>` command mode) COMPLETE in core
2026-07-12, HOST-TESTED not yet on-device.** A `Prefs` type (host-testable
line-based TOML parse/serialize — flat `key = value` bools + one string with `#`
comments, no crate pulled onto xtensa) lives on `Editor`; the host reads
`/sd/repo/.typoena.toml` at boot and applies it before the first render, and a
missing/partial file falls back to per-key defaults. Keys: `save_on_idle`,
`format_on_save`, `line_numbers` (all bool, default on) and `auto_sync`
(string, default `"10m"`, **schema + default only** — nothing reads it yet).
`line_numbers` is live: `gutter_cols()` returns 0 when off, so the text reclaims
the gutter's columns (the `gutter - 1` field width made saturating to avoid the
underflow). The palette `>` command mode (VS Code semantics — a leading `>` in
the query switches file search to the command list) exposes the three booleans
as live toggles; Enter flips the pref, applies it at once, queues a new
`Effect::SavePrefs` (the editor serializes; the host does the atomic write to
`.typoena.toml`, which rides the next `:sync` to other devices), and confirms
the new state on the snackbar. **The list stays open after a toggle** so several
prefs flip in one visit (Esc/`Cmd-P` closes); **`:settings` opens the palette
straight into `>` mode** as a one-command shortcut (both requested by the user
2026-07-12, chosen over a separate settings modal — same surface, no duplicate
machinery). **Three "decide before build" calls:** (1) the
idle auto-save is **unformatted** — `:fmt` runs only on explicit `:w`/`:sync`, so
tables/blank-lines are never reflowed mid-session; (2) the per-device `auto_sync`
override (card-local `typoena.conf`) is **deferred** — auto_sync is inert in
v0.5, so there is nothing yet to override; (3) `> auto sync: <dur>` as a palette
command is **deferred to v0.7** — a control that changes a value nothing reads
would be a dead switch. `save_on_idle` is honoured host-side: a silent idle
auto-save (no snackbar, no forced e-ink flash — a safety net, not an action)
fires once per typing burst after a 1.5 s pause. 141 editor tests + 28 keymap
tests pass; the no-git firmware binary builds clean. Firmware bumped **0.4.0 →
0.5.0** (the v0.5 feature set is met). **Boot-read of the prefs file CONFIRMED ON
DEVICE 2026-07-12** — a `.typoena.toml` in `typoena-test` with non-default values
(`save_on_idle=false`, `line_numbers=false`, `auto_sync="5m"`) logged back
`prefs: Prefs { save_on_idle: false, format_on_save: true, line_numbers: false,
auto_sync: "5m" }` at boot, a byte-exact parse (comments skipped, bools + quoted
string read). Remaining gate (still pending): the palette `>` live toggle +
`SavePrefs` write-back, and the `save_on_idle` autosave (this test ran with it
off).

- [x] `Cmd-P` opens fuzzy file palette over **both** `/sd/repo/` and
      `/sd/local/` — **landed and CONFIRMED ON DEVICE 2026-07-12** (Spike 11: no
      ghosting on the transient panel); scope shows as the inline
      `repo/…` / `local/…` label instead of a `[git]`/`[local]` badge.
- [~] Open, switch, close buffers (keep ≤ 3 in memory) — **open + switch + the
      ≤ 3 LRU-resident model with dirty-aware save-before-evict done in core**
      (host-tested); `:e <path>` **and the palette** drive it today. Explicit
      **close** still to come.
- [x] `:e` and palette share the same recent-files list — both open via
      `open_path`, which pushes to the in-core MRU that orders the palette.
- [x] `:enew` creates a new file — **done in core (host-tested) 2026-07-12.**
      Scope is read from the path (`local/x` / `repo/x` select it, the palette
      label form; a bare name uses the current scope) rather than a modal
      prompt — the resolved scope is echoed in the snackbar. The `/sd` prefix is
      optional throughout (`/sd/repo/x` = `/repo/x` = `repo/x`).
- [x] Delete a file — **core done (host-tested) 2026-07-12;** `:delete` unlinks
      the current file via `Effect::Delete`. For a Tracked file the removal reaches
      the next `:sync` Publish's staged set. **Spike 14 (on-device) found the
      staging incomplete:** `add_all(["*"])` alone did not stage the deletion, so
      `stage_and_commit` now also runs `update_all(["*"])` (`git add -u`) — the
      two together are `git add -A`. A Local file is just unlinked. The snackbar
      now confirms the delete and flags that a Tracked file needs `:sync`.
      **Verified on device 2026-07-12** — the `:enew`→`:sync`→`:delete`→`:sync`
      cycle removed test.txt from origin.
- [~] `Ctrl-G` is disabled / hidden when the current buffer is local-scope —
      **`:sync` / Publish is blocked in-core for a Local buffer** (posts "Publish
      unavailable (Local)"); the side-panel affordance that hides/greys the
      gesture is the remaining half.
- [ ] The side panel briefly shows file count on `Ctrl-G` when the publish bundles
      more than one dirty Tracked file (e.g. `"publishing 3 files: abc1234"`),
      so workspace-scoped behaviour stays visible to the user
- [x] **Preferences file** `/sd/repo/.typoena.toml` — a git-tracked,
      hand-editable TOML file for editor behaviour, deliberately **distinct from
      the `/sd/typoena.conf` card secrets** (Wi-Fi / PAT / remote / author,
      gitignored, never committed — see v0.1). Read at boot; a missing file or
      key falls back to the defaults below. **Core done 2026-07-12** (a `Prefs`
      type on `Editor`, host-testable parse/serialize, applied via
      `Editor::set_prefs` before the first render); full reference:
      [`typoena-toml.md`](typoena-toml.md). Keys:
  - [x] `save_on_idle` (bool, default `true`) — auto-save the current buffer on
        the idle typing-pause, so `:w` becomes optional rather than required.
        **Honoured host-side** as a *silent* save (no snackbar, no forced e-ink
        flash — a safety net, not an action), unformatted, once per typing burst
        after a 1.5 s pause.
  - [x] `format_on_save` (bool, default `true`) — run `:fmt` (table alignment,
        blank-line collapse, trailing-whitespace strip) on the buffer before it
        is persisted, so `:sync` is **fmt → save → commit → push** and `:w`
        saves formatted. Implemented in-core 2026-07-11 (`Editor`), now **driven
        by this key**. **Open question RESOLVED (2026-07-12):** fmt runs only on
        an explicit `:w`/`:sync`; the `save_on_idle` auto-save is deliberately
        **unformatted**, so tables/blank lines are never reflowed mid-session
        (the caret would jump under you on every thinking pause).
  - [x] `line_numbers` (bool, default `true`) — show the absolute line-number
        gutter (built always-on in v0.2). Off reclaims the gutter's columns for
        text (`gutter_cols()` → 0); the palette `> line numbers: on/off` command
        toggles it live. **Done 2026-07-12.**
  - [ ] `auto_sync` (duration string, default `"10m"`; `"0"` / omitted
        disables; **min clamp ~`"2m"`** so a palette typo can't drain the
        battery) — a *max-staleness cap*, not a wall-clock timer:
        **opportunistic, rate-limited** Publish. Push when already awake + dirty
        (coalesced into the idle-pause, ≤ once per `auto_sync`) and once on the
        way into sleep if dirty; **never wake from deep sleep purely to sync**.
        Wi-Fi energy is a `1/T` curve whose knee sits at 5–10 min, and
        `save_on_idle` already owns local data safety — so 10 min halves the
        sync energy of a 5-min default for no real risk. Full derivation:
        [`tradeoff-curves/wifi-auto-sync.md`](tradeoff-curves/wifi-auto-sync.md).
        The **schema + default (`"10m"`) live here in v0.5** and round-trip
        through `Prefs`; **nothing reads the value yet** — the periodic side
        rides the better-git work (v0.7) and must interact with light / deep
        sleep (v0.8). Marked `[~]`: parsed and preserved, no behaviour.
  - [x] Open question RESOLVED (2026-07-12): the per-device sync cadence override
        (a card-local `typoena.conf` layer over the committed prefs) is
        **deferred** — `auto_sync` is inert in v0.5, so there is nothing yet to
        override; revisit when v0.7 makes the periodic push real.
- [x] **Palette command mode** — typing `>` at the `Cmd-P` palette switches it
      from file search to a command list (VS Code-style). **Done in core
      2026-07-12.** The v0.5 commands toggle the three boolean `.typoena.toml`
      prefs — `> save on idle`, `> format on save`, `> line numbers` — each label
      carrying its live state; Enter flips the pref, applies it at once, queues
      `Effect::SavePrefs` (persist to the file), and confirms on the snackbar.
      **The list stays open after a toggle** (flip several, Esc/`Cmd-P` closes),
      and **`:settings` opens it directly** — both added 2026-07-12 as the "change
      config from the device" surface (chosen over a separate settings modal).
      This command list is the discoverable surface later actions (`:fmt`, theme,
      font) also register into. **`> auto sync: <dur>` deferred to v0.7** — a
      value control that changes nothing readable would be a dead switch.

## v0.6 — Markdown affordances — [~]

**Status:** render affordances done early; the 80-col ruler and the snippet
engine remain (snippets are net-new scope, added 2026-07-08).

- [x] Heading lines bolded in render (faux-bold double-strike)
- [x] List continuation on Enter inside `- ` / `1. ` (with empty-item exit)
- [x] Soft-wrap at word boundaries
- [ ] Optional column ruler at 80
- [ ] **Snippets** — trigger-driven text expansion for Markdown authoring
      (Zed-inspired, but no completion popup: e-ink's ~630 ms refresh rules out
      a live filtering menu, and it fights the distraction-free premise). Shape,
      mirroring the existing `list_marker` insert-transform:
  - [ ] Tab in Insert mode triggers expansion: if the word immediately before
        the caret matches a snippet prefix, expand it; otherwise insert spaces
        as today (`expand_snippet(word) -> Option<(body, stops)>`, alongside
        `list_marker`).
  - [ ] A snippet body is literal text plus numbered empty tab stops `$1 … $n`
        and a final `$0`. There is no placeholder text (`${1:label}`) — the
        editor has no selection/overtype model, so a placeholder would just be
        text to delete. There are no dynamic or computed values either (e.g. no
        `date` — there's no RTC; the wall clock is valid only after Wi-Fi+SNTP,
        so it'd stamp 1970 on a cold boot).
  - [ ] After expansion the caret lands on `$1`; Tab advances to the next stop,
        forward only (no Shift-Tab). Stored stop offsets shift with edits at the
        caret (all pending stops are always after it). The session auto-aborts
        on Esc, a mode change, or a motion that leaves the stops.
  - [ ] On a typing pause (same throttle as the insert cursor / word-count
        refresh — the panel never repaints per keystroke), if the word before
        the caret is a snippet prefix, the side panel shows the hint (the target
        expansion). Quiet while typing; the hint appears on pause.
  - [ ] The snippet table is hard-coded in the binary to start; a git-syncable
        file on SD (`/sd/repo/.snippets`) is a later option, deferred while SD
        is still blocked.
  - [ ] Starter set: link `[$1]($2)$0`, image `![$1]($2)$0`, fenced code block,
        etc.

## v0.7 — Search + better git — [~]

**Status:** the **`:gl` pull command landed in the editor** (2026-07-11,
host-tested) — `Effect::Pull` + a firmware stub; the on-device fetch +
fast-forward is still to build. Search not started.

- [ ] `/` forward search, `n N`
- [~] `:gl` — pull: fetch + **fast-forward only**, refuse on divergence and
      surface it (renamed from the planned `:Gpull`). Editor command +
      `Effect::Pull` done 2026-07-11 (host-tested); the git-thread
      fetch/fast-forward in `git_sync` remains (only push is wired today).

## v0.8 — Power: battery + sleep — [ ]

- [ ] Measure idle / typing / push current draw on bench
- [ ] 18650 + IP5306 charge board, soft power switch
- [ ] Light sleep on idle > 30 s (keyboard interrupt wakes)
- [ ] Deep sleep on lid close (reed switch); restore cursor + buffer
- [ ] Battery indicator in the side panel

## v0.9 — Robustness — [ ]

- [ ] Crash-safe writes (write to `.tmp`, fsync, rename)
- [ ] Recover from interrupted push (re-attempt on next save)
- [ ] SD card removal / reinsert handling
- [ ] Wi-Fi reconnect with backoff
- [ ] On-device provisioning + settings screen: SSID, PAT rotation, default
      remote, commit author (replaces the v0.1 dev-only NVS-flashing path —
      first release usable by someone who is not the firmware author)

## v1.0 — Polish — [ ]

- [ ] Boot time ≤ 3 s to usable cursor — currently ~4.26 s; the ~1.9 s cold-boot
      full refresh is a hard e-ink floor, so ≤ 3 s is marginal (see
      [`notes/boot-time-budget.md`](notes/boot-time-budget.md))
- [ ] Font selection (at least one serif + one mono) with adjustable font
      size, switchable at runtime and persisted across reboots
- [ ] Theme: light / dark (inverted e-ink), switchable at runtime and
      persisted across reboots
- [ ] Enclosure design files in `hardware/`
- [ ] User guide

## v1.x — Stretch / nice-to-have

- 10.3" panel upgrade via IT8951
- Multiple remotes / repos
- Stats: words today, streak
- BLE-HID fallback for wireless keyboards
