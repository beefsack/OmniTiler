//! DPI awareness and system metrics.

use windows_sys::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    GetSystemMetricsForDpi, GetThreadDpiAwarenessContext, SetProcessDpiAwarenessContext,
    SetThreadDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics;

/// True when the effective awareness context is already Per-Monitor-V2.
pub fn thread_pm_v2_active() -> bool {
    let current = unsafe { GetThreadDpiAwarenessContext() };
    unsafe {
        AreDpiAwarenessContextsEqual(current, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) != 0
    }
}

/// Set process Per-Monitor-V2 awareness; false when the API refuses.
pub fn set_process_pm_v2() -> bool {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) != 0 }
}

/// Run physical-pixel native work from a command that does not initialize the
/// PMv2 owner loop. If Windows refuses the context, do not run the operation
/// with virtualized coordinates.
pub(crate) fn with_pm_v2<T>(work: impl FnOnce() -> T) -> Option<T> {
    if thread_pm_v2_active() {
        return Some(work());
    }
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous.is_null() {
        return None;
    }
    struct Reset(windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT);
    impl Drop for Reset {
        fn drop(&mut self) {
            unsafe { SetThreadDpiAwarenessContext(self.0) };
        }
    }
    let _reset = Reset(previous);
    Some(work())
}

pub fn system_metric(index: i32) -> i32 {
    unsafe { GetSystemMetrics(index) }
}

/// System DPI for new top-level windows (`GetDpiForSystem`), defaulting to
/// 96 when the query fails. Used for 9pt message-font sizing and client
/// scaling before any window exists.
pub fn system_dpi() -> u32 {
    let dpi = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForSystem() };
    if dpi == 0 { 96 } else { dpi }
}

/// DPI-sized metric with the plain fallback when the sized query yields
/// nothing usable.
pub fn system_metric_for_dpi(index: i32, dpi: u32) -> i32 {
    let sized = unsafe { GetSystemMetricsForDpi(index, dpi) };
    if sized > 0 {
        sized
    } else {
        system_metric(index)
    }
}
