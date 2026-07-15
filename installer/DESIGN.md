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
2. **Configure** — collect Wi-Fi SSID/pass, git remote, GitHub user, GitHub
   token, and commit identity. Pre-fill via the derive ladder (below); the
   token comes from **Sign in with GitHub** (^G, device flow) or a pasted PAT.
   The remote accepts shorthand — `you/notes`, `you/notes.git`,
   `github.com/you/notes`, and SSH pastes (`git@host:…`, `ssh://…`) all expand
   to the canonical HTTPS clone URL (`expand_remote_url`, live "→ will use …"
   hint under the field); the conf and the clone only ever see the expanded
   form, since the device's libgit2 is HTTPS-only.
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
- **Token never derived** — it comes from the ^G sign-in or is typed, never
  scraped from this Mac; plaintext on FAT means physical custody is the
  control. Device-flow tokens carry only the Typoena app's `contents:write`
  on repos the user granted; a pasted PAT should be fine-grained and scoped
  the same way. The clone uses the token so private notes repos work.

_Dropped vs. the old `just load` (now moot with clone-from-remote): rsync
mirror, `--ff-only` source refresh, `.gitignore` exclude list, `git repack -ad`._

## Config derive ladder (Configure step)

Each value: explicit input → derived from this Mac → prompt.
`author` ← `git config user.{name,email}` · `gh_user` ← `gh api user` ·
`ssid` ← live SSID if it can be read, else the top preferred network as a
**flagged guess** · `wifi_pass` ← Keychain (on ^K, may prompt macOS) ·
`remote` ← typed · `token` ← ^G Sign in with GitHub (device flow) or a typed
PAT (never derived).

**Wi-Fi SSID is best-effort, not authoritative.** On macOS 15+ (incl. Tahoe
26) `networksetup -getairportnetwork` reports "not associated" even when
connected, and `ipconfig`/`system_profiler` return `<redacted>` unless the
process holds Location Services permission — which a `curl | sh` binary won't.
So `config.rs` tries the real current SSID (dynamic Wi-Fi device via
`-listallhardwareports`, then `getairportnetwork`, then `ipconfig getsummary`),
and when all are blocked, falls back to the top of
`-listpreferredwirelessnetworks` as a guess. A guess sets `wifi_ssid_guessed`,
and the Configure step flags the field so the user confirms rather than trusts
it.

## Architecture / crates

- `ratatui` + its crossterm backend — TUI.
- `git2` — clone the remote onto the card, confirm origin. _[SD slice]_
- Config templates + snippet catalog embedded via `include_str!`
  (self-contained).

## Open items (not blocking the current slices)

- ~~**Hosting**~~ — RESOLVED: public GitHub release on `typoena/typewriter`
  (`installer-v0.1.0`; originally released under `jcalixte/typewriter`, repo
  transferred to the org 2026-07-15); `install.sh` pulls from
  `releases/latest/download`.
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
**UX pass (2026-07-15).** Applied across the wizard, no new slices:
- **Full keyboard nav, no arrows required** — `Tab`/`Shift-Tab` move forward/back
  through fields *and* steps (spilling at the ends), vim `h/j/k/l` on the
  non-form steps. Arrows still work.
- **Progress affordances in the sidebar** — steps show `✓` done / `▸` current /
  dim pending, plus a `move` box (`Tab next` · `⇧Tab back`) and a live gate hint
  (`fill required` / `write card first` / `→ <next>`), so "when/where can I go"
  is always visible.
- **Preflight hides the Mac's own storage** — the SD-card check reports only
  genuinely removable cards (via `diskutil`), never names `Macintosh HD`; a
  machine's own disk showing as "available" alarmed users.
- **Animated brand header** — a single block caret types the name `typoena`
  then continues into the site's tagline ("A distraction-free writing machine."),
  paced against a wall-clock `Instant` (not the render tick, so it's
  cadence-independent). The caret is solid while writing, blinks for 10 s once
  both lines are done, then settles. Both lines centred (reserved widths keep
  them from drifting as they fill), with a blank line of margin above and below.
- **Live clone progress bar** — `git clone --progress` is streamed by splitting
  on `\r`/`\n` (line-buffered reading swallows the in-place ticks) and parsed
  into a gauge (`Receiving objects  42%`); the scrolling log keeps only the
  phase-final `done.` lines.

4. **install.sh + release/hosting** — DONE 2026-07-14. Universal macOS binary
   (lipo arm64+x86_64, stripped) published as a public GitHub release on
   the firmware repo (now `typoena/typewriter`), tag `installer-v0.1.0`, with a
   `.sha256` sidecar.
   `typoena.dev/install.sh` (in the [[typoena-site]] repo): Darwin guard → curl
   binary + sidecar from `releases/latest/download` → `shasum -c` verify → `exec
   … </dev/tty`. Verified end-to-end (mirror, live release, full typoena.dev
   chain). The interactive TUI run + real card write/eject still await a TTY.
5. **Sign in with GitHub (device flow)** — DONE 2026-07-15 (`auth.rs`).
   Replaces hand-creating a PAT: `^G` on Configure asks GitHub for a one-time
   code (client_id `Iv23liwgnE86ITDpBdnn`, the org-owned "Typoena" GitHub App —
   public by design, no client secret in the device flow), shows it big,
   auto-opens github.com/login/device, and polls in the background
   (`authorization_pending`/`slow_down` honored, Esc cancels via an atomic
   flag) until the `ghu_` token lands in the GitHub-token field. The panel is
   modal on Configure (plain Esc/n/q cancel; ^N/^P are inert so a reflexive
   step-jump can't kill the flow). HTTP is system `curl` (no HTTP crate);
   GitHub's OAuth endpoints answer form-encoded, parsed by a tiny
   percent-decoding parser. `ghu_` tokens speak the same
   `x-access-token:<token>` basic auth as a PAT, so the clone path and the
   firmware are unchanged; manual PAT paste remains the fallback. If the app
   has token expiry on, the success message flags the token's lifetime.
   **Authorization ≠ installation** (hit in the field the same day): the token
   proves identity, but it can only reach repos the app is *installed* on — a
   clone of an uninstalled repo 403s (`Write access to repository not
   granted`). The SD step detects that stderr signature and fails with a hint
   pointing at `github.com/apps/typoena/installations/new` + Enter-to-retry
   (no new sign-in needed; token access is evaluated live).
6. **Proactive access check** — DONE 2026-07-15 (v0.4.0). Since the device
   flow never installs the app, *every* first-time ^G user would hit that 403
   — so the token is probed against `GET /repos/{owner}/{repo}` the moment
   sign-in completes. No access → a yellow flag on Configure with the fix;
   `^O` opens the install page and a background watcher re-probes (5 s, ~2 min)
   so the flag turns green the moment the repo is granted. (^I would be the
   mnemonic, but Ctrl-I *is* Tab on a terminal.) Verdicts remember the remote
   they judged — editing the field retires a stale flag. Advisory only:
   non-GitHub hosts and network failures stay silent, the SD step still runs,
   and slice 5's 403 hint remains the backstop.
