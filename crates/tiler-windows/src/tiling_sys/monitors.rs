#![forbid(unsafe_code)]
use super::*;

/// Verify Per-Monitor-V2 awareness before any geometry call. Checks the
/// effective context first (a manifest may already set it) and only sets when
/// needed, verifying afterward. Fails closed: a wrong awareness mode
/// silently virtualizes every rectangle.
pub(crate) fn ensure_pm_v2() -> Result<()> {
    if crate::ffi::thread_pm_v2_active() {
        return Ok(());
    }
    if !crate::ffi::set_process_pm_v2() {
        return Err(err("refuse: SetProcessDpiAwarenessContext failed"));
    }
    if !crate::ffi::thread_pm_v2_active() {
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

/// Display-topology dirty flag, set by the owner-window procedures on
/// `WM_DISPLAYCHANGE` / `WM_DPICHANGED` / `WM_SETTINGCHANGE` broadcasts
/// and consumed once by the owner loop to re-read monitors. Sticky until
/// consumed, so coalesced broadcasts still refresh exactly once (same
/// pattern as the pager rediscover and accent flags). Topology changes
/// always broadcast; with no owner window to receive them (tray-less runs)
/// the windowless fallback deadline covers instead.
static MONITOR_DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Mark a possibly-changed display topology (window-procedure context:
/// never waits, never logs).
pub(crate) fn mark_monitor_dirty() {
    MONITOR_DIRTY.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Take a pending topology change for one monitor re-read. Sticky bits
/// set after the take stay pending for the next turn; quiet turns never
/// enumerate.
pub(crate) fn take_monitor_dirty() -> bool {
    MONITOR_DIRTY.swap(false, std::sync::atomic::Ordering::SeqCst)
}

/// All monitor areas in physical pixels, primary first. Fails when no
/// monitor enumerates: the caller retains Engine state and skips the tick.
pub(super) fn all_monitors() -> Result<Vec<MonitorArea>> {
    let Some(infos) = crate::ffi::enumerate_monitors() else {
        return Err(err("error: monitor enumeration failed"));
    };
    let mut areas: Vec<MonitorArea> = Vec::new();
    for info in infos {
        let (Some(work), Some(full)) = (rect_from_win(info.work), rect_from_win(info.full)) else {
            continue;
        };
        let area = MonitorArea {
            device: info.device,
            work,
            full,
        };
        if info.primary {
            areas.insert(0, area);
        } else {
            areas.push(area);
        }
    }
    if areas.is_empty() {
        return Err(err("error: monitor enumeration failed"));
    }
    Ok(areas)
}

/// Full `rcMonitor` bounds of every display for the portable fullscreen
/// predicate (whole-monitor coverage, not work area).
pub(super) fn monitor_fulls(areas: &[MonitorArea]) -> Vec<Rect> {
    areas.iter().map(|a| a.full).collect()
}
