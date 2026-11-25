//! Owned-callback enumeration: top-level windows, child windows, monitors.
//!
//! The `extern "system"` callbacks and every raw context pointer live
//! here. Safe callers receive plain owned `Vec`s.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::LazyLock;

use windows_sys::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFOEXW,
    MonitorFromWindow,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, EnumChildWindows, EnumWindows, IsWindow,
    RegisterClassW, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_SETTINGCHANGE, WNDCLASSW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_POPUP,
};

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

/// One enumerated display: device key, raw work/full bounds (the caller
/// converts to its own rect type, skipping degenerate entries as before),
/// and the effective per-monitor DPI measured by this output's own
/// Per-Monitor-V2 observer window.
///
/// `dpi` is `0` when the observer could not be created/read or PMv2 is not
/// active on the calling thread: unknown, never a plausible 96. `primary`
/// marks the primary display; the returned order is primary-first.
#[derive(Clone)]
pub struct MonitorInfo {
    pub device: String,
    pub work: RECT,
    pub full: RECT,
    pub primary: bool,
    pub dpi: u32,
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
    let device = device_name(&info);
    let primary = info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0;
    let area = MonitorInfo {
        device,
        work,
        full,
        primary,
        // Filled after enumeration by the per-output PMv2 observer. The
        // callback itself never invents a DPI or creates windows.
        dpi: 0,
    };
    if primary {
        out.insert(0, area);
    } else {
        out.push(area);
    }
    1
}

/// `\\.\DISPLAYn` device name of one monitor info, with the previous
/// placeholder for an empty native name.
fn device_name(info: &MONITORINFOEXW) -> String {
    let end = info
        .szDevice
        .iter()
        .position(|c| *c == 0)
        .unwrap_or(info.szDevice.len());
    let device = String::from_utf16_lossy(&info.szDevice[..end]);
    if device.is_empty() {
        "display-?".to_owned()
    } else {
        device
    }
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
    observe_effective_dpi(&mut areas);
    Some(areas)
}

// ---------------------------------------------------------------------------
// Per-output PMv2 DPI observation.
//
// `GetDpiForWindow` is only truthful for a window associated with the
// output; `GetDpiForMonitor` is documented as unsuitable from a PMv2
// thread. One hidden top-level observer window is kept per output on the
// calling thread, including outputs with no application windows, and its
// DPI is read at enumeration. The observers persist so a scale-only change
// delivers WM_DPICHANGED to a window on that output and invalidates the
// owner even when every other window is elsewhere or the desktop is idle.
// ---------------------------------------------------------------------------

/// Observer class name (static storage: the registered class keeps the
/// pointer for the process lifetime).
static OBSERVER_CLASS: LazyLock<Vec<u16>> =
    LazyLock::new(|| "OmniTilerDpiObserver\0".encode_utf16().collect());

fn observer_class() -> &'static [u16] {
    &OBSERVER_CLASS
}

/// Register the observer class in this process, tolerating a previous
/// registration. Only called when a new observer is needed.
fn register_observer_class() -> bool {
    let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
    cls.lpfnWndProc = Some(observer_wnd_proc);
    cls.hInstance = unsafe { GetModuleHandleW(std::ptr::null()) };
    cls.lpszClassName = observer_class().as_ptr();
    let atom = unsafe { RegisterClassW(&cls) };
    atom != 0 || unsafe { GetLastError() } == ERROR_CLASS_ALREADY_EXISTS
}

/// Observer window procedure: a per-output DPI message invalidates the
/// topology read. Broadcast display/settings messages are also accepted
/// because the observer exists on every output; the dirty flag is sticky,
/// so any duplicate delivery coalesces into one owner re-read.
unsafe extern "system" fn observer_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if matches!(msg, WM_DPICHANGED | WM_DISPLAYCHANGE | WM_SETTINGCHANGE) {
        crate::tiling_sys::mark_monitor_dirty();
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Top-left origin of a 1x1 observer window at the output's centre.
/// `None` for a degenerate rectangle (never a guessed point).
fn observer_origin(full: RECT) -> Option<(i32, i32)> {
    let width = full.right.checked_sub(full.left)?;
    let height = full.bottom.checked_sub(full.top)?;
    if width <= 0 || height <= 0 {
        return None;
    }
    Some((
        full.left.checked_add(width / 2)?,
        full.top.checked_add(height / 2)?,
    ))
}

/// One hidden PMv2 observer window owned by the calling thread.
struct DpiObserver {
    device: String,
    hwnd: HWND,
    origin: (i32, i32),
}

impl DpiObserver {
    fn create(device: String, full: RECT) -> Option<Self> {
        let origin = observer_origin(full)?;
        if !register_observer_class() {
            return None;
        }
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                observer_class().as_ptr(),
                std::ptr::null(),
                WS_POPUP,
                origin.0,
                origin.1,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return None;
        }
        Some(Self {
            device,
            hwnd,
            origin,
        })
    }

    fn reposition(&mut self, origin: (i32, i32)) {
        if self.origin == origin {
            return;
        }
        let moved = unsafe {
            SetWindowPos(
                self.hwnd,
                std::ptr::null_mut(),
                origin.0,
                origin.1,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER,
            )
        };
        if moved != 0 {
            self.origin = origin;
        }
    }

    fn alive(&self) -> bool {
        unsafe { IsWindow(self.hwnd) != 0 }
    }

    fn effective_dpi(&self) -> u32 {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        if dpi == 0 {
            return 0;
        }
        // The observer may have been left on a monitor that no longer
        // carries this device name (stale topology between reads). Never
        // hand that DPI to another output.
        if observer_device_matches(self.hwnd, &self.device) {
            dpi
        } else {
            0
        }
    }
}

impl Drop for DpiObserver {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}

/// Whether `hwnd`'s nearest monitor is the named output.
fn observer_device_matches(hwnd: HWND, device: &str) -> bool {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return false;
    }
    let mut info: MONITORINFOEXW = unsafe { std::mem::zeroed() };
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if unsafe { GetMonitorInfoW(monitor, (&mut info as *mut MONITORINFOEXW).cast()) } == 0 {
        return false;
    }
    device_name(&info) == device
}

/// One output whose observer could not be created or could not be placed on
/// the named output. `origin`/`attempts` avoid retrying on every enumeration
/// while an output stays unavailable; an output that moves retries at once,
/// and a bounded stride retries even without movement.
#[derive(Clone, Copy)]
struct ObserverFailure {
    origin: (i32, i32),
    attempts: u8,
}

/// Retry one failed observer every this many reconciliations. Enumeration is
/// event-driven (plus the 2s tray-less fallback), so this is not a hot loop
/// and an output that becomes usable is picked up without a new broadcast.
const OBSERVER_RETRY_STRIDE: u8 = 8;

#[derive(Default)]
struct ObserverSet {
    windows: Vec<DpiObserver>,
    failures: HashMap<String, ObserverFailure>,
}

impl ObserverSet {
    fn new() -> Self {
        Self::default()
    }

    /// Negative-cache gate for one output at `origin`: true when this
    /// reconciliation should attempt a previously failed observer.
    fn may_retry(&mut self, device: &str, origin: (i32, i32)) -> bool {
        match self.failures.get_mut(device) {
            None => true,
            Some(failure) if failure.origin != origin => {
                *failure = ObserverFailure {
                    origin,
                    attempts: 0,
                };
                true
            }
            Some(failure) => {
                failure.attempts = failure.attempts.saturating_add(1);
                failure.attempts % OBSERVER_RETRY_STRIDE == 0
            }
        }
    }

    fn record_failure(&mut self, device: &str, origin: (i32, i32)) {
        let attempts = self
            .failures
            .get(device)
            .filter(|failure| failure.origin == origin)
            .map_or(0, |failure| failure.attempts);
        self.failures.insert(
            device.to_owned(),
            ObserverFailure {
                origin,
                attempts: attempts.saturating_add(1),
            },
        );
    }
}

thread_local! {
    /// Per-thread observer ownership: windows created here are moved and
    /// destroyed only here, and this thread's message pump (the owner loop)
    /// receives their DPI notifications. The set drops at thread exit, which
    /// destroys every owned observer exactly once.
    static DPI_OBSERVERS: RefCell<ObserverSet> = RefCell::new(ObserverSet::new());
}

/// Fill each monitor's `dpi` from its persistent per-output observer.
/// Observers are reconciled against the current device set: missing ones
/// are created, moved with their monitor, and stale ones destroyed. A
/// non-PMv2 thread leaves every `dpi` at 0 (unknown) and creates nothing.
fn observe_effective_dpi(infos: &mut [MonitorInfo]) {
    if !super::thread_pm_v2_active() {
        return;
    }
    DPI_OBSERVERS.with(|slot| {
        let Ok(mut observers) = slot.try_borrow_mut() else {
            return;
        };
        // Drop observers and failure rows for disappeared outputs, or
        // observers whose window vanished.
        observers.windows.retain(|observer| {
            observer.alive()
                && infos
                    .iter()
                    .any(|info| info.device.as_str() == observer.device.as_str())
        });
        observers.failures.retain(|device, _| {
            infos
                .iter()
                .any(|info| info.device.as_str() == device.as_str())
        });
        for info in infos.iter_mut() {
            let Some(origin) = observer_origin(info.full) else {
                info.dpi = 0;
                continue;
            };
            if let Some(observer) = observers
                .windows
                .iter_mut()
                .find(|observer| observer.device.as_str() == info.device.as_str())
            {
                observer.reposition(origin);
                info.dpi = observer.effective_dpi();
                continue;
            }
            if !observers.may_retry(&info.device, origin) {
                info.dpi = 0;
                continue;
            }
            let Some(observer) = DpiObserver::create(info.device.clone(), info.full) else {
                observers.record_failure(&info.device, origin);
                info.dpi = 0;
                continue;
            };
            let dpi = observer.effective_dpi();
            if dpi == 0 {
                // Created on the wrong output (unavailable display or a
                // same-reconciliation topology race): do not retain a
                // permanently mismatched window; retry on the bounded
                // stride/change instead.
                observers.record_failure(&info.device, origin);
                info.dpi = 0;
                continue;
            }
            observers.failures.remove(&info.device);
            info.dpi = dpi;
            observers.windows.push(observer);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::RECT;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn observer_origin_is_the_output_centre() {
        assert_eq!(observer_origin(rect(0, 0, 800, 600)), Some((400, 300)));
        assert_eq!(observer_origin(rect(-1920, 0, 0, 1080)), Some((-960, 540)));
        assert_eq!(
            observer_origin(rect(1920, -200, 4480, 1240)),
            Some((3200, 520))
        );
    }

    #[test]
    fn degenerate_output_has_no_observer_origin() {
        assert_eq!(observer_origin(rect(0, 0, 0, 600)), None);
        assert_eq!(observer_origin(rect(0, 0, 800, 0)), None);
        assert_eq!(observer_origin(rect(800, 0, 0, 600)), None);
        assert_eq!(observer_origin(rect(i32::MIN, 0, i32::MAX, 600)), None);
    }

    #[test]
    fn failed_observer_retry_is_bounded_and_output_move_resets_it() {
        let mut observers = ObserverSet::new();
        assert!(observers.may_retry("display", (0, 0)));
        observers.record_failure("display", (0, 0));
        for _ in 0..OBSERVER_RETRY_STRIDE - 2 {
            assert!(!observers.may_retry("display", (0, 0)));
        }
        assert!(observers.may_retry("display", (0, 0)));
        // A moved output is a new placement, not the failed one.
        assert!(observers.may_retry("display", (1920, 0)));
        assert_eq!(
            observers
                .failures
                .get("display")
                .map(|failure| failure.origin),
            Some((1920, 0))
        );
    }

    /// Live, read-only host probe: every enumerated output gets its own
    /// persistent PMv2 observer and a non-zero DPI. Independently creates
    /// one invisible PMv2 window at each output's centre and checks the two
    /// reads agree. Mutates no display setting and closes every fixture
    /// window. Ignored by default; run with `-- --ignored --nocapture`.
    #[test]
    #[ignore = "live PMv2 per-output DPI observers"]
    fn live_per_output_observers_report_each_monitor_dpi() {
        assert!(
            super::super::set_process_pm_v2() || super::super::thread_pm_v2_active(),
            "PMv2 awareness"
        );
        let infos = enumerate_monitors().expect("monitor enumeration");
        assert!(!infos.is_empty(), "at least one output");
        for info in &infos {
            println!("{}: dpi={}", info.device, info.dpi);
            assert!(
                info.dpi > 0,
                "{} observer DPI must be measured (unknown is 0)",
                info.device
            );
            let (x, y) = observer_origin(info.full).expect("usable output rect");
            let window = super::super::test_window::OwnedWindow::create(
                "OmniTiler DPI fixture",
                x,
                y,
                1,
                1,
                false,
            )
            .expect("independent fixture window");
            // The fixture window is a different class/queue but the same
            // process awareness and output, so its DPI must agree with the
            // production observer.
            let fixture_dpi = unsafe { GetDpiForWindow(hwnd_of(window.hwnd())) };
            assert_eq!(
                fixture_dpi, info.dpi,
                "{} observer and independent fixture window disagree",
                info.device
            );
        }
    }
}
