# Roadmap — version details

Frequent releases. Each version is a usable artifact, not a checkpoint.
The macro-plan (Gantt) lives in the [README](../README.md#roadmap); this file
holds the per-version scope. The user-facing requirements and engineering
targets each release feeds into are tracked in [`qfd.md`](qfd.md).

## Status — synced 2026-07-07

The editor **core** (`firmware/src/editor.rs`) has been built 2–3 versions
ahead of the device **releases**. No release has shipped: v0.1's hardware gate
(SD, splash, wiring the git/save path into the app binary) is still open, even
though v0.2 navigation and most of v0.6 Markdown already run. Version numbers
are unchanged — they track shippable device releases, not core progress.

Marks: `[x]` done in core · `[~]` partially done · `[ ]` not started. An
inline `(✓)` marks the done half of a split item.

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
- [ ] One hard-coded file (`/sd/repo/notes.md`) opens on boot — SD spike-only, not in `main.rs`
- [x] Insert-only editing, backspace, enter, arrow keys — modal editor overshot this early (see v0.2)
- [x] Line wrap, no line numbers yet — soft-wrap done early (see v0.6)
- [ ] Save on `Ctrl-S` → SD — SD blocked, not wired to `main.rs`
- [x] Wi-Fi credentials + remote URL + PAT + author baked into the binary at
      build time via env vars (no NVS, no on-device provisioning UI in v0.1)
- [~] `Ctrl-G` runs: `git add .` → commit with an ISO-8601 timestamp message →
      `git push`; on push failure, `git pull --no-edit` then retry the push
      (no-op short-circuit when nothing is staged). Proven on device in the
      `git_sync` spike (✓); not yet wired to the editor.
- [~] Partial refresh on edits (✓ Spike 5); full refresh on save — save not wired yet

Out of scope: Vim, palette, multiple files, branches, conflict handling.

## v0.2 — Vim navigation — [~]

**Status:** navigation done in core; remaining = `Ctrl-d/u`, the line-number
gutter, and the UTF-8 buffer. Shipped early beyond scope: a read-only **View**
mode and the full `d`/`c` operator + text-object grammar (see v0.3 / v0.4).

- [x] Mode state machine (Normal / Insert / View), mode indicator in the status strip
- [~] Movement: `h j k l`, `w b e`, `0 $`, `gg G` (✓); `Ctrl-d Ctrl-u` remain
- [x] `i a o O A` to enter Insert
- [x] `Esc` returns to Normal
- [ ] Line numbers in the left gutter: relative in Normal mode (current line
      shown as its absolute number), absolute in Insert mode — Spike 13 first
- [ ] Groundwork — UTF-8-correct buffer: caret motions and edits step by
      character, not byte (drop the ASCII == byte-offset assumption in
      `editor.rs`), so every motion added here and later stays correct once
      accented input lands. Done early so it isn't retrofitted across the whole
      motion/text-object surface. Render font is already ISO-8859-15 (Latin-9),
      so accented glyphs display.

## v0.2.5 — International input — [ ]

**Status:** not started (depends on the v0.2 UTF-8-correct buffer).

A small focused release between navigation and editing. US-International
dead-key accent composition, resolved in the keyboard layer (`usb_kbd.rs`) so
the editor still receives a single `Key::Char`. Builds on the v0.2
UTF-8-correct buffer and the ISO-8859-15 render font.

- [ ] Dead keys — grave, acute, circumflex, diaeresis, tilde — compose with
      the next letter: à é ê ë ñ, ç (via `'`+c), both cases
- [ ] `'`+space emits a literal apostrophe (the everyday apostrophe path); a
      dead key followed by a non-composing letter emits the accent then the
      letter
- [ ] A non-character event (Enter, Backspace, arrows) flushes any pending
      accent as its literal first
- [ ] Pending-accent indicator in the side-panel status strip

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

**Status:** done early — only the 80-col ruler remains.

- [x] Heading lines bolded in render (faux-bold double-strike)
- [x] List continuation on Enter inside `- ` / `1. ` (with empty-item exit)
- [x] Soft-wrap at word boundaries
- [ ] Optional column ruler at 80

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
