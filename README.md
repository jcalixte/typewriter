# Typoena

A distraction-free, hackable, DIY writing machine. ESP32-S3 + e-ink + a real
mechanical keyboard. You write Markdown, you commit, you push. Nothing else
runs on it.

> **Status: pre-MVP, hardware on bench.** Display, USB keyboard, live typing
> with partial refresh, Wi-Fi + TLS, and on-device git push are all verified
> in spikes. No release has shipped yet — v0.1 is blocked on SD (waiting on a
> compatible card), the boot splash, and wiring the save/publish path into the
> app binary. Live per-item status: [`docs/roadmap.md`](docs/roadmap.md) ·
> failure write-ups: [`docs/postmortems/`](docs/postmortems/README.md).

---

## Vision

A single-purpose appliance that boots into a text editor with a Vim keymap,
edits Markdown files, and (optionally) pushes them to a git remote (GitHub
first) over Wi-Fi. No browser, no notifications, no apps. Open lid → write →
push (or don't) → close lid.

Two file scopes coexist on the SD card — formal definitions in
[`CONTEXT.md`](CONTEXT.md):

- **Tracked** — lives in the git working copy, gets **Published** when the
  user presses `Ctrl-G`.
- **Local** — never leaves the device. Permanently-private: journal entries,
  scratch, things that aren't anyone else's business. There is no "promote
  to Tracked" gesture — scope is fixed at file creation.

Same editor, same keymap; the difference is just whether `Ctrl-G` (publish to
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
[`GLOSSARY.md`](GLOSSARY.md). A memory-safety review of the Rust `unsafe`/FFI
surface (mostly `usb_kbd.rs`) is in [`MEMORY_AUDIT.md`](MEMORY_AUDIT.md).

| Layer         | Choice                                                                                              | Notes                                                                                                                                                                                                                                                       |
| ------------- | --------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| HAL / runtime | `esp-idf-svc`, `esp-idf-hal`                                                                        | std build: heap, threads, VFS, mbedtls, Wi-Fi stack.                                                                                                                                                                                                        |
| Display       | Custom SSD1683 driver (`src/epd.rs`) + `embedded-graphics`                                          | Dual-controller 792×272 panel; dirty-rect partial refresh (~630 ms measured).                                                                                                                                                                               |
| UI layer      | Custom thin widget layer                                                                            | Ratatui's API _shape_ without its char-grid terminal model ([ADR-002](docs/adr.md#adr-002-ui-strategy--custom-widgets-on-embedded-graphics-not-ratatui)).                                                                                                   |
| Editor core   | Custom, in-tree (`src/editor.rs`)                                                                   | Modal (Normal / Insert / View / Command), motions, operators + text objects. Plain-ASCII buffer until the v0.2 UTF-8 work.                                                                                                                                  |
| USB host      | `esp-idf` TinyUSB bindings                                                                          | Boot-protocol HID; verified on hardware (Spike 4).                                                                                                                                                                                                          |
| Git           | **libgit2 via `git2`**, built as an esp-idf component with mbedTLS (`firmware/components/libgit2/`) | `gix` was the original pick but can't push over HTTPS — the [ADR-004](docs/adr.md#adr-004-git-implementation--gitoxide-gix) kill-switch fired ([postmortem](docs/postmortems/2026-07-05-spike7-gix-https-push.md)). On-device add → commit → push verified. |
| TLS           | `mbedtls` via `esp-idf`                                                                             | GitHub HTTPS with the chain checked against embedded roots; ≈35 KB heap measured during handshake (Spike 6).                                                                                                                                                |
| Auth          | HTTPS + GitHub PAT                                                                                  | v0.1 bakes credentials in at build time via `TW_*` env vars; provisioning + at-rest protection is [ADR-011](docs/adr.md#adr-011-credential-provisioning--how-the-pat-reaches-the-device-and-is-protected-at-rest) (open), on-device settings land in v0.9.  |
| Filesystem    | FAT on SD (`esp_vfs_fat`)                                                                           | Working copy lives here. Internal LittleFS holds config.                                                                                                                                                                                                    |

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
Per-version scope, current `[x]`/`[~]` marks, and the Macroplan source live in
[`docs/roadmap.md`](docs/roadmap.md).

| Version                                                 | Theme        | One-liner                                    |
| ------------------------------------------------------- | ------------ | -------------------------------------------- |
| [v0.1](docs/roadmap.md#v01--mvp-it-writes-it-pushes--)  | MVP          | Boots, edits one file, `Ctrl-G` pushes.      |
| [v0.2](docs/roadmap.md#v02--vim-navigation--)           | Vim nav      | Normal/Insert, motions, line numbers.        |
| [v0.2.5](docs/roadmap.md#v025--international-input--)   | Intl input   | US-Intl dead keys: à é ê ç, `'`+space = `'`. |
| [v0.3](docs/roadmap.md#v03--vim-editing--)              | Vim edit     | `dd yy p`, undo/redo, counts.                |
| [v0.4](docs/roadmap.md#v04--visual-mode--ex-commands--) | Visual + ex  | `v V`, `:w :q :e` command line.              |
| [v0.5](docs/roadmap.md#v05--file-palette--multi-file--) | Files        | `Ctrl-P` over `/repo` + `/local`, buffers.   |
| [v0.6](docs/roadmap.md#v06--markdown-affordances--)     | Markdown     | Headings, list continuation, soft-wrap.      |
| [v0.7](docs/roadmap.md#v07--search--better-git--)       | Search + git | `/` search, `:Gpull`.                        |
| [v0.8](docs/roadmap.md#v08--power-battery--sleep--)     | Power        | 18650 + sleep + lid switch.                  |
| [v0.9](docs/roadmap.md#v09--robustness--)               | Robustness   | Crash-safe writes, reconnect, settings.      |
| [v1.0](docs/roadmap.md#v10--polish--)                   | Polish       | Boot ≤ 3 s, fonts, themes, enclosure, guide. |
| [v1.x](docs/roadmap.md#v1x--stretch--nice-to-have)      | Stretch      | 10.3″ panel, multi-remote, stats, BLE.       |

---

## Repo layout

```
/firmware                 Rust crate, esp-idf-rs target
                          (SD card mounted at runtime contains /repo and /local)
  /src
    main.rs               app binary — editor + display + USB (SD/git not wired yet)
    editor.rs             modal editor core: buffer, modes, keymap, :commands
    epd.rs                SSD1683 dual-controller e-ink driver
    usb_kbd.rs            TinyUSB host glue, HID → key events
    /bin                  on-device spike binaries (sd_fat, wifi_tls, git_push,
                          git_sync, git_smoke)
  /components/libgit2     libgit2 as an esp-idf CMake component (mbedTLS);
                          source vendored as a git submodule
  build.rs                bakes TW_* env vars (Wi-Fi, PAT, author) — v0.1 config path
/spikes                   desktop spikes (spike7 git push proof, pre-device)
/docs                     ADRs, QFD, hardware, roadmap, per-version specs,
                          spikes.md, postmortems/, notes/
/hardware                 enclosure — parametric OpenSCAD case (case/) + renders
CONTEXT.md                project glossary — Tracked / Local / Save / Publish, and
                          the principles that fall out of them
GLOSSARY.md               methodology glossary — the WHAT / Function /
                          Characteristic / Metric / Target ontology layers
package.json              pnpm + oxfmt — formatting toolchain for docs/JSON
```

---

## Open questions / risks (tracked, not yet resolved)

- [ ] SD on the bench: the 133 GB card rejects CMD59; waiting on a genuine
      ≤32 GB card ([postmortem](docs/postmortems/2026-07-05-spike3-sd-cmd59.md)).
- [ ] Heap fragmentation over a long writing session with the PSRAM allocator.
- [ ] Real-world e-ink ghosting with the current partial-refresh cadence.
- [~] Use-after-free freeing the in-flight USB transfer on keyboard unplug —
  fixed in code, pending an on-device hot-plug run to confirm
  ([`MEMORY_AUDIT.md`](MEMORY_AUDIT.md) finding #1).

Retired risks ([gix push](docs/postmortems/2026-07-05-spike7-gix-https-push.md),
TinyUSB HID stability, TLS heap, libgit2-on-xtensa) and how they died:
[`docs/spikes.md`](docs/spikes.md) and
[`docs/postmortems/`](docs/postmortems/README.md).

These get resolved by writing code, not by deciding harder.
