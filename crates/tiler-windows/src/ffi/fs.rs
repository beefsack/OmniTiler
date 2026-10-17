//! Directory-change notification watches.
//!
//! The notification handle closes with `FindCloseChangeNotification`
//! (never `CloseHandle`), so it is owned here with the matching `Drop`.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

/// Owned directory-change notification handle.
pub struct ChangeNotification {
    raw: HANDLE,
}

impl Drop for ChangeNotification {
    fn drop(&mut self) {
        unsafe {
            FindCloseChangeNotification(self.raw);
        }
    }
}

/// Arm a flat watch on `dir` with `filter`; `None` when the directory
/// cannot be watched. Wide-string scratch is owned here.
pub fn watch_directory(dir: &Path, filter: u32) -> Option<ChangeNotification> {
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
    let raw = unsafe { FindFirstChangeNotificationW(wide.as_ptr(), 0, filter) };
    if raw.is_null() || raw == INVALID_HANDLE_VALUE {
        return None;
    }
    Some(ChangeNotification { raw })
}

/// Raw waitable notification handle for the owner's wait set. Borrowed
/// from the owned watch; the wait only observes.
pub fn watch_raw_handle(watch: &ChangeNotification) -> HANDLE {
    watch.raw
}

/// Non-blocking signaled probe (zero-timeout).
pub fn watch_signaled(watch: &ChangeNotification) -> bool {
    unsafe { WaitForSingleObject(watch.raw, 0) == 0 }
}

/// Re-arm after a wait, BEFORE inspecting the files (the caller's
/// race-safe contract). `false` when the watch broke.
#[must_use]
pub fn rearm_watch(watch: &ChangeNotification) -> bool {
    unsafe { FindNextChangeNotification(watch.raw) != 0 }
}

fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain([0]).collect()
}

/// Atomically move `from` onto `to`, replacing any existing `to` with
/// write-through semantics. Readers see the old or the new complete body,
/// never a mix. `Err` carries the OS error at call time.
pub fn move_file_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    let from_w = wide_path(from);
    let to_w = wide_path(to);
    let ok = unsafe {
        MoveFileExW(
            from_w.as_ptr(),
            to_w.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Atomically move `from` onto `to` without replacing: a present `to`
/// fails (the OS `AlreadyExists` error) instead of being overwritten, so
/// the queued body is never torn or replaced. `Err` carries the OS error
/// at call time.
pub fn move_file_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    let from_w = wide_path(from);
    let to_w = wide_path(to);
    let ok = unsafe { MoveFileExW(from_w.as_ptr(), to_w.as_ptr(), MOVEFILE_WRITE_THROUGH) };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
