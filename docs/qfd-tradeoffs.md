# 7. Tradeoffs and their why, linked to ADRs

Plain-language summary of what we accepted in exchange for what.
T-IDs are referenced from the [§5 cascade tree](qfd-house-2.md#the-cascade--what--function--how--components) and the tension list
below.

| ID  | Tradeoff                                        | Got                                                                                                  | Paid                                                                                                                                                  | ADR       |
| --- | ----------------------------------------------- | ---------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | --------- |
| T1  | std (esp-idf-rs) over no_std (esp-hal)          | Heap, threads, VFS, mbedtls, room for a full git stack (proved out by libgit2)                       | +1 MB binary, +5–10 min builds                                                                                                                        | [ADR-001] |
| T2  | Custom widget layer over Ratatui                | Dirty-rects aligned to e-ink regions; 200 KB binary back                                             | 500 LoC we own and maintain                                                                                                                           | [ADR-002] |
| T3  | e-ink medium over FSTN / memory LCD / OLED      | Paper aesthetic; 0 W idle persistence; medium enforces writing posture                               | ~200–300 ms typing latency; periodic full-refresh flash (scroll worst-case)                                                                           | [ADR-003] |
| T4  | `libgit2` (`git2`) over `gitoxide`: the [ADR-004] kill-switch, fired 2026-07-06 | Working HTTPS push on-device; mature pack/transport code riding ESP-IDF's mbedTLS                    | FFI + a C build (esp-idf CMake component); two vendored C deltas to maintain (`esp_mbedtls_stream.c` double-free fix + TLS session resumption); an mmap profile that needed hard caps (mwindow, odb) | [ADR-004] |
| T5  | HTTPS + GitHub token over SSH                   | Simplest auth the device transport supports; App device-flow tokens (`ghu_`) ride the same header as a PAT, so wizard/installer sign-in changed nothing in the git path | Long-lived secret on device, now **plaintext in `/sd/typoena.conf`** (both provisioning paths write it; physical custody of the card is the control); encrypted-at-rest is the open [ADR-011]      | [ADR-005], [ADR-011] |
| T6  | `std::thread` over `embassy` or `tokio`         | Boring, debuggable, real stack traces; no exec to tune                                               | ~76 KB total stack across 5 tasks                                                                                                                     | [ADR-006] |
| T7  | FAT-on-SD + LittleFS-on-flash split             | Desktop can read SD; config survives SD reformat                                                     | Two filesystems to manage; FAT's power-loss weakness mitigated by atomic-rename                                                                       | [ADR-007] |
| T8  | Wall power for v0.1, battery deferred           | Measure real draw before sizing the cell                                                             | Tethered MVP; not the final aesthetic                                                                                                                 | [ADR-008] |
| T9  | USB host (TinyUSB) over BLE-HID                 | No radio contention with Wi-Fi during push; keyboard powered from the device                         | One more USB connector on enclosure                                                                                                                   | [ADR-009] |
| T10 | Atomic Push (`:gp`, was `Ctrl-G`) + auto-timestamp commit message | One action, one outcome; matches the user's existing `gct` workflow; no modal prompt to slow H1 latency | Commit history is timestamp noise; the device authors replay commits the user never sees; reversal would break muscle memory                          | [ADR-010] |
| T11 | Splice commit over full index write             | Real-repo Push exists at all: ~19–24 s vs 611 s / OOM on the index path; dirty-path journal makes it power-pull-safe | Desktop-side edits to the card are never committed by the device; hand-edits on a computer must be pushed from that computer                         | [sync-commit-staging](tradeoff-curves/sync-commit-staging.md) |
| T12 | Media stays in git, never on the card           | Killed `:gl`'s last OOM path; pull/apply touches text only; the repo stays whole for remote readers | Stale card media; phantom `git status` noise if the card is mounted on a computer; never hand-commit from the card                                   | (2026-07-14, `is_media_path`) |
| T13 | Shallow clone + ~30 MB repo gate at onboarding  | First-run clone fits device memory and minutes-scale patience                                        | Repos over the gate are refused at the repo-pick step (libgit2 has no partial clone, so tip media would download even if never written)               | [wizard](v0.9-onboarding-wizard.md) |
| T14 | Installer provisions the card, never flashes    | No USB-flash toolchain in the user path; devices ship pre-flashed; installer stays a small TUI      | Field firmware updates cannot lean on the installer: auto-update becomes a device-side problem (open, macroplan v1.x)                                | [installer/DESIGN.md](../installer/DESIGN.md) |
| T15 | `curl … \| sh` one-liner over app-store/dmg     | Zero-friction start from typoena.dev; checksum-verified download; quarantine handled                | Pipe-to-shell trust ask; macOS-only today; the site and the GitHub release become launch-path infrastructure to keep up                               | (site repo `install.sh`) |

### Conflicts left explicitly unresolved by v0.1

These are the live tensions we are watching, not deciding harder. Each
carries the trigger that would force the decision: a tension without a
trigger is a decision being avoided, not deferred.

- **FAT loose-object cost vs H7's v1.0 target** (falls out of T11). The
  convicted residual of Push latency is FAT's linear directory scans
  (~0.4 s per loose write against the 256-dir `objects/` fan-out), bounded
  at ≤ ~2 s per commit and **accepted** for now; the lever is pack-not-loose
  writes. Until then the ≤ 10 s v1.0 H7 target is not honest for deep
  paths. **Trigger to revisit:** a v1.0 planning pass that keeps the ≤ 10 s
  target, or warm root-level `:gp` regressing past ~15 s.
- **Keep-alive race vs H6.** Run 8's push died on a connection idled out
  during a long marking gap; repack shrank the gap so run 9 succeeded:
  the race is *avoided*, not fixed. Durable fix = reconnect-on-stale in the
  http layer. **Trigger to revisit:** any recurrence of the run-8 signature
  (`SSL Generic error` mid-push), or before v1.0 claims ≥ 99 %.
- **Token at rest ([ADR-011], open, the Paid side of T5).** Both
  provisioning paths write the GitHub token plaintext to
  `/sd/typoena.conf`; physical custody of the card is the only control.
  Encrypted-at-rest (C15's eFuse key, C11) stays designed-but-unbuilt.
  **Trigger to revisit:** the device or card starts leaving the home, a
  second user's token lands on a card, or a token broader than the App's
  `contents:write` scope is ever provisioned.
- **Onboarding reach vs simplicity** (T13, T15). The wizard types Wi-Fi
  passwords on the device and the installer is macOS-only; the SoftAP
  companion webapp (phone-driven hand-off) was chosen over BLE 2026-07-16
  and **deferred**. **Trigger to revisit:** a real first-time user blocked
  by either path: no Mac for the installer, or defeated by on-device
  password entry.
- **[ADR-007] vs H8** (T7). Power loss between FAT rename and dir flush
  yields the previous saved version. We document this as expected behavior.
  **Trigger to revisit:** soak or power-pull testing showing it trigger on
  a routine save: then it is a bug, not a documented behavior.
- **W13 typography paths.** v0.1 ships one mono font; v1.0's
  writing-tool-tone outcome admits two paths (mono = developer comfort,
  serif = typewriter feel). Not yet decided whether to ship both or one.
  Cost preview per added font: +H9 glyph-cache footprint, +H10 binary for
  embedded assets. **Trigger to revisit:** the v1.0 design pass opening, or
  a serif asset being proposed for any earlier release, whichever first.
- **[ADR-008] vs W11+W14** (T8). Wall power in v0.1 is now an explicit
  disappointment of two WHATs, not one (battery W11 + portability W14).
  The disappointment is bounded by [ADR-008]'s commitment to measure
  current draw on real hardware before sizing v0.8's cell: spec the
  cell against measured numbers, not against the spec sheet. The [§3](qfd-house-1.md#3-house-of-quality--whats--hows)
  promotion of H13 (current draw) from #11 to #7 is the matrix
  registering this. **Trigger to revisit:** bench multimeter numbers
  landing (H13's "measured only" fulfilled): that starts v0.8 cell
  sizing.


[ADR-001]: adr.md#adr-001-language-and-runtime--rust-on-esp-idf-rs-std
[ADR-002]: adr.md#adr-002-ui-strategy--custom-widgets-on-embedded-graphics-not-ratatui
[ADR-003]: adr.md#adr-003-display-medium--e-ink-gdey0579t93-panel
[ADR-004]: adr.md#adr-004-git-implementation--gitoxide-gix
[ADR-005]: adr.md#adr-005-auth--https--github-personal-access-token
[ADR-006]: adr.md#adr-006-concurrency--stdthread--channels-no-async-runtime
[ADR-007]: adr.md#adr-007-storage-split--fat-on-sd-for-working-copy-littlefs-on-flash-for-config
[ADR-008]: adr.md#adr-008-mvp-power--wall-powered-battery-deferred-to-v08
[ADR-009]: adr.md#adr-009-keyboard-transport--usb-host-tinyusb
[ADR-010]: adr.md#adr-010-push-ux--atomic-ctrl-g-auto-timestamp-commit-message-no-user-prompt
[ADR-011]: adr.md#adr-011-credential-provisioning--how-the-pat-reaches-the-device-and-is-protected-at-rest
[ADR-012]: adr.md#adr-012-sd-on-its-own-spi3-host-not-shared-with-the-epd-on-spi2
