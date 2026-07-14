//! Snippets, the `$` snippet palette, and hint-on-pause.

use super::*;

#[test]
fn strip_labels_reduces_placeholders_to_bare_stops() {
    assert_eq!(strip_stop_labels("# ${1:Titre}"), "# $1");
    assert_eq!(strip_stop_labels("${2}"), "$2");
    assert_eq!(strip_stop_labels("[$1]($2)$0"), "[$1]($2)$0"); // plain stops untouched
    assert_eq!(strip_stop_labels("price: $ and ${3:x}"), "price: $ and $3"); // lone $ kept
}

#[test]
fn parse_body_extracts_literal_and_visit_order() {
    let (lit, stops) = parse_snippet_body("[$1]($2)$0");
    assert_eq!(lit, "[]()");
    assert_eq!(stops, vec![1, 3, 4]); // $1, $2, then $0 (end) last
}

#[test]
fn parse_body_appends_implicit_final_stop_when_no_zero() {
    let (lit, stops) = parse_snippet_body("# $1\n## $2");
    assert_eq!(lit, "# \n## ");
    assert_eq!(stops, vec![2, 6, 6]); // $1, $2, implicit rest at the end
}

#[test]
fn parse_body_no_stops_has_empty_visit_list() {
    let (lit, stops) = parse_snippet_body("- [ ] ");
    assert_eq!(lit, "- [ ] ");
    assert!(stops.is_empty());
}

#[test]
fn parse_snippets_reads_zed_json_string_and_array_bodies() {
    // r###"…"### so the `"#`/`"##` in the heading bodies don't close the string.
    let json = r###"{
        "Link": { "prefix": "link", "body": "[$1]($2)$0", "description": "Inline link" },
        "Book notes": { "prefix": "booknotes", "body": ["# ${1:Titre}", "## $2"] }
    }"###;
    let s = Snippets::parse(json).unwrap().0;
    assert_eq!(s.len(), 2);
    // BTreeMap parse → sorted by display name ("Book notes" < "Link").
    assert_eq!(s[0].name, "Book notes");
    assert_eq!(s[0].prefix, "booknotes");
    assert_eq!(s[0].body, "# $1\n## $2"); // array joined with \n, label stripped
    assert_eq!(s[0].description, ""); // omitted → empty
    assert_eq!(s[1].name, "Link");
    assert_eq!(s[1].body, "[$1]($2)$0");
    assert_eq!(s[1].description, "Inline link");
}

#[test]
fn parse_snippets_empty_and_malformed() {
    assert!(Snippets::parse("{}").unwrap().0.is_empty());
    assert!(Snippets::parse("").unwrap().0.is_empty()); // empty file = no snippets
    assert!(Snippets::parse(" \n\t").unwrap().0.is_empty()); // whitespace-only too
    assert!(Snippets::parse("{ not json").is_err()); // host logs, boots with none
}

#[test]
fn tab_expands_prefix_and_lands_on_first_stop() {
    let mut e = with_snippets(r#"{ "Link": { "prefix": "link", "body": "[$1]($2)$0" } }"#);
    e.handle(Key::Char('i'));
    for c in "link".chars() {
        e.handle(Key::Char(c));
    }
    e.handle(Key::Char('\t')); // expand
    assert_eq!(e.text, "[]()"); // trigger word replaced by the expansion
    assert_eq!(e.caret, 1); // caret on $1
    assert_eq!(e.mode(), Mode::Insert);
    assert_eq!(e.snippet_stops, vec![3, 4]); // $2 then $0 pending
}

#[test]
fn tab_advances_stops_and_typing_shifts_pending_ones() {
    let mut e = with_snippets(r#"{ "Link": { "prefix": "link", "body": "[$1]($2)$0" } }"#);
    e.handle(Key::Char('i'));
    for c in "link".chars() {
        e.handle(Key::Char(c));
    }
    e.handle(Key::Char('\t'));
    for c in "url".chars() {
        e.handle(Key::Char(c)); // type at $1
    }
    assert_eq!(e.text, "[url]()");
    assert_eq!(e.snippet_stops, vec![6, 7]); // pending shifted by +3
    e.handle(Key::Char('\t')); // → $2
    assert_eq!(e.caret, 6);
    assert_eq!(e.snippet_stops, vec![7]);
    for c in "http".chars() {
        e.handle(Key::Char(c));
    }
    assert_eq!(e.text, "[url](http)");
    e.handle(Key::Char('\t')); // → $0 (end); session ends
    assert_eq!(e.caret, 11);
    assert!(e.snippet_stops.is_empty());
}

#[test]
fn esc_ends_the_snippet_session() {
    let mut e = with_snippets(r#"{ "Link": { "prefix": "link", "body": "[$1]($2)$0" } }"#);
    e.handle(Key::Char('i'));
    for c in "link".chars() {
        e.handle(Key::Char(c));
    }
    e.handle(Key::Char('\t'));
    assert!(!e.snippet_stops.is_empty());
    e.handle(Key::Escape);
    assert_eq!(e.mode(), Mode::Normal);
    assert!(e.snippet_stops.is_empty(), "leaving Insert ends the session");
}

#[test]
fn tab_without_matching_prefix_inserts_spaces() {
    let mut e = with_snippets(r#"{ "Link": { "prefix": "link", "body": "[$1]($2)$0" } }"#);
    e.handle(Key::Char('i'));
    for c in "zzz".chars() {
        e.handle(Key::Char(c));
    }
    e.handle(Key::Char('\t'));
    assert!(e.text.starts_with("zzz"));
    assert!(e.text.len() > 3, "tab inserted whitespace, not an expansion");
    assert!(e.snippet_stops.is_empty());
}

#[test]
fn no_stop_snippet_expands_without_a_session() {
    let mut e = with_snippets(r#"{ "Todo": { "prefix": "todo", "body": "- [ ] " } }"#);
    e.handle(Key::Char('i'));
    for c in "todo".chars() {
        e.handle(Key::Char(c));
    }
    e.handle(Key::Char('\t'));
    assert_eq!(e.text, "- [ ] ");
    assert_eq!(e.caret, 6); // caret at the end, no session
    assert!(e.snippet_stops.is_empty());
}

#[test]
fn undo_after_expansion_restores_the_trigger_word() {
    let mut e = with_snippets(r#"{ "Link": { "prefix": "link", "body": "[$1]($2)$0" } }"#);
    e.handle(Key::Char('i'));
    for c in "link".chars() {
        e.handle(Key::Char(c));
    }
    e.handle(Key::Char('\t'));
    assert_eq!(e.text, "[]()");
    e.handle(Key::Escape);
    e.handle(Key::Char('u')); // undo the whole expansion
    assert_eq!(e.text, "link");
}

// ---- `$` snippet palette ----

// r##"…"## so the `"#` in the `"# $1"` heading body doesn't close the string.

#[test]
fn dollar_switches_the_palette_to_snippet_mode() {
    let e = snippet_palette(TWO_SNIPPETS, "$");
    assert!(e.palette_snippet_mode());
    assert!(!e.palette_command_mode());
    // A bare `$` lists every snippet.
    assert_eq!(e.palette_snippet_matches().len(), 2);
}

#[test]
fn backspacing_the_dollar_returns_to_file_mode() {
    let mut e = snippet_palette(TWO_SNIPPETS, "$");
    assert!(e.palette_snippet_mode());
    e.handle(Key::Backspace);
    assert!(!e.palette_snippet_mode());
    assert_eq!(e.mode(), Mode::Palette); // still open, file mode again
}

#[test]
fn snippet_filter_fuzzy_matches_name_prefix_and_description() {
    // Query the prefix.
    let e = snippet_palette(TWO_SNIPPETS, "$link");
    let m = e.palette_snippet_matches();
    assert_eq!(m.len(), 1);
    assert_eq!(e.snippets[m[0]].name, "Markdown link");
    // Query a word only in the *description* ("fiche") finds the other one.
    let e = snippet_palette(TWO_SNIPPETS, "$fiche");
    let m = e.palette_snippet_matches();
    assert_eq!(m.len(), 1);
    assert_eq!(e.snippets[m[0]].name, "Book notes");
}

#[test]
fn enter_in_snippet_mode_inserts_and_starts_the_session() {
    let mut e = snippet_palette(TWO_SNIPPETS, "$link");
    e.handle(Key::Enter);
    assert_eq!(e.mode(), Mode::Insert); // dropped into the buffer at $1
    assert_eq!(e.text, "[]()");
    assert_eq!(e.caret, 1); // on $1
    assert_eq!(e.snippet_stops, vec![3, 4]); // $2 then $0 pending
    // Inserting content closes the palette (unlike a `>` toggle, which stays).
    // A follow-up Esc must leave Insert, not reopen anything.
    e.handle(Key::Escape);
    assert_eq!(e.mode(), Mode::Normal);
}

#[test]
fn snippet_palette_insertion_is_one_undo_group() {
    let mut e = snippet_palette(TWO_SNIPPETS, "$link");
    e.handle(Key::Enter);
    e.handle(Key::Escape); // end the session, back to Normal
    e.handle(Key::Char('u')); // undo the whole insertion
    assert_eq!(e.text, ""); // buffer restored to its pre-insert state
}

#[test]
fn ctrl_n_wraps_around_the_snippet_result_list() {
    let mut e = snippet_palette(TWO_SNIPPETS, "$");
    e.handle(Key::Down);
    assert_eq!(e.palette_sel, 1); // two snippets → last index is 1
    e.handle(Key::Down); // past the end — wraps to the top
    assert_eq!(e.palette_sel, 0);
    e.handle(Key::Up); // before the top — wraps to the bottom
    assert_eq!(e.palette_sel, 1);
}

#[test]
fn empty_snippet_library_matches_nothing() {
    let mut e = Editor::new(); // no snippets set
    e.handle(Key::Palette);
    e.handle(Key::Char('$'));
    assert!(e.palette_snippet_mode());
    assert!(e.palette_snippet_matches().is_empty());
    e.handle(Key::Enter); // no-op, stays open
    assert_eq!(e.mode(), Mode::Palette);
    let _ = e.draw(true); // "(no snippets)" path must not panic
}

#[test]
fn draw_in_snippet_mode_does_not_panic() {
    let mut e = snippet_palette(TWO_SNIPPETS, "$");
    let _ = e.draw(true);
    let mut filtered = snippet_palette(TWO_SNIPPETS, "$link");
    let _ = filtered.draw(true);
}

#[test]
fn pause_hint_names_the_snippet_a_prefix_would_expand() {
    let mut e = typed_in_insert("link");
    assert_eq!(e.snippet_hint, None); // not computed per keystroke
    e.refresh_stats(); // the typing-pause throttle
    assert_eq!(e.snippet_hint.as_deref(), Some("Markdown link"));
}

#[test]
fn pause_hint_is_absent_without_a_matching_prefix() {
    let mut e = typed_in_insert("zz");
    e.refresh_stats();
    assert_eq!(e.snippet_hint, None);
}

#[test]
fn pause_hint_clears_when_leaving_insert() {
    let mut e = typed_in_insert("link");
    e.refresh_stats();
    assert!(e.snippet_hint.is_some());
    e.handle(Key::Escape); // → Normal
    e.refresh_stats(); // the main loop refreshes on non-Insert actions
    assert_eq!(e.snippet_hint, None);
}

#[test]
fn pause_hint_is_absent_during_a_live_session() {
    let mut e = typed_in_insert("link");
    e.handle(Key::Char('\t')); // expand → session live, caret on $1
    assert!(!e.snippet_stops.is_empty());
    e.refresh_stats();
    assert_eq!(e.snippet_hint, None, "mid-session Tab advances, not expands");
}

#[test]
fn draw_with_a_pause_hint_does_not_panic() {
    let mut e = typed_in_insert("link");
    e.refresh_stats();
    assert!(e.snippet_hint.is_some());
    let _ = e.draw(true); // the `» name` panel row must render cleanly
}
