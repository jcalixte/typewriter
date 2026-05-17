# Roadmap — version details

Frequent releases. Each version is a usable artifact, not a checkpoint.
The macro-plan (Gantt) lives in the [README](../README.md#roadmap); this file
holds the per-version scope. The user-facing requirements and engineering
targets each release feeds into are tracked in [`qfd.md`](qfd.md).

---

## v0.1 — MVP: "it writes, it pushes" — [ ]

The minimum thing that justifies the hardware existing. Full design:
[product](v0.1-mvp-product.md) · [technical](v0.1-mvp-technical.md).

- [ ] ESP32-S3 boots, e-ink shows splash + boot log
- [ ] USB host enumerates the Nuphy, key events reach the editor
- [ ] One hard-coded file (`/sd/repo/notes.md`) opens on boot
- [ ] Insert-only editing (no modes yet), backspace, enter, arrow keys
- [ ] Line wrap, no line numbers yet
- [ ] Save on `Ctrl-S` → SD
- [ ] Wi-Fi credentials + remote URL + PAT + author baked into the binary at
      build time via env vars (no NVS, no on-device provisioning UI in v0.1)
- [ ] `Ctrl-G` runs: `git add .` → commit with an ISO-8601 timestamp message →
      `git push`; on push failure, `git pull --no-edit` then retry the push
      (no-op short-circuit when nothing is staged). PAT from first-run setup.
- [ ] Partial refresh on edits; full refresh on save

Out of scope: Vim, palette, multiple files, branches, conflict handling.

## v0.2 — Vim navigation — [ ]

- [ ] Mode state machine (Normal / Insert), mode indicator in status line
- [ ] Movement: `h j k l`, `w b e`, `0 $`, `gg G`, `Ctrl-d Ctrl-u`
- [ ] `i a o O A` to enter Insert
- [ ] `Esc` returns to Normal
- [ ] Line numbers in the left gutter: relative in Normal mode (current line
      shown as its absolute number), absolute in Insert mode

## v0.3 — Vim editing — [ ]

- [ ] `x dd yy p P`, `dw dd d$`, repeat with `.`
- [ ] Undo / redo (`u`, `Ctrl-r`) — bounded history in PSRAM
- [ ] Numeric prefixes (`3dd`, `5j`)

## v0.4 — Visual mode + ex commands — [ ]

- [ ] Visual char (`v`) and line (`V`) modes, `y d c` on selections
- [ ] `:` command line: `:w :q :wq :e <path>`
- [ ] Status line shows file path, dirty flag, mode

## v0.5 — File palette + multi-file — [ ]

- [ ] `Ctrl-P` opens fuzzy file palette over **both** `/sd/repo/` and
      `/sd/local/`, with a scope marker (e.g. `[git]` / `[local]`) per result
- [ ] Open, switch, close buffers (keep ≤ 3 in memory)
- [ ] `:e` and palette share the same recent-files list
- [ ] `:enew` creates a new file — prompts for scope (tracked vs local)
- [ ] `Ctrl-G` is disabled / hidden when the current buffer is local-scope
- [ ] Status line briefly shows file count on `Ctrl-G` when the publish bundles
      more than one dirty Tracked file (e.g. `"publishing 3 files: abc1234"`),
      so workspace-scoped behaviour stays visible to the user

## v0.6 — Markdown affordances — [ ]

- [ ] Heading lines bolded in render
- [ ] List continuation on Enter inside `- ` / `1. `
- [ ] Soft-wrap at word boundaries
- [ ] Optional column ruler at 80

## v0.7 — Search + better git — [ ]

- [ ] `/` forward search, `n N`
- [ ] `:Gpull` (fetch + fast-forward only; refuse on conflict and surface it)

## v0.8 — Power: battery + sleep — [ ]

- [ ] Measure idle / typing / push current draw on bench
- [ ] 18650 + IP5306 charge board, soft power switch
- [ ] Light sleep on idle > 30 s (keyboard interrupt wakes)
- [ ] Deep sleep on lid close (reed switch); restore cursor + buffer
- [ ] Battery indicator in status line

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
