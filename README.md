# Typoena

A distraction-free, hackable, DIY writing machine. ESP32-S3 + e-ink + a real
mechanical keyboard. You write Markdown, you commit, you push. Nothing else
runs on it.

> **Status: v0.7 shipped, hardware on bench.** v0.1 (MVP — boots, edits,
> publishes) shipped 2026-07-11; v0.2 through v0.7 (vim navigation and editing,
> file palette + multi-buffer, Markdown affordances, `/` search, `:gp` push /
> `:gl` pull) followed within the week. Firmware is at 0.7.0; next up is v0.8
> (battery + sleep). Live per-item status:
> [`docs/macroplan.md`](docs/macroplan.md) · doc index:
> [`docs/README.md`](docs/README.md) · failure write-ups:
> [`docs/postmortems/`](docs/postmortems/README.md) · improvement loops:
> [`docs/kaizen/`](docs/kaizen/README.md).

---

## Vision

A single-purpose appliance that boots into a text editor with a Vim keymap,
edits Markdown files, and (optionally) pushes them to a git remote (GitHub
first) over Wi-Fi. No browser, no notifications, no apps. Open lid → write →
push (or don't) → close lid.

Two file scopes coexist on the SD card — formal definitions in
[`CONTEXT.md`](CONTEXT.md):

- **Tracked** — lives in the git working copy, gets **Published** when the
  user runs `:gp`.
- **Local** — never leaves the device. Permanently-private: journal entries,
  scratch, things that aren't anyone else's business. There is no "promote
  to Tracked" gesture — scope is fixed at file creation.

Same editor, same keymap; the difference is just whether `:gp` (publish to
the remote) is offered.

---

## Hardware

**ESP32-S3-N16R8** (16 MB flash, 8 MB PSRAM) · **GDEY0579T93** 5.79″ e-ink
strip (792×272, ~2.9:1 — biases the UX toward "current line + recent context",
the writing posture we want) · **Nuphy wired USB keyboard** with the S3 as USB
host · **microSD over SPI** · **USB-C wall power** for the MVP, battery in
v0.8.

Full part table, rationale, and bench status:
[`docs/hardware.md`](docs/hardware.md). Enclosure — a parametric,
3D-printable typewriter-body case (OpenSCAD): [`hardware/case/`](hardware/case/README.md).

---

## Software stack

**Language: Rust on `esp-idf-rs` (std).** Every stack decision — language, UI
strategy, display, git lib, auth, concurrency, storage, power, keyboard
transport — has an ADR in [`docs/adr.md`](docs/adr.md), including the rejected
alternatives (Ratatui, Gleam + Shore on AtomVM, C/Arduino — ADR-001/002). How
each decision is weighted against the user-facing requirements lives in
[`docs/qfd.md`](docs/qfd.md); the ontology those docs use is defined in
[`GLOSSARY.md`](GLOSSARY.md). Where a default traces to a cost curve rather than
a discrete pick — energy, latency, or memory bending against an interval or size
— the curve and its knee live in
[`docs/tradeoff-curves/README.md`](docs/tradeoff-curves/README.md). A
memory-safety review of the Rust `unsafe`/FFI
surface (mostly `usb_kbd.rs`) is in [`MEMORY_AUDIT.md`](MEMORY_AUDIT.md).

| Layer         | Choice                                                                                              | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| ------------- | --------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| HAL / runtime | `esp-idf-svc`, `esp-idf-hal`                                                                        | std build: heap, threads, VFS, mbedtls, Wi-Fi stack.                                                                                                                                                                                                                                                                                                                                                                                          |
| Display       | Custom SSD1683 driver (`firmware/src/epd.rs`) + `embedded-graphics`                                 | Dual-controller 792×272 panel; dirty-rect partial refresh (~630 ms measured). Panel geometry + the in-memory `Frame` live in the shared `display/` crate.                                                                                                                                                                                                                                                                                     |
| UI layer      | Custom thin widget layer                                                                            | Ratatui's API _shape_ without its char-grid terminal model ([ADR-002](docs/adr.md#adr-002-ui-strategy--custom-widgets-on-embedded-graphics-not-ratatui)).                                                                                                                                                                                                                                                                                     |
| Editor core   | Custom, in-tree (`editor/` crate)                                                                   | Modal (Normal / Insert / Visual / VisualLine / View / Command / Palette), motions, operators + text objects; UTF-8 buffer fed by the dead-key composer; smartcase + accent-folded `/` search. Host-built and host-tested off the xtensa target.                                                                                                                                                                                               |
| USB host      | `esp-idf` TinyUSB bindings                                                                          | Boot-protocol HID; verified on hardware (Spike 4).                                                                                                                                                                                                                                                                                                                                                                                            |
| Git           | **libgit2 via `git2`**, built as an esp-idf component with mbedTLS (`firmware/components/libgit2/`) | `gix` was the original pick but can't push over HTTPS — the [ADR-004](docs/adr.md#adr-004-git-implementation--gitoxide-gix) kill-switch fired ([postmortem](docs/postmortems/2026-07-05-spike7-gix-https-push.md)). `:gp` publish + `:gl` pull verified on device; ~16 s cold-`:gp` [latency breakdown](docs/notes/sync-latency.md); how the real notes repo went from a 611 s brick to a 24 s sync: [kaizen](docs/kaizen/real-repo-sync.md). |
| TLS           | `mbedtls` via `esp-idf`                                                                             | GitHub HTTPS with the chain checked against embedded roots; ≈35 KB heap measured during handshake (Spike 6).                                                                                                                                                                                                                                                                                                                                  |
| Auth          | HTTPS + GitHub PAT                                                                                  | v0.1 bakes credentials in at build time via `TW_*` env vars; provisioning + at-rest protection is [ADR-011](docs/adr.md#adr-011-credential-provisioning--how-the-pat-reaches-the-device-and-is-protected-at-rest) (open), on-device settings land in v0.9.                                                                                                                                                                                    |
| Filesystem    | FAT on SD (`esp_vfs_fat`)                                                                           | Working copy lives here (`/sd/repo` + `/sd/local`); editor prefs are a git-tracked [`.typoena.toml`](docs/typoena-toml.md) in the repo.                                                                                                                                                                                                                                                                                                       |

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

Releases are frequent, and every version is a usable artifact rather than a
checkpoint. Per-version scope, current `[x]`/`[~]` marks, and the Macroplan
source live in [`docs/macroplan.md`](docs/macroplan.md).

| Version                                                   | Theme        | One-liner                                    |
| --------------------------------------------------------- | ------------ | -------------------------------------------- |
| [v0.1](docs/macroplan.md#v01--mvp-it-writes-it-pushes--)  | MVP          | Boots, edits one file, `:gp` publishes.      |
| [v0.2](docs/macroplan.md#v02--vim-navigation--)           | Vim nav      | Normal/Insert, motions, line numbers.        |
| [v0.2.5](docs/macroplan.md#v025--international-input--)   | Intl input   | US-Intl dead keys: à é ê ç, `'`+space = `'`. |
| [v0.3](docs/macroplan.md#v03--vim-editing--)              | Vim edit     | `dd yy p`, undo/redo, counts.                |
| [v0.4](docs/macroplan.md#v04--visual-mode--ex-commands--) | Visual + ex  | `v`/`V` select + `y d c`, `gr` read, `:` ex. |
| [v0.5](docs/macroplan.md#v05--file-palette--multi-file--) | Files        | `Cmd-P` over `/repo` + `/local`, buffers.    |
| [v0.6](docs/macroplan.md#v06--markdown-affordances--)     | Markdown     | Headings, list continuation, soft-wrap.      |
| [v0.7](docs/macroplan.md#v07--search--better-git--)       | Search + git | `/` search, `:gl` pull.                      |
| [v0.8](docs/macroplan.md#v08--power-battery--sleep--)     | Power        | 18650 + sleep + lid switch.                  |
| [v0.9](docs/macroplan.md#v09--robustness--)               | Robustness   | Crash-safe writes, reconnect, settings.      |
| [v1.0](docs/macroplan.md#v10--polish--)                   | Polish       | Boot ≤ 3 s, fonts, themes, enclosure, guide. |
| [v1.x](docs/macroplan.md#v1x--stretch--nice-to-have)      | Stretch      | 10.3″ panel, multi-remote, stats, BLE.       |

---

## Repo layout

```
/firmware                 Rust crate, esp-idf-rs target — the app + on-device bins
                          (SD card mounted at runtime contains /repo and /local)
  /src
    main.rs               app binary — editor + display + USB + SD + git, wired
    epd.rs                SSD1683 dual-controller e-ink driver
    usb_kbd.rs            TinyUSB host glue, HID → key events
    git_sync.rs           :gp/:gl service — dirty-path journal, O(depth) splice
                          commit, push/pull on a dedicated 96 KB git thread
    persistence.rs        SD mount, atomic save, crash recovery
    net.rs                Wi-Fi + SNTP bring-up (bounded retry)
    /bin                  on-device spike + bench binaries (sd_fat, sd_bench,
                          wifi_tls, git_push, git_sync, git_smoke, git_bench,
                          splash)
  /components/libgit2     libgit2 as an esp-idf CMake component (mbedTLS);
                          source vendored as a git submodule
  build.rs                bakes TW_* env vars (Wi-Fi, PAT, author) — the config
                          path until v0.9's on-device settings
/editor                   modal editor core crate — buffer, modes, motions,
                          palette, search, render; host-built and host-tested
                          off the xtensa target
/display                  panel geometry + in-memory Frame (embedded-graphics
                          DrawTarget), shared by the epd driver and the editor
/keymap                   pure HID boot-keyboard decode — host-testable, fuzzable
/spikes                   desktop spikes (spike7 git push proof, pre-device)
/docs                     the design record — index: docs/README.md
                          (ADRs, QFD, macroplan, per-version specs, spikes.md,
                          postmortems/, notes/, tradeoff-curves/, kaizen/)
/hardware                 enclosure — parametric OpenSCAD case (case/) + renders
CONTEXT.md                project glossary — Tracked / Local / Save / Publish, and
                          the principles that fall out of them
GLOSSARY.md               methodology glossary — the WHAT / Function /
                          Characteristic / Metric / Target ontology layers
package.json              pnpm + oxfmt — formatting toolchain for docs/JSON
```

---

## Open questions / risks (tracked, not yet resolved)

- [ ] Heap fragmentation over a long writing session with the PSRAM allocator.
- [ ] Real-world e-ink ghosting with the current partial-refresh cadence.
- [~] Use-after-free freeing the in-flight USB transfer on keyboard unplug —
  fixed in code, pending an on-device hot-plug run to confirm
  ([`MEMORY_AUDIT.md`](MEMORY_AUDIT.md) finding #1).

Retired risks ([gix push](docs/postmortems/2026-07-05-spike7-gix-https-push.md),
[SD CMD59 rejection](docs/postmortems/2026-07-05-spike3-sd-cmd59.md), TinyUSB HID
stability, TLS heap, libgit2-on-xtensa) and how they died:
[`docs/spikes.md`](docs/spikes.md) and
[`docs/postmortems/`](docs/postmortems/README.md).

These get resolved by writing code, not by deciding harder.
