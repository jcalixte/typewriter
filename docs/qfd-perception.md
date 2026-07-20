# Perception scores (guessed)

> Source of truth for the right-hand perception zone of the
> [House 1](qfd-house-1.md) diagram: re-score here first, then mirror
> the zone's TikZ coordinates there, same day. Hub: [`qfd.md`](qfd.md).

Five products on the 0–5 scale, scored against each WHAT. Reference
configurations: **reMarkable 2 + Type Folio**, **Freewrite Traveler**,
**Freewrite Smart Typewriter**, **Pomera DM250** (DM250 has a reflective
monochrome LCD, not e-ink, flagged in W1 / W8). The Typoena column is the
**shipped device as of 2026-07-16** (v0.1 delivered 2026-07-11, v0.5–v0.7
delivered 2026-07-12/14, v0.9 wizard slices 0–5a hardware-verified), rebased
on measured hardware results and lived use; the four competitors remain
single-rater guesses. Three Typoena moves since the 2026-07-11 rebase: W1
2 to 3 (the ~630 ms figure turned out to be the erase/caret tier: additive
typing rides the ~100–130 ms windowed-Y partial, projected, bench confirm
pending), W12 3 to 4 (v0.5 multi-file + Local scope shipped on device), and the
new W15 row (wizard + installer + install.sh one-liner).

Freewrite Traveler scores assume the
[Sailfish firmware](https://getfreewrite.com/blogs/writing-success/freewrite-sailfish-firmware)
(released 2025-11-19), which rewrote the OS in Rust, cut keystroke latency
40–100 %, and trimmed power draw −30 % typing / −50 % idle on both
Traveler and Smart Typewriter Gen 3. Three rows rescored upward as a
result: W1 Traveler 3 to 4 / Smart 2 to 3 (Smart's larger panel still trails
Traveler by one notch), W5 both 3 to 4 (boot accelerated, no pushed
number), W9 both 1 to 2 (Rust rewrite explicitly unblocked features that
JS could not carry; still closed so neither reaches reMarkable's
hackable-Linux 3).

| ID  | WHAT (truncated)                                  | Typoena | reM. | Frw.T | Frw.S | Pom. | Rationale (shortest defensible)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| --- | ------------------------------------------------- | :-----: | :--: | :---: | :---: | :--: | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| W1  | Sub-second response to typing                     |    3    |  1   |   4   |   3   |  5   | Typoena additive typing rides the windowed-Y partial, ~100–130 ms projected (2026-07-14 re-read; bench confirm pending), competitive with the Freewrites, but erase/caret events still pay the ~630 ms full-area partial, so 3 not 4; reMarkable e-ink visibly laggy on a typing-focused device, tested less responsive than Smart Typewriter, and latency is so load-bearing for W1 that it earns a 1 not a 2; both Freewrites post-Sailfish trimmed latency 40–100 % (Frw.T plausibly inside 200 ms; Frw.S still trails by one notch on larger panel); Pomera LCD ~zero. |
| W2  | Pushing is one deliberate action away          |    5    |  4   |   4   |   4   |  2   | `:gp` atomic (one command, splice → commit → push, rejected-push replay included, verified on device 2026-07-14); reMarkable + Freewrite cloud-sync is one-tap but not git; Pomera = USB/SD copy or QR transfer.                                                                                                                                                                                                                                                                                                                          |
| W3  | Pulling power never corrupts the file             |    4    |  4   |   2   |   2   |  2   | Typoena: atomic-rename + fsync, plus the dirty-path journal at `/sd/.typoena-dirty` making an interrupted Push power-pull-safe (2026-07-13); the actual power-pull test is still deferred to v0.9, so 4 not 5. reMarkable journals. Freewrite + Pomera: forum reports of corruption on yank.                                                                                                                                                                                                                                            |
| W4  | Provisioning never interrupts writing             |    5    |  2   |   2   |   2   |  5   | Typoena: config is read once at boot from `/sd/typoena.conf`; reconfiguration lives behind `:setup` (reboot → reset menu), never mid-session. reM/Frw need Wi-Fi + account. Pomera: literally none.                                                                                                                                                                                                                                                                                                                                       |
| W5  | Quick boot to a writing cursor                    |    4    |  3   |   4   |   4   |  5   | Typoena measured 4.26 s cold (2026-07-11). reMarkable cold-boots ~20 s (great from sleep). Both Freewrites accelerated post-Sailfish (no pushed number; were ~10–15 s e-ink wake). Pomera ~3 s.                                                                                                                                                                                                                                                                                                                                         |
| W6  | Long sessions without crash / lag / drift         |    4    |  3   |   4   |   4   |  5   | Typoena: 1 h soak attested 2026-07-11 (real use, no crash / lag / leak): one proven hour vs rivals' years, so 4 not 5. Freewrite famously stable (both variants). Pomera firmware is decades-mature.                                                                                                                                                                                                                                                                                                                                      |
| W7  | Nothing on the device competes with prose         |    5    |  2   |   5   |   5   |  5   | reMarkable has apps, menus, drawing, PDFs. Freewrite + Pomera are single-purpose; Typoena by design.                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| W8  | The UI never moves except when I move it          |    4    |  3   |   4   |   4   |  5   | reMarkable animates more; Typoena uses dirty-rects; Freewrites minimal motion; Pomera near-static LCD.                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| W9  | Codebase absorbs the planned roadmap              |    4    |  3   |   2   |   2   |  1   | Modular Rust Typoena; reMarkable is hackable Linux; both Freewrites carry Sailfish (Rust rewrite explicitly unblocked features JS could not carry) but closed; Pomera closed firmware.                                                                                                                                                                                                                                                                                                                                                     |
| W10 | I can repair or fork it with hobbyist tools       |    5    |  4   |   2   |   2   |  1   | Typoena: open BOM + ESP32. reMarkable: rooted Linux + community ROMs. Freewrite + Pomera: closed.                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| W11 | Multi-day battery life (v0.8 onward)              |    1    |  5   |   5   |   5   |  4   | Typoena v0.1 = wall-powered (battery deferred). reMarkable + both Freewrites legendary (~4 weeks; Sailfish trimmed −30 % typing / −50 % idle). Pomera ~24 h.                                                                                                                                                                                                                                                                                                                                                                               |
| W12 | Local-only files coexist with git scope           |    4    |  1   |   2   |   2   |  3   | Typoena: shipped in v0.5 (on-device 2026-07-12): `/sd/local` never pushes, palette walks both scopes; 4 not 5 while the scope model has one shipped week of lived use. reMarkable cloud-only. Freewrites have local + Postbox but no VCS. Pomera = pure local.                                                                                                                                                                                                                                                                         |
| W13 | Typography sets a writing-tool tone               |    3    |  5   |   2   |   2   |  2   | Typoena v0.1: single mono (serif option in v1.0). reMarkable: rich type rendering. Freewrite + Pomera: utilitarian.                                                                                                                                                                                                                                                                                                                                                                                                                        |
| W14 | I can carry the device and write away from a desk |    2    |  4   |   5   |   1   |  5   | Typoena still wall-powered (ADR-008): desk-bound until v0.8's battery, though the parametric case (`hardware/case/`, OpenSCAD) now exists. reMarkable + Type Folio bag-friendly with bulk. Freewrite Traveler is the form-factor reference (~1.6 lb, folds). Smart Typewriter ~5 lb, desk-bound. Pomera DM250 pocketable foldable.                                                                                                                                                                                                        |
| W16 | Any file / action / edit one motion away          |    5    |  2   |   2   |   2   |  3   | Typoena: fuzzy palette over ~1100 files (Cmd-P + 2 chars + Enter), modal editing grammar with counts, `>`/`:` command palette, one-line install: the seiton claim made scoreable, and self-scored on the product's own home turf (discount accordingly). reMarkable Type Folio: touch menus, no keyboard grammar. Freewrites: gloriously few destinations but shallow reach: folder switch plus arrow keys, editing mid-document is famously costly. Pomera: menu-driven file list, no modal grammar.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| W15 | First-time setup without developer tools          |    4    |  2   |   3   |   3   |  5   | Typoena: two verified paths: on-device wizard (Wi-Fi scan-pick, QR device-flow sign-in, repo pick + shallow clone; slices 0–5a on hardware 2026-07-16) and the `curl … \| sh` installer (checksum-verified, no toolchain); 4 not 5 while factory-reset/repo-switch are on-device-pending and repos > ~30 MB are refused. reMarkable needs account + companion app. Freewrites need a Postbox account. Pomera: no setup at all.                                                                                                            |

**Totals** (sum across 16 WHATs, no weighting): Typoena 62, Pomera 58,
Freewrite Traveler 52, reMarkable 48, Freewrite Smart Typewriter 47.
History: Typoena 52 to 51 at the 2026-07-11 measurement rebase (W6 +1 soak,
W1 −2 on the ~630 ms reading), then 51 to 57 at the 2026-07-16 rebase (W1 +1
once the ~630 ms figure was re-read as the erase tier, W12 +1 on shipped
v0.5, W15 4 new); every product gained its W15 row (Pomera +5, Traveler and
Smart +3, reMarkable +2). The 2026-07-17 W16 row (reach) added Typoena +5,
Pomera +3, the rest +2; the five sits on the dimension the product was
literally built around, so it is the least independent cell in the table.
Traveler pre-Sailfish 44; Smart pre-Sailfish 39;
reMarkable W1 dropped 3 to 2 to 1 across two rounds of author testing.
Typoena's lead over Pomera is four points (two of them from the
self-scored W16 row, so read it as the same two-point contest it was)
and still hinges on the same
dimensions: W14 (portability) and W1's erase tier are where the tethered
e-ink device loses ground; v0.8 (battery) and a faster erase/caret path are
what widen it. The "Pomera + Wi-Fi + git + hackable BOM" framing from
`README.md` holds, and W15 is now measurable product surface (wizard +
installer), not aspiration.

## Characteristic benchmarks (measured, not rated)

The 0–5 scores above are perception; where actual numbers exist they are
collected here. A number beats a rating, and a blank beats a guess, so the
competitor columns stay empty except where a pushed figure or an author
test exists (marked *(a)* when anecdotal). The sparseness is itself the
finding: Typoena's column is bench data, the market's is marketing copy.

| Characteristic                    | Typoena (measured)                          | reM.       | Frw.T | Frw.S | Pom.        |
| --------------------------------- | ------------------------------------------- | ---------- | ----- | ----- | ----------- |
| H1 type latency, additive typing  | ~100–130 ms projected (bench confirm owed)  |            |       |       |             |
| H1 erase / caret tier             | ~630 ms                                     |            |       |       |             |
| H4 boot (cold, to cursor)         | 4.26 s (2026-07-11)                         | ~20 s *(a)* |      |       | ~3 s *(a)*  |
| H5 endurance                      | ≥ 1 h attested (2026-07-11)                 |            |       |       |             |
| H7 Push (real repo, warm)      | ~19 s `:gp` (2026-07-14)                    | n/a (no git) | n/a | n/a   | n/a         |
| H16 onboarding (blank → cursor)   | unmeasured (≤ 10 min target)                |            |       |       | ~0 (no setup) |
| H17 reach (keystrokes to target)  | 4 to any file by construction (Cmd-P + 2-char query + Enter); session median unmeasured |  |  |  |             |

Post-Sailfish Freewrite latency ("cut 40–100 %") and Pomera's "LCD ~zero"
are real signals but not numbers: they stay in the rationale column above,
not here. When a competitor cell fills in, the corresponding perception
score should be re-checked against it.

## Caveats

- **Single-rater bias.** All sixteen rows are scored from the project
  author's POV. A reMarkable buyer would weight W11 (battery) at 10 and
  W12 (git) at 1, flipping the totals. [§1's segment table](qfd-house-1.md#who-is-voting--user-segments) (U1/U2) now
  makes this structural: the weights are U1-asserted, and the re-derive
  recipe is written down for when a second real segment appears.
- **Configuration matters.** Freewrite Smart Typewriter and Traveler are
  both tracked; they diverge most on W1 / W5 because of display tech
  (Smart's larger panel is slower to refresh). Traveler is still the
  more direct competitor on form factor.
- **W3 / W6 Freewrite scores are anecdotal.** Forum reports, not bench
  data. Treat the 2 / 4 as "we'd need to test this" rather than fact.
- **No price column.** Typoena-as-BOM is materially cheaper than the
  competitors but cost is not a WHAT in [§1](qfd-house-1.md#1-customer-requirements-the-whats), so it's absent here.
  Worth a row if a v0.x WHAT ever calls it out.

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
