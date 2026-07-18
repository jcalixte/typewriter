//! Multi-file buffers and `:enew` / `:delete`.

use super::*;

#[test]
fn wrap_text_packs_words_and_splits_overlong_tokens() {
    // Short message: one line.
    assert_eq!(wrap_text("saved", 15), vec!["saved"]);
    // Word-wraps on the space, keeping the actionable tail.
    assert_eq!(
        wrap_text("save FAILED - retry :w", 15),
        vec!["save FAILED -", "retry :w"]
    );
    // A token longer than the width is hard-split rather than truncated.
    assert_eq!(
        wrap_text("supercalifragilistic", 8),
        vec!["supercal", "ifragili", "stic"]
    );
    assert!(wrap_text("", 15).is_empty());
}

#[test]
fn resolve_path_maps_prefixes_and_bare_names() {
    assert_eq!(
        resolve_path("/sd/local/j.md", Scope::Tracked),
        ("/sd/local/j.md".to_string(), Scope::Local)
    );
    assert_eq!(
        resolve_path("/sd/repo/n.md", Scope::Local),
        ("/sd/repo/n.md".to_string(), Scope::Tracked)
    );
    // A leading `local/` or `repo/` segment selects scope (the palette label
    // form), independent of the current buffer's scope.
    assert_eq!(
        resolve_path("local/j.md", Scope::Tracked),
        ("/sd/local/j.md".to_string(), Scope::Local)
    );
    assert_eq!(
        resolve_path("repo/n.md", Scope::Local),
        ("/sd/repo/n.md".to_string(), Scope::Tracked)
    );
    // The `/sd` prefix is optional: `/repo/x` and `/local/x` (leading slash,
    // no `/sd`) resolve into the same scopes as their `/sd/…` spellings.
    assert_eq!(
        resolve_path("/repo/n.md", Scope::Local),
        ("/sd/repo/n.md".to_string(), Scope::Tracked)
    );
    assert_eq!(
        resolve_path("/local/j.md", Scope::Tracked),
        ("/sd/local/j.md".to_string(), Scope::Local)
    );
    // A bare name lands in the current buffer's scope directory.
    assert_eq!(
        resolve_path("draft.md", Scope::Local),
        ("/sd/local/draft.md".to_string(), Scope::Local)
    );
    assert_eq!(
        resolve_path("draft.md", Scope::Tracked),
        ("/sd/repo/draft.md".to_string(), Scope::Tracked)
    );
}

#[test]
fn an_edit_marks_dirty_and_mark_saved_clears_it() {
    let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "hi".into());
    assert!(!e.dirty()); // a freshly loaded buffer is clean
    e.handle(Key::Char('x')); // delete a char
    assert!(e.dirty());
    e.mark_saved("/sd/repo/a.md");
    assert!(!e.dirty());
}

#[test]
fn opening_a_nonresident_file_queues_a_load() {
    let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "A".into());
    edit(&mut e, "/sd/local/j.md");
    assert_eq!(
        e.take_effects(),
        vec![Effect::Load {
            path: "/sd/local/j.md".into(),
            scope: Scope::Local,
        }]
    );
    // The active buffer does not change until the host loads and installs it.
    assert_eq!(e.path(), "/sd/repo/a.md");
}

#[test]
fn install_loaded_parks_current_and_activates_the_target() {
    let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "A".into());
    e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, "hello B".into());
    assert_eq!(e.path(), "/sd/repo/b.md");
    assert_eq!(e.text(), "hello B");
    assert_eq!(e.mode(), Mode::Normal);
}

#[test]
fn switching_back_to_a_resident_buffer_needs_no_load() {
    let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "AAA".into());
    assert_eq!(e.caret, 2); // caret on A's last char
    e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, "BBBBB".into());
    // A is parked (resident) — switching back reads memory, not disk.
    edit(&mut e, "/sd/repo/a.md");
    assert!(e.take_effects().is_empty());
    assert_eq!(e.path(), "/sd/repo/a.md");
    assert_eq!(e.text(), "AAA");
    assert_eq!(e.caret, 2); // its caret came back with it
}

#[test]
fn the_register_is_global_across_buffers() {
    let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "word".into());
    e.handle(Key::Char('y')); // yy — yank the line
    e.handle(Key::Char('y'));
    e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, String::new());
    e.handle(Key::Char('p')); // paste it into the other buffer
    assert!(e.text().contains("word"));
}

#[test]
fn a_dirty_parked_buffer_is_saved_when_evicted() {
    let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "A".into());
    // Dirty the active buffer, then push it out of the ≤3 resident window.
    e.handle(Key::Char('i'));
    e.handle(Key::Char('!'));
    e.handle(Key::Escape);
    assert!(e.dirty());
    e.take_effects(); // discard anything queued so far
    e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, "B".into()); // parks A(dirty)
    e.install_loaded("/sd/repo/c.md".into(), Scope::Tracked, "C".into()); // parked: [A,B]
    assert!(e.take_effects().is_empty()); // nothing evicted yet
    e.install_loaded("/sd/repo/d.md".into(), Scope::Tracked, "D".into()); // evicts A
    let effs = e.take_effects();
    assert_eq!(effs.len(), 1, "the evicted dirty buffer must be saved");
    match &effs[0] {
        Effect::Save { path, .. } => assert_eq!(path, "/sd/repo/a.md"),
        other => panic!("expected a Save of A, got {other:?}"),
    }
}

#[test]
fn reboot_autosaves_every_dirty_resident_buffer() {
    // Dirty the active buffer and a parked one, then `:reboot` saves both (active
    // first, then parked) ahead of the restart — the fan-out loses nothing.
    let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "A".into());
    e.handle(Key::Char('i'));
    e.handle(Key::Char('!'));
    e.handle(Key::Escape); // A dirty
    e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, "B".into()); // parks A(dirty)
    e.handle(Key::Char('i'));
    e.handle(Key::Char('!'));
    e.handle(Key::Escape); // B (now active) dirty
    e.take_effects(); // discard anything queued during setup
    ex(&mut e, "reboot");
    confirm(&mut e);
    let effs = e.take_effects();
    assert_eq!(kinds(&effs), vec![Kind::Save, Kind::Save, Kind::Reboot]);
    let saved: Vec<&str> = effs
        .iter()
        .filter_map(|ef| match ef {
            Effect::Save { path, .. } => Some(path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(saved, vec!["/sd/repo/b.md", "/sd/repo/a.md"]);
}

#[test]
fn a_clean_parked_buffer_is_dropped_silently_on_eviction() {
    let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "A".into());
    // A is never edited (clean); filling past ≤3 must evict it without a Save.
    e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, "B".into());
    e.install_loaded("/sd/repo/c.md".into(), Scope::Tracked, "C".into());
    e.take_effects();
    e.install_loaded("/sd/repo/d.md".into(), Scope::Tracked, "D".into());
    assert!(e.take_effects().is_empty()); // clean buffer: no save on evict
}

// ---- :enew / :delete (v0.5 slice 3) ----

#[test]
fn enew_creates_a_dirty_empty_buffer_and_asks_the_host_for_nothing() {
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "A".into());
    ex(&mut e, "enew draft.md");
    assert_eq!(e.path(), "/sd/repo/draft.md"); // bare name → current (Tracked) scope
    assert_eq!(e.scope(), Scope::Tracked);
    assert_eq!(e.text(), "");
    assert!(e.dirty()); // fresh + unsaved, so eviction/`:w` will persist it
    assert_eq!(e.mode(), Mode::Normal);
    // `:enew` allocates no card IO — it neither loads nor saves.
    assert!(e.take_effects().is_empty());
}

#[test]
fn enew_derives_local_scope_from_the_path() {
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "A".into());
    ex(&mut e, "enew local/journal.md");
    assert_eq!(e.path(), "/sd/local/journal.md");
    assert_eq!(e.scope(), Scope::Local);
}

#[test]
fn enew_adds_the_new_file_to_the_palette_list() {
    let mut e = palette_editor(&["/sd/repo/notes.md", "/sd/repo/todo.md"]);
    ex(&mut e, "enew draft.md");
    assert!(files_vec(&e).contains(&"/sd/repo/draft.md".to_string()));
    // and it is findable in the palette without a disk re-enumeration
    e.handle(Key::Palette);
    for c in "draft".chars() {
        e.handle(Key::Char(c));
    }
    assert_eq!(palette_labels(&e), vec!["repo/draft.md"]);
}

#[test]
fn enew_of_an_already_open_file_switches_without_clobbering() {
    let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "AAA".into());
    e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, "BBB".into()); // parks A
    e.take_effects();
    ex(&mut e, "enew /sd/repo/a.md"); // A is parked (resident) — switch, don't empty it
    assert_eq!(e.path(), "/sd/repo/a.md");
    assert_eq!(e.text(), "AAA"); // contents preserved, not clobbered to empty
    assert!(e.take_effects().is_empty()); // resident: no Load
}

#[test]
fn enew_without_a_name_is_a_usage_noop() {
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "A".into());
    ex(&mut e, "enew");
    assert_eq!(e.path(), "/sd/repo/notes.md"); // unchanged
    assert!(e.take_effects().is_empty());
    assert_eq!(e.mode(), Mode::Normal);
}

#[test]
fn delete_prompts_before_touching_anything() {
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, String::new());
    ex(&mut e, "delete");
    // The prompt is up; nothing has happened yet — no effect, file still active.
    assert_eq!(e.mode(), Mode::Confirm);
    assert_eq!(e.path(), "/sd/repo/notes.md");
    assert!(e.take_effects().is_empty());
}

#[test]
fn confirming_the_prompt_queues_the_delete() {
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, String::new());
    ex(&mut e, "delete");
    e.handle(Key::Char('y')); // confirm
    assert_eq!(
        e.take_effects(),
        vec![Effect::Delete {
            path: "/sd/repo/notes.md".into(),
            scope: Scope::Tracked,
        }]
    );
    // No file remains active (nothing else was resident): a scratch buffer.
    assert_eq!(e.path(), "");
    assert_eq!(e.text(), "");
    assert_eq!(e.mode(), Mode::Normal);
}

#[test]
fn cancelling_the_prompt_leaves_the_file_untouched() {
    let mut e = palette_editor(&["/sd/repo/notes.md", "/sd/repo/todo.md"]);
    ex(&mut e, "delete");
    e.handle(Key::Char('n')); // anything but y/Y cancels
    assert_eq!(e.mode(), Mode::Normal);
    assert_eq!(e.path(), "/sd/repo/notes.md"); // still the active file
    assert!(files_vec(&e).contains(&"/sd/repo/notes.md".to_string())); // not dropped
    assert!(e.take_effects().is_empty()); // no Delete queued
}

#[test]
fn esc_at_the_prompt_cancels_too() {
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, String::new());
    ex(&mut e, "delete");
    e.handle(Key::Escape);
    assert_eq!(e.mode(), Mode::Normal);
    assert_eq!(e.path(), "/sd/repo/notes.md");
    assert!(e.take_effects().is_empty());
}

#[test]
fn d_is_an_alias_for_delete() {
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, String::new());
    ex(&mut e, "d"); // the shorthand also prompts...
    assert_eq!(e.mode(), Mode::Confirm);
    e.handle(Key::Char('y')); // ...and deletes on confirm
    assert_eq!(kinds(&e.take_effects()), vec![Kind::Delete]);
}

#[test]
fn delete_never_saves_the_discarded_buffer_even_when_dirty() {
    let mut e = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "A".into());
    e.handle(Key::Char('x')); // dirty it
    assert!(e.dirty());
    ex(&mut e, "delete");
    e.handle(Key::Char('y')); // confirm
    // The buffer is being deleted, so it is discarded, not saved: Delete only.
    assert_eq!(kinds(&e.take_effects()), vec![Kind::Delete]);
}

#[test]
fn delete_switches_to_the_most_recently_parked_buffer() {
    let mut e = Editor::with_file("/sd/repo/a.md".into(), Scope::Tracked, "AAA".into());
    e.install_loaded("/sd/repo/b.md".into(), Scope::Tracked, "BBB".into()); // active B, A parked
    e.take_effects();
    ex(&mut e, "delete"); // deletes B, restores A
    e.handle(Key::Char('y')); // confirm
    assert_eq!(e.path(), "/sd/repo/a.md");
    assert_eq!(e.text(), "AAA"); // A came back from RAM, caret/undo with it
    match &e.take_effects()[..] {
        [Effect::Delete { path, .. }] => assert_eq!(path, "/sd/repo/b.md"),
        other => panic!("expected a single Delete of B, got {other:?}"),
    }
}

#[test]
fn delete_drops_the_file_from_the_palette_list() {
    let mut e = palette_editor(&["/sd/repo/notes.md", "/sd/repo/todo.md"]);
    ex(&mut e, "delete"); // notes.md is active
    e.handle(Key::Char('y')); // confirm
    e.take_effects();
    assert!(!files_vec(&e).contains(&"/sd/repo/notes.md".to_string()));
    e.handle(Key::Palette);
    for c in "md".chars() {
        e.handle(Key::Char(c)); // reach the search threshold
    }
    assert_eq!(palette_labels(&e), vec!["repo/todo.md"]); // only the survivor
}

#[test]
fn delete_of_a_local_file_carries_local_scope() {
    let mut e = Editor::with_file("/sd/local/j.md".into(), Scope::Local, "diary".into());
    ex(&mut e, "delete");
    e.handle(Key::Char('y')); // confirm
    match &e.take_effects()[..] {
        [Effect::Delete { path, scope }] => {
            assert_eq!(path, "/sd/local/j.md");
            assert_eq!(*scope, Scope::Local);
        }
        other => panic!("expected a Local Delete, got {other:?}"),
    }
}

#[test]
fn delete_on_an_unnamed_buffer_is_a_noop() {
    let mut e = Editor::new(); // scratch, empty path — nothing on disk to delete
    ex(&mut e, "delete");
    assert!(e.take_effects().is_empty());
    assert_eq!(e.mode(), Mode::Normal); // no prompt: nothing to delete
}
