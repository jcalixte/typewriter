//! `.typoena.toml` preferences: parsing, serialisation, and option cycling.


/// The git-tracked preferences file. Read at boot and rewritten when a palette
/// `>` command changes a pref, so the setting survives a reboot and rides the
/// next `:gp` to every device that clones the repo. Deliberately **distinct**
/// from the gitignored `/sd/typoena.conf` device secrets (Wi-Fi / PAT / remote /
/// author, never committed — see v0.1): behaviour is shared, secrets are not.
pub const PREFS_PATH: &str = "/sd/repo/.typoena.toml";

/// Editor preferences, mirroring the git-tracked [`PREFS_PATH`] TOML. The host
/// reads the file at boot and applies it with [`Editor::set_prefs`]; the palette
/// `>` command mode toggles a pref live and queues an [`Effect::SavePrefs`] to
/// write the change back. Every key falls back to the [`Default`] below, so a
/// missing, empty, or partial file still yields a full, usable `Prefs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefs {
    /// Auto-save the active buffer on the idle typing-pause, so `:w` becomes
    /// optional. The idle save is **unformatted** — a safety net against power
    /// loss, not a formatting pass; `:fmt` only runs on an explicit `:w`/`:gp`
    /// (see [`format_on_save`](Prefs::format_on_save)) so text is never reflowed
    /// mid-session. Honoured by the host loop, not the core.
    pub save_on_idle: bool,
    /// Run `:fmt` (table alignment, blank-line collapse, trailing-whitespace
    /// strip) on the buffer before an explicit `:w`/`:gp` persist.
    pub format_on_save: bool,
    /// Show the absolute line-number gutter (built always-on in v0.2). Off
    /// reclaims the gutter's columns for text — applied live by [`gutter_cols`].
    pub line_numbers: bool,
    /// Boot into the file that was active when the device powered off, instead
    /// of the default note. Only the *choice* lives here; the last-active path
    /// itself is device state, kept in a device-local marker beside the dirty
    /// journal — never in this git-tracked file, where every buffer switch
    /// would dirty the repo and devices would fight over one "last file".
    /// Honoured by the host at boot (falling back to the default note when the
    /// marker is missing or stale), not the core.
    pub open_last_on_boot: bool,
    /// Panel colour polarity: `"light"` (native black-ink-on-white-paper) or
    /// `"dark"` (white-on-black). On the 1-bit panel this is a whole-frame invert
    /// applied at the end of [`draw`](Editor::draw) via [`Frame::invert`], so any
    /// value other than `"dark"` reads as light. The palette rotates it through
    /// [`THEME_OPTIONS`]; a hand-typed value still round-trips.
    pub theme: String,
    /// Max-staleness cap for opportunistic auto-push, as a duration string.
    /// The palette rotates it through [`AUTO_SYNC_OPTIONS`] (`"2m"`..`"30m"`);
    /// hand-editing can still set any string. **Persisted-but-inert in v0.5** —
    /// the periodic push that reads it rides v0.7/v0.8, so cycling it changes the
    /// stored/displayed value but triggers nothing yet.
    pub auto_sync: String,
    /// Rows of context [`adjust_scroll`](Editor::adjust_scroll) keeps above and
    /// below the caret (vim's `scrolloff`). `0` restores the old edge-triggered
    /// behaviour; `2` is the default for the 13-row panel. The margin collapses at
    /// the buffer's first and last line (no blank rows past the ends) and is capped
    /// at `(ROWS - 1) / 2` so it can never squeeze the caret out. Honoured in
    /// Normal/Insert/Visual; View-mode viewport nav is unaffected.
    pub scroll_margin: usize,
    /// The device timezone, as a **POSIX TZ string** (e.g. Paris:
    /// `CET-1CEST,M3.5.0,M10.5.0/3`). Applied at boot by the host
    /// (`setenv("TZ", …)` + `tzset()`), so `localtime_r` — and thus the `:inbox`
    /// note's dated filename/title — reads the local calendar day. **Not** an IANA
    /// name (`Europe/Paris`): ESP-IDF's newlib ships no zoneinfo database, so a
    /// bare zone name would silently stay UTC — the DST rule must be spelled out.
    /// Empty (the default) leaves the clock at UTC. Purely a host concern: the
    /// pure core never reads it, it just rides `.typoena.toml` to every device
    /// that clones the repo. The `>` palette doesn't cycle it (free-form, not a
    /// preset) — hand-edit it here.
    pub timezone: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            save_on_idle: true,
            format_on_save: true,
            line_numbers: true,
            open_last_on_boot: true,
            theme: "light".into(),
            auto_sync: "10m".into(),
            scroll_margin: 2,
            timezone: String::new(),
        }
    }
}

impl Prefs {
    /// Parse a [`PREFS_PATH`] file, falling back to [`Default`] for any missing or
    /// unrecognized key (so a partial or empty file still yields a full `Prefs`).
    /// A deliberately tiny line-based reader: these are flat `key = value` pairs
    /// (bool, or a quoted string) with `#` comments — not a general TOML parser,
    /// so it pulls no crate onto the xtensa build and stays host-testable here. An
    /// unparseable value for a key leaves that key at its default.
    pub fn parse(src: &str) -> Self {
        let mut p = Self::default();
        for line in src.lines() {
            // Strip a trailing/whole-line `#` comment, then split `key = value`.
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((key, val)) = line.split_once('=') else {
                continue;
            };
            let (key, val) = (key.trim(), val.trim());
            match key {
                "save_on_idle" => {
                    if let Some(b) = parse_bool(val) {
                        p.save_on_idle = b;
                    }
                }
                "format_on_save" => {
                    if let Some(b) = parse_bool(val) {
                        p.format_on_save = b;
                    }
                }
                "line_numbers" => {
                    if let Some(b) = parse_bool(val) {
                        p.line_numbers = b;
                    }
                }
                "open_last_on_boot" => {
                    if let Some(b) = parse_bool(val) {
                        p.open_last_on_boot = b;
                    }
                }
                "theme" => p.theme = val.trim_matches('"').to_string(),
                "auto_sync" => p.auto_sync = val.trim_matches('"').to_string(),
                "scroll_margin" => {
                    if let Ok(n) = val.parse::<usize>() {
                        p.scroll_margin = n;
                    }
                }
                "timezone" => p.timezone = val.trim_matches('"').to_string(),
                _ => {}
            }
        }
        p
    }

    /// Serialize back to the [`PREFS_PATH`] form, with a header comment pointing at
    /// both edit paths. Round-trips with [`parse`](Prefs::parse). Ends in a newline
    /// like a normal text file; `save_path`'s guarded final-newline write leaves it
    /// as exactly one.
    pub fn to_toml(&self) -> String {
        format!(
            "# Typoena editor preferences — hand-editable, git-tracked.\n\
             # Edit here, or change live from the Cmd-P palette (type `>`).\n\
             save_on_idle = {}\n\
             format_on_save = {}\n\
             line_numbers = {}\n\
             open_last_on_boot = {}\n\
             theme = \"{}\"\n\
             auto_sync = \"{}\"\n\
             scroll_margin = {}\n\
             # POSIX TZ (e.g. CET-1CEST,M3.5.0,M10.5.0/3); empty = UTC.\n\
             timezone = \"{}\"\n",
            self.save_on_idle,
            self.format_on_save,
            self.line_numbers,
            self.open_last_on_boot,
            self.theme,
            self.auto_sync,
            self.scroll_margin,
            self.timezone,
        )
    }
}

/// Parse a TOML boolean literal, or `None` for anything else (so a typo leaves
/// the key at its default rather than silently reading as `false`).
pub(crate) fn parse_bool(v: &str) -> Option<bool> {
    match v {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// The panel-polarity presets the palette rotates [`Prefs::theme`] through.
pub(crate) const THEME_OPTIONS: [&str; 2] = ["light", "dark"];

/// The auto-push intervals the palette rotates [`Prefs::auto_sync`] through.
/// Hand-editing the TOML can still set any duration string; these are just the
/// values the `>` palette cycles.
pub(crate) const AUTO_SYNC_OPTIONS: [&str; 5] = ["2m", "5m", "10m", "15m", "30m"];

/// The option after `current` in `options`, wrapping past the end — the
/// rotate-on-Enter for a preset string pref. A `current` that isn't in the list
/// (e.g. hand-typed into the TOML) snaps to the first option, so one Enter
/// always lands on a known value.
pub(crate) fn next_option<'a>(current: &str, options: &[&'a str]) -> &'a str {
    match options.iter().position(|&o| o == current) {
        Some(i) => options[(i + 1) % options.len()],
        None => options[0],
    }
}

/// The scroll-margin (scrolloff) values the palette rotates
/// [`Prefs::scroll_margin`] through. Hand-editing the TOML can set any value
/// (capped at render time to `(ROWS - 1) / 2`); these are just what the `>`
/// palette cycles.
pub(crate) const SCROLL_MARGIN_OPTIONS: [usize; 4] = [0, 1, 2, 3];

/// [`next_option`] for a numeric preset: the value after `current`, wrapping,
/// and snapping an off-list value to the head so one Enter always lands on a
/// known option.
pub(crate) fn next_usize_option(current: usize, options: &[usize]) -> usize {
    match options.iter().position(|&o| o == current) {
        Some(i) => options[(i + 1) % options.len()],
        None => options[0],
    }
}
