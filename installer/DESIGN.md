# Typoena installer — design

A self-contained macOS CLI (ratatui TUI) that prepares an SD card so a
**pre-flashed** Typoena is ready to use the moment the card goes in. The public
entry point is the one-liner on typoena.dev:

    curl -fsSL https://typoena.dev/install.sh | sh

`install.sh` downloads this prebuilt binary; the binary does the rest. The user
needs **no repo checkout and no Rust toolchain** — just the card.

## Decisions (2026-07-14)

- **Self-contained end-user tool.** No `just`, no typewriter checkout. The
  binary bundles what it needs (config templates, snippet catalog). The proven
  `just` bash (`firmware/justfile`) is the *reference spec* for the safety
  behaviours, ported to Rust — not shelled out to.
- **The installer never flashes.** Devices ship **pre-flashed from
  manufacturing**; setup is SD-card-only. Firmware field updates
  (auto-update) are a **device/roadmap** concern, not the installer's — see
  [`docs/macroplan.md`](../docs/macroplan.md) (v1.x note).
- **The card's repo is a fresh `git clone` from the remote** (HTTPS + PAT),
  written straight onto the card. There is no local source clone to mirror, so
  none of the rsync machinery applies: **no `--ff-only` refresh, no `.gitignore`
  excludes, no repack** — a fresh clone already contains only tracked files and
  is a single pack, and its origin is already the HTTPS URL we cloned from.
- **Lives in `typewriter/installer/`**, tracked in the firmware repo so it
  versions in lockstep with the config templates + snippet catalog it ships.

## Phase pipeline (the wizard)

1. **Preflight** — a mounted card + git present. Advisory; warnings don't block.
2. **Configure** — collect Wi-Fi SSID/pass, git remote, GitHub user, PAT, and
   commit identity. Pre-fill via the derive ladder (below).
3. **SD card** — pick the card (refuse on ambiguity), `git clone` the remote
   onto `/repo`, seed `.typoena.toml` + snippets if absent, write
   `typoena.conf`, strip `._*`, eject.
4. **Done** — "put the card in your Typoena and power on."

## Safety behaviours to keep (from `firmware/justfile` — do NOT regress)

- **Card-ambiguity refusal** — never guess when >1 removable volume; a wrong
  guess lets a write hit the wrong disk. Refuse and ask.
- **`.typoena-dirty` guard** — refuse to overwrite a card that carries
  unpublished device edits; offer backup-and-discard.
- **AppleDouble `._*`** — `dot_clean` before eject; `._pack-*.idx` corrupts the
  pack scan (Mac git *and* device libgit2).
- **PAT never derived** — always typed; fine-grained, `contents:write` on the
  one repo; plaintext on FAT means physical custody is the control. The clone
  uses the PAT so private notes repos work.

_Dropped vs. the old `just load` (now moot with clone-from-remote): rsync
mirror, `--ff-only` source refresh, `.gitignore` exclude list, `git repack -ad`._

## Config derive ladder (Configure step)

Each value: explicit input → derived from this Mac → prompt.
`author` ← `git config user.{name,email}` · `gh_user` ← `gh api user` ·
`ssid` ← active Wi-Fi (`networksetup`) · `wifi_pass` ← Keychain (on ^K, may
prompt macOS) · `remote`, `pat` ← typed (PAT never derived).

## Architecture / crates

- `ratatui` + its crossterm backend — TUI.
- `git2` — clone the remote onto the card, confirm origin. _[SD slice]_
- Config templates + snippet catalog embedded via `include_str!`
  (self-contained).

## Open items (not blocking the current slices)

- ~~**Hosting**~~ — RESOLVED: public GitHub release on `jcalixte/typewriter`
  (`installer-v0.1.0`); `install.sh` pulls from `releases/latest/download`.
- **Non-macOS** — Linux/Windows later; slice work is macOS-first.
- **Clone target** — cloning ~hundreds of MB directly onto FAT via a reader;
  measure, and fall back to clone-to-temp-then-copy if it's too slow.
- **Re-provision** — DONE for the destructive case: an existing card is handled
  by an explicit **wipe-and-reclone** (`y`-confirmed screen showing origin +
  HEAD + unpublished-edit count; removes only `repo/` + the dirty journal, then
  clones fresh). Follow-ups: a config-only rewrite that rotates the PAT /
  switches Wi-Fi *without* recloning (like `just provision`), and backing up
  `.typoena-dirty` edits before wiping instead of only warning.

## Slice plan

1. **App shell + Preflight** — DONE 2026-07-14. Branded wizard; card + git
   detection; `--check` headless mode.
2. **Configure** — DONE 2026-07-14. Form + derive ladder, masked secrets,
   Keychain fill, required-field validation.
3. **SD card** — DONE 2026-07-14 (fresh-card path). Pick card (boot disk
   excluded) → `git clone` onto it (single pack, clean HTTPS origin) → seed
   `.typoena.toml` → write `typoena.conf` → strip `._*` → eject; the long clone
   runs on a worker thread streaming progress. Verified: card detection on real
   hardware and clone + seed + conf via `--list-cards` / `--dry-run-sd`. Full
   interactive run + real write/eject await a blank card + a TTY.
4. **install.sh + release/hosting** — DONE 2026-07-14. Universal macOS binary
   (lipo arm64+x86_64, stripped) published as a public GitHub release on
   `jcalixte/typewriter`, tag `installer-v0.1.0`, with a `.sha256` sidecar.
   `typoena.dev/install.sh` (in the [[typoena-site]] repo): Darwin guard → curl
   binary + sidecar from `releases/latest/download` → `shasum -c` verify → `exec
   … </dev/tty`. Verified end-to-end (mirror, live release, full typoena.dev
   chain). The interactive TUI run + real card write/eject still await a TTY.
