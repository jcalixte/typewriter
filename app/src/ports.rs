//! Application ports — the frontiers the run-loop [`Runtime`](crate::Runtime)
//! depends on.
//!
//! Each trait is a contract the application layer needs and the outer firmware
//! layer fulfils (dependency inversion: the port is owned by the consumer). The
//! esp-idf adapters live in the `firmware` crate and are injected at
//! composition; the `Runtime` names only these traits, never a concrete type,
//! so it builds and is tested on the host with in-memory doubles.
//!
//! The hardware-*device* ports ([`Screen`](hal::Screen),
//! [`Keyboard`](hal::Keyboard)) live one layer down in the `hal` crate; the
//! ports here are application/infrastructure capabilities — persistence, the
//! sync transport, the wall clock, platform lifecycle, and the file index.
//!
//! The `git` feature does not reach this layer: a light editor build injects the
//! no-op [`SyncService`]/`System` adapters, a full build the git-backed ones.
//! The `Runtime` is identical either way — the `Skipped`/`Unsupported` variants
//! below carry the difference.

use editor::Date;

/// Durable storage of buffers on the card — the byte-level file operations the
/// loop performs. The dirty-path journal that couples a save to a later publish
/// lives behind [`SyncService`], not here.
pub trait Storage {
    /// Atomically write `contents` to `path`. Errors are surfaced, not fatal:
    /// the in-RAM buffer stays the source of truth for a retry.
    fn save_path(&self, path: &str, contents: &str) -> anyhow::Result<()>;
    /// Read `path` from the card.
    fn load_path(&self, path: &str) -> anyhow::Result<String>;
    /// Unlink `path` from the card.
    fn delete_path(&self, path: &str) -> anyhow::Result<()>;
    /// Record the active file, for the `open_last_on_boot` resume marker.
    fn record_last_file(&self, path: &str);
}

/// What dispatching a publish (`:gp`) did — the loop maps this to a snackbar.
pub enum PublishDispatch {
    /// Handed to the sync backend; the result arrives later via
    /// [`SyncService::poll_outcome`].
    Dispatched,
    /// The backend is gone (thread down); nothing will report back.
    ThreadDown,
    /// No sync backend in this build (light editor build) — a no-op.
    Skipped,
}

/// What dispatching a pull (`:gl`) did.
pub enum PullDispatch {
    Dispatched,
    /// The dirty journal is non-empty: pulling would fold those saved-but-
    /// unpublished paths into a local commit first, so the UI asks the user to
    /// confirm before that happens (the commit is user-visible). On confirm the
    /// UI re-dispatches the pull with `commit_dirty: true`.
    NeedsCommitConfirm,
    ThreadDown,
    Skipped,
}

/// A completed publish, mirrored from the git transport into a git-free shape so
/// the app layer stays pure.
pub enum PublishOutcome {
    /// Pushed a new commit — the short oid.
    Pushed(String),
    UpToDate,
    /// Failed — a ready-to-show reason string.
    Failed(String),
}

/// A completed pull.
pub enum PullOutcome {
    Pulled(String),
    Rebased(String),
    UpToDate,
    LocalAhead,
    Failed(String),
}

/// The outcome of a finished background sync operation.
pub enum SyncOutcome {
    Publish(PublishOutcome),
    Pull(PullOutcome),
}

/// The publish/pull transport (git over the wire), plus the dirty-path journal
/// that gates it. Fire-and-forget: [`publish`](SyncService::publish) /
/// [`pull`](SyncService::pull) dispatch, and the result returns later via
/// [`poll_outcome`](SyncService::poll_outcome). The backend owns the dirty
/// journal — it takes the pending paths on publish (and on a committing pull)
/// and settles them when the outcome lands — so the app layer never touches it.
pub trait SyncService {
    /// Dispatch a publish of the whole Tracked working copy.
    fn publish(&self) -> PublishDispatch;
    /// Dispatch a fetch + fast-forward/rebase pull. `commit_dirty` false is a
    /// bare `:gl`: if the dirty journal is non-empty it returns
    /// [`NeedsCommitConfirm`](PullDispatch::NeedsCommitConfirm) instead of
    /// dispatching, so the UI can confirm the commit first. `commit_dirty` true
    /// (the confirmed retry) folds the journal into a local commit, then pulls.
    fn pull(&self, commit_dirty: bool) -> PullDispatch;
    /// Non-blocking poll for a finished operation. The backend has already
    /// settled the dirty journal by the time this returns.
    fn poll_outcome(&self) -> Option<SyncOutcome>;
}

/// The wall clock and the idle CPU-yield the loop needs. `today` is `None` until
/// the clock is trustworthy — there is no battery-backed RTC, so it sits at the
/// epoch until the first sync sets it (see [`editor::Date`]).
pub trait Clock {
    /// Today's calendar day, or `None` while the clock is unset.
    fn today(&self) -> Option<Date>;
    /// Briefly yield the CPU when the idle loop has nothing to paint.
    fn idle_yield(&self);
}

/// What preparing a `:setup` reboot did.
pub enum SetupDispatch {
    /// Marker written; the caller paints the notice, then calls
    /// [`System::reboot`].
    Ready,
    /// Could not persist the setup marker — stay put and report it.
    MarkerFailed,
    /// This build has no wizard to reboot into (light editor build).
    Unsupported,
}

/// Platform lifecycle: the device restart, and preparing a reboot-into-setup.
pub trait System {
    /// Prepare a `:setup` reboot (persist the boot marker). See [`SetupDispatch`].
    fn prepare_setup(&self) -> SetupDispatch;
    /// Restart the device. Never returns.
    fn reboot(&self) -> !;
}

/// The palette's background file index — a recursive walk of the card, run off
/// the UI loop on its own thread. [`request_rewalk`](FileIndex::request_rewalk)
/// kicks a fresh walk; [`poll_result`](FileIndex::poll_result) picks up a
/// finished one as a newline-joined path blob.
pub trait FileIndex {
    /// Spawn a fresh walk (at boot, and after a pull moves the working copy).
    fn request_rewalk(&self);
    /// A finished walk's path blob, if one is ready.
    fn poll_result(&self) -> Option<String>;
}
