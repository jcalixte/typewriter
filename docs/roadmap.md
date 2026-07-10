# Roadmap — version details

Frequent releases. Each version is a usable artifact, not a checkpoint.
This file holds the macro-plan (Macroplan block below) and the per-version
scope. The user-facing requirements and engineering targets each release feeds
into are tracked in [`qfd.md`](qfd.md).

## Status — synced 2026-07-11

The editor **core** has been built 2–3 versions ahead of the device
**releases**, and is now **extracted into a host-testable `editor` crate** (plus
a `display` crate for the panel framebuffer) so `cargo test` exercises it off the
xtensa target. No release has shipped: v0.1's hardware gate (SD, splash, wiring
the git/save path into the app binary) is still open, even though v0.2
navigation, **v0.2.5 international input (hardware-verified 2026-07-11)**, and
most of v0.6 Markdown already run. Version numbers are unchanged — they track
shippable device releases, not core progress.

Marks: `[x]` done in core · `[~]` partially done · `[ ]` not started. An
inline `(✓)` marks the done half of a split item.

## Macro-plan

Macroplan source — paste into the macroplan app to render the week-by-week
view. `original` dates are the June 2026 baseline and never move; slips get
appended as `reestimates`, per-item actuals live in the Status block above.

```macroplan
title = "Typoena — macro plan"

[[feature]]
name = "v0.1 it writes, it pushes"
start = 2026-06-01
original = 2026-06-29
status = "at-risk"
note = "Overdue — core editing + on-device git push proven in spikes, but blocked on SD (compatible ≤32 GB card), boot splash, and wiring save/publish into main.rs."

[[feature]]
name = "v0.2 navigation"
start = 2026-06-29
original = 2026-07-20
status = "on-track"
note = "Core work landed early: motions and modes already run; gutter, Ctrl-d/u, UTF-8 buffer remain."

[[feature]]
name = "v0.2.5 international input"
start = 2026-07-20
original = 2026-08-03
status = "on-track"
note = "Done early + hardware-verified 2026-07-11: dead-key accent composer in the keymap crate, editor buffer made UTF-8-correct. Side-panel pending marker dropped by decision (stale before the ~630 ms panel repaint)."

[[feature]]
name = "v0.3 editing"
start = 2026-08-03
original = 2026-08-24
status = "on-track"
note = "Deletes, counts, and operator grammar done early in core; yank/paste, undo/redo remain."

[[feature]]
name = "v0.4 visual + ex"
start = 2026-08-24
original = 2026-09-07
status = "on-track"
note = ": command-line mechanism and :fmt done early; Visual mode not started."

[[feature]]
name = "v0.5 palette + multi-file"
start = 2026-09-07
original = 2026-09-28

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

---

## v0.1 — MVP: "it writes, it pushes" — [~]

The minimum thing that justifies the hardware existing. Full design:
[product](v0.1-mvp-product.md) · [technical](v0.1-mvp-technical.md).

**Status:** core editing + partial refresh run on device. **Blocked** on three
integration items: SD (Spike 3 — awaiting a compatible ≤32 GB card), the boot
splash (Spike 9), and wiring the git/save path into the app binary — today it
lives in the `git_sync` / `sd_fat` spike bins, not `main.rs`.

- [~] ESP32-S3 boots (✓); e-ink shows Typoena splash + boot log — splash pending Spike 9
- [x] USB host enumerates the Nuphy, key events reach the editor (Spike 4)
- [ ] One hard-coded file (`/sd/repo/notes.md`) opens on boot — SD spike-only,
      not in `main.rs`. The card is pre-seeded from a computer (`just init`
      copies a full clone to `/sd/repo` + writes config), never cold-cloned on
      device — see [note](notes/git-sync-images-and-repo-size.md).
- [x] Insert-only editing, backspace, enter, arrow keys — modal editor overshot this early (see v0.2)
- [x] Line wrap, no line numbers yet — soft-wrap done early (see v0.6)
- [ ] Save on `Ctrl-S` → SD — SD blocked, not wired to `main.rs`
- [~] Wi-Fi credentials + remote URL + PAT + author: today baked into the binary
      via `env!()` (no NVS, no on-device provisioning UI in v0.1). Migrating to
      `/sd/typoena.conf` on the card, provisioned by `just provision` (or
      `just init` for a fresh card) from the same `firmware/.env` the build uses
      (minimum input — rotate the PAT or switch networks without a reflash, no
      card re-copy). Firmware to read it at boot instead of
      `env!()` — TODO, rides with the SD wiring into `main.rs`.
- [~] `Ctrl-G` runs: `git add .` → commit with an ISO-8601 timestamp message →
  `git push`; on push failure, `git pull --no-edit` then retry the push
  (no-op short-circuit when nothing is staged). Proven on device in the
  `git_sync` spike (✓); not yet wired to the editor.
- [ ] Split the display into **writing column** (~60 cols) + **side panel**
      (~20 cols) for all metadata — the surface every later panel feature
      writes to. Not built yet: `editor.rs` still renders full-width 79 cols.
      Defined in [`CONTEXT.md` § Screen regions](../CONTEXT.md#screen-regions)
      and [product § Screen layout](v0.1-mvp-product.md#screen-layout).
- [~] Partial refresh on edits (✓ Spike 5); full refresh on save — save not wired yet

Out of scope: Vim, palette, multiple files, branches, conflict handling.

## v0.2 — Vim navigation — [~]

**Status:** navigation done in core; the **UTF-8-correct buffer landed
2026-07-11** (hardware-verified). Remaining = `Ctrl-d/u` and the line-number
gutter. Shipped early beyond scope: a read-only **View** mode and the full
`d`/`c` operator + text-object grammar (see v0.3 / v0.4).

- [x] Mode state machine (Normal / Insert / View), mode indicator in the status strip
- [~] Movement: `h j k l`, `w b e`, `0 $`, `gg G` (✓); `Ctrl-d Ctrl-u` remain
- [x] `i a o O A` to enter Insert
- [x] `Esc` returns to Normal
- [ ] Line numbers in the left gutter: relative in Normal mode (current line
      shown as its absolute number), absolute in Insert mode — Spike 13 first
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

## v0.3 — Vim editing — [~]

**Status:** deletes and counts done; **yank/paste, undo/redo, and `.` repeat
remain** — there is no register or recorded-op mechanism yet. The `d`/`c`
operator grammar and text objects landed here ahead of schedule (the roadmap
had scheduled only `dd`/`dw`/`d$`).

- [~] `x dd`, `dw dd d$` (✓); `yy p P` and repeat with `.` remain (need a register / recorded op)
- [ ] Undo / redo (`u`, `Ctrl-r`) — bounded history in PSRAM
- [x] Numeric prefixes (`3dd`, `5j`)
- [x] Ahead of schedule: `c` change operator + text objects
      (`ciw`, `di(`, `ca"`, … — inner/around, nesting-aware)

## v0.4 — Visual mode + ex commands — [~]

**Status:** the `:` command-line mechanism is built (Command mode + status-strip
echo), but only `:fmt` exists — `:w :q :wq :e` remain. Visual mode is not
started.

**DECISION (2026-07-07):** `v`/`V` = **Visual** selection (vim-standard). The
read-only **View** (reading/scroll) mode currently bound to `v`/`V` moves off
those keys and gets its own trigger (exact key TBD when Visual lands). View mode
stays — it just frees `v`/`V` for Visual.

- [ ] Visual char (`v`) and line (`V`) modes, `y d c` on selections
- [~] `:` command line: `:w :q :wq :e <path>` (mechanism ✓; these commands remain)
- [x] Ahead of schedule / unscheduled: `:fmt` Markdown formatter
      (table alignment, blank-line collapse, trailing-whitespace strip)

## v0.5 — File palette + multi-file — [ ]

**Status:** not started (Spikes 11 + 14 retire the panel-mechanism and
buffer-lifecycle risk first).

- [ ] `Ctrl-P` opens fuzzy file palette over **both** `/sd/repo/` and
      `/sd/local/`, with a scope marker (e.g. `[git]` / `[local]`) per result
- [ ] Open, switch, close buffers (keep ≤ 3 in memory)
- [ ] `:e` and palette share the same recent-files list
- [ ] `:enew` creates a new file — prompts for scope (tracked vs local)
- [ ] Delete a file — removes it from the SD card; for a Tracked file the
      removal reaches the next `Ctrl-G` Publish's staged set (`git rm` / `add -A`
      semantics, not plain `git add .`); a Local file is just unlinked
- [ ] `Ctrl-G` is disabled / hidden when the current buffer is local-scope
- [ ] The side panel briefly shows file count on `Ctrl-G` when the publish bundles
      more than one dirty Tracked file (e.g. `"publishing 3 files: abc1234"`),
      so workspace-scoped behaviour stays visible to the user

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
  - [ ] **Trigger:** Tab in Insert mode. If the word immediately before the
        caret matches a snippet prefix, expand it; otherwise insert spaces as
        today (`expand_snippet(word) -> Option<(body, stops)>`, alongside
        `list_marker`).
  - [ ] **Body syntax:** literal text + numbered empty tab stops `$1 … $n` and a
        final `$0`. **No placeholder text** (`${1:label}`) — the editor has no
        selection/overtype model, so a placeholder would just be text to delete.
        **No dynamic/computed values** (e.g. no `date` — there's no RTC; the
        wall clock is valid only after Wi-Fi+SNTP, so it'd stamp 1970 on a cold
        boot).
  - [ ] **Tab-stop session:** after expansion the caret lands on `$1`; Tab
        advances to the next stop, **forward only** (no Shift-Tab). Stored stop
        offsets shift with edits at the caret (all pending stops are always
        after it). Session auto-aborts on Esc, a mode change, or a motion that
        leaves the stops.
  - [ ] **Indicator:** on a typing pause (same throttle as the insert cursor /
        word-count refresh — the panel never repaints per keystroke), if the
        word before the caret is a snippet prefix, the **side panel** shows the
        hint (the target expansion). Quiet while typing; hint appears on pause.
  - [ ] **Source:** snippet table hard-coded in the binary to start; a
        git-syncable file on SD (`/sd/repo/.snippets`) is a later option,
        deferred while SD is still blocked.
  - [ ] Starter set: link `[$1]($2)$0`, image `![$1]($2)$0`, fenced code block,
        etc.

## v0.7 — Search + better git — [ ]

**Status:** not started.

- [ ] `/` forward search, `n N`
- [ ] `:Gpull` (fetch + fast-forward only; refuse on conflict and surface it)

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

- [ ] Boot time ≤ 3 s to usable cursor
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
