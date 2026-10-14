use super::*;

const MONITORINFOF_PRIMARY: u32 = 1;

/// Verify Per-Monitor-V2 awareness before any geometry call. Checks the
/// effective context first (a manifest may already set it) and only sets when
/// needed, verifying afterward. Fails closed: a wrong awareness mode
/// silently virtualizes every rectangle.
pub(crate) fn ensure_pm_v2() -> Result<()> {
    let current = unsafe { GetThreadDpiAwarenessContext() };
    if unsafe { AreDpiAwarenessContextsEqual(current, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
        != 0
    {
        return Ok(());
    }
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } == 0 {
        return Err(err("refuse: SetProcessDpiAwarenessContext failed"));
    }
    let current = unsafe { GetThreadDpiAwarenessContext() };
    if unsafe { AreDpiAwarenessContextsEqual(current, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
        == 0
    {
        return Err(err("refuse: PMv2 awareness not active"));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(super) struct MonitorArea {
    /// Stable session key: the device name (`szDevice`, e.g. `\\.\DISPLAY1`).
    /// Rectangles decide placement; this key decides session identity.
    pub(super) device: String,
    pub(super) work: Rect,
    pub(super) full: Rect,
}

unsafe extern "system" fn monitor_enum(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    state: LPARAM,
) -> i32 {
    let out = unsafe { &mut *(state as *mut Vec<MonitorArea>) };
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
    if let (Some(work), Some(full)) = (rect_from_win(work), rect_from_win(full)) {
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
        let area = MonitorArea { device, work, full };
        let primary = info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0;
        if primary {
            out.insert(0, area);
        } else {
            out.push(area);
        }
    }
    1
}

/// All monitor areas in physical pixels, primary first. Fails when no
/// monitor enumerates: the caller retains Engine state and skips the tick.
pub(super) fn all_monitors() -> Result<Vec<MonitorArea>> {
    let mut areas: Vec<MonitorArea> = Vec::new();
    let ok = unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(monitor_enum),
            &mut areas as *mut Vec<MonitorArea> as LPARAM,
        )
    };
    if ok == 0 || areas.is_empty() {
        return Err(err("error: monitor enumeration failed"));
    }
    Ok(areas)
}

/// Full `rcMonitor` bounds of every display for the portable fullscreen
/// predicate (whole-monitor coverage, not work area).
pub(super) fn monitor_fulls(areas: &[MonitorArea]) -> Vec<Rect> {
    areas.iter().map(|a| a.full).collect()
}
