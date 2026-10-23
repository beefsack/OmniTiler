//! Safe window predicates, styles, geometry, properties, and queries.
//!
//! Every function takes a numeric window id (`u64`, as the existing model
//! does) and owns all pointers/buffers internally. Reads fail closed.

use windows_sys::Win32::Foundation::{GetLastError, HWND, POINT, RECT, SetLastError};
use windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute;
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetClassNameW, GetDesktopWindow, GetForegroundWindow,
    GetParent, GetPropW, GetShellWindow, GetSystemMetrics, GetWindow, GetWindowLongW,
    GetWindowPlacement, GetWindowRect, GetWindowThreadProcessId, HWND_NOTOPMOST, HWND_TOPMOST,
    IsIconic, IsWindow, IsWindowVisible, IsZoomed, MINMAXINFO, PostMessageW, RemovePropW,
    SMTO_ABORTIFHUNG, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SendMessageTimeoutW, SetForegroundWindow, SetPropW, SetWindowLongW, SetWindowPlacement,
    SetWindowPos, SetWindowTextW, ShowWindow, ShowWindowAsync, WINDOWPLACEMENT, WM_CLOSE,
    WM_GETMINMAXINFO, WM_NCHITTEST, WindowFromPoint,
};

use super::hwnd_of;
use super::input::{HITTEST_TIMEOUT_MS, pack_mouse_lparam};

const DWMWA_CLOAKED: u32 = 14;
const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;

pub fn is_window(hwnd: u64) -> bool {
    unsafe { IsWindow(hwnd_of(hwnd)) != 0 }
}

pub fn is_visible(hwnd: u64) -> bool {
    unsafe { IsWindowVisible(hwnd_of(hwnd)) != 0 }
}

pub fn is_iconic(hwnd: u64) -> bool {
    unsafe { IsIconic(hwnd_of(hwnd)) != 0 }
}

pub fn is_zoomed(hwnd: u64) -> bool {
    unsafe { IsZoomed(hwnd_of(hwnd)) != 0 }
}

pub fn foreground_window() -> u64 {
    unsafe { GetForegroundWindow() as usize as u64 }
}

pub fn set_foreground_window(hwnd: u64) -> bool {
    unsafe { SetForegroundWindow(hwnd_of(hwnd)) != 0 }
}

/// Owning process id of one window; `0` when unreadable (same fail-closed
/// shape as the previous inline reads, which left the zeroed pid in place).
pub fn window_pid(hwnd: u64) -> u32 {
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd_of(hwnd), &mut pid);
    }
    pid
}

/// Thread id owning one window (the `nullmut pid` query shape).
pub fn window_thread_id(hwnd: u64) -> u32 {
    unsafe { GetWindowThreadProcessId(hwnd_of(hwnd), std::ptr::null_mut()) }
}

pub fn current_thread_id() -> u32 {
    unsafe { GetCurrentThreadId() }
}

pub fn attach_thread_input(from: u32, to: u32, attach: bool) -> bool {
    unsafe { AttachThreadInput(from, to, attach as i32) != 0 }
}

/// Raw style-word read (no error discrimination, as before).
pub fn window_style(hwnd: u64) -> u32 {
    unsafe { GetWindowLongW(hwnd_of(hwnd), GWL_STYLE) as u32 }
}

/// Raw extended-style read (no error discrimination, as before).
pub fn window_exstyle(hwnd: u64) -> u32 {
    unsafe { GetWindowLongW(hwnd_of(hwnd), GWL_EXSTYLE) as u32 }
}

/// Style-word read with the `SetLastError` discrimination: `None` only when
/// the word reads zero with a live error (fail closed, never guessed).
pub fn window_style_checked(hwnd: u64) -> Option<u32> {
    unsafe {
        SetLastError(0);
    }
    let style = unsafe { GetWindowLongW(hwnd_of(hwnd), GWL_STYLE) as u32 };
    let err = unsafe { GetLastError() };
    if style == 0 && err != 0 {
        None
    } else {
        Some(style)
    }
}

/// Exactly like `SetWindowLongW` plus `GetLastError`: `(prev, err)`.
pub fn set_window_style(hwnd: u64, next: i32) -> (i32, u32) {
    unsafe {
        SetLastError(0);
    }
    let prev = unsafe { SetWindowLongW(hwnd_of(hwnd), GWL_STYLE, next) };
    let err = unsafe { GetLastError() };
    (prev, err)
}

pub fn window_rect(hwnd: u64) -> Option<RECT> {
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(hwnd_of(hwnd), &mut rect) } == 0 {
        return None;
    }
    Some(rect)
}

pub fn set_window_pos(hwnd: u64, x: i32, y: i32, w: i32, h: i32, flags: u32) -> bool {
    unsafe { SetWindowPos(hwnd_of(hwnd), std::ptr::null_mut(), x, y, w, h, flags) != 0 }
}

/// Frame reapply without moving, sizing, ordering, or activating.
pub fn reapply_frame(hwnd: u64) -> bool {
    set_window_pos(
        hwnd,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
    )
}

/// Z-order anchor for [`set_window_pos_after`]: the only anchors the
/// tiling writes use (topmost, explicit non-topmost, or no order change).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionAfter {
    Topmost,
    NotTopmost,
    NoOrderChange,
}

pub fn set_window_pos_after(
    hwnd: u64,
    after: PositionAfter,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    flags: u32,
) -> bool {
    let anchor: HWND = match after {
        PositionAfter::Topmost => HWND_TOPMOST,
        PositionAfter::NotTopmost => HWND_NOTOPMOST,
        PositionAfter::NoOrderChange => std::ptr::null_mut(),
    };
    unsafe { SetWindowPos(hwnd_of(hwnd), anchor, x, y, w, h, flags) != 0 }
}

pub fn set_topmost(hwnd: u64, top: bool) -> bool {
    let after: HWND = if top { HWND_TOPMOST } else { HWND_NOTOPMOST };
    unsafe {
        SetWindowPos(
            hwnd_of(hwnd),
            after,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        ) != 0
    }
}

pub fn show_window_async(hwnd: u64, cmd: i32) -> bool {
    unsafe { ShowWindowAsync(hwnd_of(hwnd), cmd) != 0 }
}

/// Synchronous show command without activation guarantees (the caller picks
/// a non-activating command and verifies with a readback). No return: the
/// caller reads back visibility instead of trusting the call.
pub fn show_window(hwnd: u64, cmd: i32) {
    unsafe {
        ShowWindow(hwnd_of(hwnd), cmd);
    }
}

/// Ask one owned window to close via `WM_CLOSE`; false when the post fails.
pub fn post_close(hwnd: u64) -> bool {
    unsafe { PostMessageW(hwnd_of(hwnd), WM_CLOSE, 0, 0) != 0 }
}

pub fn window_placement(hwnd: u64) -> Option<WINDOWPLACEMENT> {
    let mut placement: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
    placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    if unsafe { GetWindowPlacement(hwnd_of(hwnd), &mut placement) } == 0 {
        return None;
    }
    Some(placement)
}

pub fn set_window_placement(hwnd: u64, placement: &WINDOWPLACEMENT) -> bool {
    unsafe { SetWindowPlacement(hwnd_of(hwnd), placement) != 0 }
}

/// Owner handle as a numeric id; `0` when unowned.
pub fn owner_of(hwnd: u64) -> u64 {
    unsafe { GetWindow(hwnd_of(hwnd), GW_OWNER) as usize as u64 }
}

/// Topmost native window under one cursor point; `0` when none. Pure
/// observation (no activation, no focus): feeds the cursor-hit managed
/// binding in capture, never a policy input by itself.
pub fn window_at_point(x: i32, y: i32) -> u64 {
    unsafe { WindowFromPoint(POINT { x, y }) as usize as u64 }
}

/// Parent handle as a numeric id; `0` when top-level.
pub fn parent_of(hwnd: u64) -> u64 {
    unsafe { GetParent(hwnd_of(hwnd)) as usize as u64 }
}

pub fn shell_window() -> u64 {
    unsafe { GetShellWindow() as usize as u64 }
}

pub fn desktop_window() -> u64 {
    unsafe { GetDesktopWindow() as usize as u64 }
}

pub fn class_name(hwnd: u64) -> String {
    try_class_name(hwnd).unwrap_or_default()
}

/// Fallible class-name read; `None` when the query fails (fail closed, so
/// the caller can distinguish "unknown class" from a class mismatch).
pub fn try_class_name(hwnd: u64) -> Option<String> {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd_of(hwnd), buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..n as usize]))
}

fn prop_name_wide(name: &str) -> Vec<u16> {
    name.encode_utf16().chain([0]).collect()
}

pub fn get_prop(hwnd: u64, name: &str) -> Option<usize> {
    let wide = prop_name_wide(name);
    let raw = unsafe { GetPropW(hwnd_of(hwnd), wide.as_ptr()) };
    if raw.is_null() {
        return None;
    }
    Some(raw as usize)
}

pub fn set_prop(hwnd: u64, name: &str, value: usize) -> bool {
    let wide = prop_name_wide(name);
    unsafe { SetPropW(hwnd_of(hwnd), wide.as_ptr(), value as _) != 0 }
}

/// Remove one window property, returning the removed value when one was
/// present (`None` when absent). Reads fail closed like [`get_prop`].
pub fn take_prop(hwnd: u64, name: &str) -> Option<usize> {
    let wide = prop_name_wide(name);
    let removed = unsafe { RemovePropW(hwnd_of(hwnd), wide.as_ptr()) };
    if removed.is_null() {
        return None;
    }
    Some(removed as usize)
}

pub fn remove_prop(hwnd: u64, name: &str) {
    let wide = prop_name_wide(name);
    unsafe {
        RemovePropW(hwnd_of(hwnd), wide.as_ptr());
    }
}

pub fn dpi_for_window(hwnd: u64) -> u32 {
    unsafe { GetDpiForWindow(hwnd_of(hwnd)) }
}

/// DWM cloaked fact; `None` when the query fails (fail closed).
pub fn cloaked(hwnd: u64) -> Option<bool> {
    let mut value: i32 = 0;
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd_of(hwnd),
            DWMWA_CLOAKED,
            (&mut value as *mut i32).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    } == 0;
    if ok { Some(value != 0) } else { None }
}

/// DWM extended-frame bounds; `None` when the query fails (fail closed).
pub fn extended_frame_bounds(hwnd: u64) -> Option<RECT> {
    let mut raw: RECT = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd_of(hwnd),
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut raw as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    } == 0;
    if ok { Some(raw) } else { None }
}

/// `WM_NCHITTEST` probe at one cursor point; `None` when the send fails
/// (hung or unreadable: the caller classifies `Unknown`).
pub fn hit_test_at(hwnd: u64, x: i32, y: i32) -> Option<u32> {
    let mut result: usize = 0;
    let sent = unsafe {
        SetLastError(0);
        SendMessageTimeoutW(
            hwnd_of(hwnd),
            WM_NCHITTEST,
            0,
            pack_mouse_lparam(x, y),
            SMTO_ABORTIFHUNG,
            HITTEST_TIMEOUT_MS,
            &mut result,
        )
    };
    if sent == 0 {
        return None;
    }
    Some(result as u32)
}

/// Raw `WM_GETMINMAXINFO` round trip: whether the send dispatched, the
/// error code captured immediately after (so a Timeout never misreports
/// as Failed), and the possibly-updated info. Timing/outcome mapping
/// stays with the caller.
pub struct MinmaxQuery {
    pub sent: bool,
    pub error: u32,
    pub info: MINMAXINFO,
}

pub fn query_minmax(hwnd: u64, timeout_ms: u32) -> MinmaxQuery {
    let mut info = seed_minmax_info(dpi_for_window(hwnd));
    let mut result: usize = 0;
    let sent = unsafe {
        SetLastError(0);
        SendMessageTimeoutW(
            hwnd_of(hwnd),
            WM_GETMINMAXINFO,
            0,
            &mut info as *mut MINMAXINFO as isize,
            SMTO_ABORTIFHUNG,
            timeout_ms,
            &mut result,
        )
    };
    let error = unsafe { GetLastError() };
    MinmaxQuery {
        sent: sent != 0,
        error,
        info,
    }
}

/// Correctly seeded `MINMAXINFO` for one DPI: minimum track from that
/// DPI, maximum track plus maximized size/position per the normal
/// `WM_GETMINMAXINFO` contract, seeded from system metrics with the plain
/// fallback, never by copying another window's struct. An app that leaves
/// a field untouched therefore reports the system default instead of a
/// fabricated zero absence.
pub fn seed_minmax_info(dpi: u32) -> MINMAXINFO {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SM_CXMAXTRACK, SM_CXMINTRACK, SM_CXSCREEN, SM_CYMAXTRACK, SM_CYMINTRACK, SM_CYSCREEN,
    };
    let dpi = if dpi == 0 { 96 } else { dpi };
    let mut info: MINMAXINFO = unsafe { std::mem::zeroed() };
    let min_w = system_metric_for_dpi_local(SM_CXMINTRACK, dpi);
    let min_h = system_metric_for_dpi_local(SM_CYMINTRACK, dpi);
    if min_w > 0 && min_h > 0 {
        info.ptMinTrackSize.x = min_w;
        info.ptMinTrackSize.y = min_h;
    }
    let max_track_w = system_metric_for_dpi_local(SM_CXMAXTRACK, dpi);
    let max_track_h = system_metric_for_dpi_local(SM_CYMAXTRACK, dpi);
    if max_track_w > 0 && max_track_h > 0 {
        info.ptMaxTrackSize.x = max_track_w;
        info.ptMaxTrackSize.y = max_track_h;
    }
    let scr_w = system_metric_for_dpi_local(SM_CXSCREEN, dpi);
    let scr_h = system_metric_for_dpi_local(SM_CYSCREEN, dpi);
    if scr_w > 0 && scr_h > 0 {
        info.ptMaxSize.x = scr_w;
        info.ptMaxSize.y = scr_h;
    }
    info
}

fn system_metric_for_dpi_local(index: i32, dpi: u32) -> i32 {
    let sized = unsafe { windows_sys::Win32::UI::HiDpi::GetSystemMetricsForDpi(index, dpi) };
    if sized > 0 {
        sized
    } else {
        unsafe { GetSystemMetrics(index) }
    }
}

/// Synchronous show command reporting whether the window was previously
/// visible (the `ShowWindow` return contract). The caller decides what a
/// previously-invisible window means instead of trusting the call.
pub fn show_window_prev_visible(hwnd: u64, cmd: i32) -> bool {
    unsafe { ShowWindow(hwnd_of(hwnd), cmd) != 0 }
}

/// Exactly like `SetWindowPos` plus `GetLastError`: `(applied, err)`, so
/// the caller keeps the previous `win32-{code}` failure vocabulary.
pub fn set_window_pos_detailed(
    hwnd: u64,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    flags: u32,
) -> (bool, u32) {
    unsafe {
        SetLastError(0);
    }
    let ok = unsafe { SetWindowPos(hwnd_of(hwnd), std::ptr::null_mut(), x, y, w, h, flags) != 0 };
    let err = unsafe { GetLastError() };
    (ok, err)
}

/// Best-effort status title on one window; `false` when the call refuses.
/// Never reads title text.
pub fn set_window_text(hwnd: u64, text: &str) -> bool {
    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
    unsafe { SetWindowTextW(hwnd_of(hwnd), wide.as_ptr()) != 0 }
}
