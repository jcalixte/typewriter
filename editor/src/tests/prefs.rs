//! Preferences (`.typoena.toml`) and the live gutter toggle.

use super::*;

// ---- Preferences (.typoena.toml) ----

#[test]
fn prefs_default_matches_the_documented_defaults() {
    let p = Prefs::default();
    assert!(p.save_on_idle);
    assert!(p.format_on_save);
    assert!(p.line_numbers);
    assert!(p.open_last_on_boot);
    assert_eq!(p.theme, "light");
    assert_eq!(p.auto_sync, "10m");
    assert_eq!(p.scroll_margin, 2);
    assert_eq!(p.timezone, ""); // empty -> host leaves the clock at UTC
}

#[test]
fn prefs_parse_reads_timezone_posix_string() {
    let p = Prefs::parse("timezone = \"CET-1CEST,M3.5.0,M10.5.0/3\"\n");
    assert_eq!(p.timezone, "CET-1CEST,M3.5.0,M10.5.0/3");
    // Missing key -> empty (UTC), never a bogus zone.
    assert_eq!(Prefs::parse("").timezone, "");
}

#[test]
fn prefs_parse_falls_back_to_defaults_for_missing_keys() {
    // Only one key present; the rest stay at their defaults.
    let p = Prefs::parse("line_numbers = false\n");
    assert!(!p.line_numbers);
    assert!(p.save_on_idle); // untouched -> default
    assert!(p.format_on_save);
    assert_eq!(p.auto_sync, "10m");
}

#[test]
fn prefs_parse_reads_all_keys_and_ignores_comments_and_junk() {
    let src = "\
        # a header comment\n\
        save_on_idle = false   # trailing comment\n\
        format_on_save = false\n\
        line_numbers = false\n\
        open_last_on_boot = false\n\
        auto_sync = \"2m\"\n\
        bogus_key = whatever\n\
        not a pair\n";
    let p = Prefs::parse(src);
    assert!(!p.save_on_idle);
    assert!(!p.format_on_save);
    assert!(!p.line_numbers);
    assert!(!p.open_last_on_boot);
    assert_eq!(p.auto_sync, "2m");
}

#[test]
fn prefs_parse_keeps_default_on_an_unparseable_bool() {
    // A typo in a bool value leaves that key at its default, not `false`.
    let p = Prefs::parse("save_on_idle = yes\n");
    assert!(p.save_on_idle); // "yes" isn't a TOML bool -> default (true)
}

#[test]
fn prefs_to_toml_round_trips_through_parse() {
    let p = Prefs {
        save_on_idle: false,
        format_on_save: true,
        line_numbers: false,
        open_last_on_boot: false,
        theme: "dark".into(),
        font: "jetbrains-mono".into(),
        auto_sync: "5m".into(),
        scroll_margin: 3,
        fast_partial: true,
        companion: false,
        timezone: "CET-1CEST,M3.5.0,M10.5.0/3".into(),
    };
    assert_eq!(Prefs::parse(&p.to_toml()), p);
}

#[test]
fn prefs_parse_reads_companion_and_defaults_on() {
    assert!(Prefs::default().companion); // Typo ships on; the pref is the opt-out
    assert!(!Prefs::parse("companion = false\n").companion);
    assert!(Prefs::parse("companion = true\n").companion);
    // A non-bool value leaves it at the (on) default rather than reading false.
    assert!(Prefs::parse("companion = off\n").companion);
}

#[test]
fn prefs_parse_reads_fast_partial_and_defaults_off() {
    assert!(!Prefs::default().fast_partial); // experimental — off unless asked
    assert!(Prefs::parse("fast_partial = true\n").fast_partial);
    assert!(!Prefs::parse("fast_partial = false\n").fast_partial);
    // A non-bool value leaves it at the (off) default rather than reading false.
    assert!(!Prefs::parse("fast_partial = sometimes\n").fast_partial);
}

#[test]
fn prefs_parse_reads_scroll_margin_and_keeps_default_on_junk() {
    assert_eq!(Prefs::parse("scroll_margin = 0\n").scroll_margin, 0);
    assert_eq!(Prefs::parse("scroll_margin = 4\n").scroll_margin, 4);
    // A non-numeric value leaves the key at its default rather than 0.
    assert_eq!(Prefs::parse("scroll_margin = lots\n").scroll_margin, 2);
}

#[test]
fn prefs_parse_reads_theme_and_auto_sync_strings() {
    let p = Prefs::parse("theme = \"dark\"\nauto_sync = \"15m\"\n");
    assert_eq!(p.theme, "dark");
    assert_eq!(p.auto_sync, "15m");
}

#[test]
fn prefs_parse_reads_font_and_defaults_to_builtin() {
    assert_eq!(Prefs::default().font, "default"); // built-in Misc Fixed
    assert_eq!(Prefs::parse("font = \"jetbrains-mono\"\n").font, "jetbrains-mono");
    // The font key is emitted by to_toml, so it survives a `:gp` to other devices.
    assert!(Prefs::default().to_toml().contains("font = \"default\""));
}

#[test]
fn empty_prefs_file_yields_defaults() {
    assert_eq!(Prefs::parse(""), Prefs::default());
}

// ---- line_numbers pref (live gutter toggle) ----

#[test]
fn line_numbers_off_reclaims_the_gutter_columns() {
    let mut e = Editor::with_text("one\ntwo\nthree".into());
    assert!(e.text_cols() < WRITE_COLS); // gutter present by default
    e.prefs.line_numbers = false;
    assert_eq!(e.gutter_cols(), 0);
    assert_eq!(e.text_cols(), WRITE_COLS); // full width reclaimed
}

#[test]
fn draw_with_line_numbers_off_does_not_panic() {
    // The `gutter - 1` field width would underflow if unguarded.
    let mut e = Editor::with_text("alpha\nbeta\ngamma".into());
    e.prefs.line_numbers = false;
    let _ = e.draw(true);
}
