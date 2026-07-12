# `.typoena.snippets.json` — snippet library

> The git-tracked file that holds your trigger-driven text expansions for
> Markdown authoring. Hand-editable (and Zed-compatible, so you can paste your
> existing snippets straight in), synced across devices like your notes. Landed
> in **v0.6** (see [`macroplan.md`](macroplan.md)). The editing surfaces — inline
> Tab-expansion and the `$` palette — are specified in
> [`v0.6-markdown.md`](v0.6-markdown.md).
>
> **Three files, three concerns, don't confuse them.** `.typoena.snippets.json`
> is *content* (your templates). [`.typoena.toml`](typoena-toml.md) is *behaviour*
> (auto-save, gutter). `/sd/typoena.conf` is *secrets* (Wi-Fi, PAT), gitignored
> and never committed. The first two live in the repo and sync; the third is
> per-device.

## Location

```
/sd/repo/.typoena.snippets.json
```

It sits in the Tracked repo beside [`.typoena.toml`](typoena-toml.md), so it is
**committed and pushed** like any note and **syncs to every device** that clones
the repo. Your snippet library follows you. It is read **once at boot**; a
**missing, empty, or malformed file is fine** — you simply have no snippets, and
the editor runs unchanged.

## Format

Deliberately **Zed's snippet JSON shape**, so the contents of a Zed
`snippets/markdown.json` paste in unmodified:

```json
{
  "Markdown link": {
    "prefix": "link",
    "body": "[$1]($2)$0",
    "description": "Inline link"
  },
  "Book notes": {
    "prefix": "fiche",
    "body": ["# $1", "", "## $2 — $3", "", "## What the book is about", ""],
    "description": "Fiche de lecture"
  }
}
```

- The top-level key is the **display name** (what the `$` palette shows).
- `prefix` — the word that triggers inline Tab-expansion.
- `body` — a **string**, or an **array of lines** joined with `\n` (Zed's form;
  it sidesteps embedded-newline escaping and reads cleanly for multi-line
  templates).
- `description` — optional but recommended: the `$` palette fuzzy-matches it and
  shows it, so it is how you find a snippet you don't remember the prefix for.

### Tab stops

A body is literal text plus numbered stops:

- `$1 … $n` — empty stops the caret visits in order.
- `$0` — the final resting place (defaults to the end of the insertion if absent).
- `${n:label}` — **accepted, but the label is stripped** to a bare `$n`. The
  editor has no selection/overtype model, so a label would just be text to
  delete; on a device with no completion popup it could never be shown as a
  prompt anyway. The **headings and structure carry the template** — the labels
  were only hints. This is what lets a Zed file with `${1:Titre}` load as-is.
- **No dynamic or computed values** (no `date`, no `clipboard`). There is no RTC
  — the wall clock is valid only after Wi-Fi + SNTP, so a `date` snippet would
  stamp 1970 on a cold boot. A stop is empty or it is literal; nothing else.

## The two surfaces

Every snippet works both ways — there is **no hidden two-tier rule** where some
are "inline only" and some are "palette only". Inline Tab is the fast path you
reach for once a prefix is in muscle memory; the `$` palette is discovery.

### Inline Tab-expansion (Insert mode)

Type a prefix, press **Tab**. If the word immediately before the caret matches a
snippet prefix, it expands; otherwise Tab inserts spaces as it does today. (Tab
arrives as an ordinary character, so this is a check inside the Insert-mode
handler, alongside the existing list-continuation transform.)

On a **typing pause** — the same throttle as the word-count / cursor refresh, so
never a per-keystroke e-ink flash — if the word before the caret is a prefix, the
right side panel shows a quiet hint (`» Book notes`), on the row above the mode
line. The panel is ~15 columns, so the hint is the **snippet name**, not the whole
body; the full preview is what the `$` palette is for. (The marker is `»`, not a
tab glyph: the panel font is ISO-8859-15, which has no `↹`.)

### `$` palette (browse + insert)

Open the palette (`Cmd-P`) and type **`$`** — the same sigil mechanism as `>` for
commands. The query after the `$` fuzzy-matches name, prefix, and description;
`Ctrl-N`/`Ctrl-P` move the selection; **Enter inserts the body at the caret** and
starts the tab-stop session (dropping you into Insert at `$1`). The empty-palette
placeholder legends the sigils: `Go to file · > settings · $ snippets`.

## The tab-stop session

Identical whether the snippet was expanded inline or inserted from the palette:

- After insertion the caret lands on **`$1`** (or the end, if the body has no
  stops), in **Insert** mode.
- **Tab advances** to the next stop, **forward only** (no Shift-Tab). The last
  Tab lands on `$0` / the end and ends the session.
- Pending stop offsets sit **after the caret** and shift with the edits you make
  at each stop, so typing at `$1` keeps `$2 … $n` correctly placed.
- The session **auto-aborts** on Esc, a mode change, or a motion that leaves the
  stop range — after which the buffer is just text and Tab inserts spaces again.

## Parsing

The parse lives in the host-testable `editor` crate (`Snippets::parse`), using
`serde_json` — JSON string escapes (`\n`, `\"`, `\uXXXX`) are a foot-gun to
hand-roll, and `serde_json` is battle-tested; the editor crate is `std`, so it
compiles for xtensa via esp-idf. This is the **one new dependency** the feature
adds. The firmware reads the file at boot and hands the parsed list to
`Editor::set_snippets`, mirroring how `.typoena.toml` is read and applied via
`set_prefs`. A parse error is **non-fatal**: log it and boot with no snippets,
rather than refusing to start over a stray comma.

## Editing it

- **On your computer (the normal path).** It's plain JSON in your notes repo —
  edit it in your real editor, copy entries over from Zed, commit, and it reaches
  the device on the next clone/sync. This is deliberately where the heavy editing
  happens; the appliance is for writing, not for maintaining a JSON library.
- **First-time setup.** [`just init`](../firmware/README.md#provisioning-an-sd-card)
  seeds this file from a curated catalog — you pick which snippet groups you
  want and it writes the selected subset into `repo/.typoena.snippets.json`
  (committed on the device's first `:sync`). See
  [`v0.6-markdown.md`](v0.6-markdown.md) for the catalog.
- **On-device hand-edit — deferred.** The palette hides dotfiles, and `:e` was
  dropped in v0.6, so there is no in-editor path to this file yet. When one is
  wanted it returns as a discoverable `> edit snippets` command that opens the
  file directly, rather than resurrecting a general `:e`.

## See also

- [`v0.6-markdown.md`](v0.6-markdown.md) — the editing surfaces, the `$`/`>`
  palette model, and the setup-recipe snippet catalog.
- [`typoena-toml.md`](typoena-toml.md) — the sibling prefs file this is kept
  separate from, and the `>` command palette snippets share the surface with.
- [`macroplan.md`](macroplan.md) — v0.6 scope.
