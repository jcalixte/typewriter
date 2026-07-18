//! The light-build sync adapter — publish/pull are inert.
//!
//! A light editor build (no `git` feature) carries no libgit2 and no transport,
//! so `:gp`/`:gl` log the same "skipped" line the old inline path did and report
//! nothing back. The full build uses [`crate::infrastructure::sync_git`] instead.

/// [`app::SyncService`] for a light editor build (no `git` feature): publish and
/// pull are inert, logging the same "skipped" line the old inline path did.
pub struct NullSyncService;

impl app::SyncService for NullSyncService {
    fn publish(&self) -> app::PublishDispatch {
        log::info!(":gp — saved; light build (no `git` feature) — push skipped");
        app::PublishDispatch::Skipped
    }
    fn pull(&self, _commit_dirty: bool) -> app::PullDispatch {
        log::info!(":gl — light build (no `git` feature) — pull skipped");
        app::PullDispatch::Skipped
    }
    fn poll_outcome(&self) -> Option<app::SyncOutcome> {
        None
    }
}
