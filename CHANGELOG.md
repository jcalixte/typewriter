# Typoena changelog

The Typoena firmware, release by release, newest first. Your device updates
itself over Wi-Fi — run `:update` to pull the latest build. The macOS setup
tool (the installer) is versioned separately and listed at the end.

_Generated from the commit history with [git-cliff](https://git-cliff.org)._

## Firmware


### [0.8.0] — 2026-07-21
#### Added
- **changelog:** Auto-generate CHANGELOG.md from commits with git-cliff
- **editor:** Add :pub/:publish to mark a note .pub.md
- **wizard:** Erase and dedicate a bring-your-own card on consent
- **firmware:** Fast partial waveform experiment (0x32 custom LUT)
- **firmware:** Swap in real Good Display GDEY0579T93 partial waveform
- **firmware:** Trim fast partial LUT — drop weakest tail phase per group
- **firmware:** Fast partial FR 0x04->0x08 — 420ms -> 266ms, solid black
- **firmware:** Keep-hot charge-pump experiment + re-scope validated comments
- **display:** Simplify boot splash to a lowercase wordmark
- **app:** Clear panel ghosting with scheduled full refreshes
- **editor:** Show friendlier filenames in the panel and palette
- **editor:** Open the command palette with Cmd+Shift+P
- **display:** Bake alternate mono font families into MonoFont atlases
- **editor:** Choose the writing font family from the settings palette
- **editor:** Group the side panel into file, sync and vim tiers

#### Changed
- **firmware:** Pin the file-walk to Core1 so it can't starve the UI

#### Fixed
- **build:** Compile firmware at opt-level 2 to dodge Xtensa miscompile

### [0.7.9] — 2026-07-19
#### Added
- **editor:** Add :about splash and name the active file in the panel
- **ota:** Name the running version in the up-to-date notice

### [0.7.7] — 2026-07-19
#### Added
- **reboot:** Add :reboot command with auto-save and restart screen
- **delete:** Add :d alias and a y/n confirmation guard
- **reboot:** Confirm :reboot and :setup with the y/n guard
- **wizard:** Require typing the repo name to confirm a repo switch
- **inbox:** Add :inbox and :oldest fleeting-note commands
- **palette:** Prefill folder and Tab-complete the new-file name
- **firmware:** Add demo + kbd bench bins on a shared panel engine
- **conf:** Complete a bare repo name with the GitHub user
- **sync:** Fold unpublished saves into a commit on :gl behind a confirm
- **firmware:** Over-the-air firmware update via :update

#### Fixed
- **firmware:** Fire unbaked-config warning on full builds

### [0.7.5] — 2026-07-17
#### Added
- **boot:** Async splash refresh + background palette file walk
- **editor:** Make / search case-insensitive
- **editor:** Smartcase + accent-folded search
- **editor:** Rename :sync to :gp
- **sync:** Skip media paths in the pull apply
- **editor:** Open the Cmd-P palette from every mode
- **editor:** Add open_last_on_boot pref with palette toggle
- **firmware:** Boot into the last-active file
- **editor:** Let a palette-query space match any separator
- **editor:** Continue blockquote markers on Enter
- **git-sync:** Rebase local work onto origin on :gl divergence
- **conf:** Extract the typoena.conf schema into a shared crate
- **firmware:** Read typoena.conf from the card at boot
- **wizard:** The onboarding wizard's step/field state machine
- **firmware:** First-boot wizard gate + Wi-Fi step on device
- **wizard:** Land the GitHub device-flow sign-in step
- **wizard,firmware:** On-device GitHub sign-in with a QR on the panel
- **wizard:** Show Wi-Fi password by default, Tab to hide
- **wizard:** Pick the Wi-Fi SSID from a device scan
- **wizard:** List the installation's repos on the device
- **wizard:** Shallow-clone the chosen repo on the device
- **wizard:** Add reset-mode menu for :setup re-entry
- **setup:** Wire :setup to reboot into the onboarding wizard
- **persistence:** Add FAT-safe recursive delete + factory_reset
- **setup:** Factory reset from the :setup reset menu
- **persistence:** Add wipe_repo for the :setup repo switch
- **setup:** Repo switch from the :setup reset menu
- **editor:** Cmd+S saves from any mode, dirty-guarded
- **editor:** Scroll_margin pref keeps caret context (vim scrolloff)
- **editor:** Rotate scroll_margin from the > palette
- **epd:** Add experimental partial-refresh temperature knob
- **focus:** Silent-timer Pomodoro with a masking rest curtain

#### Changed
- **palette:** Intern the file list into one PSRAM blob
- **sync:** Reuse the TLS session and skip the pull's needless fetch
- **epd:** Raise SPI clock to 20 MHz, close dead latency levers
- **epd:** Drop set_ram_area settle delay (was a full FreeRTOS tick)
- **boot:** Cut cold boot 4159→3239 ms (memtest off, 240 MHz)

#### Fixed
- **sync:** Apply the pull fast-forward as a tree diff, not checkout_tree
- **firmware:** Resume wizard from card state, not baked config
- **wizard:** Pre-spawn the clone worker before Wi-Fi to avoid ENOMEM
- **wizard:** Install TLS trust store and retry the clone cleanly
- **wizard:** Wrap the size-gate refusal so it doesn't clip off-panel
- No more panel going back and forth when typing

### [0.7.0] — 2026-07-13
#### Added
- Add glyphs
- **persistence:** Add mount_for_git with a larger open-file budget
- **sync:** Commit by splicing journaled dirty paths onto HEAD
- **palette:** Walk the card recursively for the file list
- **palette:** Show recents only until the query reaches two chars
- Wrap palette selection; fix push classing, file walk, card load
- **editor:** / forward search with n/N repeat (v0.7)
- **sync:** :gl pull — fetch + fast-forward only on the git thread

#### Changed
- **sync:** Instrument and benchmark commit-staging latency
- **git:** Cache emulated pack mmap to avoid re-reading the pack
- **sync:** Bench the index-free commit path and mmap cache
- **sync:** Enable FatFS fast seek for pack reads
- **sync:** Cache small pack windows and evict mmap cache on munmap
- **palette:** Trust dirent d_type instead of a per-entry stat

#### Fixed
- **sync:** Patch the mbedtls stream double-free that reset the chip
- **sync:** Move mbedTLS allocations to PSRAM
- **sync:** Survive push heap exhaustion and instrument the push path
- **sync:** Budget mmaps at 64KB/1.5MB, trace the push heap live

### [0.6.0] — 2026-07-12
#### Added
- **editor:** Add theme and auto_sync preset prefs with rotate-on-Enter
- **editor:** Add snippet library, tab-stop engine, and $ palette
- **editor:** Hint the matching snippet in the panel on typing pause
- **editor:** Generalise the > palette into a command registry
- **firmware:** Read .typoena.snippets.json at boot into set_snippets

#### Fixed
- **persistence:** Show a saved note's trailing newline as an empty line

### [0.5.0] — 2026-07-11
#### Added
- Add multi-file buffer foundation (v0.5 slice 1)
- **editor:** Add v0.5 file palette (Cmd-P) with fuzzy match
- **editor:** Add :enew and :delete with real git-staging (v0.5 slice 3)
- **persistence:** End saved files with a trailing newline
- **editor:** Add .typoena.toml prefs and palette settings (v0.5 slice 4)

### [0.1.0] — 2026-07-11
#### Added
- **firmware:** Blink on-board WS2812 alongside GPIO 2
- **firmware:** Drive GDEY0579T93 EPD via dual-SSD1683 driver
- **firmware:** Read keycodes from a USB HID boot keyboard
- **firmware:** Add partial refresh to the EPD driver
- **firmware:** Type on the e-paper panel with partial refresh
- **firmware:** Add vim text objects and the change operator
- **firmware:** Add Wi-Fi + TLS spike (Spike 6)
- **editor:** Use ISO-8859-15 font for accented glyphs
- **firmware:** Add SD/FAT spike (Spike 3)
- **firmware:** Enable octal PSRAM
- **firmware:** Vendor libgit2 v1.9.4 as a git submodule
- **firmware:** Add libgit2 esp-idf component (CMake + mbedTLS shims)
- **firmware:** Wire libgit2 component + bind git2 in system mode
- **firmware:** Add flash-FAT storage partition table
- **firmware:** Bake and document git push env vars
- **firmware:** Add on-device git push spike (git_push)
- **firmware:** Verify git-push TLS chain against embedded GitHub CAs
- **firmware:** Add persistent-clone git sync spike (git_sync)
- **firmware:** Add RECLONE_EACH_BOOT toggle to git_sync
- **editor:** Add Markdown affordances and a :fmt formatter
- **usb:** Track keyboard connection state
- **editor:** Split display into writing column and side panel
- **editor:** Put the mode indicator at the side-panel bottom-left
- **keymap:** Extract pure HID decode into host-testable crate
- **keymap:** Add US-International dead-key accent composer
- **wifi:** Retry association with backoff in shared connect helper
- **usb-kbd:** Compose dead-key accents before enqueuing keys
- **keymap:** Repurpose the physical Esc key to type backtick/tilde
- **editor:** Add :w/:sync commands as host save/publish effects
- **sd:** Reformat a fresh card on mount failure in the spike
- **sd:** Move the SD spike to its own SPI3 host
- **persistence:** Add SD mount + atomic save/load module
- **editor:** Add text() getter and with_text seed constructor
- **firmware:** Boot-load and save the note via persistence
- **firmware:** Gate :sync publish behind the git feature
- **editor:** Boot in Normal mode and add a panel snackbar
- **firmware:** Post loaded/saved snackbars to the panel
- **firmware:** Wire SD persistence + git publish into the editor
- **firmware:** Log per-phase :sync timing breakdown
- **keymap,editor:** Add Ctrl-d/u half-page scroll
- **editor:** Absolute line-number gutter
- **editor:** Edit the : command line with Ctrl-W / Cmd-Backspace
- **editor,firmware:** Add :gl fast-forward pull command
- **editor:** Format the buffer on save/sync (format_on_save)
- **keymap:** Add Ctrl-r redo intent
- **editor:** Add v0.3 editing — register, undo/redo, and dot-repeat
- **editor:** Add Visual mode (v/V) with y/d/c, move View to gr

#### Changed
- **firmware:** Window partial refresh to the edited rows
- **firmware:** Optimistic :sync — push first, reconcile only on reject

#### Fixed
- Save to local for the mvp 0.1
- **firmware:** Raise main task stack to 96 KB for libgit2 depth
- **libgit2:** Make the loose ODB persist on FATFS via POSIX shims
- **firmware:** Recover git_sync from a leftover clone dir
- **libgit2:** Create objects writable so FAT can delete them
- **firmware:** Delete repo dir via path-based recursion, not remove_dir_all
- **usb-kbd:** Quiesce in-flight transfer before free; guard hot-plug leaks
- **editor:** Make the buffer UTF-8-correct for accented input
- **sd:** Unlink target before f_rename to overwrite on FAT
- **sd:** Correct stale shared-bus log strings to SPI3
- **firmware:** Harden :sync publish — skip macOS sidecars, fast-forward first
- **firmware:** Keep the editor alive when a panel paint fails
- **editor:** Reveal the pasted block when it runs past the fold


## Installer

The macOS setup tool that prepares an SD card — install it with
`curl -fsSL https://typoena.dev/install.sh | sh`.


### [0.5.0] — 2026-07-19
#### Added
- Frame the header as the device's e-ink screen
- Add --wipe card-reformat TUI
- Make just wipe headless with --no-eject
- Complete a bare repo name against the GitHub user
- Fill the GitHub username from the ^G sign-in

### [0.4.0] — 2026-07-15
#### Added
- Proactive repo-access check at Configure

### [0.3.1] — 2026-07-15
#### Added
- Explain the app-install 403 on clone

### [0.3.0] — 2026-07-15
#### Added
- Sign in with GitHub replaces hand-created PATs
- Accept remote-URL shorthand

### [0.2.2] — 2026-07-15
#### Added
- Slow the header typewriter to 150ms per key

### [0.2.1] — 2026-07-15
#### Added
- Center the header and type both lines with one caret
- Jump a whole step with Ctrl-N / Ctrl-P

### [0.2.0] — 2026-07-15
#### Added
- Run blocking scans off the UI thread with a spinner
- Animate the typoena header with a typewriter intro

#### Fixed
- Stop subprocess output corrupting the TUI

### [0.1.1] — 2026-07-15
#### Added
- Improve wizard UX (nav, progress, wifi, cards)

### [0.1.0] — 2026-07-14
#### Added
- Self-contained macOS SD-card setup CLI

