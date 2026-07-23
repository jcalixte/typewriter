//! Typo, the tucano companion: the word-count milestone ladder (fired through
//! the existing notice slot on the throttled stats refresh), its per-buffer
//! baseline, and the mood state the host's render engine drives. The
//! refresh-cycle transitions themselves (pool rotation, frustration threshold)
//! are host behaviour, tested in `app::render`.

use super::*;

#[test]
fn milestone_floor_walks_the_ladder() {
    assert_eq!(milestone_floor(0), 0);
    assert_eq!(milestone_floor(499), 0);
    assert_eq!(milestone_floor(500), 500);
    assert_eq!(milestone_floor(1_999), 1_000);
    assert_eq!(milestone_floor(2_000), 2_000);
    assert_eq!(milestone_floor(4_999), 2_000);
    assert_eq!(milestone_floor(9_999), 5_000);
    assert_eq!(milestone_floor(10_000), 10_000);
    // Past 10k the ladder is every 10k — no more mid-steps.
    assert_eq!(milestone_floor(19_999), 10_000);
    assert_eq!(milestone_floor(70_123), 70_000);
}

#[test]
fn group_thousands_formats_the_panel_figures() {
    assert_eq!(group_thousands(0), "0");
    assert_eq!(group_thousands(500), "500");
    assert_eq!(group_thousands(5_000), "5,000");
    assert_eq!(group_thousands(1_234_567), "1,234,567");
}

/// Grow the buffer to `words` space-separated words through the Insert path,
/// then refresh the throttled stats the way the host's typing-pause repaint does.
fn write_up_to(e: &mut Editor, words: usize) {
    e.handle(Key::Char('i'));
    for _ in 0..words {
        for c in "w ".chars() {
            e.handle(Key::Char(c));
        }
    }
    e.handle(Key::Escape);
    e.refresh_stats();
}

#[test]
fn crossing_a_milestone_posts_the_notice_and_the_anticipation_face() {
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, String::new());
    write_up_to(&mut e, 500);
    assert_eq!(e.notice.as_deref(), Some("500 words!"));
    assert_eq!(e.companion_mood(), display::typo::Mood::Anticipation);

    // The next keystroke dismisses it like any snackbar — and the threshold
    // never re-fires: deleting below and typing back over 500 stays quiet.
    e.handle(Key::Char('d'));
    e.handle(Key::Char('w')); // dw: drop the first word (below 500)
    e.refresh_stats();
    assert_eq!(e.notice, None);
    write_up_to(&mut e, 1);
    assert_eq!(e.notice, None, "a re-crossed threshold must not celebrate twice");
}

#[test]
fn a_buffer_loaded_past_a_threshold_starts_baselined_there() {
    // Opening a 2,600-word file celebrates nothing...
    let text = "w ".repeat(2_600);
    let mut e = Editor::with_file("/sd/repo/big.md".into(), Scope::Tracked, text);
    e.refresh_stats();
    assert_eq!(e.notice, None, "loading a long file is not an achievement");

    // ...but writing on to the *next* rung still counts.
    e.handle(Key::Char('g'));
    e.handle(Key::Char('g')); // caret home so `i` doesn't disturb the tail
    write_up_to(&mut e, 2_500);
    assert_eq!(e.notice.as_deref(), Some("5,000 words!"));
}

#[test]
fn companion_off_silences_milestones() {
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, String::new());
    e.set_prefs(Prefs { companion: false, ..Prefs::default() });
    write_up_to(&mut e, 500);
    assert_eq!(e.notice, None);
    assert_eq!(e.companion_mood(), display::typo::Mood::Neutral);
}

/// Any ink in the *lower half* of Typo's fixed face box? The lower half is
/// probed because it holds the sprite's belly/beak curves but sits below the
/// deepest possible notice extent (a 3-row filename + 4-line notice ends at
/// y = 152), so it can never confuse notice ink for face ink.
fn face_inked(f: &Frame) -> bool {
    (FACE_Y + 48..FACE_Y + 96).any(|y| {
        (PANEL_X..display::WIDTH as i32).any(|x| ink_at(f, x as usize, y as usize))
    })
}

#[test]
fn the_face_box_is_inked_only_while_the_companion_is_on() {
    let mut e = Editor::with_text(String::new());
    assert!(face_inked(&e.draw(true)), "companion on: Typo lives in the panel");
    e.set_prefs(Prefs { companion: false, ..Prefs::default() });
    assert!(!e.draw(true).bytes().is_empty());
    assert!(!face_inked(&e.draw(true)), "companion off: the panel is text-only");
}

#[test]
fn a_notice_long_enough_to_reach_the_face_box_wins_over_the_face() {
    // A 3-row wrapped filename pushes the sync tier down, so a 4-line notice's
    // tail runs past FACE_Y: the transient notice text must win the pixels, and
    // Typo returns once a keystroke clears it.
    let mut e = Editor::with_file(
        "/sd/repo/2026-07-24-a-very-long-title-that-wraps.md".into(),
        Scope::Tracked,
        "some words".into(),
    );
    e.set_notice("pull FAILED - network unreachable, check wifi and retry :gl");
    assert!(!face_inked(&e.draw(true)), "the wrapped notice owns the tier");
    e.handle(Key::Char('j')); // any key dismisses the snackbar
    assert!(face_inked(&e.draw(true)), "notice gone: Typo is back");
}

#[test]
fn switching_buffers_rebaselines_the_ladder() {
    // Cross 500 in A, switch to a fresh B: B's first 500 words still celebrate
    // (its baseline is its own), and switching back to A stays quiet.
    let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, String::new());
    write_up_to(&mut e, 500);
    assert_eq!(e.notice.as_deref(), Some("500 words!"));

    edit(&mut e, "repo/b.md");
    e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, String::new());
    write_up_to(&mut e, 500);
    assert_eq!(e.notice.as_deref(), Some("500 words!"), "each buffer earns its own rungs");
}
