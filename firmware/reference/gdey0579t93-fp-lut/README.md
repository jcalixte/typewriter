# Good Display GDEY0579T93 reference driver

Vendor reference driver for the **GDEY0579T93** (5.79", 792×272, dual SSD1683
master/slave cascade) e-paper panel. Supplied by Good Display support by email
(received 2026-07-21), from the archive `S-GDEY0579T93-FP(LUT)-20250814.rar`.

This is kept as the **source of truth for the fast/partial waveform** — the custom
`0x32` LUT this panel actually needs, which we could not derive from the OTP-only
public demos. See `../../src/drivers/screen_epd.rs` (`FAST_PARTIAL_LUT`).

## The waveforms (in `Display_EPD_W21.c`)

Three panel-specific 233-byte LUT arrays. Byte layout: 227 bytes of phase table
(written to `0x32`) + 6 config bytes (EOPT `0x3F`, VGH `0x03`, VSH1/VSH2/VSL `0x04`,
VCOM `0x2C`).

| Array           | Purpose        | Trigger (`0x22`) |
| --------------- | -------------- | ---------------- |
| `LUT_DATA`      | full refresh   | —                |
| `LUT_DATA1`     | fast full      | `0xC7`           |
| `LUT_DATA_part` | **partial**    | `0xCF`           |

Our per-keystroke windowed path triggers with `0xCF`, so **`LUT_DATA_part`** is the
one we integrated. The per-refresh recipe is `Epaper_Partial()` in this file;
`main.c` shows the overall call ordering (init → base-map → partial loop).

## What was kept / dropped

The original `.rar` was a full Keil STM32F103 demo project (~14 MB). Only the EPD
driver is reference-relevant, so this folder keeps just:

- `Display_EPD_W21.c` / `.h` — the LUTs + init/refresh recipes (the payload)
- `Display_EPD_W21_spi.c` / `.h` — bit-banged SPI cmd/data layer
- `main.c` — the demo's call sequence, for ordering reference only

Dropped: the STM32 harness (`STM32F10x_FWLib/`, `CORE/`, `SYSTEM/`, fonts, GUI),
all `OBJ/` build artifacts, and `Ap_29demo.h` (608 KB of demo image bitmaps).
Because that image header is gone, `main.c`'s `#include "Ap_29demo.h"` and its
`gImage_*` references dangle — expected; **`main.c` is not compiled here**, it's
read for the call ordering only.

## Notes

- `Display_EPD_W21.c` was converted **GBK → UTF-8** (it has Chinese comments); the
  other files were ASCII and are verbatim.
- Original platform: STM32F103 + Keil, single controller. Ours: ESP32-S3 driving
  the two cascaded SSD1683s, so every command is issued twice (`cmd` and `cmd|0x80`).
