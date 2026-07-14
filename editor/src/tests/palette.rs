//! The file palette (Ctrl-P) and the `>` command palette.

use super::*;

#[test]
fn fuzzy_score_matches_subsequence_case_insensitively() {
    assert!(fuzzy_score("notes", "repo/notes.md").is_some());
    assert!(fuzzy_score("NOTES", "repo/notes.md").is_some()); // case-insensitive
    assert!(fuzzy_score("rpnm", "repo/notes.md").is_some()); // scattered subsequence
    assert!(fuzzy_score("xyz", "repo/notes.md").is_none()); // not a subsequence
    assert_eq!(fuzzy_score("", "anything"), Some(0)); // empty query matches all
}

#[test]
fn fuzzy_score_space_matches_any_separator() {
    assert!(fuzzy_score("la conv", "repo/la-convergence.md").is_some()); // space finds '-'
    assert!(fuzzy_score("notes md", "repo/notes.md").is_some()); // space finds '.'
    assert!(fuzzy_score("repo notes", "repo/notes.md").is_some()); // space finds '/'
    assert!(fuzzy_score("la conv", "repo/laconvergence.md").is_none()); // still needs a separator
}

#[test]
fn fuzzy_score_ranks_word_boundaries_above_midword() {
    // "no" after the "/" boundary in repo/notes beats a mid-word hit.
    let boundary = fuzzy_score("no", "repo/notes.md").unwrap();
    let midword = fuzzy_score("no", "cannotes.md").unwrap();
    assert!(boundary > midword, "{boundary} !> {midword}");
}

#[test]
fn set_file_list_sorts_and_dedups() {
    let mut e = Editor::new();
    e.set_file_list(vec![
        "/sd/repo/b.md".into(),
        "/sd/repo/a.md".into(),
        "/sd/repo/b.md".into(),
    ]);
    assert_eq!(files_vec(&e), vec!["/sd/repo/a.md", "/sd/repo/b.md"]);
}

#[test]
fn cmd_p_opens_the_palette_and_esc_closes_it() {
    let mut e = palette_editor(&["/sd/repo/notes.md", "/sd/repo/todo.md"]);
    e.handle(Key::Palette);
    assert_eq!(e.mode(), Mode::Palette);
    e.handle(Key::Escape);
    assert_eq!(e.mode(), Mode::Normal);
}

#[test]
fn cmd_p_toggles_the_palette_closed() {
    let mut e = palette_editor(&["/sd/repo/notes.md"]);
    e.handle(Key::Palette);
    e.handle(Key::Palette); // same chord closes
    assert_eq!(e.mode(), Mode::Normal);
}

#[test]
fn cmd_p_opens_the_palette_from_insert_ending_the_session_like_esc() {
    let mut e = typed("hi");
    e.handle(Key::Palette);
    assert_eq!(e.mode(), Mode::Palette);
    assert_eq!(e.caret, 1); // caret dropped onto the last inserted char, as Esc does
    e.handle(Key::Escape);
    assert_eq!(e.mode(), Mode::Normal); // closing lands in Normal, not back in Insert
    assert_eq!(e.text, "hi");
}

#[test]
fn cmd_p_opens_the_palette_from_visual_dropping_the_selection() {
    let mut e = typed("hello");
    e.handle(Key::Escape);
    e.handle(Key::Char('v')); // charwise Visual, anchor set
    assert_eq!(e.mode(), Mode::Visual);
    e.handle(Key::Palette);
    assert_eq!(e.mode(), Mode::Palette);
    assert_eq!(e.visual_anchor, None); // selection gone, as Esc would leave it
    e.handle(Key::Escape);
    assert_eq!(e.mode(), Mode::Normal);
}

#[test]
fn cmd_p_opens_the_palette_from_visual_line() {
    let mut e = typed("hello");
    e.handle(Key::Escape);
    e.handle(Key::Char('V'));
    assert_eq!(e.mode(), Mode::VisualLine);
    e.handle(Key::Palette);
    assert_eq!(e.mode(), Mode::Palette);
    assert_eq!(e.visual_anchor, None);
}

#[test]
fn cmd_p_opens_the_palette_from_view_mode() {
    let mut e = typed("hello");
    e.handle(Key::Escape);
    e.handle(Key::Char('g'));
    e.handle(Key::Char('r')); // gr — go-read (View)
    assert_eq!(e.mode(), Mode::View);
    e.handle(Key::Palette);
    assert_eq!(e.mode(), Mode::Palette);
    e.handle(Key::Escape);
    assert_eq!(e.mode(), Mode::Normal);
}

#[test]
fn cmd_p_opens_the_palette_from_command_abandoning_the_line() {
    let mut e = palette_editor(&["/sd/repo/notes.md"]);
    e.handle(Key::Char(':'));
    for c in "del".chars() {
        e.handle(Key::Char(c)); // half-typed command
    }
    e.handle(Key::Palette);
    assert_eq!(e.mode(), Mode::Palette);
    assert_eq!(e.cmdline, ""); // the abandoned `:del` is gone
    assert_eq!(e.palette_query, ""); // and didn't leak into the palette query
    e.handle(Key::Escape);
    assert_eq!(e.mode(), Mode::Normal);
    assert!(e.take_effects().is_empty()); // `:del` never ran
}

#[test]
fn cmd_p_mid_insert_aborts_the_dot_recording() {
    let mut e = typed("ab"); // `i a b` — a dot recording in progress
    e.handle(Key::Palette); // palette trip aborts it
    e.handle(Key::Escape); // back to Normal — would normally complete a recording
    e.handle(Key::Char('.'));
    assert_eq!(e.text, "ab"); // nothing to repeat: no re-insert, no reopened palette
    assert_eq!(e.mode(), Mode::Normal);
}

#[test]
fn a_completed_dot_survives_a_palette_round_trip() {
    let mut e = typed("abc");
    e.handle(Key::Escape); // caret on 'c'
    e.handle(Key::Char('x')); // dot = [x], text "ab"
    e.handle(Key::Palette);
    e.handle(Key::Escape); // palette round trip
    e.handle(Key::Char('.')); // still repeats the x
    assert_eq!(e.text, "a");
}

#[test]
fn typing_filters_and_enter_opens_the_picked_file() {
    let mut e = palette_editor(&[
        "/sd/repo/notes.md",
        "/sd/repo/todo.md",
        "/sd/local/journal.md",
    ]);
    e.handle(Key::Palette);
    for c in "todo".chars() {
        e.handle(Key::Char(c));
    }
    e.handle(Key::Enter);
    assert_eq!(e.mode(), Mode::Normal); // palette closed
    let effs = e.take_effects();
    assert_eq!(kinds(&effs), vec![Kind::Load]);
    match &effs[0] {
        Effect::Load { path, scope } => {
            assert_eq!(path, "/sd/repo/todo.md");
            assert_eq!(*scope, Scope::Tracked);
        }
        other => panic!("expected a Load, got {other:?}"),
    }
}

#[test]
fn opening_a_local_file_from_the_palette_carries_local_scope() {
    let mut e = palette_editor(&["/sd/repo/notes.md", "/sd/local/journal.md"]);
    e.handle(Key::Palette);
    for c in "journal".chars() {
        e.handle(Key::Char(c));
    }
    e.handle(Key::Enter);
    match &e.take_effects()[0] {
        Effect::Load { path, scope } => {
            assert_eq!(path, "/sd/local/journal.md");
            assert_eq!(*scope, Scope::Local); // scope derived from the /sd/local path
        }
        other => panic!("expected a Local Load, got {other:?}"),
    }
}

#[test]
fn opening_the_active_file_from_the_palette_is_a_noop() {
    let mut e = palette_editor(&["/sd/repo/notes.md", "/sd/repo/todo.md"]);
    e.handle(Key::Palette);
    for c in "notes".chars() {
        e.handle(Key::Char(c));
    }
    e.handle(Key::Enter);
    // notes.md is already active: no Load, just a closed palette.
    assert!(e.take_effects().is_empty());
    assert_eq!(e.mode(), Mode::Normal);
}

#[test]
fn half_page_keys_move_the_selection_wrapping() {
    let mut e = palette_editor(&["/sd/repo/a.md", "/sd/repo/b.md", "/sd/repo/c.md"]);
    e.handle(Key::Palette);
    for ch in "md".chars() {
        e.handle(Key::Char(ch)); // reach the search threshold: all three match
    }
    assert_eq!(e.palette_sel, 0);
    e.handle(Key::HalfPageDown);
    assert_eq!(e.palette_sel, 1);
    e.handle(Key::HalfPageDown);
    e.handle(Key::HalfPageDown); // wraps past the last row to the top
    assert_eq!(e.palette_sel, 0);
    e.handle(Key::HalfPageUp); // wraps back to the last row
    assert_eq!(e.palette_sel, 2);
}

#[test]
fn ctrl_n_p_navigate_the_palette() {
    let mut e = palette_editor(&["/sd/repo/a.md", "/sd/repo/b.md", "/sd/repo/c.md"]);
    e.handle(Key::Palette);
    for ch in "md".chars() {
        e.handle(Key::Char(ch)); // reach the search threshold: all three match
    }
    e.handle(Key::Down); // Ctrl-n
    assert_eq!(e.palette_sel, 1);
    e.handle(Key::Down);
    e.handle(Key::Down); // wraps past the last row to the top
    assert_eq!(e.palette_sel, 0);
    e.handle(Key::Up); // Ctrl-p wraps back to the last row
    assert_eq!(e.palette_sel, 2);
}

#[test]
fn ctrl_n_p_move_by_a_line_in_normal_mode() {
    // Three lines; caret starts on the last char of line 3 (with_file posture).
    let mut e = Editor::with_file("/sd/repo/n.md".into(), Scope::Tracked, "aa\nbb\ncc".into());
    e.handle(Key::Char('g')); // gg → top (line 1)
    e.handle(Key::Char('g'));
    assert_eq!(e.caret, 0);
    e.handle(Key::Down); // Ctrl-n → line 2
    assert_eq!(e.caret, 3); // start of "bb"
    e.handle(Key::Down); // → line 3
    assert_eq!(e.caret, 6); // start of "cc"
    e.handle(Key::Up); // Ctrl-p → line 2
    assert_eq!(e.caret, 3);
}

#[test]
fn ctrl_n_takes_a_count_in_normal_mode() {
    let mut e = Editor::with_file("/sd/repo/n.md".into(), Scope::Tracked, "aa\nbb\ncc".into());
    e.handle(Key::Char('g'));
    e.handle(Key::Char('g')); // top
    e.handle(Key::Char('2'));
    e.handle(Key::Down); // 2<C-n> → down two lines
    assert_eq!(e.caret, 6); // start of "cc"
}

#[test]
fn ctrl_n_p_scroll_in_view_mode() {
    let mut e = Editor::with_file("/sd/repo/n.md".into(), Scope::Tracked, "a\nb\nc\nd".into());
    e.handle(Key::Char('g'));
    e.handle(Key::Char('r')); // gr → View
    assert_eq!(e.mode(), Mode::View);
    let top = e.scroll_top();
    e.handle(Key::Down); // Ctrl-n scrolls like j
    assert_eq!(e.scroll_top(), top + 1);
    e.handle(Key::Up); // Ctrl-p scrolls like k
    assert_eq!(e.scroll_top(), top);
}

#[test]
fn editing_the_query_resets_the_selection_to_the_top() {
    let mut e = palette_editor(&["/sd/repo/a.md", "/sd/repo/b.md"]);
    e.handle(Key::Palette);
    for ch in "md".chars() {
        e.handle(Key::Char(ch)); // reach the search threshold: both match
    }
    e.handle(Key::HalfPageDown);
    assert_eq!(e.palette_sel, 1);
    e.handle(Key::Char('a')); // a query edit resets the selection
    assert_eq!(e.palette_sel, 0);
}

#[test]
fn backspace_on_an_empty_query_closes_the_palette() {
    let mut e = palette_editor(&["/sd/repo/a.md"]);
    e.handle(Key::Palette);
    e.handle(Key::Backspace);
    assert_eq!(e.mode(), Mode::Normal);
}

#[test]
fn short_query_lists_recents_only() {
    let mut e = palette_editor(&["/sd/repo/b.md", "/sd/repo/a.md", "/sd/repo/c.md"]);
    // No opens yet: below the search threshold there is nothing to show.
    assert!(palette_labels(&e).is_empty());
    // Open c.md through the palette; it becomes the recents-only result.
    e.handle(Key::Palette);
    for ch in "c.md".chars() {
        e.handle(Key::Char(ch));
    }
    e.handle(Key::Enter);
    e.take_effects(); // drop the queued Load; we only care about the MRU
    assert_eq!(palette_labels(&e), vec!["repo/c.md"]);
}

#[test]
fn two_char_query_reveals_the_full_file_list() {
    let mut e = palette_editor(&["/sd/repo/b.md", "/sd/repo/a.md", "/sd/repo/c.md"]);
    e.handle(Key::Palette);
    e.handle(Key::Char('m')); // one char: still recents-only (none yet)
    assert!(e.palette_matches().is_empty());
    e.handle(Key::Char('d')); // "md": the full list, fuzzy-ranked
    assert_eq!(palette_labels(&e), vec!["repo/a.md", "repo/b.md", "repo/c.md"]);
}

#[test]
fn recents_float_above_the_full_list_on_a_matching_query() {
    let mut e = palette_editor(&["/sd/repo/b.md", "/sd/repo/a.md", "/sd/repo/c.md"]);
    // Open c.md so it is the MRU head.
    e.handle(Key::Palette);
    for ch in "c.md".chars() {
        e.handle(Key::Char(ch));
    }
    e.handle(Key::Enter);
    e.take_effects();
    // "md" scores the three labels equally; the stable sort keeps the
    // recently-opened c.md in front of the sorted rest.
    e.handle(Key::Palette);
    for ch in "md".chars() {
        e.handle(Key::Char(ch));
    }
    assert_eq!(palette_labels(&e), vec!["repo/c.md", "repo/a.md", "repo/b.md"]);
}

#[test]
fn draw_in_palette_mode_does_not_panic() {
    let mut e = palette_editor(&["/sd/repo/a.md", "/sd/local/j.md"]);
    e.handle(Key::Palette);
    let _ = e.draw(true); // empty query, no recents: "(type to search)"
    e.handle(Key::Char('j')); // one char, still below the threshold
    let _ = e.draw(true);
    e.handle(Key::Char('m')); // at the threshold: the ranked list
    let _ = e.draw(true);
    // Empty file list: the "(no files on card)" path must also be safe.
    let mut empty = Editor::new();
    empty.handle(Key::Palette);
    let _ = empty.draw(true);
}

#[test]
fn leading_gt_switches_the_palette_to_command_mode() {
    let e = palette_type(&["/sd/repo/notes.md"], ">");
    assert!(e.palette_command_mode());
    // Every prefs command is offered on a bare `>`.
    assert_eq!(e.palette_command_matches().len(), PALETTE_CMDS.len());
}

#[test]
fn backspacing_the_gt_returns_to_file_mode() {
    let mut e = palette_type(&["/sd/repo/notes.md"], ">");
    assert!(e.palette_command_mode());
    e.handle(Key::Backspace);
    assert!(!e.palette_command_mode());
    assert_eq!(e.mode(), Mode::Palette); // still open, just file mode again
}

#[test]
fn command_filter_fuzzy_matches_the_label() {
    let e = palette_type(&["/sd/repo/notes.md"], ">line");
    let matches = e.palette_command_matches();
    assert_eq!(matches.len(), 1);
    assert_eq!(PALETTE_CMDS[matches[0]], PaletteCmd::LineNumbers);
}

#[test]
fn command_label_reflects_current_pref_state() {
    let e = palette_editor(&["/sd/repo/notes.md"]);
    assert_eq!(e.command_label(PaletteCmd::LineNumbers), "line numbers: on");
}

#[test]
fn running_a_command_toggles_the_pref_live_and_queues_a_save_prefs() {
    // >line<Enter> flips line_numbers off, in-core, and asks the host to persist.
    let mut e = palette_type(&["/sd/repo/notes.md"], ">line");
    assert!(e.prefs().line_numbers);
    e.handle(Key::Enter);
    assert!(!e.prefs().line_numbers); // applied live
    assert_eq!(e.mode(), Mode::Palette); // stays open for more toggles
    assert_eq!(kinds(&e.take_effects()), vec![Kind::SavePrefs]);
}

#[test]
fn command_mode_stays_open_across_multiple_toggles() {
    // Flip line numbers, then retype the query for save-on-idle and flip that,
    // without the palette closing between them; each toggle persists. Navigate
    // by fuzzy filter, not registry position.
    let mut e = palette_type(&["/sd/repo/notes.md"], ">line");
    let before = e.prefs().line_numbers;
    e.handle(Key::Enter);
    assert_eq!(e.prefs().line_numbers, !before); // applied live
    assert_eq!(e.mode(), Mode::Palette); // stays open
    assert_eq!(kinds(&e.take_effects()), vec![Kind::SavePrefs]);
    // Still open: retype the query for a different pref (">idle" is unique to
    // "save on idle").
    for _ in 0.."line".len() {
        e.handle(Key::Backspace);
    }
    for c in "idle".chars() {
        e.handle(Key::Char(c));
    }
    let before = e.prefs().save_on_idle;
    e.handle(Key::Enter);
    assert_eq!(e.prefs().save_on_idle, !before);
    assert_eq!(e.mode(), Mode::Palette);
    assert_eq!(kinds(&e.take_effects()), vec![Kind::SavePrefs]);
}

#[test]
fn save_prefs_carries_the_serialized_prefs() {
    let mut e = palette_type(&["/sd/repo/notes.md"], ">line");
    e.handle(Key::Enter);
    let effects = e.take_effects();
    let Effect::SavePrefs { contents } = &effects[0] else {
        panic!("expected SavePrefs, got {effects:?}");
    };
    // The written TOML reflects the toggled state and round-trips.
    assert!(!Prefs::parse(contents).line_numbers);
}

#[test]
fn running_a_command_confirms_the_new_state_on_the_snackbar() {
    let mut e = palette_type(&["/sd/repo/notes.md"], ">save");
    e.handle(Key::Enter);
    // save_on_idle default true -> off; the notice names the new state.
    assert_eq!(e.notice.as_deref(), Some("save on idle: off - saved"));
}

// ---- Preset (non-boolean) prefs: rotate through options on Enter ----

#[test]
fn next_option_rotates_and_wraps() {
    assert_eq!(next_option("light", &THEME_OPTIONS), "dark");
    assert_eq!(next_option("dark", &THEME_OPTIONS), "light"); // wraps
    assert_eq!(next_option("10m", &AUTO_SYNC_OPTIONS), "15m");
    assert_eq!(next_option("30m", &AUTO_SYNC_OPTIONS), "2m"); // wraps
}

#[test]
fn next_option_snaps_an_unknown_value_to_the_head() {
    // A hand-typed value outside the preset list lands on the first option.
    assert_eq!(next_option("sepia", &THEME_OPTIONS), "light");
    assert_eq!(next_option("7m", &AUTO_SYNC_OPTIONS), "2m");
}

#[test]
fn theme_command_label_reflects_the_current_value() {
    let e = palette_editor(&["/sd/repo/notes.md"]);
    assert_eq!(e.command_label(PaletteCmd::Theme), "theme: light");
    assert_eq!(e.command_label(PaletteCmd::AutoSync), "auto sync: 10m");
}

#[test]
fn running_the_theme_command_rotates_the_preset_and_queues_a_save_prefs() {
    // >theme<Enter> flips light -> dark, in-core, and asks the host to persist.
    let mut e = palette_type(&["/sd/repo/notes.md"], ">theme");
    assert_eq!(e.prefs().theme, "light");
    e.handle(Key::Enter);
    assert_eq!(e.prefs().theme, "dark"); // applied live
    assert_eq!(e.mode(), Mode::Palette); // stays open for more changes
    assert_eq!(kinds(&e.take_effects()), vec![Kind::SavePrefs]);
    e.handle(Key::Enter);
    assert_eq!(e.prefs().theme, "light"); // rotates back, wrapping
}

#[test]
fn running_the_auto_sync_command_walks_the_interval_presets() {
    // Default 10m; Enter rotates 10m -> 15m -> 30m -> 2m (wrap).
    let mut e = palette_type(&["/sd/repo/notes.md"], ">auto");
    assert_eq!(e.prefs().auto_sync, "10m");
    for expected in ["15m", "30m", "2m"] {
        e.handle(Key::Enter);
        assert_eq!(e.prefs().auto_sync, expected);
    }
}

#[test]
fn dark_theme_inverts_the_whole_frame() {
    // The dark frame is the exact bitwise inverse of the light one.
    let mut e = Editor::with_text("hello world".into());
    e.caret = 0;
    let light = e.draw(true).bytes().to_vec();
    e.prefs.theme = "dark".into();
    let dark = e.draw(true).bytes().to_vec();
    assert_eq!(light.len(), dark.len());
    assert!(light.iter().zip(&dark).all(|(l, d)| *l == !*d));
}

#[test]
fn a_no_match_command_query_runs_nothing() {
    let mut e = palette_type(&["/sd/repo/notes.md"], ">zzzzz");
    assert!(e.palette_command_matches().is_empty());
    let before = e.prefs().clone();
    e.handle(Key::Enter);
    assert_eq!(e.prefs(), &before); // nothing toggled
    assert!(e.take_effects().is_empty()); // nothing queued
    assert_eq!(e.mode(), Mode::Palette); // stays open so the query can be fixed
}

#[test]
fn ctrl_n_moves_the_command_selection_wrapping() {
    let mut e = palette_type(&["/sd/repo/notes.md"], ">");
    for _ in 0..PALETTE_CMDS.len() - 1 {
        e.handle(Key::Down); // Ctrl-N down to the last command
    }
    assert_eq!(e.palette_sel, PALETTE_CMDS.len() - 1);
    e.handle(Key::Down); // wraps back to the top
    assert_eq!(e.palette_sel, 0);
    e.handle(Key::Up); // and Ctrl-P wraps back to the bottom
    assert_eq!(e.palette_sel, PALETTE_CMDS.len() - 1);
}

#[test]
fn settings_command_opens_the_palette_in_command_mode() {
    let (e, _) = command("settings");
    assert_eq!(e.mode(), Mode::Palette);
    assert!(e.palette_command_mode()); // dropped straight into `>` mode
    assert_eq!(e.palette_command_matches().len(), PALETTE_CMDS.len());
}

#[test]
fn draw_in_command_mode_does_not_panic() {
    let mut e = palette_type(&["/sd/repo/notes.md"], ">");
    let _ = e.draw(true);
    let mut none = palette_type(&["/sd/repo/notes.md"], ">zzzzz"); // "(no command)"
    let _ = none.draw(true);
}

// ---- `>` command palette generalisation (v0.6) ----

#[test]
fn command_labels_for_the_new_actions() {
    let e = Editor::new();
    assert_eq!(e.command_label(PaletteCmd::NewFile), "new file...");
    assert_eq!(e.command_label(PaletteCmd::Format), "format");
    assert_eq!(e.command_label(PaletteCmd::Publish), "publish");
}

#[test]
fn format_command_runs_and_closes() {
    // A one-shot: it formats the buffer (extra trailing blanks collapse) and
    // closes the palette — and, being an action not a toggle, queues no
    // SavePrefs. `>format` ranks the action above `format on save` (registry
    // order breaks the tie).
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "a\n\n\n".into());
    e.handle(Key::Palette);
    for c in ">format".chars() {
        e.handle(Key::Char(c));
    }
    e.handle(Key::Enter);
    assert_eq!(e.text(), "a\n"); // formatted → not the FormatOnSave toggle
    assert_eq!(e.mode(), Mode::Normal); // one-shot closes
    assert!(!kinds(&e.take_effects()).contains(&Kind::SavePrefs));
}

#[test]
fn publish_command_saves_and_pushes_then_closes() {
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "hi".into());
    e.handle(Key::Palette);
    for c in ">publish".chars() {
        e.handle(Key::Char(c));
    }
    e.handle(Key::Enter);
    assert_eq!(e.mode(), Mode::Normal);
    assert_eq!(kinds(&e.take_effects()), vec![Kind::Save, Kind::Publish]);
}

#[test]
fn publish_command_is_unavailable_in_a_local_buffer() {
    let mut e = Editor::with_file("/sd/local/j.md".into(), Scope::Local, "hi".into());
    e.handle(Key::Palette);
    for c in ">publish".chars() {
        e.handle(Key::Char(c));
    }
    e.handle(Key::Enter);
    assert_eq!(e.mode(), Mode::Normal);
    assert!(e.take_effects().is_empty()); // Local never reaches the remote
}

#[test]
fn new_file_command_opens_the_filename_input_step() {
    let mut e = palette_type(&["/sd/repo/notes.md"], ">new");
    e.handle(Key::Enter);
    assert_eq!(e.mode(), Mode::Palette); // still open — now the input step
    assert_eq!(e.palette_step, PaletteStep::NewFile);
    assert!(e.palette_query.is_empty()); // the list filter gave way to the name
}

#[test]
fn new_file_step_creates_the_typed_file_and_switches() {
    let mut e = palette_type(&["/sd/repo/notes.md"], ">new");
    e.handle(Key::Enter); // → input step
    for c in "draft.md".chars() {
        e.handle(Key::Char(c));
    }
    e.handle(Key::Enter);
    assert_eq!(e.mode(), Mode::Normal); // committed and closed
    assert_eq!(e.path(), "/sd/repo/draft.md"); // scope inherited from notes.md
    assert!(e.dirty()); // a fresh, unsaved file
}

#[test]
fn new_file_step_backspace_returns_to_the_command_list() {
    let mut e = palette_type(&["/sd/repo/notes.md"], ">new");
    e.handle(Key::Enter); // input step, empty name
    e.handle(Key::Backspace); // nothing to erase → step back to the `>` list
    assert_eq!(e.palette_step, PaletteStep::List);
    assert!(e.palette_command_mode()); // query restored to ">"
    assert_eq!(e.mode(), Mode::Palette);
}

#[test]
fn new_file_step_empty_enter_stays_in_the_step() {
    let mut e = palette_type(&["/sd/repo/notes.md"], ">new");
    e.handle(Key::Enter); // input step
    e.handle(Key::Enter); // empty name → no-op, still awaiting one
    assert_eq!(e.palette_step, PaletteStep::NewFile);
    assert_eq!(e.mode(), Mode::Palette);
    assert_eq!(e.path(), "/sd/repo/notes.md"); // unchanged
}

#[test]
fn draw_in_new_file_step_does_not_panic() {
    let mut e = palette_type(&["/sd/repo/notes.md"], ">new");
    e.handle(Key::Enter);
    let _ = e.draw(true); // empty-name placeholder path
    for c in "draft.md".chars() {
        e.handle(Key::Char(c));
    }
    let _ = e.draw(true); // with a typed name
}

#[test]
fn e_command_is_retired() {
    // `:e` no longer opens files — bare Cmd-P does. `:e foo` is now a no-op.
    let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "A".into());
    ex(&mut e, "e /sd/repo/b.md");
    assert_eq!(e.path(), "/sd/repo/a.md"); // unchanged
    assert!(e.take_effects().is_empty()); // nothing queued
}
