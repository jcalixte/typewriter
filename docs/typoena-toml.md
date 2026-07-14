# `.typoena.toml` — editor preferences

> The git-tracked file that controls how the editor behaves — auto-save,
> format-on-save, the line-number gutter, the boot file, and the panel theme. Hand-editable, or
> changed live from the `Cmd-P` palette (booleans flip; the theme and auto-sync
> interval rotate through preset options on **Enter**). Landed in **v0.5** (see
> [`macroplan.md`](macroplan.md)).
>
> **Not to be confused with `/sd/typoena.conf`** — that holds the device
> *secrets* (Wi-Fi, PAT, remote URL, commit author), is gitignored, and is never
> committed. `.typoena.toml` is *behaviour*, shared across devices; `typoena.conf`
> is *secrets*, per-device. See [v0.1 product](v0.1-mvp-product.md).

## Location

```
/sd/repo/.typoena.toml
```

It lives inside the Tracked repo (`/sd/repo`), so it is **committed and pushed**
like any note — which means the preferences **sync to every device** that clones
the repo. That is deliberate: your editor behaviour follows you. (A per-device
override for the one genuinely device-specific key, `auto_sync`, may layer on top
later via `typoena.conf` — worth it only once `auto_sync` actually does something
in v0.8. See the [auto_sync](#auto_sync) note.)

The file is read **once at boot**, before the first screen is drawn (so
`line_numbers` shapes the opening frame, and `open_last_on_boot` picks the file
that frame shows). A **missing, empty, or partial file is
fine** — every absent key falls back to its default below, so a fresh card just
works with no config present.

## Keys

| Key | Type | Default | Options | Effect |
| --- | --- | --- | --- | --- |
| `save_on_idle` | bool | `true` | `true` / `false` | Auto-save the current buffer on the idle typing-pause, so `:w` is optional. |
| `format_on_save` | bool | `true` | `true` / `false` | Run `:fmt` on the buffer before an explicit `:w`/`:sync`. |
| `line_numbers` | bool | `true` | `true` / `false` | Show the absolute line-number gutter. Off reclaims its columns for text. |
| `open_last_on_boot` | bool | `true` | `true` / `false` | Boot into the file that was active at power-off, instead of `notes.md`. |
| `theme` | string | `"light"` | `light` / `dark` | Panel colour polarity. `dark` inverts the whole frame to white-on-black. |
| `auto_sync` | string | `"10m"` | `2m` / `5m` / `10m` / `15m` / `30m` | Max-staleness cap for opportunistic auto-publish. **Value only — no behaviour yet** (rides v0.8, with the sleep work). |

The **Options** column is what the palette rotates through on **Enter**; a
boolean is just the two-option case. Hand-editing a string key can still set any
value — the palette only cycles the presets.

### Example

```toml
# Typoena editor preferences — hand-editable, git-tracked.
# Edit here, or change live from the Cmd-P palette (type `>`).
save_on_idle = true
format_on_save = true
line_numbers = true
open_last_on_boot = true
theme = "light"
auto_sync = "10m"
```

### `save_on_idle`

When on, the firmware quietly persists a dirty, named buffer once typing has
paused (~1.5 s), so a power pull can't cost more than the last couple of seconds
of writing. It is a **safety net, not an action**:

- **Silent.** No snackbar, no forced screen refresh. A visible confirmation on
  every pause would cost a ~630 ms e-ink flash purely to say "saved" — exactly
  the gratuitous flashing the panel avoids elsewhere. `:w` remains the *loud*
  save (it posts `saved`).
- **Unformatted.** The idle save never runs `:fmt` — see the
  [format_on_save](#format_on_save) note for why.
- Fires **once per typing burst**; a failed save doesn't retry-storm (it's kept
  in RAM and re-attempted on the next burst, or on `:w`).

### `format_on_save`

Runs `:fmt` — table alignment, blank-line collapse, trailing-whitespace strip —
on the buffer *before* it is persisted, so `:sync` is **fmt → save → commit →
push** and `:w` saves formatted.

**Formatting only happens on an explicit `:w`/`:sync`.** The `save_on_idle`
auto-save is deliberately left unformatted: if it reformatted on every idle
pause, tables would reflow and blank lines collapse *mid-session*, with the caret
jumping under you every time you paused to think. Formatting is a deliberate act;
the safety-net save is not.

### `line_numbers`

Shows the absolute line-number gutter (built always-on in v0.2). Turning it off
returns the gutter's columns to the text, so prose gets the full writing width.
Applied **live** — toggling it from the palette redraws immediately with (or
without) the gutter.

### `open_last_on_boot`

Boot into the file that was active when the device powered off, instead of the
default `notes.md` — power the typewriter back on and you are where you left
off, caret on the last character as always.

Only the **choice** lives in this file. The last-active *path* is device state,
not shared behaviour: the firmware keeps it in a device-local marker
(`/sd/.typoena-last`, beside the dirty journal at the card root), rewritten on
every buffer switch. It is deliberately **not** stored here — a tracked key
would dirty the repo on every file switch (riding each `:gp` as noise) and make
two devices fight over a single "last file".

The fallback is safe by construction: a missing marker, a garbled one (power
pull mid-write), or a marker naming a file that no longer exists (deleted here,
or removed by a `:gl` from another device) simply boots the default `notes.md`.
Turning the pref **off** doesn't stop the marker being recorded — it only stops
boot from reading it — so flipping it back on resumes correctly from the very
next boot.

### `theme`

Panel colour polarity: `light` (the native black-ink-on-white-paper) or `dark`
(white-on-black). On the 1-bit e-paper panel this is not a palette swap but a
**whole-frame invert** applied at the very end of the render, so text, selection,
caret, side panel and command palette all flip together and each stays legible.
Any value other than `dark` reads as light. Applied **live** — cycling it from
the palette repaints inverted at once.

> **On e-paper, `dark` is not free.** Partial refreshes over a mostly-black field
> ghost more than over white, and the panel is tuned for white-background reading.
> It works, but expect a slightly muddier refresh than `light` — verify on-device.

### `auto_sync`

A duration string that will one day cap how stale the published copy is allowed
to get — an *opportunistic, rate-limited* push, not a wall-clock timer. The
palette rotates it through the presets `2m` / `5m` / `10m` / `15m` / `30m`
(hand-editing can still set any string, e.g. `"0"`/empty to disable). **The value
is only stored and displayed in v0.5 — nothing reads it yet:** the periodic push
lands with the sleep work in v0.8 (originally pencilled on v0.7's git work;
v0.7 closed 2026-07-14 with manual `:gl`/`:gp` only), so
cycling the interval today changes what will be honoured *then*, not now.
Rationale for the `"10m"` default:
[`tradeoff-curves/wifi-auto-sync.md`](tradeoff-curves/wifi-auto-sync.md).

## Editing it

Two ways, both landing in the same file:

1. **By hand** — it's plain text on the card; edit it on your computer and reboot
   to apply. (The palette hides dotfiles, but you can still open it in-editor with
   `:e repo/.typoena.toml`.)
2. **Live, from the device** — open the settings list either way:
   - **`:settings`** — drops you straight into it, or
   - **`Cmd-P`** then type **`>`** — switches the file palette to the command
     list (VS Code semantics).

   Every pref appears carrying its current state:

   ```
   > save on idle: on
     format on save: on
     line numbers: on
     open last on boot: on
     theme: light
     auto sync: 10m
   ```

   `Ctrl-N`/`Ctrl-P` move the selection; **Enter** advances the selected pref to
   its next value, applies it at once, writes the change back to `.typoena.toml`,
   and confirms the new state on the snackbar (e.g. `theme: dark - saved`). A
   boolean flips; the theme and auto-sync interval **rotate through their preset
   options and wrap** — same key, so the palette is uniformly "press Enter to
   change". **The list stays open** so you can change several prefs in a row;
   **Esc** (or `Cmd-P`) closes it. Each change rides the next `:sync` to your
   other devices.

   `auto_sync` is a value command now, but has no behaviour to drive until v0.8 —
   cycling it sets the interval that the future periodic push will honour.

## Parsing

The reader is a deliberately tiny **line-based** parser, not a general TOML
library — the file is flat `key = value` pairs (a bool, or a quoted string) with
`#` comments, so a full TOML crate isn't worth pulling onto the firmware build.
It lives in the host-testable `editor` crate (`Prefs::parse` / `Prefs::to_toml`).
Rules:

- A `#` starts a comment to end of line (whole-line or trailing).
- Blank lines and lines without `=` are ignored.
- An **unrecognized key** is ignored; an **unparseable value** (e.g.
  `save_on_idle = yes`) leaves *that key* at its default rather than reading as
  `false`.
- Any key not present falls back to its default, so partial files are valid.

Because `Prefs::to_toml` round-trips with `Prefs::parse`, a palette edit rewrites
the whole file in canonical form (with the header comment) — hand-added comments
elsewhere in the file are not preserved across a palette toggle.

## See also

- [`macroplan.md`](macroplan.md) — v0.5 scope and the decisions behind these keys.
- [`v0.1-mvp-product.md`](v0.1-mvp-product.md) — the `typoena.conf` device secrets
  this file is kept separate from.
- [`tradeoff-curves/wifi-auto-sync.md`](tradeoff-curves/wifi-auto-sync.md) — why
  `auto_sync` defaults to 10 minutes.
