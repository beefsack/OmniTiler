//! Session-only WinArranging parameter reads/writes.
//!
//! The `SystemParametersInfoW` buffers stay inside these functions. The
//! setter omits `SPIF_UPDATEINIFILE` (no profile write) and passes
//! `SPIF_SENDCHANGE`, matching the previous call sites; every setter pairs
//! with an exact readback by the caller.

use windows_sys::Win32::UI::WindowsAndMessaging::{
    SPI_GETWINARRANGING, SPI_SETWINARRANGING, SPIF_SENDCHANGE, SystemParametersInfoW,
};

/// Exact live `SPI_GETWINARRANGING` boolean; `None` on API error (fail
/// closed, as before).
pub fn get_win_arranging() -> Option<bool> {
    let mut value: i32 = 0;
    let ok = unsafe {
        SystemParametersInfoW(SPI_GETWINARRANGING, 0, (&mut value as *mut i32).cast(), 0)
    };
    if ok == 0 {
        return None;
    }
    Some(value != 0)
}

/// Session-only `SPI_SETWINARRANGING` write with the `uiParam` boolean
/// encoding and a NULL `pvParam` (the `SPI_SETBEEP`/`SPI_SETSHOWSOUNDS`
/// convention); `false` when the API reports failure. The caller verifies
/// with an exact readback instead of trusting this bit.
pub fn set_win_arranging_session(enabled: bool) -> bool {
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_SETWINARRANGING,
            u32::from(enabled),
            std::ptr::null_mut(),
            SPIF_SENDCHANGE,
        )
    };
    ok != 0
}
