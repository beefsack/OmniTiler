//! Owned per-output taskbar pager overlays (`cfg(windows)` only).
//!
//! Mechanism: one process-owned external overlay window per served output
//! (class `OmniTilerTaskbarPager`), styled exactly like the validated probe:
//! `WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE` with `WS_POPUP |
//! WS_VISIBLE`, GDI-painted cells, `SetWindowPos(TOPMOST, NOACTIVATE)`.
//! The windows never activate (`WS_EX_NOACTIVATE` plus `MA_NOACTIVATE`),
//! skip Alt+Tab via `WS_EX_TOOLWINDOW`, and are positioned from physical
//! PMv2 coordinates (PMv2 is ensured before registration, like the loop).
//!
//! What this module never does: no injection or hooks into Explorer, no
//! AppBar messages, no placeholder or Start-button manipulation, no
//! taskbar resize, and no synthetic input. Clicks hit-test the stored
//! portable frame and queue one bounded [`SelectScope`](crate::tray_pager)
//! per cell; the loop thread drains the queue and dispatches through the
//! existing `workspace_do_select` resolver. WinEvent out-of-context delivery
//! is not used here at all: only app-local window messages (paint, click,
//! `TaskbarCreated`, display/DPI broadcasts).
//!
//! Tray exclusion is structural: strips are refused unless they end 2px left
//! of the proven `TrayNotifyWnd` edge (see [`crate::pager`]); secondary
//! taskbars always refuse because their clock area cannot be proven.
//! Fullscreen/game safety is fail-closed: the host hides every strip on any
//! suspend veto, elevated foreground, or unreadable fact, and z-order is
//! reasserted only while the host reports safe.
//!
//! Logging is sparse lifecycle only (counts and reasons, never HWNDs,
//! paths, or titles); creation failures retry forever with a capped delay.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use tiler_core::geometry::Rect;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};

use crate::pager::{OutputPager, TaskbarFacts, TaskbarKind, cell_at};
use crate::pager_icons::{IconImage, icon_rect_for_mini};
use crate::tray_pager::SelectScope;

type DynError = Box<dyn std::error::Error>;

fn err(msg: impl Into<String>) -> DynError {
    Box::new(std::io::Error::other(msg.into()))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// Overlay window class (distinct from tray/border/underlay/preview so
/// residue audits count the carrier exactly).
pub const PAGER_OVERLAY_CLASS: &str = "OmniTilerTaskbarPager";
/// Bounded pending click queue (overflow drops the newest click).
const MAX_PENDING_CLICKS: usize = 8;
/// Topmost reassert cadence while safe (never over an unsafe turn).
const ZASSERT_INTERVAL: Duration = Duration::from_secs(2);
/// Creation-failure retry: 5s doubling, 60s cap, forever.
const RETRY_BASE_SECS: u64 = 5;
const RETRY_CAP_SECS: u64 = 60;

static REDISCOVER: AtomicBool = AtomicBool::new(false);
static TASKBAR_MSG: AtomicU32 = AtomicU32::new(0);
/// Set when a display/DPI/settings broadcast lands: the loop thread hides
/// every strip at once and revalidates from fresh facts before showing again,
/// so no stale pixels survive a calibration change.
static STALE_HIDE: AtomicBool = AtomicBool::new(false);

/// Whether Explorer (or display/DPI calibration) asked for rediscovery.
#[must_use]
pub fn take_rediscover() -> bool {
    REDISCOVER.swap(false, Ordering::SeqCst)
}

/// Whether a calibration broadcast asked for an immediate hide plus
/// revalidation (consumed once by the host).
#[must_use]
pub fn take_stale_hide() -> bool {
    STALE_HIDE.swap(false, Ordering::SeqCst)
}

fn want_rediscover() {
    REDISCOVER.store(true, Ordering::SeqCst);
}

fn want_stale_hide() {
    STALE_HIDE.store(true, Ordering::SeqCst);
    want_rediscover();
}

/// Per-window painted frame plus the click queue (boxed into
/// `GWLP_USERDATA`, reclaimed by the owner on destroy).
struct WindowState {
    ids: Vec<String>,
    output: String,
    cells: Vec<Rect>,
    active: usize,
    minis: Vec<Vec<Rect>>,
    /// Owned artwork bytes parallel to `minis` (flattened by the worker, so
    /// paint only copies). `Arc` keeps per-pump frame stores cheap.
    icons: Vec<Vec<Option<Arc<IconImage>>>>,
    /// Overlay DPI the frame was laid out for (icon sizing).
    dpi: u32,
    font: windows_sys::Win32::Graphics::Gdi::HFONT,
    queue: Arc<Mutex<Vec<SelectScope>>>,
}

fn window_state_of(hwnd: HWND) -> *mut WindowState {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GWLP_USERDATA, GetWindowLongPtrW};
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState }
}

fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}

/// Blit one worker-flattened icon into its centered slot. The buffer is
/// opaque by construction, so `SRCCOPY` is exact: no alpha channel travels
/// the paint path and the overlay stays a plain opaque GDI surface.
fn paint_icon(hdc: windows_sys::Win32::Graphics::Gdi::HDC, dest: RECT, image: &IconImage) {
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, StretchDIBits,
    };
    const SRCCOPY: u32 = 0x00CC0020;
    if !image.is_valid() {
        return;
    }
    let w = dest.right - dest.left;
    let h = dest.bottom - dest.top;
    if w <= 0 || h <= 0 {
        return;
    }
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = image.width as i32;
    bmi.bmiHeader.biHeight = -(image.height as i32);
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    unsafe {
        StretchDIBits(
            hdc,
            dest.left,
            dest.top,
            w,
            h,
            0,
            0,
            image.width as i32,
            image.height as i32,
            image.bgra.as_ptr().cast(),
            &bmi,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
    }
}

fn paint_frame(hwnd: HWND, state: &WindowState) {
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, CreatePen, CreateSolidBrush, DT_CENTER, DT_SINGLELINE, DT_VCENTER,
        DeleteObject, DrawTextW, EndPaint, FillRect, LineTo, MoveToEx, PAINTSTRUCT, PS_SOLID,
        SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;
    let mut ps: PAINTSTRUCT = unsafe { std::mem::zeroed() };
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    if hdc.is_null() {
        return;
    }
    let mut client: RECT = unsafe { std::mem::zeroed() };
    unsafe {
        GetClientRect(hwnd, &mut client);
    }
    for (index, cell) in state.cells.iter().enumerate() {
        let origin_x = state.cells.first().map(|first| first.x).unwrap_or(0);
        let local = RECT {
            left: cell.x - origin_x,
            top: 0,
            right: cell.x - origin_x + cell.w,
            bottom: cell.h,
        };
        let active = index == state.active;
        let (r, g, b) = if active { (42, 130, 218) } else { (58, 58, 58) };
        let brush = unsafe { CreateSolidBrush(rgb(r, g, b)) };
        if !brush.is_null() {
            unsafe {
                FillRect(hdc, &local, brush);
                DeleteObject(brush as _);
            }
        }
        // Miniature lighter window rectangles from the real workspace rects,
        // each with its centered application icon when artwork landed.
        let empty_icons: Vec<Option<Arc<IconImage>>> = Vec::new();
        let cell_icons = state.icons.get(index).unwrap_or(&empty_icons);
        for (pos, mini) in state
            .minis
            .get(index)
            .cloned()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let mini = *mini;
            let local_mini = RECT {
                left: mini.x - origin_x,
                top: mini.y - cell.y,
                right: mini.x - origin_x + mini.w,
                bottom: mini.y - cell.y + mini.h,
            };
            let fill = unsafe { CreateSolidBrush(rgb(168, 168, 168)) };
            if !fill.is_null() {
                unsafe {
                    FillRect(hdc, &local_mini, fill);
                    DeleteObject(fill as _);
                }
            }
            let pen = unsafe { CreatePen(PS_SOLID, 1, rgb(215, 215, 215)) };
            if !pen.is_null() {
                unsafe {
                    let old = SelectObject(hdc, pen as _);
                    MoveToEx(hdc, local_mini.left, local_mini.top, std::ptr::null_mut());
                    LineTo(hdc, local_mini.right - 1, local_mini.top);
                    LineTo(hdc, local_mini.right - 1, local_mini.bottom - 1);
                    LineTo(hdc, local_mini.left, local_mini.bottom - 1);
                    LineTo(hdc, local_mini.left, local_mini.top);
                    SelectObject(hdc, old);
                    DeleteObject(pen as _);
                }
            }
            // Owned artwork blits opaquely into the centered slot (omitted
            // when the miniature cannot fit a legible icon).
            if let Some(image) = cell_icons.get(pos).and_then(|slot| slot.as_ref())
                && let Some(slot) = icon_rect_for_mini(mini, state.dpi)
            {
                paint_icon(
                    hdc,
                    RECT {
                        left: slot.x - origin_x,
                        top: slot.y - cell.y,
                        right: slot.x - origin_x + slot.w,
                        bottom: slot.y - cell.y + slot.h,
                    },
                    image,
                );
            }
        }
        // Light 1px cell border.
        let pen = unsafe { CreatePen(PS_SOLID, 1, rgb(200, 200, 200)) };
        if !pen.is_null() {
            unsafe {
                let old = SelectObject(hdc, pen as _);
                MoveToEx(hdc, local.left, local.top, std::ptr::null_mut());
                LineTo(hdc, local.right - 1, local.top);
                LineTo(hdc, local.right - 1, local.bottom - 1);
                LineTo(hdc, local.left, local.bottom - 1);
                LineTo(hdc, local.left, local.top);
                SelectObject(hdc, old);
                DeleteObject(pen as _);
            }
        }
        // Active outline plus the 1-based number on every cell.
        if active {
            let pen = unsafe { CreatePen(PS_SOLID, 2, rgb(255, 255, 255)) };
            if !pen.is_null() {
                unsafe {
                    let old = SelectObject(hdc, pen as _);
                    MoveToEx(hdc, local.left + 1, local.top + 1, std::ptr::null_mut());
                    LineTo(hdc, local.right - 2, local.top + 1);
                    LineTo(hdc, local.right - 2, local.bottom - 2);
                    LineTo(hdc, local.left + 1, local.bottom - 2);
                    LineTo(hdc, local.left + 1, local.top + 1);
                    SelectObject(hdc, old);
                    DeleteObject(pen as _);
                }
            }
        }
        let label = wide(&(index + 1).to_string());
        let mut text_rect = local;
        unsafe {
            SetBkMode(hdc, TRANSPARENT as i32);
            SetTextColor(hdc, rgb(255, 255, 255));
            // Scalable per-overlay font (created at the overlay DPI, deleted
            // with the window). A failed font falls back to the stock font:
            // numbers stay legible, never missing.
            let old_font = if state.font.is_null() {
                std::ptr::null_mut()
            } else {
                SelectObject(hdc, state.font as _)
            };
            DrawTextW(
                hdc,
                label.as_ptr(),
                -1,
                &mut text_rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            if !old_font.is_null() {
                SelectObject(hdc, old_font);
            }
        }
    }
    unsafe {
        EndPaint(hwnd, &ps);
    }
}

unsafe extern "system" fn pager_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CREATESTRUCTW, DefWindowProcW, GWLP_USERDATA, SetWindowLongPtrW, WM_CREATE, WM_DESTROY,
        WM_DISPLAYCHANGE, WM_DPICHANGED, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_PAINT,
        WM_SETTINGCHANGE,
    };
    if msg == WM_CREATE {
        let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        return 0;
    }
    // Explorer restarts arrive as the registered broadcast, never WM_CREATE.
    if msg == TASKBAR_MSG.load(Ordering::SeqCst) && msg != 0 {
        want_rediscover();
        return 0;
    }
    if msg == WM_DISPLAYCHANGE || msg == WM_SETTINGCHANGE || msg == WM_DPICHANGED {
        // Geometry/DPI calibration may have moved the taskbar: hide this
        // overlay at once (app-local, no foreign interaction) and let the
        // loop thread revalidate from fresh facts before showing again, so
        // no stale pixels survive the change.
        want_stale_hide();
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE,
            );
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, std::ptr::null(), 1);
        }
        return 0;
    }
    if msg == WM_MOUSEACTIVATE {
        use windows_sys::Win32::UI::WindowsAndMessaging::MA_NOACTIVATE;
        return MA_NOACTIVATE as LRESULT;
    }
    if msg == WM_PAINT {
        let raw = window_state_of(hwnd);
        if !raw.is_null() {
            paint_frame(hwnd, unsafe { &*raw });
        }
        return 0;
    }
    if msg == WM_LBUTTONDOWN {
        let raw = window_state_of(hwnd);
        if !raw.is_null() {
            let state = unsafe { &*raw };
            let x = (lparam & 0xffff) as i32;
            let y = ((lparam >> 16) & 0xffff) as i32;
            let origin = state.cells.first().map(|first| (first.x, first.y));
            if let Some((ox, oy)) = origin
                && let Some(index) = cell_at(&state.cells, ox + x, oy + y)
                && let Some(workspace) = state.ids.get(index)
            {
                let ordinal = index as u32 + 1;
                if let Ok(mut queue) = state.queue.lock()
                    && queue.len() < MAX_PENDING_CLICKS
                {
                    queue.push(SelectScope::new(&state.output, workspace, ordinal));
                }
            }
        }
        return 0;
    }
    if msg == WM_DESTROY {
        // Owned by `PagerOverlays` (detach-then-destroy there): never free
        // here, so exactly one owner exists either way.
        let _ = wparam;
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Verify Per-Monitor-V2 awareness before any overlay geometry (same
/// fail-closed contract as the loop's own check; duplicated here because the
/// loop's monitor module is not visible from the crate root).
fn ensure_pm_v2() -> Result<(), DynError> {
    use windows_sys::Win32::UI::HiDpi::{
        AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        GetThreadDpiAwarenessContext, SetProcessDpiAwarenessContext,
    };
    let current = unsafe { GetThreadDpiAwarenessContext() };
    if unsafe { AreDpiAwarenessContextsEqual(current, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
        != 0
    {
        return Ok(());
    }
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } == 0 {
        return Err(err("refuse: pager SetProcessDpiAwarenessContext failed"));
    }
    let current = unsafe { GetThreadDpiAwarenessContext() };
    if unsafe { AreDpiAwarenessContextsEqual(current, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
        == 0
    {
        return Err(err("refuse: pager PMv2 awareness not active"));
    }
    Ok(())
}

fn rect_from_raw(raw: RECT) -> Option<Rect> {
    if raw.right <= raw.left || raw.bottom <= raw.top {
        return None;
    }
    let w = raw.right.saturating_sub(raw.left);
    let h = raw.bottom.saturating_sub(raw.top);
    if w <= 0 || h <= 0 || w > 16384 || h > 16384 {
        return None;
    }
    Some(Rect {
        x: raw.left,
        y: raw.top,
        w,
        h,
    })
}

fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut raw: RECT = unsafe { std::mem::zeroed() };
    if hwnd.is_null()
        || unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut raw) }
            == 0
    {
        return None;
    }
    rect_from_raw(raw)
}

unsafe extern "system" fn secondary_enum(hwnd: HWND, state: LPARAM) -> i32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClassNameW;
    let out = unsafe { &mut *(state as *mut Vec<HWND>) };
    let mut name = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, name.as_mut_ptr(), name.len() as i32) };
    if len > 0 {
        let class = String::from_utf16_lossy(&name[..len as usize]);
        if class == "Shell_SecondaryTrayWnd" {
            out.push(hwnd);
        }
    }
    1
}

/// Read the primary taskbar fresh: `Shell_TrayWnd` rect and visibility plus
/// the `TrayNotifyWnd` tray edge and the live taskbar DPI. Cheap enough for
/// every pump (four reads, no enumeration); `None` when Explorer is gone or
/// any read fails, which the host treats as unknown facts (hide, never lay
/// out from memory). 96 DPI is only the fallback when `GetDpiForWindow`
/// itself fails, never an assumption about the monitor.
#[must_use]
pub fn read_primary_taskbar() -> Option<TaskbarFacts> {
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowExW, FindWindowW, IsWindowVisible,
    };
    let tray = unsafe { FindWindowW(wide("Shell_TrayWnd").as_ptr(), std::ptr::null()) };
    if tray.is_null() {
        return None;
    }
    let rect = window_rect(tray)?;
    let notify = unsafe {
        FindWindowExW(
            tray,
            std::ptr::null_mut(),
            wide("TrayNotifyWnd").as_ptr(),
            std::ptr::null(),
        )
    };
    let tray_left = (!notify.is_null())
        .then(|| window_rect(notify))
        .flatten()
        .map(|notify_rect| notify_rect.x);
    let dpi = unsafe { GetDpiForWindow(tray) };
    Some(TaskbarFacts {
        rect,
        tray_left,
        kind: TaskbarKind::Primary,
        visible: unsafe { IsWindowVisible(tray) } != 0,
        dpi: if dpi == 0 {
            crate::pager::PAGER_BASE_DPI
        } else {
            dpi
        },
    })
}

/// Enumerate secondary taskbars (`Shell_SecondaryTrayWnd`). Heavier than the
/// primary read, so the host calls this only on the slow revalidate cadence.
/// Every entry refuses layout (`secondary-tray-unproven`): the tray edge is
/// unprovable there, so this only feeds refusal logging, never positions.
#[must_use]
pub fn discover_secondaries() -> Vec<TaskbarFacts> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, IsWindowVisible};
    let mut bars = Vec::new();
    let mut secondaries: Vec<HWND> = Vec::new();
    unsafe {
        EnumWindows(
            Some(secondary_enum),
            &mut secondaries as *mut Vec<HWND> as LPARAM,
        );
    }
    for hwnd in secondaries {
        if let Some(rect) = window_rect(hwnd) {
            bars.push(TaskbarFacts {
                rect,
                tray_left: None,
                kind: TaskbarKind::Secondary,
                visible: unsafe { IsWindowVisible(hwnd) } != 0,
                dpi: crate::pager::PAGER_BASE_DPI,
            });
        }
    }
    bars
}

/// Live owner pager: one overlay window per served output plus the shared
/// click queue the loop thread drains. The queue is owned by the loop (so
/// suppression can drop it with no windows present); each window carries a
/// clone for its click path.
pub struct PagerOverlays {
    windows: HashMap<String, (HWND, u32)>,
    queue: Arc<Mutex<Vec<SelectScope>>>,
    log_path: Option<PathBuf>,
    failures: u32,
    retry_at: Option<Instant>,
    last_zassert: Option<Instant>,
    last_hash: u64,
}

/// Scalable per-overlay number font at `dpi` (9pt Segoe UI, like the
/// first-run dialog). Null when creation fails: paint then falls back to the
/// stock font instead of skipping numbers.
fn pager_font(dpi: u32) -> windows_sys::Win32::Graphics::Gdi::HFONT {
    use windows_sys::Win32::Graphics::Gdi::CreateFontW;
    let height = -((9 * dpi.max(1) as i32 + 36) / 72);
    unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            1,
            0,
            0,
            0,
            0,
            wide("Segoe UI").as_ptr(),
        )
    }
}

impl PagerOverlays {
    /// Register the overlay class and the `TaskbarCreated` broadcast.
    /// PMv2 must precede overlay creation; this ensures it first (the loop
    /// already did, so this is normally a no-op verification). `queue` is
    /// the loop-owned click queue every window pushes into.
    pub fn new(
        log_path: Option<PathBuf>,
        queue: &Arc<Mutex<Vec<SelectScope>>>,
    ) -> Result<Self, DynError> {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CS_HREDRAW, CS_VREDRAW, IDC_ARROW, LoadCursorW, RegisterClassW, RegisterWindowMessageW,
            WNDCLASSW,
        };
        ensure_pm_v2()?;
        let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
        let class_w = wide(PAGER_OVERLAY_CLASS);
        let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
        cls.style = CS_HREDRAW | CS_VREDRAW;
        cls.lpfnWndProc = Some(pager_wnd_proc);
        cls.hInstance = hinst;
        cls.hCursor = unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) };
        cls.lpszClassName = class_w.as_ptr();
        let atom = unsafe { RegisterClassW(&cls) };
        if atom == 0 {
            let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            if code != 1410 {
                return Err(err("error: pager RegisterClass failed"));
            }
        }
        let taskbar_created =
            unsafe { RegisterWindowMessageW(wide(crate::tray::TASKBAR_CREATED_MESSAGE).as_ptr()) };
        if taskbar_created == 0 {
            return Err(err("error: pager TaskbarCreated register failed"));
        }
        TASKBAR_MSG.store(taskbar_created, Ordering::SeqCst);
        want_rediscover();
        Ok(Self {
            windows: HashMap::new(),
            queue: Arc::clone(queue),
            log_path,
            failures: 0,
            retry_at: None,
            last_zassert: None,
            last_hash: 0,
        })
    }

    fn log_line(&self, value: serde_json::Value) {
        use std::io::Write;
        if let Some(path) = self.log_path.as_ref()
            && let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
        {
            let _ = writeln!(file, "{value}");
        }
    }

    fn retry_due(&self, now: Instant) -> bool {
        self.retry_at.is_none_or(|at| now >= at)
    }

    fn note_failure(&mut self, now: Instant) {
        self.failures = self.failures.saturating_add(1);
        let backoff = (RETRY_BASE_SECS << self.failures.min(4)).min(RETRY_CAP_SECS);
        self.retry_at = Some(now + Duration::from_secs(backoff));
    }

    /// Present one pump's frames: create/move/show served outputs, destroy
    /// departed ones, repaint on frame change, and reassert topmost only on
    /// the safe cadence. Creation failures back off with a capped delay and
    /// retry forever. Steady-state visibility reads stay (correctness over
    /// syscall counting); moves and repaints are change-gated.
    pub fn sync(&mut self, pagers: &[OutputPager], now: Instant) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_TOPMOST, SW_SHOWNA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
            SetWindowPos, ShowWindow,
        };
        let hash = crate::pager::snapshot_hash(pagers);
        // Drop overlays for outputs no longer served.
        let live: Vec<String> = pagers.iter().map(|pager| pager.output.clone()).collect();
        let gone: Vec<String> = self
            .windows
            .keys()
            .filter(|output| !live.contains(output))
            .cloned()
            .collect();
        for output in gone {
            if let Some((hwnd, _)) = self.windows.remove(&output) {
                self.detach_and_destroy(hwnd);
            }
        }
        if !self.retry_due(now) {
            return;
        }
        let mut created = 0u32;
        let mut failed = 0u32;
        for pager in pagers {
            // A DPI change recreates the overlay so the font and geometry
            // match the new facts; anything else reuses the window.
            if let Some((hwnd, dpi)) = self.windows.get(&pager.output).copied()
                && dpi != pager.dpi
            {
                self.detach_and_destroy(hwnd);
                self.windows.remove(&pager.output);
            }
            let hwnd = match self.windows.get(&pager.output) {
                Some((hwnd, _)) => *hwnd,
                None => match self.create_window(pager) {
                    Some(hwnd) => {
                        self.windows.insert(pager.output.clone(), (hwnd, pager.dpi));
                        created += 1;
                        hwnd
                    }
                    None => {
                        failed += 1;
                        continue;
                    }
                },
            };
            let frame_changed = self.store_frame(hwnd, pager);
            if frame_changed || created > 0 {
                unsafe {
                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        pager.strip.x,
                        pager.strip.y,
                        pager.strip.w,
                        pager.strip.h,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, std::ptr::null(), 1);
                }
            }
            if unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd) } == 0 {
                unsafe {
                    ShowWindow(hwnd, SW_SHOWNA);
                }
            }
        }
        if failed > 0 {
            self.note_failure(now);
            self.log_line(serde_json::json!({
                "event": "pager-window",
                "outcome": "create-failed",
                "failed": failed,
                "retry_secs": self.retry_at.map(|at| at.saturating_duration_since(now).as_secs()),
            }));
        } else {
            if self.failures > 0 {
                self.failures = 0;
                self.retry_at = None;
            }
            if created > 0 {
                self.log_line(serde_json::json!({
                    "event": "pager-window",
                    "outcome": "created",
                    "outputs": created,
                }));
            }
        }
        // Periodic topmost reassert, safe-cadence only: the host calls sync
        // only while the gate is open, so reaching here means no fullscreen,
        // elevation, or unknown fact holds this turn.
        if self
            .last_zassert
            .is_none_or(|at| now.duration_since(at) >= ZASSERT_INTERVAL)
        {
            self.last_zassert = Some(now);
            for (hwnd, _) in self.windows.values() {
                unsafe {
                    SetWindowPos(
                        *hwnd,
                        HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                    );
                }
            }
        }
        self.last_hash = hash;
    }

    /// Hide every strip (suppression turns): windows persist for the next
    /// safe turn, so no destroy/create churn hides information.
    pub fn hide_all(&mut self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
        for (hwnd, _) in self.windows.values() {
            unsafe {
                ShowWindow(*hwnd, SW_HIDE);
            }
        }
    }

    fn create_window(&self, pager: &OutputPager) -> Option<HWND> {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
            WS_VISIBLE,
        };
        let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
        let class_w = wide(PAGER_OVERLAY_CLASS);
        let title_w = wide("OmniTiler taskbar pager");
        let state = Box::new(WindowState {
            ids: pager.ids.clone(),
            output: pager.output.clone(),
            cells: pager.cells.iter().map(|cell| cell.rect).collect(),
            active: pager.active_index,
            minis: pager.cells.iter().map(|cell| cell.minis.clone()).collect(),
            icons: pager
                .cells
                .iter()
                .map(|cell| cell.mini_icons.clone())
                .collect(),
            dpi: pager.dpi,
            font: pager_font(pager.dpi),
            queue: Arc::clone(&self.queue),
        });
        let raw = Box::into_raw(state);
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                class_w.as_ptr(),
                title_w.as_ptr(),
                WS_POPUP | WS_VISIBLE,
                pager.strip.x,
                pager.strip.y,
                pager.strip.w,
                pager.strip.h,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinst,
                raw.cast(),
            )
        };
        if hwnd.is_null() {
            // Creation failed: the frame box owns a live HFONT here (it is
            // normally freed in `detach_and_destroy`), so release it before
            // dropping the box. No `Drop` on `WindowState` exists precisely
            // to avoid double-freeing fonts of successfully created windows.
            unsafe {
                let state = Box::from_raw(raw);
                if !state.font.is_null() {
                    windows_sys::Win32::Graphics::Gdi::DeleteObject(state.font as _);
                }
            }
            return None;
        }
        Some(hwnd)
    }

    /// Store one frame into its window; true when anything the paint reads
    /// changed, so moves and repaints stay change-gated without cloning on
    /// every pump.
    fn store_frame(&self, hwnd: HWND, pager: &OutputPager) -> bool {
        let raw = window_state_of(hwnd);
        if raw.is_null() {
            return false;
        }
        let state = unsafe { &mut *raw };
        let cells: Vec<Rect> = pager.cells.iter().map(|cell| cell.rect).collect();
        let minis: Vec<Vec<Rect>> = pager.cells.iter().map(|cell| cell.minis.clone()).collect();
        // `Arc` equality short-circuits on pointer identity, so steady-state
        // artwork costs one pointer compare per miniature, not a byte walk.
        let icons: Vec<Vec<Option<Arc<IconImage>>>> = pager
            .cells
            .iter()
            .map(|cell| cell.mini_icons.clone())
            .collect();
        if state.ids == pager.ids
            && state.output == pager.output
            && state.cells == cells
            && state.active == pager.active_index
            && state.minis == minis
            && state.icons == icons
        {
            return false;
        }
        state.ids = pager.ids.clone();
        state.output = pager.output.clone();
        state.cells = cells;
        state.active = pager.active_index;
        state.minis = minis;
        state.icons = icons;
        true
    }

    fn detach_and_destroy(&self, hwnd: HWND) {
        use windows_sys::Win32::Graphics::Gdi::DeleteObject;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DestroyWindow, GWLP_USERDATA, SetWindowLongPtrW,
        };
        let raw = window_state_of(hwnd);
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            DestroyWindow(hwnd);
            if !raw.is_null() {
                let state = Box::from_raw(raw);
                if !state.font.is_null() {
                    DeleteObject(state.font as _);
                }
            }
        }
    }
}

impl Drop for PagerOverlays {
    /// Plain RAII teardown: every overlay destroyed, every frame box freed
    /// exactly once. Process exit destroys them implicitly (no residue).
    fn drop(&mut self) {
        let hwnds: Vec<HWND> = self.windows.values().map(|(hwnd, _)| *hwnd).collect();
        for hwnd in hwnds {
            self.detach_and_destroy(hwnd);
        }
        self.windows.clear();
    }
}
