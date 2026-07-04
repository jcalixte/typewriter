# Typoena firmware

Rust crate targeting `xtensa-esp32s3-espidf`. See the project root
[`README.md`](../README.md) and
[`docs/v0.1-mvp-technical.md`](../docs/v0.1-mvp-technical.md) for the wider
context.

## Current state

**Spike 1 — Blink.** Toggles GPIO 2 every 500 ms and logs `blink N` to the
USB-serial console. This proves three things only:

1. The Espressif Rust toolchain (Xtensa) is installed and on PATH.
2. The crate links against `esp-idf-svc` and compiles for
   `xtensa-esp32s3-espidf`.
3. Basic GPIO output works on real silicon (verified post-flash, once the
   board is on the bench).

Everything past that — EPD, SD, USB host, partial refresh, Wi-Fi/TLS,
gitoxide push — is its own follow-up spike per
[`docs/v0.1-mvp-technical.md`](../docs/v0.1-mvp-technical.md#hardware-bring-up-order).

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
peripherals. If you want to drive the on-board addressable LED instead,
that's WS2812 on GPIO 48 and needs a different driver — out of scope for
Spike 1.

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
