//! Fleeting-note commands: `:inbox`/`:in` (open/create today's note) and
//! `:oldest`/`:old` (open the oldest note for cleanup).

use super::*;

/// A fixed "today" so the dated filename/title are deterministic.
const TODAY: Date = Date { year: 2026, month: 7, day: 18 };
/// The note `:inbox` names for [`TODAY`].
const INBOX_TODAY: &str = "/sd/repo/_inbox/2026-07-18.md";

// ---- :inbox / :in --------------------------------------------------------

#[test]
fn inbox_creates_todays_note_prefilled_dirty_and_in_normal() {
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, String::new());
    e.set_today(Some(TODAY));
    ex(&mut e, "inbox");
    assert_eq!(e.path(), INBOX_TODAY);
    assert_eq!(e.scope(), Scope::Tracked);
    assert_eq!(e.text(), "# 18/07/2026\n\n"); // dated heading + a blank line to write on
    assert!(e.dirty(), "a fresh note must be dirty so eviction/:w persists it");
    assert_eq!(e.mode(), Mode::Normal);
    assert!(e.file_list_contains(INBOX_TODAY), "must be findable in the palette at once");
    assert!(e.take_effects().is_empty(), "creation is in-RAM; no host IO until saved");
}

#[test]
fn inbox_alias_in_creates_the_same_note() {
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, String::new());
    e.set_today(Some(TODAY));
    ex(&mut e, "in");
    assert_eq!(e.path(), INBOX_TODAY);
    assert_eq!(e.text(), "# 18/07/2026\n\n");
}

#[test]
fn inbox_opens_an_existing_note_from_disk_without_clobbering() {
    // Today's note is already on the card (in the palette file list) but not
    // resident: `:inbox` must Load it, not replace it with an empty buffer.
    let mut e = palette_editor(&[INBOX_TODAY]);
    e.set_today(Some(TODAY));
    ex(&mut e, "inbox");
    assert_eq!(
        e.take_effects(),
        vec![Effect::Load { path: INBOX_TODAY.into(), scope: Scope::Tracked }],
    );
    assert_eq!(e.path(), "/sd/repo/notes.md", "active unchanged until the Load lands");
    assert!(!e.dirty(), "must not have created/dirtied a new empty buffer");
}

#[test]
fn inbox_reopening_the_active_note_keeps_its_edits() {
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, String::new());
    e.set_today(Some(TODAY));
    ex(&mut e, "inbox"); // create today's note, now active
    e.handle(Key::Char('i'));
    for c in "hello".chars() {
        e.handle(Key::Char(c));
    }
    e.handle(Key::Escape);
    assert!(e.text().contains("hello"));
    ex(&mut e, "inbox"); // again -> switch to it, never reset it
    assert_eq!(e.path(), INBOX_TODAY);
    assert!(e.text().contains("hello"), "reopening today's note must keep its content");
    assert!(e.take_effects().is_empty(), "re-opening the active buffer is a no-op");
}

#[test]
fn inbox_refuses_when_the_clock_is_unset() {
    // `today` defaults to None (no sync has set the clock this power cycle).
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, String::new());
    ex(&mut e, "inbox");
    assert!(e.take_effects().is_empty(), "must queue no IO with no date");
    assert_eq!(e.path(), "/sd/repo/notes.md", "must not create or switch buffers");
    assert!(!e.dirty());
    assert_eq!(e.notice.as_deref(), Some("clock not set - :gl first"));
}

// ---- :oldest / :old ------------------------------------------------------

#[test]
fn oldest_opens_the_chronologically_first_inbox_note() {
    // ISO-dated names sort chronologically, so the oldest is the first `_inbox/`
    // entry in the (sorted) file list — 2026-06-15 here.
    let mut e = palette_editor(&[
        "/sd/repo/notes.md",
        "/sd/repo/_inbox/2026-07-08.md",
        "/sd/repo/_inbox/2026-06-15.md",
        "/sd/repo/_inbox/2026-06-30.md",
        "/sd/repo/zzz.md",
    ]);
    ex(&mut e, "oldest");
    assert_eq!(
        e.take_effects(),
        vec![Effect::Load { path: "/sd/repo/_inbox/2026-06-15.md".into(), scope: Scope::Tracked }],
    );
}

#[test]
fn oldest_alias_old_works() {
    let mut e = palette_editor(&["/sd/repo/_inbox/2026-06-15.md", "/sd/repo/_inbox/2026-07-08.md"]);
    ex(&mut e, "old");
    assert_eq!(kinds(&e.take_effects()), vec![Kind::Load]);
}

#[test]
fn oldest_on_an_empty_inbox_notices_and_does_nothing() {
    let mut e = palette_editor(&["/sd/repo/notes.md", "/sd/repo/journal.md"]);
    ex(&mut e, "oldest");
    assert!(e.take_effects().is_empty());
    assert_eq!(e.notice.as_deref(), Some("inbox empty"));
}

#[test]
fn oldest_ignores_non_md_files_and_lookalike_dirs() {
    let mut e = palette_editor(&[
        "/sd/repo/_inboxes/2020-01-01.md", // lookalike dir, not `_inbox/`
        "/sd/repo/_inbox/2026-06-15.txt",  // in the inbox but not markdown
        "/sd/repo/_inbox/2026-06-20.md",   // the real oldest .md
    ]);
    ex(&mut e, "oldest");
    assert_eq!(
        e.take_effects(),
        vec![Effect::Load { path: "/sd/repo/_inbox/2026-06-20.md".into(), scope: Scope::Tracked }],
    );
}
