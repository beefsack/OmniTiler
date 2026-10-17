//! Shell launch verbs with owned string buffers.

use std::os::windows::ffi::OsStrExt;

/// Open one `https://` URL with the default browser (`open` verb,
/// `SW_SHOWNORMAL`). `Err(rc)` carries the `ShellExecuteW` return when it
/// reports failure (`<= 32`), exactly as before.
pub fn open_url(url: &str) -> Result<(), usize> {
    let op: Vec<u16> = std::ffi::OsStr::new("open")
        .encode_wide()
        .chain([0])
        .collect();
    let target: Vec<u16> = std::ffi::OsStr::new(url).encode_wide().chain([0]).collect();
    let rc = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1, // SW_SHOWNORMAL
        )
    };
    if (rc as usize) <= 32 {
        return Err(rc as usize);
    }
    Ok(())
}
