#![forbid(unsafe_code)]
use super::*;

use windows_sys::Win32::Storage::FileSystem::{
    FILE_NOTIFY_CHANGE_CREATION, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
};

/// Directory-change wake for the ledger (stop/workspace/resize requests)
/// and settings directories.
///
/// The owner blocks on this handle instead of polling for file changes.
/// Only the wake is event-driven: the files themselves are still read and
/// validated by the existing poll paths (exact-owner admission,
/// consume-before-dispatch, invalid-keeps-last-good), so a notification
/// carries no content and a spurious wake is one cheap turn.
///
/// Two masks, matched to publisher contracts:
///
/// - Ledger (`watched_names`, file-name events only): every request file
///   the owner consumes is published atomically (workspace/resize via
///   `publish_no_overwrite`, stop via `publish_replace`: same-directory
///   temp plus rename), so the rename that wakes the owner always exposes
///   complete content — no partial-read window exists. File-name events
///   therefore catch every create/replace/delete the owner must serve
///   while ignoring the owner's own per-turn trace-log appends
///   (last-write/size only), which previously woke every trace turn.
/// - Settings (`watched`, content plus names): external editors may save
///   in place (last-write with no rename), so content flags stay.
///
/// Race-safe rearm contract: [`DirWatch::rearm`] runs BEFORE the files are
/// inspected on every turn. A write landing between the rearm and the
/// inspection re-signals the handle, and a write landing between the
/// inspection and the next wait leaves it signaled, so the wait returns
/// immediately. The losing order (inspect, then rearm) would drop a write
/// concurrent with the inspection. Own name changes (request consume
/// deletes, atomic publishes, the log's initial create) re-signal
/// harmlessly and boundedly: the next inspection simply finds the request
/// itself or nothing new, and no inspection creates files, so there is no
/// self-wake loop.
///
/// Missing directories, setup failure, and rearm failure all report `None`
/// / `false` (never a wedge, never a spin); the caller degrades to a
/// bounded poll with an explicit reason and retries setup there.
///
/// The raw notification handle closes with `FindCloseChangeNotification`
/// (never `CloseHandle`); ownership lives in `crate::ffi`.
pub(super) struct DirWatch {
    inner: crate::ffi::ChangeNotification,
}

impl DirWatch {
    /// Arm a new watch on `dir` (flat, files and size/time/creation
    /// changes). `None` when the directory cannot be watched. Settings
    /// use: external editors may save in place, so content flags stay.
    pub(super) fn watched(dir: &Path) -> Option<Self> {
        Self::arm(
            dir,
            FILE_NOTIFY_CHANGE_FILE_NAME
                | FILE_NOTIFY_CHANGE_DIR_NAME
                | FILE_NOTIFY_CHANGE_LAST_WRITE
                | FILE_NOTIFY_CHANGE_SIZE
                | FILE_NOTIFY_CHANGE_CREATION,
        )
    }

    /// Arm a name-only watch on `dir` (flat, creates/renames/deletes;
    /// content appends and rewrites never signal). Ledger use: every
    /// consumed request file is published atomically, so the signaled
    /// rename always exposes complete content. `None` when the directory
    /// cannot be watched.
    pub(super) fn watched_names(dir: &Path) -> Option<Self> {
        Self::arm(dir, FILE_NOTIFY_CHANGE_FILE_NAME)
    }

    fn arm(dir: &Path, filter: u32) -> Option<Self> {
        crate::ffi::watch_directory(dir, filter).map(|inner| Self { inner })
    }

    /// Raw waitable notification handle for the owner's wait set.
    pub(super) fn raw(&self) -> windows_sys::Win32::Foundation::HANDLE {
        crate::ffi::watch_raw_handle(&self.inner)
    }

    /// Non-blocking signaled probe (zero-timeout): true when a change is
    /// already pending. Used only by hermetic tests; the loop relies on
    /// the blocking wait set plus the rearm-before-inspect contract.
    #[cfg(test)]
    pub(super) fn pending(&self) -> bool {
        crate::ffi::watch_signaled(&self.inner)
    }

    /// Re-arm after a wait, BEFORE inspecting the files (see the contract
    /// above). Safe to call when nothing signaled (re-baselines without
    /// losing: no change occurred since the last arm). `false` when the
    /// watch broke; the caller drops it and degrades.
    #[must_use]
    pub(super) fn rearm(&self) -> bool {
        crate::ffi::rearm_watch(&self.inner)
    }
}
