//! DPI awareness and system metrics.

use windows_sys::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    GetSystemMetricsForDpi, GetThreadDpiAwarenessContext, SetProcessDpiAwarenessContext,
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
