# Roadmap — version details

Frequent releases. Each version is a usable artifact, not a checkpoint.
This file holds the macro-plan (Macroplan block below) and the per-version
scope. The user-facing requirements and engineering targets each release feeds
into are tracked in [`qfd.md`](qfd.md).

## Status — synced 2026-07-11

The editor **core** has been built 2–3 versions ahead of the device
**releases**, and is now **extracted into a host-testable `editor` crate** (plus
a `display` crate for the panel framebuffer) so `cargo test` exercises it off the
xtensa target. **v0.1 shipped 2026-07-11** (late against the 2026-06-29
baseline): SD storage, save, and **git publish are all wired into the app binary
and hardware-verified** (`:sync` commits on the SD `/sd/repo` and pushes to a
test repo), and the **boot splash (Spike 9) is confirmed on the panel** — a
vector `typoena`-in-a-circle shown at startup while the SD mounts, then the
editor comes up. **Cold boot measured ~5.5 s** (power-on → cursor, instrumented
2026-07-11 — the 3.5 s eyeball undercounted; `esp_timer` sees only the ~4.1 s
app-side slice, missing the bootloader + ~0.74 s PSRAM memtest). **Over the ≤ 5 s
gate**; **fix implemented 2026-07-11** — the first editor render is now a
full-area partial (~630 ms) rather than a second full refresh (~1.9 s), expected
~4.2 s, pending a reflash to re-measure + check ghosting (PSRAM memtest kept on).
The 1-hour soak is attested from real use; boot time joins the
remaining post-ship acceptance checks (power-pull recovery, 1000-word no-drop,
and `Ctrl-G`'s not-yet-built pull-then-retry → v0.9). Beyond v0.1, v0.2 navigation, **v0.2.5 international input (hardware-verified
2026-07-11)**, and most of v0.6 Markdown already run. Version numbers are
unchanged — they track shippable device releases, not core progress.

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
delivered = 2026-07-11
learning = "Shipped 12 days late. The long pole was hardware bring-up risk, not the editor: SD on a shared SPI bus (resolved by moving it to its own SPI3, ADR-012) and on-device git (gix killed, pivoted to libgit2 as an esp-idf CMake component, ADR-004). Splash landed as a vector wordmark, not the planned 1-bit bitmap — the asset-embed/blit path is deferred to v1.0."

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
note = "Also adds the git-tracked .typoena.toml preferences file (save_on_idle, auto_sync cadence) and the palette `>` command mode that edits it live."

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
editor comes up. Cold boot measured ~5.5 s (power-on → cursor, 2026-07-11) — **over the ≤ 5 s
gate**: the 3.5 s eyeball undercounted, and `esp_timer` catches only the ~4.1 s
app-side slice (it starts after the bootloader + the ~0.74 s PSRAM memtest). Fix
implemented 2026-07-11: first editor render is now a full-area partial (~4.2 s
expected, pending a reflash to verify); PSRAM memtest kept on. The 1-hour soak
is attested from real use; boot time joins the remaining post-ship acceptance
checks (power-pull recovery, 1000-word no-drop, `Ctrl-G` pull-then-retry → v0.9) —
see [product → acceptance](v0.1-mvp-product.md#acceptance-criteria).

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
- [x] Publish on **`:sync`** (the editor's command; the roadmap originally
  planned `Ctrl-G`): stage `notes.md` → commit with a timestamp message →
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
- [ ] **Preferences file** `/sd/repo/.typoena.toml` — a git-tracked,
      hand-editable TOML file for editor behaviour, deliberately **distinct from
      the `/sd/typoena.conf` card secrets** (Wi-Fi / PAT / remote / author,
      gitignored, never committed — see v0.1). Read at boot; a missing file or
      key falls back to the defaults below. Keys:
  - [ ] `save_on_idle` (bool, default `true`) — auto-save the current buffer on
        the existing idle pause (the ≥ 1 s typing-pause the panel already uses
        for its refresh), so `:w` becomes optional rather than required.
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
        The **schema + defaults live here in v0.5**; the periodic side rides the
        better-git work (v0.7) and must interact with light / deep sleep (v0.8).
  - [ ] Open question: because the file is committed, these prefs **sync to
        every device** that clones the repo — a per-device sync cadence may
        instead want a card-local override (in `typoena.conf`). Decide before
        build.
- [ ] **Palette command mode** — typing `>` at the `Ctrl-P` palette switches it
      from file search to a command list (VS Code-style). The v0.5 commands edit
      the `.typoena.toml` prefs above — e.g. `> save on idle: on/off` and
      `> auto sync: 10m` — writing the value back to the file and applying it
      live. This command list is the discoverable surface that later actions
      (`:fmt`, theme, font) also register into.

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
