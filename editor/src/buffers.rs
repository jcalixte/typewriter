//! Multi-buffer management: the active/parked buffer set, the file
//! registry and MRU list, and path resolution between the repo and local scopes.

use super::*;

/// Tracked files live here (the git working copy).
pub const REPO_DIR: &str = "/sd/repo";
/// Local files live here (never published).
pub const LOCAL_DIR: &str = "/sd/local";

/// Resolve a `:e`/`:enew` argument (or palette pick) to an absolute path +
/// [`Scope`]. Everything the writer can reach lives on the card under `/sd`, so
/// the `/sd` prefix is **optional**: `/sd/repo/x`, `/repo/x`, and `repo/x` all
/// name the same file, and nothing resolves outside `/sd`. The arg is normalized
/// to a scope-relative form (peel an optional `/sd`, then an optional leading
/// `/`), then:
/// - a leading `local/` or `repo/` segment **selects the scope** and names the
///   file in it — the same labels the palette shows (`local/journal.md`,
///   `repo/notes.md`), so a name read off the palette is typeable verbatim. Safe
///   because scopes are flat: there are no real `local/`/`repo/` subdirectories;
/// - otherwise a bare name joins the **current** buffer's scope directory, so
///   `:e draft.md` opens a sibling of the file you're in.
pub(crate) fn resolve_path(arg: &str, current: Scope) -> (String, Scope) {
    // Peel the optional `/sd` prefix, then an optional leading `/`, leaving a
    // scope-relative remainder (`repo/…`, `local/…`, or a bare name).
    let rel = arg
        .strip_prefix("/sd/")
        .or_else(|| arg.strip_prefix('/'))
        .unwrap_or(arg);
    if let Some(name) = rel.strip_prefix("local/") {
        (format!("{LOCAL_DIR}/{name}"), Scope::Local)
    } else if let Some(name) = rel.strip_prefix("repo/") {
        (format!("{REPO_DIR}/{name}"), Scope::Tracked)
    } else {
        let dir = match current {
            Scope::Tracked => REPO_DIR,
            Scope::Local => LOCAL_DIR,
        };
        (format!("{dir}/{rel}"), current)
    }
}


/// A resident-but-inactive buffer: everything needed to restore a file's editing
/// state when the user switches back, without re-reading the disk. The active
/// buffer holds these same fields inline on [`Editor`]; parking marshals them
/// out to here, activation marshals them back.
pub(crate) struct Buffer {
    pub(crate) path: String,
    scope: Scope,
    text: String,
    caret: usize,
    scroll_top: usize,
    pub(crate) dirty: bool,
    undo: Vec<(String, usize)>,
    redo: Vec<(String, usize)>,
}

/// Buffers kept resident at once — the active one plus [`MAX_RESIDENT`] − 1
/// parked (v0.5 keeps ≤ 3). Beyond this the least-recently-used parked buffer is
/// evicted; it is saved first if dirty, so an evicted buffer is never lost.
pub(crate) const MAX_RESIDENT: usize = 3;

/// Recent-files (MRU) list length — how many opens the palette remembers; they
/// are the whole result list below [`PALETTE_MIN_QUERY`] chars and float to the
/// top above it. Far more than [`MAX_RESIDENT`] (recency
/// outlives residency: a file evicted from memory is still recently *used*), but
/// bounded so the list can't grow without limit over a long session.
pub(crate) const MRU_MAX: usize = 16;


impl Editor {
    /// The host confirms `path` was persisted; clear its dirty flag wherever that
    /// buffer is resident (active or parked). A no-op for a path that is no longer
    /// in memory (already-evicted buffers were saved on the way out).
    pub fn mark_saved(&mut self, path: &str) {
        if self.path == path {
            self.dirty = false;
        }
        if let Some(b) = self.parked.iter_mut().find(|b| b.path == path) {
            b.dirty = false;
        }
    }

    /// Install a file the host read from disk in response to an [`Effect::Load`]:
    /// park the current buffer and make the loaded one active. If the target
    /// turned resident in the meantime, switch to that copy instead (its in-memory
    /// edits win over a stale disk read).
    pub fn install_loaded(&mut self, path: String, scope: Scope, contents: String) {
        if path == self.path {
            return;
        }
        if self.parked.iter().any(|b| b.path == path) {
            self.open_path(path, scope);
            return;
        }
        self.park_active();
        self.set_active(path, scope, contents);
    }

    /// Replace the active buffer's contents after the file changed on disk
    /// underneath us — a `:gl` pull fast-forwarded the working copy. Same boot
    /// posture as a fresh load (Normal, caret on the last char, clean, no undo
    /// history — the old snapshots reference the replaced text). The host only
    /// calls this when the buffer is clean; a dirty buffer's RAM edits win
    /// (last-writer-wins, like the reconcile path).
    pub fn refresh_active(&mut self, contents: String) {
        let (path, scope) = (self.path.clone(), self.scope);
        self.set_active(path, scope, contents);
    }

    /// Drop every *clean* parked buffer, so the next switch to one re-reads the
    /// disk ([`Effect::Load`]) instead of resurrecting a stale resident copy —
    /// a `:gl` pull may have rewritten any tracked file. Dirty parked buffers
    /// are kept: their unsaved edits win over the pulled state, exactly like
    /// the active buffer's.
    pub fn drop_clean_parked(&mut self) {
        self.parked.retain(|b| b.dirty);
    }

    /// Queue an [`Effect::Save`] of the active buffer. Posts "no file name" for an
    /// unnamed scratch buffer (nothing to save to) rather than writing to `""`.
    pub(crate) fn request_save_active(&mut self) {
        if self.path.is_empty() {
            self.set_notice("no file name");
            return;
        }
        self.requests.push(Effect::Save {
            path: self.path.clone(),
            scope: self.scope,
            contents: self.text.clone(),
        });
    }

    /// Switch the active buffer to `path`. If it is already resident (parked),
    /// restore that copy with its caret/scroll/undo intact — no disk read. If it
    /// is not resident, queue an [`Effect::Load`]; the host reads the file and
    /// calls [`install_loaded`](Self::install_loaded), which does the park + swap.
    /// A dirty outgoing buffer is preserved in RAM (parked) and persisted only
    /// when it is later evicted, so switching itself never blocks on IO.
    pub(crate) fn open_path(&mut self, path: String, scope: Scope) {
        if path == self.path {
            return; // already the active buffer
        }
        self.note_recent(&path); // float it to the top of the palette's MRU
        match self.parked.iter().position(|b| b.path == path) {
            Some(i) => {
                let target = self.parked.remove(i);
                self.park_active();
                self.activate(target);
            }
            None => self.requests.push(Effect::Load { path, scope }),
        }
    }

    /// Move the active buffer's editing state into a parked [`Buffer`], leaving
    /// the active fields empty for a subsequent [`activate`](Self::activate) or
    /// [`set_active`](Self::set_active). Evicts the least-recently-used parked
    /// buffer if that pushes residency over [`MAX_RESIDENT`]; an evicted dirty
    /// buffer queues a [`Effect::Save`] so no unsaved work leaves memory.
    pub(crate) fn park_active(&mut self) {
        let buf = Buffer {
            path: core::mem::take(&mut self.path),
            scope: self.scope,
            text: core::mem::take(&mut self.text),
            caret: self.caret,
            scroll_top: self.scroll_top,
            dirty: self.dirty,
            undo: core::mem::take(&mut self.undo),
            redo: core::mem::take(&mut self.redo),
        };
        self.parked.push(buf);
        // Active is currently empty, so residency == parked.len(); keep it under
        // MAX_RESIDENT so the buffer about to become active fits.
        while self.parked.len() >= MAX_RESIDENT {
            let evicted = self.parked.remove(0);
            if evicted.dirty {
                self.requests.push(Effect::Save {
                    path: evicted.path,
                    scope: evicted.scope,
                    contents: evicted.text,
                });
            }
        }
    }

    /// Restore a parked buffer into the active fields (its caret, scroll, undo,
    /// and dirty flag come back with it). Lands in Normal with input state reset.
    pub(crate) fn activate(&mut self, b: Buffer) {
        self.path = b.path;
        self.scope = b.scope;
        self.text = b.text;
        self.caret = b.caret;
        self.scroll_top = b.scroll_top;
        self.dirty = b.dirty;
        self.undo = b.undo;
        self.redo = b.redo;
        self.reset_active_input();
    }

    /// Make a freshly-loaded file the active buffer: same boot posture as
    /// [`with_file`](Self::with_file) (Normal, caret on the last char) with empty
    /// undo history and a clean dirty flag.
    pub(crate) fn set_active(&mut self, path: String, scope: Scope, text: String) {
        self.path = path;
        self.scope = scope;
        self.text = text;
        self.caret = self.text.len();
        if self.caret > self.line_start(self.caret) {
            self.caret = self.prev_char(self.caret);
        }
        self.scroll_top = 0;
        self.dirty = false;
        self.undo.clear();
        self.redo.clear();
        self.reset_active_input();
    }

    /// Reset the transient per-keystroke input state (mode, pending operator,
    /// visual anchor, command line) on a buffer swap, so nothing leaks across.
    /// The register and `.` history are deliberately left alone — they are global
    /// (vim-like), so a yank in one file pastes in another.
    pub(crate) fn reset_active_input(&mut self) {
        self.mode = Mode::Normal;
        self.visual_anchor = None;
        self.cmdline.clear();
        self.reset_pending();
    }


    /// `:enew <arg>` — create a new file and make it the active buffer. Scope is
    /// read from the path exactly like `:e` (`local/…` → Local, else Tracked;
    /// a bare name lands in the current buffer's scope), so no scope prompt is
    /// needed — the resolved scope is echoed in the snackbar instead. If the name
    /// already resolves to the active or a parked buffer, this just switches to it
    /// (no clobber); otherwise the buffer starts empty and **dirty**, so it is
    /// durable (a later eviction or `:w` persists it) and shows in the palette at
    /// once. The file is not written to disk until then — `:enew` alone allocates
    /// no card IO.
    pub(crate) fn new_file(&mut self, arg: &str) {
        let arg = arg.trim();
        if arg.is_empty() {
            self.set_notice("usage: :enew <file>");
            return;
        }
        let (path, scope) = resolve_path(arg, self.scope);
        // Already open (active or parked) — treat `:enew` of an existing name as a
        // switch rather than replacing its contents with an empty buffer.
        if path == self.path || self.parked.iter().any(|b| b.path == path) {
            self.open_path(path, scope);
            return;
        }
        self.note_recent(&path);
        self.add_to_file_list(&path);
        self.park_active();
        self.set_active(path.clone(), scope, String::new());
        // A fresh file is unsaved: mark it dirty so eviction/`:w` persists it and
        // it never silently vanishes (unlike an `:e` of a missing name).
        self.dirty = true;
        self.set_notice(format!("new {}", palette_label(&path)));
    }

    /// `:delete` — unlink the **current** file from the card and leave it. Queues
    /// an [`Effect::Delete`] (the host does the removal + reports the outcome) and
    /// updates the in-core model now: the path is dropped from the file list and
    /// MRU, and the active buffer switches to the most-recently-parked buffer, or
    /// an empty unnamed scratch if none is resident. An unnamed scratch buffer has
    /// nothing on disk, so it is a no-op with a notice. Deleting an arbitrary
    /// (non-current) file is deferred — this is the file you are looking at.
    pub(crate) fn delete_current(&mut self) {
        if self.path.is_empty() {
            self.set_notice("no file to delete");
            return;
        }
        let path = core::mem::take(&mut self.path);
        let scope = self.scope;
        self.requests.push(Effect::Delete { path: path.clone(), scope });
        self.remove_from_file_list(&path);
        self.recent.retain(|p| p != &path);
        // The current buffer is being discarded, not parked: restore the most
        // recently parked buffer if one is resident, else fall back to scratch.
        match self.parked.pop() {
            Some(b) => {
                self.note_recent(&b.path);
                self.activate(b);
            }
            None => self.set_active(String::new(), Scope::Tracked, String::new()),
        }
    }

    /// The `i`-th file path in the palette's sorted base order (a slice into
    /// [`file_blob`](Self::file_blob)).
    pub(crate) fn file_at(&self, i: usize) -> &str {
        let (s, e) = self.file_spans[i];
        &self.file_blob[s as usize..e as usize]
    }

    /// How many files the palette knows about.
    pub(crate) fn file_count(&self) -> usize {
        self.file_spans.len()
    }

    /// Insert `path` into the palette's file list, keeping the spans sorted and
    /// unique (matches [`set_file_list_joined`](Self::set_file_list_joined)'s
    /// invariant). Used by `:enew` so a just-created file is findable without a
    /// disk re-enumeration. Appends to the blob; a `String` realloc only moves
    /// bytes, the spans are indices and stay valid.
    pub(crate) fn add_to_file_list(&mut self, path: &str) {
        match self
            .file_spans
            .binary_search_by(|&(s, e)| self.file_blob[s as usize..e as usize].cmp(path))
        {
            Ok(_) => {}
            Err(i) => {
                let start = self.file_blob.len() as u32;
                self.file_blob.push_str(path);
                self.file_spans.insert(i, (start, start + path.len() as u32));
            }
        }
    }

    /// Drop `path` from the palette's file list (used by `:delete`). Only the
    /// span goes; its bytes stay in the blob as dead weight until the next
    /// host re-walk replaces the whole thing — a few dozen bytes at most.
    pub(crate) fn remove_from_file_list(&mut self, path: &str) {
        let blob = &self.file_blob;
        self.file_spans
            .retain(|&(s, e)| &blob[s as usize..e as usize] != path);
    }

    // --- File palette (Ctrl-P) ---------------------------------------------

    /// Feed the palette its file list as **one newline-joined blob** of
    /// absolute paths, enumerated by the host from `/sd/repo` and `/sd/local`.
    /// This is the device's entry point: a single large `String` lands in
    /// PSRAM (allocations ≥ 16 KB cross the SPIRAM-malloc threshold), where
    /// the same list as 1099 individual `String`s measured 182 KB of internal
    /// DRAM. Spans are sorted + deduped for a stable base order; the MRU
    /// floats recents above it. The palette is a pure view over this — nothing
    /// is read from disk until a file is actually opened.
    pub fn set_file_list_joined(&mut self, blob: String) {
        let mut spans: Vec<(u32, u32)> = Vec::new();
        let mut start = 0u32;
        for line in blob.split('\n') {
            let end = start + line.len() as u32;
            if !line.is_empty() {
                spans.push((start, end));
            }
            start = end + 1; // past the '\n'
        }
        spans.sort_by(|&(a, b), &(c, d)| blob[a as usize..b as usize].cmp(&blob[c as usize..d as usize]));
        spans.dedup_by(|&mut (a, b), &mut (c, d)| blob[a as usize..b as usize] == blob[c as usize..d as usize]);
        self.file_blob = blob;
        self.file_spans = spans;
    }

    /// [`set_file_list_joined`](Self::set_file_list_joined) from a `Vec` —
    /// convenience for hosts/tests that already hold separate strings.
    pub fn set_file_list(&mut self, files: Vec<String>) {
        self.set_file_list_joined(files.join("\n"));
    }

    /// Push `path` to the front of the recent-files MRU (dropping any earlier
    /// occurrence), bounded to [`MRU_MAX`]. Drives the palette's empty-query
    /// order, so the file you were just in sits at the top.
    pub(crate) fn note_recent(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(MRU_MAX);
    }

}
