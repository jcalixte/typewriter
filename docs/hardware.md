# Hardware — parts and rationale

Part choices for the bench build, moved here from the README. The display
medium decision (e-ink over LCD / memory LCD / OLED) is
[ADR-003](adr.md#adr-003-display-medium--e-ink-gdey0579t93-panel); power is
[ADR-008](adr.md#adr-008-mvp-power--wall-powered-battery-deferred-to-v08);
keyboard transport is
[ADR-009](adr.md#adr-009-keyboard-transport--usb-host-tinyusb).

| Part      | Choice                                                        | Why                                                                                                                                                                                                                                                                                                                                  |
| --------- | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| MCU       | **ESP32-S3-N16R8** (16 MB flash, 8 MB octal PSRAM)            | USB OTG host (for the keyboard), Wi-Fi, BLE, dual core @ 240 MHz, plenty of PSRAM for git pack data and screen buffer. Best-supported Rust target in the ESP family.                                                                                                                                                                 |
| Display   | **GDEY0579T93 + DESPI-C579 breakout** (5.79", 792×272, 1-bit) | Good Display panel matched with its own FPC breakout. Strip aspect (~2.9:1) — Freewrite-coded: ~13 lines, ~79 cols at the editor's 10px font. Tiny framebuffer (~27 KB) leaves PSRAM headroom. The DESPI-C579 is a passive level-shifter / FPC-to-header board, not an active controller — driven over plain SPI like any other epd. |
| Keyboard  | **Nuphy Air60/Halo65 wired USB-C**                            | ESP32-S3 acts as USB host via TinyUSB. BLE-HID is a fallback but contends with Wi-Fi for radio time during push.                                                                                                                                                                                                                     |
| Storage   | microSD over SPI                                              | Holds both the git working copy (`/sd/repo/`) **and** the local-only scratch space (`/sd/local/`). Internal flash is for firmware + config only.                                                                                                                                                                                     |
| Power     | **USB-C wall power for MVP**, 18650 + IP5306 in v0.8          | Measure power profile on real hardware before sizing the battery. E-ink + sleep should give multi-day battery life but battery introduces charging, safety, and BMS complexity we don't need on day one.                                                                                                                             |
| Enclosure | 3D-printed typewriter body — [`hardware/case/`](../hardware/case/)                                        | v1.0 concern.                                                                                                                                                                                                                                                                                                                        |

## Why the strip aspect

The ~2.9:1 long-narrow shape biases the UX toward "current line + recent
context" rather than "full page" — the writing posture we want. The renderer
stays resolution-agnostic so a 10.3" e-ink upgrade (v1.x) is a swap, not a
rewrite.

## Bench status

The board is on the bench and bring-up is largely done — per-spike results
live in [`spikes.md`](spikes.md) and
[`v0.1-mvp-technical.md`](v0.1-mvp-technical.md#hardware-bring-up-order),
with failure write-ups in [`postmortems/`](postmortems/README.md). Notable: the
keyboard runs bus-powered on the S3's native USB port, and SD wiring is
proven on shared SPI2 but blocked on a compatible ≤32 GB card
([postmortem](postmortems/2026-07-05-spike3-sd-cmd59.md)).
