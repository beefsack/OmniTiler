//! Owned-callback enumeration: top-level windows, child windows, monitors.
//!
//! The `extern "system"` callbacks and every raw context pointer live
//! here. Safe callers receive plain owned `Vec`s.

use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{EnumChildWindows, EnumWindows};

use super::hwnd_of;

unsafe extern "system" fn window_enum_proc(hwnd: HWND, state: LPARAM) -> i32 {
    let out = unsafe { &mut *(state as *mut Vec<u64>) };
    out.push(hwnd as usize as u64);
    1
}

unsafe extern "system" fn child_enum_proc(hwnd: HWND, state: LPARAM) -> i32 {
    let out = unsafe { &mut *(state as *mut Vec<u64>) };
    out.push(hwnd as usize as u64);
    1
}

/// Top-level window ids; `None` on API failure (the caller retains state
/// and skips the tick, never treating failure as an empty observation).
pub fn enumerate_windows() -> Option<Vec<u64>> {
    let mut out: Vec<u64> = Vec::new();
    let ok = unsafe { EnumWindows(Some(window_enum_proc), &mut out as *mut Vec<u64> as LPARAM) };
    if ok == 0 { None } else { Some(out) }
}

pub fn enumerate_child_windows(hwnd: u64) -> Option<Vec<u64>> {
    let mut out: Vec<u64> = Vec::new();
    let ok = unsafe {
        EnumChildWindows(
            hwnd_of(hwnd),
            Some(child_enum_proc),
            &mut out as *mut Vec<u64> as LPARAM,
        )
    };
    if ok == 0 { None } else { Some(out) }
}

const MONITORINFOF_PRIMARY: u32 = 1;

/// One enumerated display: device key plus raw work/full bounds (the
/// caller converts to its own rect type, skipping degenerate entries as
/// before). `primary` marks the primary display; the returned order is
/// primary-first, exactly as the previous inline callback produced.
#[derive(Clone)]
pub struct MonitorInfo {
    pub device: String,
    pub work: RECT,
    pub full: RECT,
    pub primary: bool,
}

unsafe extern "system" fn monitor_enum_proc(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    state: LPARAM,
) -> i32 {
    let out = unsafe { &mut *(state as *mut Vec<MonitorInfo>) };
    let mut info: MONITORINFOEXW = unsafe { std::mem::zeroed() };
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if unsafe { GetMonitorInfoW(monitor, (&mut info as *mut MONITORINFOEXW).cast()) } == 0 {
        return 1;
    }
    let work = RECT {
        left: info.monitorInfo.rcWork.left,
        top: info.monitorInfo.rcWork.top,
        right: info.monitorInfo.rcWork.right,
        bottom: info.monitorInfo.rcWork.bottom,
    };
    let full = RECT {
        left: info.monitorInfo.rcMonitor.left,
        top: info.monitorInfo.rcMonitor.top,
        right: info.monitorInfo.rcMonitor.right,
        bottom: info.monitorInfo.rcMonitor.bottom,
    };
    let end = info
        .szDevice
        .iter()
        .position(|c| *c == 0)
        .unwrap_or(info.szDevice.len());
    let device = String::from_utf16_lossy(&info.szDevice[..end]);
    let device = if device.is_empty() {
        "display-?".to_owned()
    } else {
        device
    };
    let primary = info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0;
    let area = MonitorInfo {
        device,
        work,
        full,
        primary,
    };
    if primary {
        out.insert(0, area);
    } else {
        out.push(area);
    }
    1
}

/// All monitor infos in physical pixels, primary first. `None` when the
/// API fails or nothing enumerates (the caller retains state).
pub fn enumerate_monitors() -> Option<Vec<MonitorInfo>> {
    let mut areas: Vec<MonitorInfo> = Vec::new();
    let ok = unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(monitor_enum_proc),
            &mut areas as *mut Vec<MonitorInfo> as LPARAM,
        )
    };
    if ok == 0 || areas.is_empty() {
        return None;
    }
    Some(areas)
}
