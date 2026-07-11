# `.typoena.toml` — editor preferences

> The git-tracked file that controls how the editor behaves — auto-save,
> format-on-save, and the line-number gutter. Hand-editable, or toggled live
> from the `Cmd-P` palette. Landed in **v0.5** (see
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
later via `typoena.conf` — deferred until `auto_sync` actually does something in
v0.7. See the [auto_sync](#auto_sync) note.)

The file is read **once at boot**, before the first screen is drawn (so
`line_numbers` shapes the opening frame). A **missing, empty, or partial file is
fine** — every absent key falls back to its default below, so a fresh card just
works with no config present.

## Keys

| Key | Type | Default | Effect |
| --- | --- | --- | --- |
| `save_on_idle` | bool | `true` | Auto-save the current buffer on the idle typing-pause, so `:w` is optional. |
| `format_on_save` | bool | `true` | Run `:fmt` on the buffer before an explicit `:w`/`:sync`. |
| `line_numbers` | bool | `true` | Show the absolute line-number gutter. Off reclaims its columns for text. |
| `auto_sync` | string | `"10m"` | Max-staleness cap for opportunistic auto-publish. **Schema only in v0.5 — no behaviour yet.** |

### Example

```toml
# Typoena editor preferences — hand-editable, git-tracked.
# Edit here, or toggle live from the Cmd-P palette (type `>`).
save_on_idle = true
format_on_save = true
line_numbers = true
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

### `auto_sync`

A duration string (`"10m"`, `"2m"`, `"0"`/empty to disable) that will one day cap
how stale the published copy is allowed to get — an *opportunistic, rate-limited*
push, not a wall-clock timer. **In v0.5 this is schema + default only:** the value
is parsed, preserved through a round-trip, and shown nowhere editable — **nothing
reads it yet.** The periodic push itself rides the better-git work in v0.7 and
must interact with sleep in v0.8. Rationale for the `"10m"` default:
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

   The three boolean prefs appear as toggles carrying their current state:

   ```
   > save on idle: on
     format on save: on
     line numbers: on
   ```

   `Ctrl-N`/`Ctrl-P` move the selection; **Enter** flips the selected pref,
   applies it at once, writes the change back to `.typoena.toml`, and confirms
   the new state on the snackbar (e.g. `line numbers: off - saved`). **The list
   stays open** so you can flip several prefs in a row; **Esc** (or `Cmd-P`)
   closes it. Each change rides the next `:sync` to your other devices.

   `auto_sync` is **not** a palette command in v0.5 (it has no behaviour to
   drive yet); it returns as a value command in v0.7.

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
