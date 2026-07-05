# Typoena firmware

Rust crate targeting `xtensa-esp32s3-espidf`. See the project root
[`README.md`](../README.md) and
[`docs/v0.1-mvp-technical.md`](../docs/v0.1-mvp-technical.md) for the wider
context.

## Current state

**Modal editor (vim modes) — modes verified 2026-07-05.** The firmware is now a
small vim-style modal text editor. [`src/editor.rs`](src/editor.rs) owns the
buffer, caret, motions, and per-mode rendering; [`src/main.rs`](src/main.rs) is
the hardware loop that drains keystrokes, redraws, and picks a refresh strategy;
[`src/usb_kbd.rs`](src/usb_kbd.rs) decodes editing chords and a dual-role Caps
key. The buffer is pure ASCII, so a byte offset doubles as the caret's character
index (Tab expands to spaces on insert).

Modes (shown live in a small status strip below the text):

- **Insert** — the boot mode; keys type at the caret. `Ctrl+W` /
  `Ctrl+Backspace` delete the previous word, `Cmd+Backspace` deletes to the
  start of the line.
- **Normal** — motions `h j k l`, `w b e`, `0` `$`, `gg` `G`; edits `x`, `dd`,
  and the `d` / `c` (change) operators over motions and text objects — `ciw`,
  `daw`, `di(`, `ci"`, … (bracket pairs are nesting-aware); `i a A I o O` to
  enter insert; count prefixes like `3j`, `2dd`.
- **View** — read-only reading: `j` / `k` scroll, `space` pages, `gg` / `G`
  jump; edits are locked out.

**Caps Lock is dual-role**: tapped it is `Esc` (→ Normal); held it is `Ctrl`.
So Caps no longer types capitals — use Shift.

Rendering reuses the partial refresh from Spike 5: additive Insert typing stays
on the fast windowed path with a ~750 ms debounced caret, while caret moves,
deletes, mode switches, and View scrolling take a clean full-area partial
(~630 ms). Count prefixes collapse repeated motion into a single refresh, which
matters at this latency.

Known rough edges (deferred): no backspace auto-repeat (the keyboard is on
`SET_IDLE(0)` and only key-downs are tracked), non-sticky column on `j` / `k`,
the `$` / end-of-line block caret sits one cell past the last char, `iw` / `aw`
are whitespace-delimited (like vim's `iW` / `aW`), and `cw` isn't special-cased
to `ce`.

**Spike 6 — Wi-Fi + TLS: verified 2026-07-05.** A separate binary —
[`src/bin/wifi_tls.rs`](src/bin/wifi_tls.rs), flashed with `just flash-wifi` —
kept apart from the editor firmware. It brings up the station, syncs the clock
over SNTP (mbedtls validates the server cert against wall time, so the 1970 RTC
has to be corrected first), then does an HTTPS GET to `https://api.github.com/`
with cert-chain validation against the esp-idf certificate bundle
(`esp_crt_bundle_attach`), and logs status, a body preview, and free heap around
the handshake (TLS heap pressure is a watched risk). A validated GET is the gate
for Spike 7 (gitoxide push over HTTPS + PAT).

Bench result (WPA2-PSK AP, 2.4 GHz): associate ~3 s → DHCP → SNTP first sync →
`esp-x509-crt-bundle: Certificate validated` → `HTTPS GET … → 200`, reading real
GitHub JSON. **TLS handshake cost ≈ 35 KB heap** (265 → 229 KB, recovered
after), clean and repeatable across reboots. Note: PSRAM is **not** enabled yet
(only ~339 KB internal heap) — TLS fits, but Spike 7's gitoxide working set will
need `CONFIG_SPIRAM` turned on first.

Credentials are build-time: copy [`.env.example`](.env.example) to `.env`, set
`TW_WIFI_SSID` / `TW_WIFI_PASS`, and `just` loads them (dotenv) so `build.rs`
bakes them in. `.env` is gitignored; the editor build (`just flash`) needs none
of it. `sdkconfig.defaults` gains the full certificate bundle and a bigger main
task stack for the mbedtls handshake — a one-time esp-idf reconfigure on the
next build.

**Spike 5 — partial refresh + typing: verified 2026-07-04.** `main.rs` wires
the keyboard to the panel: [`src/usb_kbd.rs`](src/usb_kbd.rs) feeds decoded
key-downs (US layout, edge-detected) into a queue, and the main loop keeps a
wrapped, scrolling text buffer that it draws with a **partial refresh**
(`Epd::display_frame_partial`) per keystroke batch, plus a periodic full
refresh to clear ghosting. First spike where input and output run together.
Measured on the bench at 4 MHz SPI: partial refresh ~630 ms, full ~1870 ms —
the partial waveform (~490 ms, all 272 rows) dominates. Follow-up: windowed-Y
partial refresh (drive only the edited line's rows) to cut per-keystroke
latency.

**Spike 4 — USB host keyboard: verified 2026-07-04.**
[`src/usb_kbd.rs`](src/usb_kbd.rs) drives the ESP-IDF USB Host Library directly
through the raw `esp-idf-sys` bindings (no managed HID class driver), enumerates
an attached keyboard, claims the boot-keyboard interface, switches it to boot
protocol, and polls the interrupt-IN endpoint — decoding each 8-byte report into
modifiers + keycodes. Verified with a `19f5:3255` keyboard: keystrokes,
modifiers, and rollover all decode correctly.

Hardware: flash + serial over the CP2102 "UART" port (console = UART0,
independent of the USB PHY), keyboard on the native "USB" port. The keyboard
enumerated **bus-powered** — no external VBUS injection needed on this
DevKitC-1 v1.0 (keep a 5 V power cable only as a brownout fallback for
higher-power/RGB devices).

**Spike 2 — EPD: verified 2026-07-04.** The GDEY0579T93 e-paper panel is
driven through the thin dual-SSD1683 driver in [`src/epd.rs`](src/epd.rs)
(ported from GxEPD2's `GxEPD2_579_GDEY0579T93`). Verified on the bench rig over
4 MHz SPI:

- **2a — uniform fill:** clean full-panel white ↔ black refreshes, proving
  the wiring, both cascaded controllers, RAM addressing, and the full
  refresh waveform.
- **2b — graphics/text:** `epd::Frame` implements `embedded-graphics`'
  `DrawTarget`; a stroked circle straddling the master/slave seam (x = 396)
  renders round and continuous, and `FONT_10X20` text is legible — proving
  the split-and-mirror full-frame blit (`Epd::display_frame`).

Wiring: SCK 12 · DIN/MOSI 11 · CS 7 · DC 6 · RST 5 · BUSY 4, via the
DESPI-C579 breakout.

Every build is stamped by [`build.rs`](build.rs) with UTC time and
`git describe --always --dirty`; the tag is logged on serial at boot and
drawn on the panel, so the running build is always identifiable during
diagnosis.

Bring-up note: the initial symptom was per-pixel noise on the panel — a
half-seated CS jumper, not firmware. If the panel shows speckle/banding,
reseat the jumpers (CS first) before debugging code.

Next up per
[`docs/v0.1-mvp-technical.md`](../docs/v0.1-mvp-technical.md#hardware-bring-up-order):
Wi-Fi/TLS (Spike 6, implemented above), then gitoxide push (Spike 7); SD is
deferred.

**Spike 1 — Blink: verified 2026-07-04.** GPIO 2 + on-board WS2812 toggled
at 1 Hz with `blink N` on USB-serial, proving toolchain, esp-idf link, and
GPIO on real silicon. The blink code was replaced by Spike 2 in `main.rs`
(see git history: `e040a8d`).

## Quick commands

A [`justfile`](https://github.com/casey/just) wraps the common commands and
sources the espup env itself — run `just` in this directory for the list
(`build`, `flash`, `monitor`, `info`, `ports`).

## Build

Once per shell session, source the espup env (sets `LIBCLANG_PATH` and adds
the Xtensa GCC to `PATH`):

```sh
. ~/export-esp.sh
```

Then from this directory:

```sh
cargo build --release
```

The first build is slow (the esp-idf C sources are checked out and built
under `.embuild/`). Subsequent builds are incremental.

## Flash (when hardware is on the bench)

`cargo run --release` triggers `espflash flash --monitor` via the runner
configured in `.cargo/config.toml`. With the ESP32-S3-DevKitC-1 connected
over USB you should see:

```
[…] blink 0
[…] blink 1
[…] blink 2
…
```

at 1 Hz on the serial monitor, and — if an LED is wired from GPIO 2 → 330 Ω
→ GND — the LED blinks in lockstep.

## Pin choice

GPIO 2 is a safe general-purpose pin on the ESP32-S3-DevKitC-1: it's not
tied to a strapping function at boot and not muxed to the USB or PSRAM
peripherals. The blink loop also drives the on-board addressable LED —
WS2812 on GPIO 48 (GPIO 38 on DevKitC-1 v1.1 boards) — via the RMT
peripheral, so both a plain GPIO and the RMT path are exercised.

## Board pinout

The bench board follows the **ESP32-S3-DevKitC-1 v1.0** pinout — an
ESP32-S3-WROOM-1 **N16R8** module (16 MB flash, 8 MB octal PSRAM). The v1.0
revision wires the on-board WS2812 RGB LED to **GPIO 48**; v1.1 moved it to
GPIO 38, so match assignments against this diagram, not the v1.1 one.

![ESP32-S3-DevKitC-1 v1.0 pinout](docs/esp32-s3-devkitc-1-v1.0-pinout.jpg)

Source: [Espressif ESP32-S3-DevKitC-1 v1.0 user guide][devkitc-1-v1.0]. The
octal PSRAM consumes **GPIO 26–37**, so those are unavailable for peripherals.

[devkitc-1-v1.0]: https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32s3/esp32-s3-devkitc-1/user_guide_v1.0.html

## Editor / rust-analyzer

The repo-level `.zed/settings.json` configures `rust-analyzer` for this
crate:

- `cargo.target` is pinned to `xtensa-esp32s3-espidf` with
  `allTargets = false`, so RA doesn't try to also check the crate for the
  host target (which can't build `esp-idf-sys`).
- `binary.path` is pinned to the **rustup-managed** rust-analyzer
  (`stable` toolchain), not Zed's bundled one. Reason: recent Zed builds
  ship a rust-analyzer that calls `cargo metadata --lockfile-path`, which
  is still gated behind `-Z unstable-options` in cargo 1.95 and fails on
  both the `stable` and `esp` toolchains. The rustup-managed RA is
  version-locked to the cargo it ships with and avoids the flag.

If a contributor on a different machine has issues, regenerate the path:

```sh
rustup component add rust-analyzer --toolchain stable
rustup which rust-analyzer --toolchain stable
# put the printed path into .zed/settings.json under lsp.rust-analyzer.binary.path
```

Two things rust-analyzer still needs from the **environment Zed was
launched in**:

- `LIBCLANG_PATH` — required by `bindgen` inside `esp-idf-sys`.
- The Xtensa GCC on `PATH` — required by `embuild` during `cargo check`.

Both are set by `~/export-esp.sh`. The pragmatic workflow:

```sh
. ~/export-esp.sh
zed /Users/julien/jclab/typewriter   # or: open from this shell
```

If Zed is launched from Finder/Dock instead, rust-analyzer will report
`bindgen` errors on the first `esp-idf-sys` check. Close Zed, source the
env in a terminal, and relaunch from there.

## Toolchain pins

`rust-toolchain.toml` pins the channel to `esp` (installed by `espup
install`). Cargo.toml currently includes git `[patch.crates-io]` overrides
for `esp-idf-sys` / `esp-idf-hal` / `esp-idf-svc` (template default). These
follow master and may need pinning to released versions if a master commit
breaks the build.
