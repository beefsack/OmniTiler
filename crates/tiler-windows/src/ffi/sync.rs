//! Auto-reset event creation, signalling, and probing.
//!
//! Events are owned `OwnedHandle`s; no raw handle escapes except as a
//! borrowed value held by its owner.

use std::os::windows::io::{AsHandle as _, AsRawHandle as _, FromRawHandle as _, OwnedHandle};

use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};

/// Create a non-signaled auto-reset event; `None` when creation fails.
pub fn create_auto_reset_event() -> Option<OwnedHandle> {
    let raw = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
    if raw.is_null() {
        return None;
    }
    Some(unsafe { OwnedHandle::from_raw_handle(raw) })
}

/// Signal one owned event.
pub fn signal_event(handle: &OwnedHandle) -> bool {
    unsafe { SetEvent(handle.as_handle().as_raw_handle()) != 0 }
}

/// Zero-timeout signaled probe (auto-reset: observing consumes it).
pub fn event_signaled(handle: &OwnedHandle) -> bool {
    unsafe {
        WaitForSingleObject(handle.as_handle().as_raw_handle(), 0)
            == windows_sys::Win32::Foundation::WAIT_OBJECT_0
    }
}
