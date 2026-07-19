# Bench QC firmware — spec

A bring-up / go-no-go fixture firmware for the hand-soldered Typoena carrier PCB.
It exercises every connection the ESP32-S3 can reach and reports **OK / NOK** per
subsystem, so a freshly-assembled board is validated (or its bad joints located)
in one flash.

Written for the PCB migration: a **DevKitC-1 (WROOM-1 N16R8) mounted on a carrier
PCB**, with EPD, microSD, a **USB-C host port** for the keyboard, and a **LiPo +
charge IC** hand-soldered around it.

## What this is / is NOT

A firmware self-test only senses what the S3 can drive or read back. That is a
strong signal for **functional, per-subsystem OK/NOK** — a shorted SCK or a cold
CS joint makes the SD refuse to mount and the EPD BUSY handshake hang; a swapped
MOSI/MISO kills the round-trip. Those are unambiguous.

It is **not** an ICT / bed-of-nails replacement:

- No rail-voltage measurement (no ADC divider is planned on 5V/3V3) → **rails stay
  a 30-second multimeter check.**
- Battery voltage / charge current is not readable (no VBAT tap) → see the charger
  prerequisite below.
- Opens on a truly floating spare pin are only inferable via its internal pull.

## ⚠️ Hardware prerequisites (validate before trusting two of the tests)

These are wiring facts the firmware cannot compensate for. Confirm them at the
board, ideally with a multimeter *before first power-on*.

1. **USB-C host (keyboard) — sourced VBUS is the hard requirement, not CC Rp.** The
   S3 OTG exposes only **D+/D−**; it has **no CC pins**, so CC is never handled by
   the ESP32. Empirically, across every bring-up test the gating factor was
   **VBUS**, not CC: the USB-A breakout worked (VBUS hard-wired to the rail), a
   4-pin USB-C breakout worked (VBUS sourced from the board's 5V), and the devkit's
   native USB-C port stayed **dark** because its VBUS is an *input*, never sourced
   outward. The tested keyboard enumerates on VBUS alone (tolerates a floating CC).
   So on the PCB the non-negotiable is: **source 5V onto the C receptacle's VBUS**,
   and route D+/D− (both orientations, A6/B6 & A7/B7) to GPIO20/19. External
   **Rp (≈56 kΩ ×2 to 5V) is optional** here — add it only to support arbitrary,
   spec-strict USB-C keyboards; the current one does not need it.
   → Test #5 passes as long as VBUS is sourced (with the tested keyboard).
2. **Charger status (HW-373 / TP4056).** Firmware can test the charger only if its
   status output (`CHRG`, open-drain) is wired to a free GPIO — **GPIO21** (one
   wire, no resistor). On a bare HW-373 the `CHRG`/`STDBY` pins drive the on-board
   LEDs and are often **not broken out**; if there's no accessible pad, the
   battery/charge line is a **manual multimeter check** and test #7 reports `SKIP`.

   Power topology (confirmed): battery ↔ HW-373 B+/B−, and battery → **MT3608 boost
   set to 5V** → the board's 5V rail; both USB and battery power run through the
   MT3608. The boost resolves the LDO-brownout risk — the S3 always sees a stable
   5V regardless of cell voltage. Two residual caveats: **(a)** verify the MT3608
   output at ~5.0V (its trim-pot drifts; >5.5V stresses the devkit LDO); **(b)** the
   bare HW-373 (TP4056-type) is not a power-path controller — while charging, the
   MT3608 draws from the same B+, so charge-termination and the `CHRG` state read by
   test #7 are unreliable under active load. Fine for bring-up; revisit before ship.

## Target pin map (reused from the devkit, unchanged)

| Function | Bus / pin |
|---|---|
| EPD (SSD1683 / GDEY0579T93) | SPI2 — SCK 12, MOSI 11, CS 7, DC 6, RST 5, BUSY 4 |
| microSD | SPI3 — SCK 14, MOSI 15, MISO 13, CS 10 |
| USB-C keyboard | native PHY — D− 19, D+ 20, VBUS→5V, CC1/CC2 Rp |
| Status LED (WS2812) | GPIO 48 (devkit RGB) |
| Operator confirm | **BOOT button, GPIO 0** (input, pull-up, pressed = low) |
| Charger status (optional) | GPIO 21 |

Free for later: GPIO 1, 2, 8, 9 (ADC1), 16/17/18, 38, 39, 40, 41, 42, 47.
Off-limits: 26–37 (flash + octal PSRAM), 43/44 (console UART), 0/3/45/46 (strapping).

## Architecture

- New binary `firmware/src/bin/qc.rs`; recipe **`just qc`** (build + flash +
  monitor). No `git` feature — keeps the build light and fast.
- Reuses the proven drivers: `Epd`, `SdStorage`, `usb_kbd`, `NetService`.
- **Run to completion, never abort on first fault.** Each check is isolated (its
  `Result` is captured); a NOK is recorded and the suite continues, so one flash
  yields the whole fault matrix instead of fix-one-reflash-repeat.
- **Verdict model** per line: `OK` / `NOK` / `SKIP` (a dependency failed) /
  `CONFIRM?` (visual, resolved by the operator).

### Output — three layers

1. **Serial (UART0) — authoritative.** Full checklist + per-test diagnostics +
   timings. Always attached on the bench.
2. **WS2812 (GPIO 48) — coarse aggregate, survives a dead panel.** Amber = running,
   blue = waiting on operator, **green = all OK**, **red = ≥1 NOK**.
3. **EPD panel — checklist mirror,** painted only *after* test #2 passes (the panel
   is itself under test).

### Operator input (no dedicated buttons on the board)

Visual items (EPD pattern, LED colors) are confirmed with the **BOOT button**:
short press = OK, long press (>1 s) = NOK. A 30 s timeout leaves the item at
`CONFIRM?` (non-gating) logged as "not confirmed", so an unattended run still
completes and the aggregate reflects the auto-checks.

## Test suite

Ordered; each row lists the auto criteria and what a NOK points at.

| # | Check | Auto criteria | NOK → likely fault |
|---|---|---|---|
| 1 | **LED** | *(deferred)* drive R→G→B via RMT; then `CONFIRM?` — esp-idf-hal 0.46 replaced the RMT API, so this reports `SKIP` for now (serial + panel carry results) | LED net / GPIO 48 |
| 2 | **EPD handshake** | reset → BUSY toggles within timeout; `init()` ok; full refresh completes within the ~1.9 s BUSY budget | CS/DC/RST/BUSY/SCK/MOSI open or bridged; BUSY stuck high = RST or BUSY |
| 3 | **EPD pattern** | checkerboard + seam test (x=396) + text; `CONFIRM?` | pixel noise = CS; missing band = MOSI/SCK; bad seam = dual-controller |
| 4 | **SD** | mount (`format_if_mount_failed=false`); CMD59/CRC accepted; log negotiated kHz; write→read a blob byte-identical; MISO idle-high (internal pull-up is enough; log if low) | swap/open on 13/14/15/10; MISO low = pull-up |
| 5 | **USB-C keyboard** | install host lib; enumerate (log VID:PID, expect 19f5:3255); claim boot iface; SET_PROTOCOL(boot)+SET_IDLE(0); poll EP 0x81. Prompt "press a key" → decode. Then "flip the connector, press again" → re-enumerate | no enum = VBUS / **CC Rp** / D+/D−; one orientation only = D pairs or CC not bridged |
| 6 | **Wi-Fi** | scan → ≥1 AP found (log best RSSI); if creds present, associate + SNTP | antenna / RF |
| 7 | **Charger / battery** | if `CHRG`→GPIO 21: read state (open-drain, low = charging) and report; else `SKIP`. Manual step: unplug USB → device stays alive = battery+MT3608 path OK | status-pin joint; dead on unplug = boost / battery wiring |
| 8 | **GPIO short/open** | for each pin marked *isolated* in the expected-net table: drive it high, read all other isolated pins (input pull-down) → any unexpected follower = a bridge; then float + internal pull → read level (open only inferable via the pull). Bus pins skipped (covered functionally). Log "coupling-tested" vs "pull-tested only" honestly | solder bridge between adjacent nets |

## Expected-net table (fill from the schematic)

The short/open scan needs each GPIO classified. Bus pins are skipped (their
functional test covers them); only *isolated* pins are meaningfully scanned.

| GPIO | Class | Note |
|---|---|---|
| 4,5,6,7,11,12 | bus (EPD) | skip — covered by #2/#3 |
| 10,13,14,15 | bus (SD) | skip — covered by #4 |
| 19,20 | bus (USB) | skip — covered by #5 |
| 0 | button | BOOT, reads pull, low on press |
| 21 | charger | if wired |
| 48 | LED | skip — covered by #1 |
| 1,2,8,9,16,17,18,38,39,40,41,42,47 | isolated / spare | scan candidates — **confirm which are actually routed** |

## Build integration

**Implemented** at `firmware/src/bin/qc.rs` (light build — no `git`/`full`
feature, so no libgit2). Compiles clean for xtensa. Recipes:

```
just qc         # build + flash + monitor
just build-qc   # compile only (the offline build check)
just monitor-qc # serial monitor with decoded backtraces
```

Status: builds clean; **not yet hardware-verified** (the carrier PCB is still in
soldering). The verdict/report harness lives inline in `qc.rs`.

## Out of scope / limits

- Rail voltages (5V/3V3) — multimeter.
- Battery voltage / charge current — not sensed (no VBAT tap); firmware sees only
  `CHRG` if wired.
- Short/open scan: opens on genuinely floating pins are not reliably detectable.

## Rough effort

~1 day. Checks #2/#4/#5/#6 are lifts of proven spikes; new work is the verdict/
report harness, the LED + panel mirror, the BOOT-button confirm loop, the charger
GPIO read, and the short/open scanner.

## Open points / to clarify

- **USB-C keyboard runs with no CC resistor.** On the breadboard the keyboard is
  plugged straight into a 4-pin USB-C breakout with **no Rp/Rd on CC anywhere**, and
  it works. It enumerates because the keyboard tolerates a floating CC and only needs
  VBUS — but that is **not USB-C spec-compliant**, so a different or stricter keyboard
  may not come up on this board. Cheap USB-A→C adapters bake in a **56 kΩ Rp (CC→VBUS)**
  for exactly this; adding that one resistor (or a DNP footprint for it) on the PCB
  is cheap insurance if keyboard-independence ever matters.
  - **Confirmed (breadboard):** the keyboard enumerates *and* decodes keystrokes on
    the CC-less breakout — so on the PCB, sourcing VBUS is sufficient for this
    keyboard. The 56 kΩ Rp stays optional insurance for other keyboards.
- **Charger status pad.** Is `CHRG` accessible on the HW-373 to wire to GPIO21? If
  not, test #7 stays a manual multimeter check — decide whether to tack on a wire.
- **MT3608 setpoint.** Confirm the boost output is trimmed to ~5.0V (not drifted high;
  >5.5V stresses the devkit LDO).
- **TP4056 load-sharing.** Charge-termination is unreliable while the MT3608 draws
  from B+ — a shipping concern, not a bring-up blocker.
- **Expected-net table.** Fill the isolated/spare GPIO rows from the actual schematic
  before relying on the short/open scan (#8).
