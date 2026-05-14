# typewriter

A distraction-free, hackable, DIY writing machine. ESP32-S3 + e-ink + a real
mechanical keyboard. You write Markdown, you commit, you push. Nothing else
runs on it.

> Status: pre-MVP. Hardware not yet on bench. Bring-up in progress.

---

## Vision

A single-purpose appliance that boots into a text editor with a Vim keymap,
edits Markdown files, and (optionally) pushes them to a git remote (GitHub
first) over Wi-Fi. No browser, no notifications, no apps. Open lid → write →
push (or don't) → close lid.

Two file scopes coexist on the SD card:

- **Tracked** — lives in the git working copy, gets committed and pushed.
- **Local** — never leaves the device. Drafts, journal entries, scratch, things
  that aren't ready or aren't anyone else's business.

Same editor, same keymap; the difference is just whether `Ctrl-G` (commit &
push) is offered.

---

## Hardware

| Part | Choice | Why |
|---|---|---|
| MCU | **ESP32-S3-N16R8** (16 MB flash, 8 MB octal PSRAM) | USB OTG host (for the keyboard), Wi-Fi, BLE, dual core @ 240 MHz, plenty of PSRAM for git pack data and screen buffer. Best-supported Rust target in the ESP family. |
| Display | **GDEY0579T93 + DESPI-c579 breakout** (5.79", 792×272, 1-bit) | Good Display panel matched with its own FPC breakout. Strip aspect (~2.9:1) — Freewrite-coded: ~12 lines of edit area, ~95 cols. Tiny framebuffer (~27 KB) leaves PSRAM headroom. The DESPI-c579 is a passive level-shifter / FPC-to-header board, not an active controller — driven over plain SPI like any other epd. |
| Keyboard | **Nuphy Air60/Halo65 wired USB-C** | ESP32-S3 acts as USB host via TinyUSB. BLE-HID is a fallback but contends with Wi-Fi for radio time during push. |
| Storage | microSD over SPI | Holds both the git working copy (`/sd/repo/`) **and** the local-only scratch space (`/sd/local/`). Internal flash is for firmware + config only. |
| Power | **USB-C wall power for MVP**, 18650 + IP5306 in Phase 3 | Measure power profile on real hardware before sizing the battery. E-ink + sleep should give multi-day battery life but battery introduces charging, safety, and BMS complexity we don't need on day one. |
| Enclosure | 3D-printed, hinged lid | Phase 4 concern. |

**Why the 5.79" strip aspect:** less screen than a 7.5" page-shaped panel,
but the long-narrow shape biases toward "current line + recent context" —
the writing posture we actually want. The smaller framebuffer is cheap on
RAM, and SPI panels keep the GPIO budget open for SD + future peripherals.
A larger panel (10.3" via IT8951) stays on the table for v1.x once UX is
proven.

---

## Software stack

**Language: Rust on `esp-idf-rs` (std).** Decision is load-bearing — see the
rejected alternatives below.

| Layer | Crate / Component | Notes |
|---|---|---|
| HAL / runtime | `esp-idf-svc`, `esp-idf-hal` | std build, gives us heap, threads, VFS, mbedtls, Wi-Fi stack. |
| Display | `embedded-graphics` + `epd-waveshare` (or custom driver) | Pixel framebuffer with partial-refresh regions. We track dirty rects ourselves. The GDEY0579T93 uses an SSD1683-class controller; if it's not already in `epd-waveshare`, we write a small driver against `embedded-hal` SPI — ~300 LoC, low risk. |
| Editor core | Custom, in-tree | Rope buffer (`ropey`), mode state machine, Vim keymap table. |
| TUI-style layout | Custom thin layer (~500 LoC) | API inspired by Ratatui (`Layout`, `Block`, `Paragraph`) but renders directly to `embedded-graphics`. See below. |
| USB host | `esp-idf` TinyUSB bindings | Boot-protocol HID is enough for the keyboard. |
| Git | `gitoxide` (`gix`) | Pure-Rust, modular. We only need add / commit / push (smart HTTP). No libgit2, no mbedtls glue beyond what `esp-idf` already gives us. |
| TLS | `mbedtls` via `esp-idf` | Used for GitHub HTTPS. ~120 KB heap during handshake — fits in PSRAM. |
| Auth | GitHub Personal Access Token in encrypted NVS | SSH on embedded is painful; HTTPS+PAT is the pragmatic path. |
| Filesystem | FAT on SD (`esp_vfs_fat`) | Working copy lives here. Internal LittleFS holds config. |

### Why not Ratatui

Ratatui assumes a **character-grid terminal** with an ANSI backend. E-ink is a
**pixel framebuffer with partial-refresh windows**. The right primitive for
e-ink is dirty-rectangle tracking aligned to the panel's refresh regions —
Ratatui's per-cell diff model fights this. We can borrow its widget *API
shape* (it's a good one) without dragging in the terminal abstraction. Net
saving: probably 200 KB of binary and a lot of pretending the screen is a
VT100.

### Why not Gleam + Shore

BEAM doesn't run on ESP32. AtomVM does, but: memory budget is tight, Gleam-on-
AtomVM is bleeding-edge, and there are no bindings for USB host / e-ink / SD /
TLS / git in that ecosystem. Shore is also terminal-oriented, so the same
impedance mismatch as Ratatui applies. Building this on Gleam would be a
research project stacked on a research project. Revisit in 2-3 years.

### Why not C / Arduino

Workable, well-trodden, fastest path to a blinking screen. But this is a
project I want to keep evolving — Rust's refactoring leverage and type safety
pay off the moment we start adding modes, palette, search, etc.

---

## UX boundaries set by the medium

E-ink is a brutal honesty filter on UI choices. Hard constraints we design
around, not against:

- **No cursor blink.** Kills the panel and the battery.
- **Typing latency target: ≤ 200 ms** from keypress to glyph on screen, using
  partial refresh on the affected line only.
- **Full refresh every ~20 partials** to clear ghosting. User-visible flash —
  schedule it on pauses (>1 s of no input).
- **No smooth scrolling.** Page-style jumps only.
- **No animations.** Anywhere.
- **Render only changed lines**, not the viewport.

---

## Roadmap

Frequent releases. Each version is a usable artifact, not a checkpoint.

### v0.1 — MVP: "it writes, it pushes" — [ ]

The minimum thing that justifies the hardware existing. Full design:
[product](docs/v0.1-mvp-product.md) · [technical](docs/v0.1-mvp-technical.md).

- [ ] ESP32-S3 boots, e-ink shows splash + boot log
- [ ] USB host enumerates the Nuphy, key events reach the editor
- [ ] One hard-coded file (`/sd/repo/notes.md`) opens on boot
- [ ] Insert-only editing (no modes yet), backspace, enter, arrow keys
- [ ] Line wrap, no line numbers yet
- [ ] Save on `Ctrl-S` → SD
- [ ] Wi-Fi credentials via captive portal on first boot, stored in NVS
- [ ] `Ctrl-G` runs: `git add notes.md && git commit -m "wip" && git push` to a
      pre-configured remote, using a PAT entered during setup
- [ ] Partial refresh on edits; full refresh on save

Out of scope: Vim, palette, multiple files, branches, conflict handling.

### v0.2 — Vim navigation — [ ]

- [ ] Mode state machine (Normal / Insert), mode indicator in status line
- [ ] Movement: `h j k l`, `w b e`, `0 $`, `gg G`, `Ctrl-d Ctrl-u`
- [ ] `i a o O A` to enter Insert
- [ ] `Esc` returns to Normal
- [ ] Line numbers (absolute) in the left gutter

### v0.3 — Vim editing — [ ]

- [ ] `x dd yy p P`, `dw dd d$`, repeat with `.`
- [ ] Undo / redo (`u`, `Ctrl-r`) — bounded history in PSRAM
- [ ] Numeric prefixes (`3dd`, `5j`)

### v0.4 — Visual mode + ex commands — [ ]

- [ ] Visual char (`v`) and line (`V`) modes, `y d c` on selections
- [ ] `:` command line: `:w :q :wq :e <path>`
- [ ] Status line shows file path, dirty flag, mode

### v0.5 — File palette + multi-file — [ ]

- [ ] `Ctrl-P` opens fuzzy file palette over **both** `/sd/repo/` and
      `/sd/local/`, with a scope marker (e.g. `[git]` / `[local]`) per result
- [ ] Open, switch, close buffers (keep ≤ 3 in memory)
- [ ] `:e` and palette share the same recent-files list
- [ ] `:enew` creates a new file — prompts for scope (tracked vs local)
- [ ] `Ctrl-G` is disabled / hidden when the current buffer is local-scope

### v0.6 — Markdown affordances — [ ]

- [ ] Heading lines bolded in render
- [ ] List continuation on Enter inside `- ` / `1. `
- [ ] Soft-wrap at word boundaries
- [ ] Optional column ruler at 80

### v0.7 — Search + better git — [ ]

- [ ] `/` forward search, `n N`
- [ ] `:Gpull` (fetch + fast-forward only; refuse on conflict and surface it)
- [ ] `:Gbranch` to switch branches; refuse with dirty tree
- [ ] Commit message prompt instead of hard-coded `"wip"`

### v0.8 — Power: battery + sleep — [ ]

- [ ] Measure idle / typing / push current draw on bench
- [ ] 18650 + IP5306 charge board, soft power switch
- [ ] Light sleep on idle > 30 s (keyboard interrupt wakes)
- [ ] Deep sleep on lid close (reed switch); restore cursor + buffer
- [ ] Battery indicator in status line

### v0.9 — Robustness — [ ]

- [ ] Crash-safe writes (write to `.tmp`, fsync, rename)
- [ ] Recover from interrupted push (re-attempt on next save)
- [ ] SD card removal / reinsert handling
- [ ] Wi-Fi reconnect with backoff
- [ ] Settings screen: SSID, PAT rotation, default remote, commit author

### v1.0 — Polish — [ ]

- [ ] Boot time ≤ 3 s to usable cursor
- [ ] Font selection (at least one serif + one mono)
- [ ] Enclosure design files in `hardware/`
- [ ] User guide

### v1.x — Stretch / nice-to-have

- 10.3" panel upgrade via IT8951
- Multiple remotes / repos
- Spell-check (dictionary in flash, naive)
- Stats: words today, streak
- Theme: light / dark (inverted e-ink)
- BLE-HID fallback for wireless keyboards

---

## Repo layout (planned)

```
/firmware       Rust crate, esp-idf-rs target
                (SD card mounted at runtime contains /repo and /local)
  /src
    editor/     rope buffer, modes, keymap
    render/     embedded-graphics + dirty rects
    git/        gitoxide wrapper, auth
    usb/        TinyUSB host glue
    fs/         SD + NVS
  build.rs
  sdkconfig.defaults
/hardware       BOM, schematic, enclosure (later)
/docs           ADRs, power measurements
```

---

## Open questions / risks (tracked, not yet resolved)

- [ ] `gix-clone` + `gix-pack` smart-HTTP push working on `esp-idf-rs` with
      mbedtls — needs an early spike before locking the stack.
- [ ] TinyUSB host stability with arbitrary HID descriptors (Nuphy reports
      consumer-control keys we may need to ignore).
- [ ] Heap fragmentation over a long writing session with PSRAM allocator.
- [ ] Real-world e-ink ghosting with current partial-refresh cadence.

These get resolved by writing code, not by deciding harder.
