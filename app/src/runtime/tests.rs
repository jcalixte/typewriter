//! Host tests for the lifted run loop — the routing that used to be untestable
//! inline in the firmware binary. In-memory doubles stand in for every port, so
//! these run on the host with no esp-idf.

use std::cell::RefCell;
use std::convert::Infallible;
use std::rc::Rc;

use editor::{Editor, Effect, Scope};

use super::*;
use crate::ports::{
    Clock, FileIndex, PushDispatch, PushOutcome, PullDispatch, PullOutcome, SetupDispatch,
    Storage, NetOutcome, NetService, System, UpdateDispatch, UpdateOutcome,
};
use crate::render::Panel;

// ---- test doubles ---------------------------------------------------------

/// A screen that accepts every frame — the render engine's paints are no-ops.
struct MockScreen;
impl hal::Screen for MockScreen {
    type Error = Infallible;
    fn display_frame(&mut self, _fb: &[u8]) -> Result<(), Infallible> {
        Ok(())
    }
    fn display_frame_partial_window(
        &mut self,
        _fb: &[u8],
        _y0: u16,
        _h: u16,
    ) -> Result<(), Infallible> {
        Ok(())
    }
}

/// A keyboard with nothing queued and nothing attached.
struct NoKeyboard;
impl hal::Keyboard for NoKeyboard {
    fn next_key(&mut self) -> Option<hal::Key> {
        None
    }
    fn keyboard_present(&self) -> bool {
        false
    }
}

#[derive(Default)]
struct StorageLog {
    saves: Vec<(String, String)>,
    loads: Vec<String>,
    deletes: Vec<String>,
    last_files: Vec<String>,
}

/// Records every call; `load_path` echoes a canned body back.
#[derive(Clone, Default)]
struct RecStorage(Rc<RefCell<StorageLog>>);
impl Storage for RecStorage {
    fn save_path(&self, path: &str, contents: &str) -> anyhow::Result<()> {
        self.0.borrow_mut().saves.push((path.into(), contents.into()));
        Ok(())
    }
    fn load_path(&self, path: &str) -> anyhow::Result<String> {
        self.0.borrow_mut().loads.push(path.into());
        Ok("loaded-body".into())
    }
    fn delete_path(&self, path: &str) -> anyhow::Result<()> {
        self.0.borrow_mut().deletes.push(path.into());
        Ok(())
    }
    fn record_last_file(&self, path: &str) {
        self.0.borrow_mut().last_files.push(path.into());
    }
}

#[derive(Default)]
struct SyncLog {
    pushes: u32,
    pulls: u32,
    updates: u32,
    outcome: Option<NetOutcome>,
}

/// Configurable dispatch results + a single queued outcome.
#[derive(Clone)]
struct RecSync {
    log: Rc<RefCell<SyncLog>>,
    push_ret: Rc<dyn Fn() -> PushDispatch>,
    pull_ret: Rc<dyn Fn() -> PullDispatch>,
    update_ret: Rc<dyn Fn() -> UpdateDispatch>,
}
impl RecSync {
    fn new() -> Self {
        Self {
            log: Rc::new(RefCell::new(SyncLog::default())),
            push_ret: Rc::new(|| PushDispatch::Dispatched),
            pull_ret: Rc::new(|| PullDispatch::Dispatched),
            update_ret: Rc::new(|| UpdateDispatch::Dispatched),
        }
    }
}
impl NetService for RecSync {
    fn push(&self) -> PushDispatch {
        self.log.borrow_mut().pushes += 1;
        (self.push_ret)()
    }
    fn pull(&self, _commit_dirty: bool) -> PullDispatch {
        self.log.borrow_mut().pulls += 1;
        (self.pull_ret)()
    }
    fn update(&self) -> UpdateDispatch {
        self.log.borrow_mut().updates += 1;
        (self.update_ret)()
    }
    fn poll_outcome(&self) -> Option<NetOutcome> {
        self.log.borrow_mut().outcome.take()
    }
}

struct FixedClock;
impl Clock for FixedClock {
    fn today(&self) -> Option<editor::Date> {
        None
    }
    fn idle_yield(&self) {}
}

struct PanicSystem;
impl System for PanicSystem {
    fn prepare_setup(&self) -> SetupDispatch {
        SetupDispatch::MarkerFailed
    }
    fn reboot(&self) -> ! {
        panic!("reboot in test")
    }
}

#[derive(Clone, Default)]
struct RecFiles(Rc<RefCell<u32>>);
impl FileIndex for RecFiles {
    fn request_rewalk(&self) {
        *self.0.borrow_mut() += 1;
    }
    fn poll_result(&self) -> Option<String> {
        None
    }
}

/// Build a runtime around the given storage/sync/files, defaulting the rest.
fn runtime(
    ed: Editor,
    storage: RecStorage,
    sync: RecSync,
    files: RecFiles,
) -> Runtime<MockScreen> {
    let mut ed = ed;
    let panel = Panel::new(MockScreen, &mut ed).expect("first paint");
    Runtime::new(
        ed,
        panel,
        Box::new(NoKeyboard),
        Box::new(storage),
        Box::new(sync),
        Box::new(FixedClock),
        Box::new(PanicSystem),
        Box::new(files),
    )
}

// ---- pure helpers ---------------------------------------------------------

#[test]
fn file_stem_strips_dir_and_extension() {
    assert_eq!(file_stem("/sd/repo/notes.md"), "notes");
    assert_eq!(file_stem("/sd/local/2026-07-18.md"), "2026-07-18");
    assert_eq!(file_stem("bare"), "bare");
}

#[test]
fn push_notice_covers_every_variant() {
    assert_eq!(push_notice(&PushOutcome::Pushed("abc123".into())), "synced abc123");
    assert_eq!(push_notice(&PushOutcome::UpToDate), "up to date");
    assert_eq!(push_notice(&PushOutcome::Failed("no wifi".into())), "no wifi");
}

#[test]
fn pull_notice_covers_every_variant() {
    assert_eq!(pull_notice(&PullOutcome::Pulled("abc".into())), "pulled abc");
    assert_eq!(pull_notice(&PullOutcome::Rebased("def".into())), "rebased def - :gp to push");
    assert_eq!(pull_notice(&PullOutcome::UpToDate), "up to date");
    assert_eq!(pull_notice(&PullOutcome::LocalAhead), "ahead - :gp to push");
    assert_eq!(pull_notice(&PullOutcome::Failed("boom".into())), "boom");
}

// ---- effect routing -------------------------------------------------------

#[test]
fn save_effect_writes_through_storage() {
    let storage = RecStorage::default();
    let mut rt = runtime(Editor::new(), storage.clone(), RecSync::new(), RecFiles::default());
    rt.service_one(Effect::Save {
        path: "/sd/repo/notes.md".into(),
        scope: Scope::Tracked,
        contents: "hello".into(),
    });
    assert_eq!(storage.0.borrow().saves, vec![("/sd/repo/notes.md".into(), "hello".into())]);
}

#[test]
fn save_prefs_effect_writes_the_prefs_path() {
    let storage = RecStorage::default();
    let mut rt = runtime(Editor::new(), storage.clone(), RecSync::new(), RecFiles::default());
    rt.service_one(Effect::SavePrefs { contents: "line_numbers = true\n".into() });
    let saves = &storage.0.borrow().saves;
    assert_eq!(saves.len(), 1);
    assert_eq!(saves[0].0, editor::PREFS_PATH);
}

#[test]
fn delete_effect_unlinks_through_storage() {
    let storage = RecStorage::default();
    let mut rt = runtime(Editor::new(), storage.clone(), RecSync::new(), RecFiles::default());
    rt.service_one(Effect::Delete { path: "/sd/local/scratch.md".into(), scope: Scope::Local });
    assert_eq!(storage.0.borrow().deletes, vec!["/sd/local/scratch.md".to_string()]);
}

#[test]
fn rename_effect_writes_the_new_path_then_unlinks_the_old() {
    // `:pub`/`:publish` is a write-new + unlink-old at the storage layer, so the
    // file is never missing and both paths land in the dirty journal for `:gp`.
    let storage = RecStorage::default();
    let ed = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "body".into());
    let mut rt = runtime(ed, storage.clone(), RecSync::new(), RecFiles::default());
    rt.service_one(Effect::Rename {
        from: "/sd/repo/notes.md".into(),
        to: "/sd/repo/notes.pub.md".into(),
        contents: "body".into(),
    });
    let log = storage.0.borrow();
    assert_eq!(log.saves, vec![("/sd/repo/notes.pub.md".into(), "body".into())]);
    assert_eq!(log.deletes, vec!["/sd/repo/notes.md".to_string()]);
}

#[test]
fn push_effect_dispatches_to_sync() {
    let sync = RecSync::new();
    let mut rt = runtime(Editor::new(), RecStorage::default(), sync.clone(), RecFiles::default());
    rt.service_one(Effect::Push);
    assert_eq!(sync.log.borrow().pushes, 1);
}

#[test]
fn pull_effect_dispatches_to_sync() {
    let sync = RecSync::new();
    let mut rt = runtime(Editor::new(), RecStorage::default(), sync.clone(), RecFiles::default());
    rt.service_one(Effect::Pull { commit_dirty: false });
    assert_eq!(sync.log.borrow().pulls, 1);
}

#[test]
fn pull_with_unsynced_saves_opens_the_commit_confirm() {
    // The backend reports NeedsCommitConfirm when the dirty journal is non-empty;
    // the runtime must open the editor's y/n prompt rather than dispatch or fail.
    let sync = RecSync {
        pull_ret: Rc::new(|| PullDispatch::NeedsCommitConfirm),
        ..RecSync::new()
    };
    let mut rt = runtime(Editor::new(), RecStorage::default(), sync, RecFiles::default());
    rt.service_one(Effect::Pull { commit_dirty: false });
    assert_eq!(rt.ed.mode(), editor::Mode::Confirm, "unsynced :gl must prompt");
}

// ---- sync outcome ---------------------------------------------------------

#[test]
fn pull_that_moves_the_tree_reloads_active_and_rewalks() {
    let storage = RecStorage::default();
    let files = RecFiles::default();
    // A clean, named active buffer — a moving pull re-reads it from disk.
    let ed = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "old".into());
    let mut rt = runtime(ed, storage.clone(), RecSync::new(), files.clone());

    rt.handle_net_outcome(NetOutcome::Pull(PullOutcome::Pulled("abc".into())));

    assert_eq!(storage.0.borrow().loads, vec!["/sd/repo/notes.md".to_string()]);
    assert_eq!(*files.0.borrow(), 1, "palette should be re-walked after a moving pull");
}

#[test]
fn up_to_date_pull_leaves_the_tree_untouched() {
    let storage = RecStorage::default();
    let files = RecFiles::default();
    let ed = Editor::with_file("/sd/repo/notes.md".into(), Scope::Tracked, "old".into());
    let mut rt = runtime(ed, storage.clone(), RecSync::new(), files.clone());

    rt.handle_net_outcome(NetOutcome::Pull(PullOutcome::UpToDate));

    assert!(storage.0.borrow().loads.is_empty(), "no reload when the tree didn't move");
    assert_eq!(*files.0.borrow(), 0, "no re-walk when the tree didn't move");
}

// ---- firmware update ------------------------------------------------------

#[test]
fn update_effect_dispatches_to_sync() {
    let sync = RecSync::new();
    let mut rt = runtime(Editor::new(), RecStorage::default(), sync.clone(), RecFiles::default());
    rt.service_one(Effect::Update);
    assert_eq!(sync.log.borrow().updates, 1);
}

#[test]
#[should_panic(expected = "reboot in test")]
fn installed_update_reboots_into_the_new_image() {
    // A successful install makes the new slot the boot target; the runtime must
    // reboot into it. PanicSystem's reboot panics, which is the reboot signal here.
    let mut rt =
        runtime(Editor::new(), RecStorage::default(), RecSync::new(), RecFiles::default());
    rt.handle_net_outcome(NetOutcome::Update(UpdateOutcome::Installed("0.8.0".into())));
}

#[test]
fn up_to_date_update_does_not_reboot() {
    // Already newest → a notice, no restart. The test completing (PanicSystem's
    // reboot never fires) is the assertion; Failed takes the same non-reboot path.
    let mut rt =
        runtime(Editor::new(), RecStorage::default(), RecSync::new(), RecFiles::default());
    rt.handle_net_outcome(NetOutcome::Update(UpdateOutcome::UpToDate("0.7.7".into())));
    rt.handle_net_outcome(NetOutcome::Update(UpdateOutcome::Failed("no wifi".into())));
}
