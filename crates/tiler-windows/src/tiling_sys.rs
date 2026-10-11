//! Native tiling-only runtime (`cfg(windows)` only).
//!
//! One product loop: PMv2 awareness verified first, full `EnumWindows`
//! enumeration per tick, retained [`tiler_core::engine::Engine`] as layout
//! authority, `SetWindowPos` actuation with independent readback, and
//! out-of-context `SetWinEventHook` callbacks delivered on the loop thread
//! itself (the loop pumps messages; no hook thread, no join). Stop leaves all
//! geometry untouched; there is no per-window geometry ledger and nothing is
//! ever hidden.
//!
//! Production log vocabulary is structured JSON with opaque window tokens
//! only: no HWNDs, pids, process creation strings, SIDs, paths, or titles.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tiler_core::boundary::{CoreCommand, CoreReply, NoGroupReason};
use tiler_core::directional::WindowId;
use tiler_core::engine::Engine;
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::size_hints::WindowSizeHints;
use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, POINT, RECT, SetLastError};
use windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows_sys::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
    GetSystemMetricsForDpi, GetThreadDpiAwarenessContext, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::MsgWaitForMultipleObjectsEx;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE,
    EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, EnumChildWindows, EnumWindows, GW_OWNER,
    GWL_EXSTYLE, GWL_STYLE, GetClassNameW, GetCursorPos, GetDesktopWindow, GetForegroundWindow,
    GetPropW, GetShellWindow, GetSystemMetrics, GetWindow, GetWindowLongW, GetWindowPlacement,
    GetWindowRect, GetWindowThreadProcessId, HWND_NOTOPMOST, HWND_TOPMOST, IsIconic, IsWindow,
    IsWindowVisible, IsZoomed, MINMAXINFO, MSG, PM_REMOVE, PeekMessageW, QS_ALLINPUT, RemovePropW,
    SM_CXMAXTRACK, SM_CXMINTRACK, SM_CXSCREEN, SM_CYMAXTRACK, SM_CYMINTRACK, SM_CYSCREEN,
    SMTO_ABORTIFHUNG, SW_MAXIMIZE, SW_SHOWMAXIMIZED, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageTimeoutW, SetForegroundWindow,
    SetPropW, SetWindowLongW, SetWindowPlacement, SetWindowPos, ShowWindowAsync, TranslateMessage,
    WINDOWPLACEMENT, WINEVENT_OUTOFCONTEXT, WM_GETMINMAXINFO, WM_NCHITTEST,
    WPF_ASYNCWINDOWPLACEMENT, WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_THICKFRAME,
};

use crate::active_border::{
    ActiveBorderOptions, border_eligible, border_outer_rect, scale_style, scale_to_physical,
};
use crate::active_border_sys::{
    BorderOverlay, OverlayOutcome, PreviewOverlay, UnderlayOverlay, lowest_in_z,
};
use crate::group_underlay::{
    GroupUnderlayOptions, MoveSizeKind, aggregate_chord_keys, classify_hit_test, underlay_eligible,
    underlay_geometry_debug, underlay_outer_rect,
};
use crate::lifecycle::{
    WORKSPACE_REQUEST_FILE, exe_paths_equal, is_medium_rid,
    sys::{log_path_for, snap_resume, snap_suspend, stop_requested},
};
use crate::model::ProcessIdentity;
use crate::native::HeldProcess;
use crate::settings::LiveSettings;
use crate::snapkey::{
    KeyboardConfig, MAX_DISPATCH_PER_TICK, OriginVerdict, QueuedFloatIntent, QueuedSnapEvent,
    QueuedStickyIntent, QueuedWorkspaceHistoryIntent, QueuedWorkspaceSendIntent, SnapEdge, SnapOp,
    SnapOrigin, VK_LSHIFT, VK_LWIN, VK_MASK, VK_RSHIFT, VK_RWIN, VK_SHIFT, WorkspaceHistoryOp,
    WorkspaceOp, direction_name, resolve_origin,
};
use crate::storage::LedgerStore;
use crate::tiling::{
    AllowEntry, CaptureOptions, ChildrenOptions, FrameInsets, GestureIntent, HideProofOptions,
    INNER_GAP, InspectOptions, OUTER_GAP, OWNER_ID, ObservedTarget, ObservedTargetRef,
    RESIZE_REQUEST_FILE, ReadbackOutcome, RefusedTracker, ResizeOptions, ScopeHostChild,
    SkipReason, StatelessVerdict, TileOptions, TileProofOptions, TokenMap, WindowFacts,
    WorkspaceAction, WorkspaceOptions, allow_match, allowlist_digest, classify, classify_gesture,
    drop_point_in_domain, fingerprint, fullscreen_toggle_decision, hosted_child_allows,
    inspect_stateless_verdict, is_borderless_fullscreen, parse_allowlist, parse_resize_request,
    parse_workspace_request, readback_outcome, scope_allows, scope_exe_basename,
    should_hold_born_fullscreen, tick_summary_signature, tiling_domain_bounds_with,
};
use crate::win_mouse::sys::WinDragPublished;
use crate::win_mouse::{
    WINDRAG_MAX_DISPATCH_PER_TICK, WinDragEdge, WinDragKind, WinDragSnapshot, WinSettle,
    settle_pointer_journey, validate_windrag_down,
};
use crate::workspace::ManagedWorkspaces;

type DynError = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, DynError>;

fn err(msg: impl Into<String>) -> DynError {
    Box::new(std::io::Error::other(msg.into()))
}

// Extended-frame-bounds and cloaked attribute ids (stable DWM ABI).
const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
const DWMWA_CLOAKED: u32 = 14;
const MONITORINFOF_PRIMARY: u32 = 1;
// Win32 object id for window-scoped events (filters out menu/scrollbar noise).
const OBJID_WINDOW: i32 = 0;
const TICK_POLL_MS: u32 = 100;
const SLOW_POLL_MS: u64 = 2000;
// Pumped focus-settle bound: re-reads of `GetForegroundWindow` after an
// accepted setter, 50 ms pumped each so the LL-hook message pump stays live.
// No foreign wait: `pump_wait` only services the owner's own queue.
const FOCUS_SETTLE_ROUNDS: u32 = 10;
const FOCUS_SETTLE_POLL_MS: u32 = 50;

/// Shell classes that are never tile targets.
fn is_shell_class(class: &str) -> bool {
    matches!(
        class,
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "DV2ControlHost"
    )
}

/// Border-only shell-popup exclusion: the KDE `isAppletPopup` analogue.
/// Targeted, never blanket: ordinary dialogs (`#32770`), tool windows,
/// owned windows, and `explorer.exe` file browsers stay eligible. Only exact
/// shell classes plus XAML-island shell hosts gated on the shell process
/// suppress the ring. Task View suspension also hides the owned surface.
fn is_border_shell_popup(class: &str, exe_file: &str) -> bool {
    if crate::product_hide::shell_class_excluded(class) {
        return true;
    }
    if matches!(
        class,
        "MultitaskingViewFrame" | "TaskSwitcherWnd" | "NotifyIconOverflowWindow" | "Shell_Flyout"
    ) {
        return true;
    }
    // XAML-island shell surfaces (Start, Search, Widgets): class alone is
    // generic, so require the shell experience process. Never matches
    // explorer.exe file browsers or ApplicationFrameHost store apps.
    if class == "Windows.UI.Core.CoreWindow" || class == "XamlExplorerHostIslandWindow" {
        return matches!(
            exe_file,
            "startmenuexperiencehost.exe"
                | "shellexperiencehost.exe"
                | "searchhost.exe"
                | "searchui.exe"
                | "widgets.exe"
                | "textinputhost.exe"
        );
    }
    false
}

fn exe_file_name(exe_path: &str) -> &str {
    exe_path.rsplit(['/', '\\']).next().unwrap_or(exe_path)
}

/// Generic Win32 dialog class. Unowned top-level dialogs are never tile
/// targets; owned ones are already excluded as owned dialogs. The project's
/// own Settings UI (own executable plus settings class) rides the same gate
/// via [`crate::tiling::is_own_settings_window`].
const DIALOG_CLASS: &str = "#32770";

fn rect_from_win(rect: RECT) -> Option<Rect> {
    let w = (rect.right as i64 - rect.left as i64).try_into().ok()?;
    let h = (rect.bottom as i64 - rect.top as i64).try_into().ok()?;
    if w <= 0 || h <= 0 {
        return None;
    }
    Some(Rect {
        x: rect.left,
        y: rect.top,
        w,
        h,
    })
}

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
struct MonitorArea {
    /// Stable session key: the device name (`szDevice`, e.g. `\\.\DISPLAY1`).
    /// Rectangles decide placement; this key decides session identity.
    device: String,
    work: Rect,
    full: Rect,
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
fn all_monitors() -> Result<Vec<MonitorArea>> {
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
fn monitor_fulls(areas: &[MonitorArea]) -> Vec<Rect> {
    areas.iter().map(|a| a.full).collect()
}

// Hook delivery: out-of-context callbacks run on the installing (loop) thread
// while it pumps messages, so the callback and the loop share one queue lock
// with a strict invariant - the loop never holds it across a message pump -
// and the callback takes it blocking (never try_lock-and-drop, lossless).
#[derive(Debug, Clone, Copy)]
enum HookEvent {
    Wake(isize),
    /// Esc sequence captured at WinEvent delivery time in the event
    /// callback, carried to START. The pump only carries it to the START
    /// snapshot and never re-reads it later, so an Esc that arrives
    /// between the real START and the owner drain still latches (mirrors
    /// the KDE Finish pointer on MoveSizeEnd).
    MoveSizeStart(isize, u64),
    /// Release cursor captured at WinEvent delivery time in the event
    /// callback, carried to settle. `None` when the read failed: settle
    /// routes cursor-less moves as no-change. Captured here (the KDE Finish
    /// pointer), never re-read later on the pump. The END-time Esc sequence
    /// rides along so a physical Esc AFTER the END (before the owner drain)
    /// can never cancel the completed drop: settle latches ended gestures
    /// only when the END snapshot differs from START.
    MoveSizeEnd(isize, Option<(i32, i32)>, u64),
}

fn hook_queue() -> &'static Mutex<Vec<HookEvent>> {
    static QUEUE: OnceLock<Mutex<Vec<HookEvent>>> = OnceLock::new();
    QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

fn push_hook(event: HookEvent) {
    if let Ok(mut queue) = hook_queue().lock() {
        queue.push(event);
    }
}

unsafe extern "system" fn winevent_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if event == EVENT_SYSTEM_MOVESIZESTART {
        // Event-time Esc sequence: snapshot now in the callback so a
        // physical Esc between the real START and the owner drain keeps a
        // newer sequence than the snapshot and still latches.
        push_hook(HookEvent::MoveSizeStart(
            hwnd as isize,
            crate::snapkey::sys::esc_seq(),
        ));
    } else if event == EVENT_SYSTEM_MOVESIZEEND {
        // KDE Finish pointer: capture the release cursor now, at event
        // time in the callback. The pump only carries it to settle and
        // never re-reads it later. The END-time Esc sequence is captured
        // alongside so post-END edges order correctly at the drain.
        push_hook(HookEvent::MoveSizeEnd(
            hwnd as isize,
            cursor_pos(),
            crate::snapkey::sys::esc_seq(),
        ));
    } else if id_object == OBJID_WINDOW
        && matches!(
            event,
            EVENT_SYSTEM_FOREGROUND
                | EVENT_OBJECT_CREATE
                | EVENT_OBJECT_DESTROY
                | EVENT_OBJECT_SHOW
                | EVENT_OBJECT_HIDE
                | EVENT_OBJECT_LOCATIONCHANGE
        )
    {
        push_hook(HookEvent::Wake(hwnd as isize));
    }
}

/// Pump loop-thread messages (delivering hook callbacks) then wait for more.
/// One thread owns install, pump, and uninstall: no join, no hang.
fn pump_wait(timeout_ms: u32) {
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        let has = unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) };
        if has == 0 {
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    unsafe {
        MsgWaitForMultipleObjectsEx(0, std::ptr::null(), timeout_ms, QS_ALLINPUT, 0);
    }
    loop {
        let has = unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) };
        if has == 0 {
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

struct ObservedWindow {
    hwnd: u64,
    token: String,
    /// Native outer rectangle from the same `GetWindowRect` read the frame
    /// insets were measured against: one coherent observe pair, never a
    /// second torn read.
    outer: Rect,
    visible: Rect,
    insets: FrameInsets,
    identity: ObservedTarget,
    facts: WindowFacts,
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}

fn window_identity(hwnd: HWND, hwnd_u64: u64, me: &ProcessIdentity) -> Option<ObservedTarget> {
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    // The owner never tiles its own process. Its separate-process Settings
    // UI (same executable, settings class) is excluded at the dialog gate
    // below, not here.
    if pid == 0 || pid == me.pid {
        return None;
    }
    let held = HeldProcess::open(pid).ok()?;
    let ident = held.identity().ok()?;
    if ident.pid != pid {
        return None;
    }
    if ident.user_sid != me.user_sid || ident.session_id != me.session_id {
        return None;
    }
    let rid = held.integrity().ok()?;
    if !is_medium_rid(rid) {
        return None;
    }
    Some(ObservedTarget {
        hwnd: hwnd_u64,
        pid,
        process_creation: ident.process_creation,
        exe_path: ident.exe_path,
        user_sid: ident.user_sid,
        session_id: ident.session_id,
        tag: owned_tag(hwnd),
    })
}

/// Owned-helper lifetime tag (`OmniTilerLifetime` property). Empty when
/// absent: generic windows never carry it, so proof allowlists requiring a
/// nonempty tag reject them before any write.
fn owned_tag(hwnd: HWND) -> String {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetPropW;
    let name: Vec<u16> = crate::test_window::TEST_WINDOW_PROP
        .encode_utf16()
        .chain([0])
        .collect();
    let v = unsafe { GetPropW(hwnd, name.as_ptr()) };
    if v.is_null() {
        return String::new();
    }
    let token = v as usize as u64;
    if token == 0 {
        return String::new();
    }
    format!("{token:016x}")
}

/// Proof-only owned-helper verification: sibling executable, owned class,
/// lifetime tag, full process identity, and medium integrity via
/// [`crate::test_window::sys::query_owned`]. Generic windows (including
/// otherwise eligible ordinary apps) fail here even when their rectangles
/// look tileable.
fn verify_proof_owned(
    hwnd_u64: u64,
    expected: &AllowEntry,
    me: &ProcessIdentity,
) -> std::result::Result<(), &'static str> {
    let helper_exe =
        crate::test_window::sys::sibling_helper_exe().map_err(|_| "identity-changed")?;
    let snap =
        crate::test_window::sys::query_owned(hwnd_u64, &helper_exe, &me.user_sid, me.session_id)
            .map_err(|_| "identity-changed")?;
    if snap.tag != expected.tag || snap.tag.is_empty() {
        return Err("identity-changed");
    }
    if snap.process.pid != expected.pid
        || snap.process.process_creation != expected.process_creation
        || snap.process.user_sid != expected.user_sid
        || snap.process.session_id != expected.session_id
    {
        return Err("identity-changed");
    }
    if !crate::lifecycle::exe_paths_equal(&snap.process.exe_path, &expected.exe_path) {
        return Err("identity-changed");
    }
    Ok(())
}

/// Identity plus distinct window state known after identity resolution but
/// before any frame query: the stable token and the full frozen-matchable
/// identity survive even when DWM has no frame to read.
#[derive(Debug, Clone)]
pub struct KnownIdentity {
    pub identity: ObservedTarget,
    pub token: String,
}

/// Failure modes of [`observe_window`]. Identity and state stay available
/// independently of geometry: a minimized (or otherwise frameless) window
/// reports its known identity with an accurate reason instead of masquerading
/// as a changed identity, and its token stays stable across ticks.
#[derive(Debug, Clone)]
pub enum ObserveFailure {
    /// Identity never resolved (invisible, vanished, foreign session/SID,
    /// elevated, or unreadable owner): a true unknown, counted in aggregate.
    Unknown,
    /// Identity resolved: retain it, keep its token stable, and skip geometry
    /// with this reason. Boxed: the identity is large and only travels the
    /// cold failure path. `Minimized` is decided from window state before any
    /// frame query; `Unreadable` means the frames failed after identity.
    Known(Box<KnownIdentity>, SkipReason),
}

/// Observe one window fully: identity/state first, geometry second. `IsIconic`
/// is read before any frame query because iconic windows have no DWM frame;
/// a minimized window returns its known identity with `Minimized` rather than
/// an identity error. DWM cloak-query failure fails closed as `Unreadable`,
/// never as eligible. Fullscreen uses whole `rcMonitor` bounds via the
/// portable predicate: captionless covering any monitor's full rect, never
/// dimensions alone.
fn observe_window(
    hwnd: HWND,
    me: &ProcessIdentity,
    fulls: &[Rect],
    tokens: &mut TokenMap,
) -> std::result::Result<ObservedWindow, ObserveFailure> {
    use ObserveFailure::{Known, Unknown};
    if unsafe { IsWindowVisible(hwnd) } == 0 {
        return Err(Unknown);
    }
    let hwnd_u64 = hwnd as usize as u64;
    let identity = window_identity(hwnd, hwnd_u64, me).ok_or(Unknown)?;
    let token = tokens.token_for(hwnd_u64, &identity.process_creation);
    // Token clones below are per-failure (cold paths); the final use moves.
    let known = |reason| {
        Known(
            Box::new(KnownIdentity {
                identity: identity.clone(),
                token: token.clone(),
            }),
            reason,
        )
    };
    // Window state before frames: an iconic window has no composited frame,
    // so its DWM queries would fail spuriously below.
    if unsafe { IsIconic(hwnd) } != 0 {
        return Err(known(SkipReason::Minimized));
    }
    let mut outer_raw: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut outer_raw) } == 0 {
        return Err(known(SkipReason::Unreadable));
    }
    let outer = rect_from_win(outer_raw).ok_or_else(|| known(SkipReason::Unreadable))?;
    let mut visible_raw: RECT = unsafe { std::mem::zeroed() };
    let visible = if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut visible_raw as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    } == 0
    {
        rect_from_win(visible_raw).ok_or_else(|| known(SkipReason::Unreadable))?
    } else {
        return Err(known(SkipReason::Unreadable));
    };
    if !tiler_core::bounds::valid_carried_rect(visible.x, visible.y, visible.w, visible.h) {
        return Err(known(SkipReason::Unreadable));
    }
    let mut cloaked: i32 = 0;
    if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut i32).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    } != 0
    {
        return Err(known(SkipReason::Unreadable));
    }
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let exstyle = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    let owned = !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null();
    let class = class_of(hwnd);
    let captionless = style & WS_CAPTION == 0;
    let facts = WindowFacts {
        visible: true,
        minimized: unsafe { IsIconic(hwnd) } != 0,
        maximized: unsafe { IsZoomed(hwnd) } != 0,
        cloaked: cloaked != 0,
        elevated: false,
        shell: is_shell_class(&class),
        tool_window: exstyle & WS_EX_TOOLWINDOW != 0,
        owned,
        captionless_fullscreen: is_borderless_fullscreen(captionless, visible, fulls),
        no_activate: exstyle & WS_EX_NOACTIVATE != 0,
        dialog: (!owned && class == DIALOG_CLASS)
            || crate::tiling::is_own_settings_window(&class, &identity.exe_path, &me.exe_path),
    };
    Ok(ObservedWindow {
        hwnd: hwnd_u64,
        token,
        outer,
        visible,
        insets: FrameInsets::measure(outer, visible),
        identity,
        facts,
    })
}

/// Slow or hung managed windows supply unknown instead of blocking the loop.
const MIN_HINT_TIMEOUT_MS: u32 = 10;

/// Share a deadline across domains, reserving most of the poll interval for
/// other work. Native identity/frame calls and scheduling are not hard realtime.
const MIN_HINT_OP_BUDGET_MS: u64 = (TICK_POLL_MS * 2 / 5) as u64;

/// `SendMessageTimeoutW` timeout failure code: distinguishes a hung-window
/// abort from any other setter failure.
const ERROR_TIMEOUT: u32 = 1460;

/// One minimum-hint query outcome for bounded summaries. `Hint` carries a
/// usable hint; every other variant maps to unknown (no hint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HintOutcome {
    Hint,
    Timeout,
    Failed,
    Invalid,
    Budget,
}

impl HintOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Hint => "hint",
            Self::Timeout => "timeout",
            Self::Failed => "failed",
            Self::Invalid => "invalid",
            Self::Budget => "budget",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct HintStats {
    queried: usize,
    with_hint: usize,
    timeout: usize,
    failed: usize,
    invalid: usize,
    budget_skipped: usize,
}

impl HintStats {
    fn note(&mut self, outcome: HintOutcome) {
        match outcome {
            HintOutcome::Hint => {
                self.queried += 1;
                self.with_hint += 1;
            }
            HintOutcome::Timeout => {
                self.queried += 1;
                self.timeout += 1;
            }
            HintOutcome::Failed => {
                self.queried += 1;
                self.failed += 1;
            }
            HintOutcome::Invalid => {
                self.queried += 1;
                self.invalid += 1;
            }
            HintOutcome::Budget => {
                self.budget_skipped += 1;
            }
        }
    }
}

/// Fresh query deadline shared by all domains in this operation.
struct HintCx {
    deadline: Instant,
}

impl HintCx {
    fn new() -> Self {
        Self {
            deadline: Instant::now() + Duration::from_millis(MIN_HINT_OP_BUDGET_MS),
        }
    }

    fn over_budget(&self) -> bool {
        Instant::now() >= self.deadline
    }

    fn timeout_ms(&self) -> u32 {
        self.deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(u128::from(MIN_HINT_TIMEOUT_MS)) as u32
    }
}

/// One system-metrics read at the window's current DPI. `GetSystemMetricsForDpi`
/// first, plain `GetSystemMetrics` fallback: a zero read fails closed to zero
/// and the seeder leaves that field zero rather than inventing a size.
fn system_metric_for_dpi(index: i32, dpi: u32) -> i32 {
    let sized = unsafe { GetSystemMetricsForDpi(index, dpi) };
    if sized > 0 {
        return sized;
    }
    unsafe { GetSystemMetrics(index) }
}

/// Correctly seeded Rust `MINMAXINFO` for one window: minimum track from the
/// window's current DPI, maximum track plus maximized size/position per the
/// normal `WM_GETMINMAXINFO` contract. Seeded directly in Rust from supported
/// system-metrics APIs, never by copying another window's struct. An app that
/// leaves a field untouched therefore reports the system default instead of a
/// fabricated zero absence.
fn seed_minmaxinfo(hwnd: HWND) -> MINMAXINFO {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let dpi = if dpi == 0 { 96 } else { dpi };
    let mut info: MINMAXINFO = unsafe { std::mem::zeroed() };
    let min_w = system_metric_for_dpi(SM_CXMINTRACK, dpi);
    let min_h = system_metric_for_dpi(SM_CYMINTRACK, dpi);
    if min_w > 0 && min_h > 0 {
        info.ptMinTrackSize.x = min_w;
        info.ptMinTrackSize.y = min_h;
    }
    let max_track_w = system_metric_for_dpi(SM_CXMAXTRACK, dpi);
    let max_track_h = system_metric_for_dpi(SM_CYMAXTRACK, dpi);
    if max_track_w > 0 && max_track_h > 0 {
        info.ptMaxTrackSize.x = max_track_w;
        info.ptMaxTrackSize.y = max_track_h;
    }
    let scr_w = system_metric_for_dpi(SM_CXSCREEN, dpi);
    let scr_h = system_metric_for_dpi(SM_CYSCREEN, dpi);
    if scr_w > 0 && scr_h > 0 {
        info.ptMaxSize.x = scr_w;
        info.ptMaxSize.y = scr_h;
    }
    info
}

/// Fresh outer minimum-track size for one window in physical pixels plus its
/// outcome.
///
/// `SendMessageTimeoutW` with `WM_GETMINMAXINFO`, `SMTO_ABORTIFHUNG`, and
/// [`MIN_HINT_TIMEOUT_MS`] over the correctly seeded struct above. A zero
/// return means failure or timeout and the struct is ignored entirely (never
/// read); `ERROR_TIMEOUT` distinguishes hung aborts from other failures.
/// Success with an invalid track size is `Invalid`. Requires the loop's
/// verified PMv2 awareness so the track size is physical pixels.
fn query_outer_min_track(hwnd: HWND, cx: &HintCx) -> (Option<(i32, i32)>, HintOutcome) {
    let mut info = seed_minmaxinfo(hwnd);
    let mut result: usize = 0;
    let timeout_ms = cx.timeout_ms();
    if timeout_ms == 0 {
        return (None, HintOutcome::Budget);
    }
    let sent = unsafe {
        SetLastError(0);
        SendMessageTimeoutW(
            hwnd,
            WM_GETMINMAXINFO,
            0,
            &mut info as *mut MINMAXINFO as LPARAM,
            SMTO_ABORTIFHUNG,
            timeout_ms,
            &mut result,
        )
    };
    if sent == 0 {
        let code = unsafe { GetLastError() };
        if code == ERROR_TIMEOUT {
            return (None, HintOutcome::Timeout);
        }
        return (None, HintOutcome::Failed);
    }
    match crate::tiling::normalize_min_track(info.ptMinTrackSize.x, info.ptMinTrackSize.y) {
        Some(track) => (Some(track), HintOutcome::Hint),
        None => (None, HintOutcome::Invalid),
    }
}

/// Fresh minimum-size hint for one eligible observed window: a new
/// `WM_GETMINMAXINFO` query converted from outer track pixels to visible
/// physical pixels with the window's currently measured frame insets.
/// Unknown on any query failure, timeout, budget skip, or invalid remainder:
/// no hint, never a reused or persistent floor.
fn min_hint_for(window: &ObservedWindow, cx: &HintCx) -> (WindowSizeHints, HintOutcome) {
    let hwnd = window.hwnd as isize as HWND;
    let (track, outcome) = query_outer_min_track(hwnd, cx);
    if outcome != HintOutcome::Hint {
        return (WindowSizeHints::none(), outcome);
    }
    let (outer_w, outer_h) = track.expect("hint outcome carries a track");
    let hints = crate::tiling::min_hints_from_outer(outer_w, outer_h, window.insets);
    if hints.is_empty() {
        (hints, HintOutcome::Invalid)
    } else {
        (hints, HintOutcome::Hint)
    }
}

/// Fresh minimum-size hint for one verified managed hidden member without
/// showing or moving it: fresh identity fenced by the existing
/// `WindowKey` plus owned lifetime-property checks, then fresh `GetWindowRect`
/// plus `DWMWA_EXTENDED_FRAME_BOUNDS` for current insets, then a fresh
/// `WM_GETMINMAXINFO` query converted with those insets. Hidden DWM frames
/// may legitimately be absent; any measurement failure is unknown (no hint),
/// never a stale cached inset. The Engine row rectangle stays the stored
/// snapshot: fresh frames feed insets only, never adopted geometry.
fn hidden_hint_for(
    state: &TileLoop,
    key: &crate::workspace::WindowKey,
    cx: &HintCx,
) -> (WindowSizeHints, HintOutcome) {
    if cx.over_budget() {
        return (WindowSizeHints::none(), HintOutcome::Budget);
    }
    let Some(stored) = state.member_identity.get(key).cloned() else {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    };
    let Some(stored_tag) = state.member_tags.get(key).cloned() else {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    };
    if key.hwnd == 0 {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    }
    let hwnd = key.hwnd as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    }
    let mut live_pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut live_pid);
    }
    if live_pid == 0 || live_pid != key.pid {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    }
    let held = match HeldProcess::open(live_pid) {
        Ok(held) => held,
        Err(_) => return (WindowSizeHints::none(), HintOutcome::Invalid),
    };
    let live = match held.identity() {
        Ok(live) => live,
        Err(_) => return (WindowSizeHints::none(), HintOutcome::Invalid),
    };
    if !crate::workspace_owner::member_matches(key, key.hwnd, live.pid, &live.process_creation)
        || live != stored
    {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    }
    let live_tag = crate::product_hide::sys::read_member_tag(key.hwnd);
    if !crate::workspace_owner::visible_lifetime_ok(&stored_tag, live_tag.as_deref()) {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    }
    let mut outer_raw: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut outer_raw) } == 0 {
        return (WindowSizeHints::none(), HintOutcome::Failed);
    }
    let Some(outer) = rect_from_win(outer_raw) else {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    };
    let mut visible_raw: RECT = unsafe { std::mem::zeroed() };
    if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut visible_raw as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    } != 0
    {
        return (WindowSizeHints::none(), HintOutcome::Failed);
    }
    let Some(visible) = rect_from_win(visible_raw) else {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    };
    if !tiler_core::bounds::valid_carried_rect(visible.x, visible.y, visible.w, visible.h) {
        return (WindowSizeHints::none(), HintOutcome::Invalid);
    }
    let insets = FrameInsets::measure(outer, visible);
    if cx.over_budget() {
        return (WindowSizeHints::none(), HintOutcome::Budget);
    }
    let (track, outcome) = query_outer_min_track(hwnd, cx);
    if outcome != HintOutcome::Hint {
        return (WindowSizeHints::none(), outcome);
    }
    let (outer_w, outer_h) = track.expect("hint outcome carries a track");
    let hints = crate::tiling::min_hints_from_outer(outer_w, outer_h, insets);
    let outcome = if hints.is_empty() {
        HintOutcome::Invalid
    } else {
        HintOutcome::Hint
    };
    let out = if outcome == HintOutcome::Hint {
        hints
    } else {
        WindowSizeHints::none()
    };
    (out, outcome)
}

unsafe extern "system" fn enum_proc(hwnd: HWND, state: LPARAM) -> i32 {
    let out = unsafe { &mut *(state as *mut Vec<isize>) };
    out.push(hwnd as isize);
    1
}

/// Enumerate top-level HWNDs. `None` on API failure: the caller retains Engine
/// state and skips the tick, never treating a failed enumeration as a
/// complete (possibly empty) observation.
fn enumerate_hwnds() -> Option<Vec<isize>> {
    let mut out: Vec<isize> = Vec::new();
    let ok = unsafe { EnumWindows(Some(enum_proc), &mut out as *mut Vec<isize> as LPARAM) };
    if ok == 0 { None } else { Some(out) }
}

fn cursor_pos() -> Option<(i32, i32)> {
    let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return None;
    }
    Some((point.x, point.y))
}

fn log_json_at(path: &Path, value: serde_json::Value) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{value}");
    }
}

/// Trace-only placement diagnostic for the just-completed Engine op:
/// bounded startup inputs (opaque window token plus rectangle) with fit
/// outcome/reason and the resulting ordered tree description, and the
/// send anchor branch plus opaque leaf with projected rectangle/axis.
/// Gated on `state.trace`; production stays quiet. Opaque tokens and
/// integer geometry only: no HWNDs, pids, paths, or titles.
fn log_engine_placement_trace(state: &TileLoop, tick: u64, correlation: &str, op: &'static str) {
    if !state.trace {
        return;
    }
    let log_path = state.log_path.clone();
    if let Some(trace) = state.engine.last_startup_fit_trace() {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "placement-trace",
                "tick": tick,
                "correlation": correlation,
                "op": op,
                "kind": "startup-fit",
                "windows": trace.windows,
                "domain": [trace.domain_bounds.x, trace.domain_bounds.y, trace.domain_bounds.w, trace.domain_bounds.h],
                "inputs": trace.inputs.iter().map(|input| serde_json::json!({
                    "window": input.window.0,
                    "rect": [input.rect.x, input.rect.y, input.rect.w, input.rect.h],
                })).collect::<Vec<_>>(),
                "outcome": trace.outcome,
                "reason": trace.reason,
                "centre_splits": trace.centre_splits,
                "leaves": trace.leaves,
                "topology": trace.topology,
            }),
        );
    }
    if let Some(trace) = state.engine.last_send_placement() {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "placement-trace",
                "tick": tick,
                "correlation": correlation,
                "op": op,
                "kind": "send-placement",
                "anchor": trace.anchor_kind,
                "anchor_leaf": trace.anchor.as_ref().map(|leaf| leaf.0.as_str()),
                "axis": match trace.axis {
                    tiler_core::directional::Axis::Horizontal => "horizontal",
                    tiler_core::directional::Axis::Vertical => "vertical",
                },
                "projected": [trace.projected.x, trace.projected.y, trace.projected.w, trace.projected.h],
                "target_leaves": trace.target_leaves,
            }),
        );
    }
}

/// START-frozen preview source binding: the exact mover token, source
/// output/workspace, and accepted revision captured at gesture START
/// (native MoveSizeStart drain or validated project Win+Left arm). The
/// preview never rebinds mid-gesture: output/workspace/revision drift fails
/// the whole gesture closed until settle. Source bounds stay at START.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PreviewStartBinding {
    token: String,
    output: String,
    workspace: String,
    revision: u64,
}

struct PreviewBound {
    /// Carried sticky group-edge hover prior for the next sample and the
    /// final drop (exact 32/80 semantics live in the Engine resolver; this
    /// only carries the opaque value across samples).
    hover_prior: Option<tiler_core::session::DragHoverPrior>,
}

/// Fresh per-sample preview facts against the START binding. Pure input for
/// [`preview_sample_gate`]; all native reads stay with the caller.
struct PreviewFresh {
    /// This gesture already failed closed (drift/invalidation): never
    /// rebind, never sample again until settle.
    dead: bool,
    /// Cursor still on the down/start point: no pointer journey yet.
    zero: bool,
    /// Live HWND/PID/creation still matches the START member key.
    identity_ok: bool,
    /// Live member-lifetime tag still matches the START tag.
    tag_ok: bool,
    /// `member_tokens[START key]` still names the START token (no remap).
    token_ok: bool,
    /// The START member still lives on the START output/workspace.
    loc_ok: bool,
    /// Accepted revision still equals the START revision.
    revision_ok: bool,
    /// Sticky marker or Engine float exception on the mover.
    float_hold: bool,
    /// Sample point inside the START source bounds (same-output only).
    inside: bool,
}

/// One preview sample verdict: proceed to the Engine, hide transiently
/// (zero/outside: the gesture continues, the next moved sample may show),
/// or fail the whole gesture closed (drift/invalidation: no rebind, no
/// further samples, no stale render until settle clears).
enum PreviewGate {
    Proceed,
    Transient(&'static str),
    Dead(&'static str),
}

/// Pure preview sample gate: invalidation dominates (fail-closed whole
/// gesture even with a static pointer), then zero/outside hide
/// transiently. Revision drift clears the carried prior via the `Dead`
/// path (the caller drops the bound, so nothing stale can render).
fn preview_sample_gate(fresh: &PreviewFresh) -> PreviewGate {
    if fresh.dead {
        // Repeat drift can never rebind: a dead gesture stays dead.
        return PreviewGate::Dead("dead");
    }
    if !fresh.identity_ok || !fresh.tag_ok || !fresh.token_ok {
        return PreviewGate::Dead("identity-changed");
    }
    if fresh.float_hold {
        return PreviewGate::Dead("floating");
    }
    if !fresh.loc_ok {
        return PreviewGate::Dead("drift");
    }
    if !fresh.revision_ok {
        return PreviewGate::Dead("revision-drift");
    }
    if fresh.zero {
        return PreviewGate::Transient("zero");
    }
    if !fresh.inside {
        return PreviewGate::Transient("outside");
    }
    PreviewGate::Proceed
}

impl PreviewBound {
    /// Pure gate for one preview sample: real pointer movement with a live
    /// open gesture. Zero movement (cursor still on the down/start point)
    /// never shows; Esc-latched gestures never show.
    fn sample_allowed(start_x: i32, start_y: i32, x: i32, y: i32, esc_latched: bool) -> bool {
        !esc_latched && (x != start_x || y != start_y)
    }
}

/// Closed bounded hover-prior trace descriptor: `none`, or the sticky
/// group-edge side (`group-edge:left`, ...). Never carries raw NodeIds,
/// tokens, or geometry: sides are a fixed vocabulary, safe for production
/// logs and sufficient to prove prior carry equality preview-to-drop.
fn preview_prior_desc(prior: &Option<tiler_core::session::DragHoverPrior>) -> String {
    match prior {
        None => "none".to_owned(),
        Some(carried) => match &carried.prior {
            None => "none".to_owned(),
            Some(edge) => format!("group-edge:{}", edge.edge.as_str()),
        },
    }
}

/// Pure frame-grounded move gate: a native modal hold is a move iff the
/// live frame keeps the pre-gesture size in either lane (stable visible
/// DWM lane or GetWindowRect outer lane; shadow padding only shifts the
/// outer lane). A true resize moves both lanes. Review-accepted outcome:
/// keep both lanes, never the START hit test.
fn preview_same_size(
    before_w: i32,
    before_h: i32,
    outer_w: i32,
    outer_h: i32,
    vis_w: i32,
    vis_h: i32,
) -> bool {
    (vis_w == before_w && vis_h == before_h) || (outer_w == before_w && outer_h == before_h)
}
/// Pure refusal-to-hide-reason mapping for Engine preview refusals:
/// self/centre/outside refusals show no rectangle; every other kind rides
/// through for attribution.
fn preview_hide_for_refusal(kind: &str) -> &str {
    match kind {
        "unchanged" => "self",
        "unsupported-capability" => "centre",
        "cross-domain-mismatch" => "outside",
        _ => kind,
    }
}

/// Unconfirmed workspace-mode toggle releases queue in
/// [`crate::workspace::PendingReleases`]: keyed by domain with
/// latest-intent-wins so a stale float can never fire at a retiled layout,
/// dropped on confirmed toggles, stale once tiled, and retried only on
/// topology edges with current observed geometry (never polled).
struct TileLoop {
    engine: Engine,
    owner: OwnerId,
    generation: GenerationId,
    tokens: TokenMap,
    refused: RefusedTracker,
    /// Stable pre-gesture rectangles from the last reconcile observation.
    stable: HashMap<u64, Rect>,
    /// Managed HWNDs from the last reconcile observation.
    managed: HashSet<u64>,
    /// Open user gestures on managed windows only.
    active: HashSet<u64>,
    /// Pre-gesture rectangles captured at START (from `stable`, never from a
    /// mid-gesture frame; live `GetWindowRect` fallback only when `stable`
    /// has no entry, e.g. a window admitted between ticks).
    gesture_before: HashMap<u64, Rect>,
    /// Release cursor captured at MOVESIZEEND event-callback time, carried
    /// to settle. Settle never re-reads the cursor later: an END without a
    /// captured cursor routes as no-change for moves. Cleared on
    /// END-settle/removal like `gesture_before`.
    gesture_end_cursor: HashMap<u64, (i32, i32)>,
    /// START-time identity snapshot per open gesture (pid/creation from a
    /// bounded owner-side probe plus the live visible-membership lifetime
    /// tag). Settle requires the fresh window to still match this snapshot in
    /// addition to the stored member, so a reuse between START and END fails
    /// closed even when HWND/PID/creation still agree. Absent entries fall
    /// back to the stored-member check only. The tag is the member property
    /// (`OmniTilerMember`), never the owned-helper property (which is
    /// empty for ordinary windows and would refuse every ordinary gesture).
    gesture_start_key: HashMap<u64, crate::workspace::WindowKey>,
    gesture_start_tag: HashMap<u64, Option<String>>,
    tick: u64,
    allowlist: Option<Vec<AllowEntry>>,
    /// Explicit normal-mode scope filter (top-level exe basenames,
    /// normalized at parse). Empty means no filter: every eligible window is
    /// managed. Nonempty fences observation, geometry writes, workspace
    /// hide/reveal membership, and focus actuation.
    scope: Vec<String>,
    /// Explicit host-to-child scope pairs (empty means no child constraint).
    /// A listed host passes observation and hide admission only while a live
    /// hosted child matches, so a newly appearing hosted app can never ride
    /// another app's host executable into management.
    scope_hosts: Vec<ScopeHostChild>,
    trace: bool,
    suspended: bool,
    last_summary: Option<String>,
    log_path: std::path::PathBuf,
    audit_path: Option<std::path::PathBuf>,
    /// Product keyboard policy for this run (takeover switch + Win+L opt-in).
    /// Live settings edits update this in place; the hook gate republishes
    /// every pump and the classifier applies policy plus the rebound table
    /// through `apply_live_config`.
    keyboard: KeyboardConfig,
    /// Live inner/outer gaps (KDE 0..64). Saved settings supply the startup
    /// base and explicit CLI switches override; live file edits flip these
    /// and adopt through the retained update-gaps path, never a reseed.
    inner_gap: i32,
    outer_gap: i32,
    /// Live settings polling for the normal owner only (`None` on proof
    /// paths, which keep deterministic explicit configuration and never read
    /// the user's store).
    settings_live: Option<LiveSettings>,
    /// Settings directory for live polling (mirrors `settings_live`).
    settings_dir: Option<std::path::PathBuf>,
    /// Last applied rebound-chord table, so binding-only edits re-apply even
    /// when the keyboard policy is unchanged.
    last_remap: Vec<crate::snapkey::ChordRemap>,
    /// Last applied suppression table (disabled or rebound-away chords), same
    /// re-apply contract as the rebound table.
    last_disabled: Vec<crate::snapkey::ChordDisable>,
    /// Explicit startup CLI switches, authoritative for the whole run: every
    /// live settings-file apply re-applies these lanes over the file base.
    cli_overrides: crate::tiling::CliOverrides,
    /// Last published snap-queue loss count, for explicit drop evidence.
    snap_dropped: u32,
    /// Last published callback-summary loss counts, for explicit drop/filter
    /// evidence on the trace-only `snap-callback` drain.
    cb_diag_dropped: u32,
    cb_diag_filtered: u32,
    /// Last published trace-only mask-send aggregate (`sends`, `max_us`).
    mask_sends: u32,
    mask_send_max_us: u32,
    /// Chord-time origin snapshots for the keyboard callback, refreshed with
    /// every complete observation alongside `managed`.
    snap_origins: HashMap<u64, SnapOrigin>,
    /// Tiled-only managed origins for the Win+Left mouse callback,
    /// refreshed with every complete observation alongside `managed`.
    /// Floating and sticky members are excluded so their Win+Left passes
    /// through natively (titlebar-only); maximized/fullscreen members are
    /// retained, never observed, so they never appear here either.
    /// Each entry also carries the member lifetime tag read at publish
    /// time, so the hook can snapshot it into the Down edge with no native
    /// reads in the callback. Exactness stays with the per-edge owner
    /// recheck.
    windrag_origins: HashMap<u64, WinDragPublished>,
    /// Settle producer per open gesture HWND: `"native"` for the
    /// title-bar modal loop, `"windrag"` for the project-driven Win+Left
    /// stationary hold. Absent entries settle as native. Cleared on
    /// END-settle/removal like `gesture_before`.
    gesture_producer: HashMap<u64, &'static str>,
    /// Down-time pointer per open windrag gesture HWND, bound in the hook
    /// callback at down time. Settle compares it against the callback-bound
    /// release pointer (pointer delta, not rect delta: the real window
    /// keeps its source allocation mid-gesture). Cleared on END-settle/
    /// removal like `gesture_before`.
    windrag_start_cursor: HashMap<u64, (i32, i32)>,
    /// Validated Down-time bound snapshot per open windrag gesture HWND.
    /// Stored at arm time after the snapshot matches fresh evidence; the
    /// Up requires this entry (bound matching, never a rebind), and settle
    /// consumes it alongside the START fence. Cleared on END-settle/
    /// removal like `gesture_before`.
    windrag_bound: HashMap<u64, WinDragSnapshot>,
    /// Last published windrag-queue loss count, for explicit drop evidence.
    windrag_dropped: u32,
    /// Last logged tiled-origin count for the mouse hook, for change-only
    /// `windrag-origins` evidence (proves the hook binds against a live
    /// managed set without logging identity).
    windrag_origin_logged: Option<usize>,
    /// Last logged hook delivery counters, for change-only
    /// `windrag-hook-stats` evidence splitting callback delivery from owner
    /// validation.
    windrag_stats_logged: (u64, u64, u64, u64),
    /// Verified own-focus continuation across bounded drains. Set only on an
    /// exact verified owner actuation (`focus-ok`); cleared on external
    /// focus, lifetime mismatch, suspension, or gesture. Lets a stale chord
    /// origin continue from our own advance within and across batches, never
    /// a permissive retarget.
    snap_advance: Option<SnapOrigin>,
    /// Exact KDE `requestResize` repeat state for keyboard resize: the
    /// focused window, direction, and mode of the last dispatched resize
    /// plus the next press index. Keyed by [`crate::tiling::ResizeRepeat`];
    /// continued only on an exact three-way match, restarted otherwise.
    /// Key-up never resets (KDE parity). Updated only when a resize passes
    /// every pre-dispatch fence, before the Engine dispatch.
    resize_repeat: Option<crate::tiling::ResizeRepeat>,
    /// Raw `EnumWindows` count from the latest observation (targets seen
    /// before any eligibility filtering, including zero).
    last_enumerated: usize,
    /// Project-owned per-output-local workspaces. Independent Engine domains
    /// per `(output, workspace)` preserve each layout plus last focus.
    workspaces: crate::workspace::ManagedWorkspaces,
    /// Stable session tokens per member key. The token (not the ephemeral
    /// product nonce) is the Engine identity; the nonce lives only in
    /// `hidden_claims` plus the ledger while hidden.
    member_tokens: std::collections::BTreeMap<crate::workspace::WindowKey, String>,
    /// Last-known rectangles per member token (visible reads plus hidden
    /// snapshots) so Engine source/target observations stay complete.
    member_rects: HashMap<String, Rect>,
    /// Known hidden product claims by stable member key. Retained
    /// independently of the `IsWindowVisible` observation gate so hidden
    /// windows are never dropped; the nonce leaves only after verified
    /// reveal or release. Keyed by the full `(hwnd, pid, creation)` member
    /// identity, never HWND alone, so a recycled HWND never matches.
    hidden_claims: std::collections::BTreeMap<crate::workspace::WindowKey, HiddenRecord>,
    /// Stored full process identity per member key. Hide effects bind to
    /// this stored identity and refuse recycled HWNDs with no writes.
    member_identity: std::collections::BTreeMap<crate::workspace::WindowKey, ProcessIdentity>,
    /// Stored visible-membership lifetime tag per member key, stamped at
    /// admission onto the window itself. Same-process HWND reuse starts
    /// without it, so every effect re-verifies live-against-stored before
    /// trusting HWND/PID/creation. Never serialized, never trusted across
    /// runs, never removed (the property dies with its window).
    member_tags: std::collections::BTreeMap<crate::workspace::WindowKey, String>,
    /// Cached output context for the next select. Updated on every switch
    /// and refreshed from pointer/foreground context when stale.
    active_output: String,
    /// True only for the `workspace-proof` command: workspace hides are
    /// allowed, but solely for frozen-allowlist helpers verified fresh via
    /// `verify_proof_owned` before every write. Normal `tile` allows
    /// ordinary-app hides; `shortcut-proof` and `tile-proof` never hide.
    workspace_proof: bool,
    /// One-shot maximize-clear attempts at admission (KDE
    /// `maximizeAdmissionAttempts` parity): the first non-fullscreen
    /// admission of a maximized window without a retained tiled slot restores
    /// the native maximize exactly once with no automatic retry. Keyed by
    /// HWND plus process creation so a recycled HWND re-arms for the fresh
    /// window while the same window never retries.
    maximize_admission_attempted: HashSet<String>,
    /// R-MAX-06 keep-overlay latch: exact fresh lifetimes bound at tiled-first
    /// slotless admission under the live overlay value. Keyed by full member
    /// identity, never HWND alone; pruned with membership. Floating-first
    /// (R-MAX-03), pre-existing, and born-fullscreen lifetimes never enter,
    /// so their tiled admission keeps the existing one-shot clear under both
    /// setting values.
    born_keep_overlay: BTreeSet<crate::workspace::WindowKey>,
    /// Born-fullscreen hold (KDE initial-fullscreen-hold parity): first-seen
    /// fullscreen windows without a retained tile slot, tracked slotless as a
    /// planner-only exception until their first exit. Keyed by full member
    /// identity, never HWND alone. A window already seen non-fullscreen in
    /// this lifetime never enters.
    born_fullscreen: BTreeSet<crate::workspace::WindowKey>,
    /// Lifetime-known non-fullscreen member identities: every admitted or
    /// observed-eligible member lands here, so a later fullscreen transition
    /// is a managed overlay, never a born hold again. Pruned with the
    /// enumeration like the hold set.
    seen_nonfullscreen: BTreeSet<crate::workspace::WindowKey>,
    /// Last observed foreground HWND. Only an actual foreground change to a
    /// hidden member selects its workspace; event-only notifications never do.
    last_foreground: u64,
    /// Monitor device keys from the last loop pass, for disconnect/reconnect.
    known_outputs: Vec<String>,
    /// Raw `EnumWindows` HWND inventory from the latest enumeration,
    /// independent of eligibility: close cleanup and retained-occupancy
    /// decisions never mistake an unclassified window for a closed one.
    last_hwnds: HashSet<u64>,
    /// Monitor snapshot from the last loop pass: disconnect-time geometry
    /// for the survivor chooser, never post-disconnect frames as proxy.
    last_areas: Vec<MonitorArea>,
    /// Last logged minimum-size hint per Engine token, for bounded
    /// hint-change summaries plus the lifetime-bound retained overlay reuse:
    /// a retained tiled maximized/fullscreen member rides its last-known
    /// declared hint until a normal fresh query resumes. Keyed by Engine
    /// token (never HWND alone) so a recycled HWND mints a fresh token and
    /// never inherits; pruned by live membership plus row presence, and
    /// dropped immediately with member state. Float, born-hold, hidden-fresh,
    /// minimized, and cloaked rows never reuse it.
    hint_logged: HashMap<String, WindowSizeHints>,
    /// Prompt-restore wake armed by one async restore dispatch, cleared only
    /// after a gated reconcile consumes it, on expiry, or on member loss.
    restore_wake: Option<RestoreWake>,
    /// Active-border configuration for this run (default on with an explicit
    /// off flag). The border never takes geometry writes: it only reads the
    /// foreground target's fresh frame and paints the process-owned overlay.
    border: ActiveBorderOptions,
    /// Process-owned border overlay plus the last logged border signature
    /// (visibility/rect/style/target) for change-only logging.
    border_overlay: BorderOverlay,
    border_last: Option<String>,
    /// Group-underlay configuration for this run (default on with an explicit
    /// off flag). Like the border it never takes geometry writes: it resolves
    /// the focused window's immediate-parent group through the retained
    /// Engine and paints the process-owned fill beneath the lowest member.
    underlay: GroupUnderlayOptions,
    /// Process-owned underlay fill plus the last logged underlay signature
    /// for change-only logging.
    underlay_overlay: UnderlayOverlay,
    underlay_last: Option<String>,
    /// Classified open move/size gestures by HWND, sampled once at START via
    /// the official `WM_NCHITTEST` result. Only `Move` on the actual focused
    /// window feeds the underlay trigger; resize (or unknown) never does.
    /// Maintained alongside `active`, cleared on END/removal like
    /// `gesture_before`.
    move_kind: HashMap<u64, MoveSizeKind>,
    /// START-time pointer per open native gesture HWND, sampled at
    /// MoveSizeStart drain time. The preview zero gate compares the live
    /// cursor against this point (no preview on a title click with no
    /// movement); the windrag path uses `windrag_start_cursor` instead.
    /// Cleared on END-settle/removal like `gesture_before`.
    gesture_start_cursor: HashMap<u64, (i32, i32)>,
    /// Process-owned drop-preview fill plus the last logged preview
    /// signature for change-only logging. Separate carrier above windows
    /// (KWin overlay-item analogue); never mixed with the border/underlay
    /// below-target plane. Shows only a resolved Engine target slot while
    /// an accepted move gesture holds the loop; never focuses, never
    /// writes geometry, never changes topology.
    preview_overlay: PreviewOverlay,
    preview_last: Option<String>,
    /// Per-sample carried sticky group-edge hover prior per open gesture
    /// HWND. Feeds every preview sample and the final drop through the same
    /// Engine resolver. Identity/output/workspace/revision binding lives in
    /// `gesture_preview_start` (frozen at START, never rebound); drift fails
    /// the gesture closed via `preview_dead`. Cleared on
    /// finish/cancel/suspension/invalidation like `gesture_before`.
    preview_bound: HashMap<u64, PreviewBound>,
    /// START-frozen preview source binding per open gesture HWND: exact
    /// mover token, source output/workspace, and accepted revision. The
    /// preview never rebinds mid-gesture; drift fails the gesture closed
    /// (see `preview_dead`) until settle. Cleared on END-settle/removal
    /// like `gesture_before`.
    gesture_preview_start: HashMap<u64, PreviewStartBinding>,
    /// Fail-closed preview gestures: drift or invalidation killed the
    /// preview for this hold, and no later sample may rebind or render
    /// until settle clears. Cleared on END-settle/removal like
    /// `gesture_before`.
    preview_dead: HashSet<u64>,
    /// Escape latched per open managed gesture HWND from the explicit
    /// keyboard-hook down edge while the gesture holds the loop. An
    /// Esc-cancelled native move restores natively to its pre-gesture frame,
    /// so rect comparison alone cannot tell cancel from zero-move: the latch
    /// makes cancellation explicit at settle time. Cleared on END/removal
    /// like `gesture_before`. Hookless runs never set it.
    esc_latched: HashSet<u64>,
    /// Esc sequence snapshot per open gesture HWND, taken at START from the
    /// keyboard-hook monotonic counter. Only edges with a newer sequence
    /// latch (pre-gesture taps never cancel; fast taps arriving with the END
    /// batch still latch via the single post-loop drain). Cleared on
    /// END/removal like `gesture_before`.
    gesture_esc_seq: HashMap<u64, u64>,
    /// Esc sequence snapshot per open gesture HWND, taken at END from the
    /// keyboard-hook monotonic counter (event-callback time, alongside the
    /// Finish pointer). Ended gestures latch only when this differs from
    /// START, so a physical Esc after the END but before the owner drain can
    /// never cancel a completed drop. Absent entries never latch (fail
    /// closed to no-cancel; settle then classifies from geometry). Cleared
    /// on END/removal like `gesture_before`.
    gesture_end_seq: HashMap<u64, u64>,
    /// Last observed Win+Shift level for the chord-edge wake. Sampled every
    /// pump before the idle skip so a bare chord press/release wakes the
    /// visual refresh on the 100ms cadence instead of waiting for the 2s
    /// slow poll. Steady hold/release stays quiet; only the transition
    /// wakes, and the normal suspend/fullscreen path below still gates it.
    underlay_chord_last: bool,
    /// Prior `WS_EX_TOPMOST` per intentional-float member key, read at float
    /// time. Session-local only. Unfloat and graceful stop restore only bands
    /// the project raised, with no frame change.
    float_topmost_prev: std::collections::BTreeMap<crate::workspace::WindowKey, bool>,
    /// Live float frame per Engine token. Distinct from `member_rects`, which
    /// keeps the last tiled allocation frozen at float time.
    float_rects: HashMap<String, Rect>,
    /// Intentional per-window float lifetimes (exact `(hwnd, pid, creation)`
    /// identity, never HWND alone). The Engine exception is dropped by every
    /// domain release, so this carries the exception across floating
    /// release/retiling and boundary sends: row assembly rides these tokens
    /// floating even when the session carries no exception, and the next
    /// Engine observation re-adopts it. Cleared on verified unfloat, window
    /// close, and stop; pruned with membership. Sticky, born-fullscreen, and
    /// native overlays keep their own lanes and never enter here.
    floated: BTreeSet<crate::workspace::WindowKey>,
    /// Sticky-float members by stable member key to their pre-sticky float
    /// state (`true` when the window was already a normal float before
    /// sticky-on). Single runtime map, never redundant sets. Sticky rides the
    /// Engine as a slotless float plus this map plus a window-lifetime native
    /// marker; the marker (not this map) survives owner restarts.
    sticky: std::collections::BTreeMap<crate::workspace::WindowKey, bool>,
    /// Unconfirmed workspace-domain releases (toggle edges). Retried on
    /// topology edges only (never polled) until the exact domain release
    /// confirms; keyed by domain with latest-intent-wins.
    pending_releases: crate::workspace::PendingReleases,
    /// Topology fingerprint of the last pending-release retry attempt: equal
    /// fingerprints skip silently, so quiet ticks never poll or log.
    pending_retry_fp: u64,
}

/// Armed prompt-restore wake: the restored member by full identity plus the
/// Engine token, so a recycled HWND can never count as completion proof.
struct RestoreWake {
    key: crate::workspace::WindowKey,
    token: String,
    until: Instant,
}

/// One hidden member: the committed ledger claim (carrying the durable
/// show-state preimage, the authority for independent restore) plus the iconic
/// state at hide time for the in-memory reveal fast path and preimage-less
/// legacy claims.
#[derive(Debug, Clone)]
struct HiddenRecord {
    claim: crate::model::WindowIdentity,
    iconic: bool,
}

/// One retained-occupancy member: identity resolved but no tileable frame
/// (minimized without a DWM frame) or an ineligible state with a fresh
/// frame (maximized, fullscreen, cloaked). `rect` is `None` only when no
/// frame exists; the caller falls back to the last-known snapshot so hidden
/// Engine membership and layout survive. `maximized`/`fullscreen` carry the
/// fresh overlay flags for this tick so directional/pointer routes can
/// refuse before any Engine mutation (KDE maximize/fullscreen isolation
/// parity) and admission can clear a first-seen maximize exactly once.
#[derive(Debug, Clone)]
struct RetainedRow {
    key: crate::workspace::WindowKey,
    token: String,
    rect: Option<Rect>,
    maximized: bool,
    fullscreen: bool,
    /// Fresh facts for observed-ineligible rows (`None` for frameless known
    /// rows): lets admission clear only otherwise-eligible maximized members.
    facts: Option<WindowFacts>,
}

impl TileLoop {
    fn correlation(&self) -> CorrelationId {
        CorrelationId::parse(&format!("tick-{}", self.tick))
            .expect("tick correlation is a valid token")
    }

    /// Full enumeration with per-window eligibility. Proof mode prefilters by
    /// frozen-allowlist HWND before any per-window query, so non-owned windows
    /// never mint tokens, churn the map, or enter logs; `EnumWindows` still
    /// yields the complete raw count. Proof mode additionally requires
    /// frozen-allowlist membership plus owned-helper verification (sibling
    /// exe/class/lifetime-tag); malformed allowlist state is unreachable here
    /// because `cmd_tile_proof` refuses it before the loop starts. Invisible
    /// allowlist members stay frozen (not eligible) until an exact-bound
    /// `show` admission. Normal mode with a nonempty scope additionally
    /// reports out-of-scope executables as `scope-excluded` with no
    /// membership, geometry, or focus. Returns `None` when enumeration itself
    /// failed: the tick is skipped with retained Engine state.
    fn observe(
        &mut self,
        me: &ProcessIdentity,
        fulls: &[Rect],
        skipped: &mut Vec<(String, String)>,
        retained: &mut Vec<RetainedRow>,
    ) -> Option<Vec<ObservedWindow>> {
        use ObserveFailure::{Known, Unknown};
        let proof_mode = self.allowlist.is_some();
        let hwnds = enumerate_hwnds()?;
        self.last_enumerated = hwnds.len();
        self.last_hwnds = hwnds.iter().map(|raw| *raw as usize as u64).collect();
        let mut out = Vec::new();
        let mut unreadable = 0usize;
        // Every resolved identity pair mints or reuses a token; retain all of
        // them below (eligible, skipped, and frame-failed) so known windows
        // keep stable tokens instead of churning one new token per tick.
        let mut enumerated: Vec<(u64, String)> = Vec::new();
        for raw in hwnds {
            // The process-owned border overlay is never a tile target, a
            // token source, or an unreadable count: skip it before any query.
            if self.border_overlay.hwnd() == Some(raw as usize as u64) {
                continue;
            }
            // Same for the process-owned group-underlay fill: never a tile
            // target, so its own move/paint events cannot retarget tiling or
            // the underlay itself.
            if self.underlay_overlay.hwnd() == Some(raw as usize as u64) {
                continue;
            }
            let hwnd = raw as HWND;
            if proof_mode {
                // Frozen-HWND prefilter before any per-window query.
                let entries = self.allowlist.as_ref().expect("proof mode has allowlist");
                if !entries
                    .iter()
                    .any(|entry| entry.hwnd == raw as usize as u64)
                {
                    continue;
                }
            }
            let window = match observe_window(hwnd, me, fulls, &mut self.tokens) {
                Ok(window) => window,
                Err(Known(known, reason)) => {
                    // Identity known: retain it and skip with the accurate
                    // reason (minimized needs no frame; unreadable keeps its
                    // stable token instead of counting as changed identity).
                    // Retained-occupancy members ride a row with no fresh
                    // frame so Engine membership survives minimization.
                    // Out-of-scope executables never retain membership.
                    enumerated.push((known.identity.hwnd, known.identity.process_creation.clone()));
                    if !scope_allows(&self.scope, &known.identity.exe_path) {
                        skipped.push((known.token.clone(), "scope-excluded".to_owned()));
                        continue;
                    }
                    skipped.push((known.token.clone(), reason.as_str().to_owned()));
                    retained.push(RetainedRow {
                        key: crate::workspace::WindowKey {
                            hwnd: known.identity.hwnd,
                            pid: known.identity.pid,
                            creation: known.identity.process_creation.clone(),
                        },
                        token: known.token.clone(),
                        rect: None,
                        maximized: false,
                        fullscreen: false,
                        facts: None,
                    });
                    continue;
                }
                Err(Unknown) => {
                    unreadable += 1;
                    continue;
                }
            };
            enumerated.push((window.hwnd, window.identity.process_creation.clone()));
            if proof_mode {
                let entries = self.allowlist.as_ref().expect("proof mode has allowlist");
                let Some(entry) = entries
                    .iter()
                    .find(|entry| allow_match(entry, &window.identity))
                else {
                    continue;
                };
                // Owned-helper gate: non-owned windows never enter management
                // even when otherwise eligible. Invisible frozen members are
                // still verified (passive admission) but stay skipped until
                // shown.
                if verify_proof_owned(window.hwnd, entry, me).is_err() {
                    skipped.push((window.token.clone(), "identity-changed".to_owned()));
                    continue;
                }
            }
            match classify(&window.facts) {
                Ok(()) => {
                    if !scope_allows(&self.scope, &window.identity.exe_path) {
                        skipped.push((window.token.clone(), "scope-excluded".to_owned()));
                        continue;
                    }
                    // Listed hosts pass only with a live matching hosted
                    // child, re-verified every tick: a newly appearing hosted
                    // app can never ride another app's host executable into
                    // observation, membership, writes, or focus. Excluded
                    // members keep their retained snapshot (no membership
                    // loss) and re-enter when the child set is clean again.
                    if !hosted_gate_allows(
                        &window.identity.exe_path,
                        window.hwnd,
                        window.identity.pid,
                        &self.scope_hosts,
                    ) {
                        skipped.push((window.token.clone(), "scope-excluded".to_owned()));
                        retained.push(RetainedRow {
                            key: crate::workspace::WindowKey {
                                hwnd: window.identity.hwnd,
                                pid: window.identity.pid,
                                creation: window.identity.process_creation.clone(),
                            },
                            token: window.token.clone(),
                            rect: None,
                            maximized: false,
                            fullscreen: false,
                            facts: None,
                        });
                        continue;
                    }
                    out.push(window);
                }
                Err(reason) => {
                    skipped.push((window.token.clone(), reason.as_str().to_owned()));
                    // Ineligible but fully observed: the fresh frame keeps
                    // retained occupancy (maximized, fullscreen, cloaked)
                    // inside Engine membership with no geometry writes. The
                    // fresh overlay flags ride along so move/pointer routes
                    // refuse before mutation and admission clears once.
                    retained.push(RetainedRow {
                        key: crate::workspace::WindowKey {
                            hwnd: window.hwnd,
                            pid: window.identity.pid,
                            creation: window.identity.process_creation.clone(),
                        },
                        token: window.token.clone(),
                        rect: Some(window.visible),
                        maximized: window.facts.maximized,
                        fullscreen: window.facts.captionless_fullscreen,
                        facts: Some(window.facts),
                    });
                }
            }
        }
        // Bounded retain over every enumerated pair, never HWND alone and
        // never eligible-only: skipped identities keep their tokens. Known
        // hidden claims are retained too: they never pass the visible gate
        // but their tokens stay stable across ticks.
        let mut live_refs: Vec<ObservedTargetRef<'_>> = enumerated
            .iter()
            .map(|(hwnd, creation)| ObservedTargetRef {
                hwnd: *hwnd,
                creation,
            })
            .collect();
        for record in self.hidden_claims.values() {
            if !live_refs.iter().any(|t| {
                t.hwnd == record.claim.hwnd && t.creation == record.claim.process.process_creation
            }) {
                live_refs.push(ObservedTargetRef {
                    hwnd: record.claim.hwnd,
                    creation: &record.claim.process.process_creation,
                });
            }
        }
        self.tokens.retain(&live_refs);
        if unreadable > 0 {
            skipped.push((
                format!("unreadable-{unreadable}"),
                SkipReason::Unreadable.as_str().to_owned(),
            ));
        }
        track_fullscreen_holds(self, &out, retained);
        Some(out)
    }

    fn focused_token(&self, observed: &[ObservedWindow]) -> Option<WindowId> {
        let foreground = unsafe { GetForegroundWindow() } as usize as u64;
        observed
            .iter()
            .find(|w| w.hwnd == foreground)
            .map(|w| WindowId(w.token.clone()))
    }
}

/// Chord-time origin snapshot for one managed window: HWND plus the
/// process-lifetime evidence the owner rechecks. Only the token may enter logs.
fn snap_origin_of(window: &ObservedWindow) -> SnapOrigin {
    SnapOrigin {
        hwnd: window.hwnd,
        token: window.token.clone(),
        pid: window.identity.pid,
        creation: window.identity.process_creation.clone(),
    }
}

/// Managed origins for chord resolution: eligible observed windows plus
/// verified overlay-retained members (maximized/fullscreen). An overlay
/// member is retained, never eligible, so eligible-only origins break Win+M
/// restore, Win+F11 exit, focus while overlaid, and send from a maximized
/// foreground. Only existing members with full identity plus live
/// lifetime-tag match ride along: no foreign or invented members. Exactness
/// stays with the per-intent owner recheck.
fn managed_origins(
    state: &TileLoop,
    me: &ProcessIdentity,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) -> Vec<SnapOrigin> {
    let mut out: Vec<SnapOrigin> = observed.iter().map(snap_origin_of).collect();
    for row in retained {
        if !row.maximized && !row.fullscreen {
            continue;
        }
        let Some(stored) = state.member_identity.get(&row.key) else {
            continue;
        };
        if stored.pid != row.key.pid
            || stored.process_creation != row.key.creation
            || stored.user_sid != me.user_sid
            || stored.session_id != me.session_id
        {
            continue;
        }
        let live_tag = crate::product_hide::sys::read_member_tag(row.key.hwnd);
        let Some(tag) = state.member_tags.get(&row.key) else {
            continue;
        };
        if !crate::workspace_owner::visible_lifetime_ok(tag, live_tag.as_deref()) {
            continue;
        }
        if !scope_allows(&state.scope, &stored.exe_path) {
            continue;
        }
        if !hosted_gate_allows(
            &stored.exe_path,
            row.key.hwnd,
            stored.pid,
            &state.scope_hosts,
        ) {
            continue;
        }
        if let Some(entries) = state.allowlist.as_ref() {
            let Some(entry) = entries.iter().find(|e| e.hwnd == row.key.hwnd) else {
                continue;
            };
            if verify_proof_owned(row.key.hwnd, entry, me).is_err() {
                continue;
            }
        }
        out.push(SnapOrigin {
            hwnd: row.key.hwnd,
            token: row.token.clone(),
            pid: row.key.pid,
            creation: row.key.creation.clone(),
        });
    }
    out
}

/// Refresh cached management state from a complete observation: managed set,
/// stable rects, gesture retention, and the origin map the keyboard callback
/// binds chords against. The origin map is retained-aware (verified overlay
/// members included) so a maximized foreground still binds a chord origin.
/// Exactness stays with the per-intent owner recheck.
fn publish_managed(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) {
    state.managed = observed.iter().map(|w| w.hwnd).collect();
    state.stable = observed.iter().map(|w| (w.hwnd, w.visible)).collect();
    state
        .gesture_before
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .move_kind
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state.snap_origins = managed_origins(state, me, observed, retained)
        .into_iter()
        .map(|origin| (origin.hwnd, origin))
        .collect();
    // Tiled-only mouse origins: the keyboard map minus intentional floats
    // and sticky members, so Win+Left on those passes through natively.
    // Maximized/fullscreen members are retained (never observed), so they
    // are absent by construction. Each entry carries the live member tag
    // read here on the owner thread, so the hook snapshot needs no native
    // reads in the callback.
    let mut windrag: HashMap<u64, WinDragPublished> = HashMap::new();
    for window in observed {
        let origin = snap_origin_of(window);
        let Some(key) = state
            .member_tokens
            .iter()
            .find(|(_, token)| token.as_str() == window.token.as_str())
            .map(|(key, _)| key.clone())
        else {
            continue;
        };
        if state.sticky.contains_key(&key) {
            continue;
        }
        if let Some(loc) = state.workspaces.member_loc(&key)
            && (engine_is_float(state, &loc.output, &loc.workspace, &window.token)
                || !workspace_mode_tiled(state, &loc.output, &loc.workspace))
        {
            continue;
        }
        windrag.insert(
            origin.hwnd,
            WinDragPublished {
                origin,
                tag: crate::product_hide::sys::read_member_tag(window.hwnd),
            },
        );
    }
    state.windrag_origins = windrag;
    // Change-only hook-feed evidence: the callback binds against this many
    // tiled origins (identity stays out of the log).
    let origin_count = state.windrag_origins.len();
    if state.windrag_origin_logged != Some(origin_count) {
        state.windrag_origin_logged = Some(origin_count);
        log_json_at(
            &state.log_path,
            serde_json::json!({"event":"windrag-origins","tick":state.tick,"count": origin_count}),
        );
    }
    // Drop pre-gesture state for windows that left management, and disarm
    // the hook slot for them so a stale bound down can never settle later.
    for hwnd in state
        .gesture_producer
        .keys()
        .copied()
        .filter(|hwnd| !state.managed.contains(hwnd))
        .collect::<Vec<_>>()
    {
        crate::win_mouse::sys::invalidate(hwnd);
    }
    state
        .esc_latched
        .retain(|hwnd| state.managed.contains(hwnd));
    state
        .gesture_esc_seq
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_end_seq
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_end_cursor
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_start_key
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_start_tag
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_producer
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .windrag_start_cursor
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .windrag_bound
        .retain(|hwnd, _| state.managed.contains(hwnd));
}

/// Hide the border overlay with a change-only `active-border` log line.
fn hide_border(state: &mut TileLoop, reason: &str) {
    let was_visible = state.border_overlay.is_visible();
    state.border_overlay.hide();
    let signature = format!("hidden:{reason}");
    if state.border_last.as_deref() == Some(signature.as_str()) && !was_visible {
        if state.trace {
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "active-border",
                    "tick": state.tick,
                    "outcome": "hidden",
                    "reason": reason,
                }),
            );
        }
        return;
    }
    state.border_last = Some(signature);
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "active-border",
            "tick": state.tick,
            "outcome": "hidden",
            "reason": reason,
        }),
    );
}

/// Fast active-border refresh for one loop wake: reads only the foreground
/// window (fresh identity plus one fresh frame), without full window observation,
/// so gesture following does not wait for reconciliation or the 100ms poll.
/// Scope fences match tiling exactly: normal `--scope-exe`/`--scope-host-child`
/// or the frozen allowlist (plus owned-helper verification) in proof mode.
/// The overlay HWND itself is same-process and never resolves an identity, so
/// its own move/paint events cannot retarget the border.
fn refresh_active_border(state: &mut TileLoop, me: &ProcessIdentity, fulls: &[Rect]) {
    if !state.border.enabled {
        hide_border(state, "disabled");
        return;
    }
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    if foreground == 0 {
        hide_border(state, "no-target");
        return;
    }
    if state.border_overlay.hwnd() == Some(foreground) {
        return;
    }
    let hwnd = foreground as isize as HWND;
    // Proof gate before any frame read: frozen-allowlist membership plus
    // owned-helper verification, mirroring the geometry gate. A foreign
    // foreground in proof mode is never a border target.
    if let Some(entries) = state.allowlist.as_ref() {
        let entry = entries.iter().find(|e| e.hwnd == foreground);
        let Some(entry) = entry else {
            hide_border(state, "allowlist-changed");
            return;
        };
        if verify_proof_owned(foreground, entry, me).is_err() {
            hide_border(state, "identity-changed");
            return;
        }
    }
    let window = match observe_window(hwnd, me, fulls, &mut state.tokens) {
        Ok(window) => window,
        Err(ObserveFailure::Known(_, reason)) => {
            hide_border(
                state,
                match reason {
                    SkipReason::Minimized => "minimized",
                    SkipReason::Maximized => "maximized",
                    SkipReason::Fullscreen => "fullscreen",
                    SkipReason::Cloaked => "cloaked",
                    SkipReason::Shell => "shell",
                    _ => "no-target",
                },
            );
            return;
        }
        Err(ObserveFailure::Unknown) => {
            hide_border(state, "no-target");
            return;
        }
    };
    // Normal-mode scope fence (proof mode already gated above): out-of-scope
    // executables never paint, mirroring observation membership.
    if !scope_allows(&state.scope, &window.identity.exe_path)
        || !hosted_gate_allows(
            &window.identity.exe_path,
            window.hwnd,
            window.identity.pid,
            &state.scope_hosts,
        )
    {
        hide_border(state, "scope-excluded");
        return;
    }
    let hidden_workspace = state.hidden_claims.keys().any(|k| k.hwnd == foreground)
        && state
            .workspaces
            .member_loc(
                &state
                    .hidden_claims
                    .keys()
                    .find(|k| k.hwnd == foreground)
                    .cloned()
                    .expect("hidden key present"),
            )
            .is_some_and(|loc| loc.hidden);
    // Targeted shell-popup gate (border-only): base tiling shell plus
    // Start/taskbar-flyout/Alt+Tab/TaskView analogues. Ordinary dialogs,
    // tool windows, and owned windows stay eligible here.
    let exe_file = exe_file_name(&window.identity.exe_path).to_ascii_lowercase();
    let border_shell = window.facts.shell || is_border_shell_popup(&class_of(hwnd), &exe_file);
    if border_eligible(
        true,
        true,
        window.visible,
        window.facts.minimized,
        window.facts.captionless_fullscreen,
        window.facts.maximized,
        window.facts.cloaked,
        hidden_workspace,
        border_shell,
    )
    .is_err()
    {
        hide_border(
            state,
            if window.facts.minimized {
                "minimized"
            } else if window.facts.captionless_fullscreen {
                "fullscreen"
            } else if window.facts.maximized {
                "maximized"
            } else if window.facts.cloaked {
                "cloaked"
            } else if hidden_workspace {
                "hidden-workspace"
            } else if border_shell {
                "shell"
            } else {
                "no-target"
            },
        );
        return;
    }
    // Mirror the target's topmost band so a topmost active window stays
    // bordered; ordinary targets demote a previously topmost ring. Read-only
    // style query, never a foreign write.
    let topmost = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST != 0;
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let Some((width_px, gap_px, radius_px)) = scale_style(&state.border.style, dpi) else {
        hide_border(state, "dpi-scale");
        return;
    };
    if width_px == 0 {
        hide_border(state, "zero-width");
        return;
    }
    let Some(outer) = border_outer_rect(window.visible, gap_px, width_px) else {
        hide_border(state, "geometry");
        return;
    };
    let outcome = state.border_overlay.show_at(
        outer,
        width_px,
        gap_px,
        radius_px,
        &state.border.style,
        &window.token,
        topmost,
        window.hwnd,
    );
    match outcome {
        OverlayOutcome::Hidden => {
            // Bounded failure line with the concrete error; change-only
            // unless tracing. State was already hidden by the overlay.
            // NOTE: the shown signature must NOT be installed first: doing
            // so flips border_last between shown and failure every tick and
            // re-logs the same failure per tick. Compare/set only failure.
            let failure = format!(
                "hidden:present-failed:{}",
                state.border_overlay.last_error().unwrap_or("unknown")
            );
            let changed_failure = state.border_last.as_deref() != Some(failure.as_str());
            if changed_failure {
                state.border_last = Some(failure);
            }
            if changed_failure || state.trace {
                log_json_at(
                    &state.log_path,
                    serde_json::json!({
                        "event": "active-border",
                        "tick": state.tick,
                        "outcome": "hidden",
                        "reason": "present-failed",
                        "last_error": state.border_overlay.last_error(),
                    }),
                );
            }
        }
        OverlayOutcome::Unchanged => {
            // Resolved accent-or-fallback colour rides the signature so a
            // theme change that repaints the same geometry still logs once.
            let color_hex = state
                .border_overlay
                .current_color()
                .map(crate::active_border::render_color)
                .unwrap_or_else(|| crate::active_border::render_color(state.border.style.color));
            let signature = format!(
                "shown:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                window.token,
                outer.x,
                outer.y,
                outer.w,
                outer.h,
                width_px,
                gap_px,
                topmost,
                color_hex
            );
            let changed = state.border_last.as_deref() != Some(signature.as_str());
            if changed {
                state.border_last = Some(signature);
            }
            if state.trace && changed {
                log_json_at(
                    &state.log_path,
                    serde_json::json!({
                        "event": "active-border",
                        "tick": state.tick,
                        "outcome": "unchanged",
                        "target": window.token,
                    }),
                );
            }
        }
        OverlayOutcome::Moved if state.trace => {
            // Position-only follows are trace-only; production stays quiet
            // off the lifecycle transitions below.
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "active-border",
                    "tick": state.tick,
                    "outcome": "moved",
                    "target": window.token,
                    "outer": [outer.x, outer.y, outer.w, outer.h],
                    "style_px": [width_px, gap_px, radius_px],
                }),
            );
        }
        OverlayOutcome::Moved => {}
        _ => {
            let color_hex = state
                .border_overlay
                .current_color()
                .map(crate::active_border::render_color)
                .unwrap_or_else(|| crate::active_border::render_color(state.border.style.color));
            let signature = format!(
                "shown:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                window.token,
                outer.x,
                outer.y,
                outer.w,
                outer.h,
                width_px,
                gap_px,
                topmost,
                color_hex
            );
            let changed = state.border_last.as_deref() != Some(signature.as_str());
            if changed {
                state.border_last = Some(signature);
            }
            if !(changed || state.trace) {
                return;
            }
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "active-border",
                    "tick": state.tick,
                    "outcome": match outcome {
                        OverlayOutcome::Shown => "shown",
                        OverlayOutcome::Redrew => "redrew",
                        _ => "shown",
                    },
                    "target": window.token,
                    "outer": [outer.x, outer.y, outer.w, outer.h],
                    "style_px": [width_px, gap_px, radius_px],
                    "color": color_hex,
                    "dib_checksum": state.border_overlay.snapshot()["dib_checksum"],
                }),
            );
        }
    }
}

/// Hide the underlay fill with a change-only `group-underlay` log line.
fn hide_underlay(state: &mut TileLoop, reason: &str) {
    let was_visible = state.underlay_overlay.is_visible();
    state.underlay_overlay.hide();
    let signature = format!("hidden:{reason}");
    if state.underlay_last.as_deref() == Some(signature.as_str()) && !was_visible {
        if state.trace {
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "group-underlay",
                    "tick": state.tick,
                    "outcome": "hidden",
                    "reason": reason,
                }),
            );
        }
        return;
    }
    state.underlay_last = Some(signature);
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "group-underlay",
            "tick": state.tick,
            "outcome": "hidden",
            "reason": reason,
        }),
    );
}

/// Hide the drop preview with change-only logging (no-op when already
/// hidden). Every cancel, refusal, zero, self/centre/outside, suspend,
/// teardown, and drift path funnels here so no stale rectangle survives.
fn hide_preview(state: &mut TileLoop, reason: &str) {
    let was_visible = state.preview_overlay.is_visible();
    state.preview_overlay.hide();
    let signature = format!("hidden:{reason}");
    if state.preview_last.as_deref() == Some(signature.as_str()) && !was_visible {
        return;
    }
    state.preview_last = Some(signature);
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "drag-preview",
            "tick": state.tick,
            "outcome": "hidden",
            "reason": reason,
        }),
    );
}

/// Level-observed Win+Shift chord: either Win side plus any Shift side held.
/// Extras are allowed (only these keys are read) and either press order works
/// because this samples levels, never sequences edges. Independent of the
/// keyboard-hook gesture path, so a bare hold with no other key still reads.
fn chord_held_now() -> (bool, bool) {
    let down = |vk: u32| unsafe { GetAsyncKeyState(vk as i32) } < 0;
    aggregate_chord_keys(
        down(VK_LWIN),
        down(VK_RWIN),
        down(VK_SHIFT),
        down(VK_LSHIFT),
        down(VK_RSHIFT),
    )
}

/// `WM_NCHITTEST` timeout for one move/size classification: bounded like the
/// minimum-hint query, fail-closed to `Unknown` on timeout or failure.
const HITTEST_TIMEOUT_MS: u32 = 10;

/// Classify one open move/size gesture at START via the official
/// `WM_NCHITTEST` result at the current cursor position. `EVENT_SYSTEM_MOVESIZESTART`
/// does not distinguish move from resize; asking the window itself (through
/// `DefWindowProc`, system-marshalled like `WM_GETMINMAXINFO`) is a bounded
/// START-drain heuristic: no new hook, no SC_MOVE/SC_SIZE interception, no
/// deferred rect comparison (which cannot show a stationary start). Hung or
/// unreadable windows classify `Unknown` and never trigger.
fn classify_move_size_start(hwnd_u64: u64) -> MoveSizeKind {
    let hwnd = hwnd_u64 as isize as HWND;
    let mut cursor: POINT = unsafe { std::mem::zeroed() };
    if unsafe { GetCursorPos(&mut cursor) } == 0 {
        return MoveSizeKind::Unknown;
    }
    // MAKELPARAM packing: low word x, high word y (truncation is the packing).
    let lparam = ((cursor.y as u32) << 16) | (cursor.x as u32 & 0xFFFF);
    let mut result: usize = 0;
    let sent = unsafe {
        SetLastError(0);
        SendMessageTimeoutW(
            hwnd,
            WM_NCHITTEST,
            0,
            lparam as usize as LPARAM,
            SMTO_ABORTIFHUNG,
            HITTEST_TIMEOUT_MS,
            &mut result,
        )
    };
    if sent == 0 {
        return MoveSizeKind::Unknown;
    }
    classify_hit_test(result as u32)
}

/// Fresh outer rectangle for one HWND via a single `GetWindowRect` read.
/// START-time fallback only, when `stable` has no pre-gesture entry. `None`
/// on failure: the caller treats the gesture as no-start.
fn live_outer_rect(hwnd_u64: u64) -> Option<Rect> {
    let hwnd = hwnd_u64 as isize as HWND;
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return None;
    }
    let w = rect.right.saturating_sub(rect.left);
    let h = rect.bottom.saturating_sub(rect.top);
    if w <= 0 || h <= 0 {
        return None;
    }
    Some(Rect {
        x: rect.left,
        y: rect.top,
        w,
        h,
    })
}

/// Fresh renderable check for one underlay anchor candidate: a live,
/// visible, non-iconic, non-cloaked top-level window. DWM cloak-query
/// failure fails closed (not renderable). Smallest causal gate so a stale
/// Engine member (minimized/hidden/cloaked between reconciles) never wins
/// the lowest-in-z anchor.
fn underlay_anchor_renderable(hwnd_u64: u64) -> bool {
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return false;
    }
    if unsafe { IsWindowVisible(hwnd) } == 0 {
        return false;
    }
    if unsafe { IsIconic(hwnd) } != 0 {
        return false;
    }
    let mut cloaked: i32 = 0;
    if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut i32).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    } != 0
    {
        return false;
    }
    cloaked == 0
}

/// Map one Engine `no-group` reason to the closed underlay hide vocabulary.
/// `NoParentGroup` is the root-leaf focus the resolver reports; focus drift
/// between the fresh foreground read and the retained focus reports
/// `focus-changed` and may restore on the next valid refresh.
fn underlay_no_group_reason(reason: NoGroupReason) -> &'static str {
    match reason {
        NoGroupReason::Pending => "pending",
        NoGroupReason::Diverged => "diverged",
        NoGroupReason::NoParentGroup => "root-leaf",
        NoGroupReason::FocusMismatch | NoGroupReason::FocusUnmapped => "focus-changed",
        NoGroupReason::NoSession
        | NoGroupReason::DomainMismatch
        | NoGroupReason::StaleRevision
        | NoGroupReason::NoTree => "no-group",
    }
}

/// Fast group-underlay refresh for one loop wake: the movement-only staged
/// lifetime (A Win+Shift chord, B focused interactive move) over the
/// Engine-projected source-group union.
///
/// Trigger is the shared OR: level-observed Win+Shift (either side, extras
/// allowed, either order) or a classified `Move` gesture on the actual
/// foreground window. Resize alone never triggers; the chord stays
/// independent during resize. Move-start evidence never depends on modifier
/// observation.
///
/// Geometry is the authoritative Engine route only: `CoreCommand::ActiveGroup`
/// through `resolve_active_group`/`describe_active_group` with the fresh
/// actual focused token (never an unfocused dragged id, which would persist
/// false retained focus). Pending/drag residue fails closed in the resolver;
/// the union shown is always the projected source union, never live dragged
/// bounds. The fill lands directly beneath the lowest renderable member in
/// `EnumWindows` order (the border surface included when visible), i.e. below
/// members and below the active border.
fn refresh_group_underlay(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    if !state.underlay.enabled {
        hide_underlay(state, "disabled");
        return;
    }
    let (win_held, shift_held) = chord_held_now();
    let chord = tiler_core::visual::group_underlay_chord_held(win_held, shift_held);
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    if foreground == 0 {
        hide_underlay(state, "no-target");
        return;
    }
    if state.border_overlay.hwnd() == Some(foreground)
        || state.underlay_overlay.hwnd() == Some(foreground)
    {
        return;
    }
    // Fresh actual focused-subject match: only a classified move on this exact
    // foreground window feeds the move arm. Stale START entries for other
    // windows never match, and END/removal clearing drops them.
    let move_active = state
        .move_kind
        .get(&foreground)
        .is_some_and(|kind| *kind == MoveSizeKind::Move);
    if !tiler_core::visual::group_underlay_trigger(chord, move_active) {
        hide_underlay(state, "idle");
        return;
    }
    if let Some(entries) = state.allowlist.as_ref() {
        let entry = entries.iter().find(|e| e.hwnd == foreground);
        let Some(entry) = entry else {
            hide_underlay(state, "allowlist-changed");
            return;
        };
        if verify_proof_owned(foreground, entry, me).is_err() {
            hide_underlay(state, "identity-changed");
            return;
        }
    }
    let hwnd = foreground as isize as HWND;
    let window = match observe_window(hwnd, me, fulls, &mut state.tokens) {
        Ok(window) => window,
        Err(ObserveFailure::Known(_, reason)) => {
            hide_underlay(
                state,
                match reason {
                    SkipReason::Minimized => "minimized",
                    SkipReason::Maximized => "maximized",
                    SkipReason::Fullscreen => "fullscreen",
                    _ => "no-target",
                },
            );
            return;
        }
        Err(ObserveFailure::Unknown) => {
            hide_underlay(state, "no-target");
            return;
        }
    };
    if !scope_allows(&state.scope, &window.identity.exe_path)
        || !hosted_gate_allows(
            &window.identity.exe_path,
            window.hwnd,
            window.identity.pid,
            &state.scope_hosts,
        )
    {
        hide_underlay(state, "scope-excluded");
        return;
    }
    if window.facts.minimized {
        hide_underlay(state, "minimized");
        return;
    }
    if window.facts.maximized {
        hide_underlay(state, "maximized");
        return;
    }
    if window.facts.captionless_fullscreen {
        hide_underlay(state, "fullscreen");
        return;
    }
    // Tiled membership only: a focused window with no Engine membership has
    // no group to highlight.
    let member_key = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == window.token.as_str())
        .map(|(key, _)| key.clone());
    let Some(member_key) = member_key else {
        hide_underlay(state, "floating");
        return;
    };
    let Some(loc) = state.workspaces.member_loc(&member_key).cloned() else {
        hide_underlay(state, "no-group");
        return;
    };
    // Floats hold no tile group: the fill stays hidden on a focused float
    // while the active border still marks it. Floating workspaces hold no
    // tiled group either: every member frame stays native.
    if engine_is_float(state, &loc.output, &loc.workspace, &window.token)
        || !workspace_mode_tiled(state, &loc.output, &loc.workspace)
    {
        hide_underlay(state, "floating");
        return;
    }
    let Some((domain, domain_key)) = workspace_domain_for(
        &loc.output,
        &loc.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        hide_underlay(state, "no-group");
        return;
    };
    // Copies for the diagnostics payload: `domain` moves into the event below.
    let domain_bounds = domain.bounds;
    let domain_gap = domain.gap;
    // Advisory minimums for the ActiveGroup projection: the same retained
    // per-window hints the ordinary plan for this state used. Reads retained
    // maps only (no native queries, no writes); unknown members resolve to
    // no hints exactly like ordinary plans.
    let hint_rows: Vec<(
        String,
        String,
        String,
        tiler_core::size_hints::WindowSizeHints,
    )> = state
        .member_tokens
        .iter()
        .filter_map(|(key, token)| {
            let sibling = state.workspaces.member_loc(key)?;
            if sibling.output != loc.output || sibling.workspace != loc.workspace {
                return None;
            }
            let hints = state
                .hint_logged
                .get(token)
                .copied()
                .unwrap_or_else(tiler_core::size_hints::WindowSizeHints::none);
            Some((
                token.clone(),
                sibling.output.clone(),
                sibling.workspace.clone(),
                hints,
            ))
        })
        .collect();
    let hint_members: Vec<crate::group_underlay::UnderlayHintMember<'_>> = hint_rows
        .iter()
        .map(
            |(token, output, workspace, hints)| crate::group_underlay::UnderlayHintMember {
                token,
                output,
                workspace,
                hints: *hints,
            },
        )
        .collect();
    let event = tiler_core::boundary::CoreEvent {
        owner: state.owner.clone(),
        generation: state.generation.clone(),
        correlation: state.correlation(),
        revision: revision_for(state, &loc.output, &loc.workspace),
        fingerprint: 0,
        domain,
        domain_key,
        outer_gap: state.outer_gap,
        focused_window: WindowId(window.token.clone()),
        windows: crate::group_underlay::underlay_hint_windows(&hint_members),
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: Vec::new(),
        command: CoreCommand::ActiveGroup,
    };
    let found = match state.engine.handle(&event) {
        CoreReply::ActiveGroup(found) => found,
        CoreReply::NoGroup { reason, .. } => {
            hide_underlay(state, underlay_no_group_reason(reason));
            return;
        }
        _ => {
            hide_underlay(state, "no-group");
            return;
        }
    };
    if underlay_eligible(
        true,
        true,
        true,
        window.facts.maximized,
        window.facts.captionless_fullscreen,
        false,
    )
    .is_err()
    {
        hide_underlay(state, "no-group");
        return;
    }
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let Some((width_px, gap_px, _)) = scale_style(&state.border.style, dpi) else {
        hide_underlay(state, "dpi-scale");
        return;
    };
    let extension_logical = tiler_core::visual::group_underlay_effective_extension(
        state.underlay.style.extension,
        state.border.style.width,
    );
    let Some(extension_px) = scale_to_physical(extension_logical, dpi) else {
        hide_underlay(state, "dpi-scale");
        return;
    };
    let Some(outer) = underlay_outer_rect(found.bounds, gap_px, width_px, extension_px) else {
        hide_underlay(state, "geometry");
        return;
    };
    // Anchor beneath the lowest renderable member; the border surface sorts
    // into the same comparison only when visible so the fill stays below it
    // too. Member candidates stay identity-valid (still managed) and freshly
    // renderable (visible, non-iconic, non-cloaked); stale Engine members
    // between reconciles fail closed to `anchor-missing`.
    let mut candidates: Vec<u64> = found
        .members
        .iter()
        .filter_map(|member| {
            state
                .snap_origins
                .iter()
                .find(|(_, origin)| origin.token == member.window.0)
                .map(|(hwnd, _)| *hwnd)
        })
        .filter(|hwnd| state.managed.contains(hwnd) && underlay_anchor_renderable(*hwnd))
        .collect();
    if state.border_overlay.is_visible()
        && let Some(border_hwnd) = state.border_overlay.hwnd()
    {
        candidates.push(border_hwnd);
    }
    let Some(anchor) = lowest_in_z(&candidates) else {
        hide_underlay(state, "anchor-missing");
        return;
    };
    let topmost =
        unsafe { GetWindowLongW(anchor as isize as HWND, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST != 0;
    let color = (
        state.underlay.style.alpha,
        state.underlay.style.color.0,
        state.underlay.style.color.1,
        state.underlay.style.color.2,
    );
    let outcome = state
        .underlay_overlay
        .show_fill_at(outer, color, &window.token, topmost, anchor);
    let color_hex = crate::group_underlay::render_color_argb((
        state.underlay.style.color,
        state.underlay.style.alpha,
    ));
    // Bounded geometry observability only: the Engine union, immediate-parent
    // group/leaf, member rects, DPI/pad inputs, and domain/revision ride the
    // already-logged shown/redrew/moved lines. No new lines, no geometry or
    // placement change; the change-gated signature below is untouched.
    let member_windows: Vec<String> = found
        .members
        .iter()
        .map(|member| member.window.0.clone())
        .collect();
    let member_rects: Vec<Rect> = found.members.iter().map(|member| member.rect).collect();
    let geometry_debug = underlay_geometry_debug(crate::group_underlay::UnderlayGeometryReport {
        union_rect: found.bounds,
        group: found.group.0.as_str(),
        focused_leaf: found.focused_leaf.0.as_str(),
        member_windows: &member_windows,
        member_rects: &member_rects,
        dpi,
        gap_px,
        width_px,
        extension_px,
        domain_bounds,
        domain_gap,
        base_revision: found.base_revision,
    });
    match outcome {
        OverlayOutcome::Hidden => {
            let failure = format!(
                "hidden:present-failed:{}",
                state.underlay_overlay.last_error().unwrap_or("unknown")
            );
            let changed_failure = state.underlay_last.as_deref() != Some(failure.as_str());
            if changed_failure {
                state.underlay_last = Some(failure);
            }
            if changed_failure || state.trace {
                log_json_at(
                    &state.log_path,
                    serde_json::json!({
                        "event": "group-underlay",
                        "tick": state.tick,
                        "outcome": "hidden",
                        "reason": "present-failed",
                        "last_error": state.underlay_overlay.last_error(),
                    }),
                );
            }
        }
        OverlayOutcome::Unchanged => {
            let signature = format!(
                "shown:{}:{}:{}:{}:{}:{}:{}:{}",
                window.token,
                outer.x,
                outer.y,
                outer.w,
                outer.h,
                topmost,
                color_hex,
                state.underlay_overlay.snapshot()["dib_checksum"],
            );
            let changed = state.underlay_last.as_deref() != Some(signature.as_str());
            if changed {
                state.underlay_last = Some(signature);
            }
            if state.trace && changed {
                log_json_at(
                    &state.log_path,
                    serde_json::json!({
                        "event": "group-underlay",
                        "tick": state.tick,
                        "outcome": "unchanged",
                        "target": window.token,
                    }),
                );
            }
        }
        OverlayOutcome::Moved if state.trace => {
            let mut event = serde_json::json!({
                    "event": "group-underlay",
                    "tick": state.tick,
                    "outcome": "moved",
                    "target": window.token,
                    "outer": [outer.x, outer.y, outer.w, outer.h],
            });
            if let Some(fields) = geometry_debug.as_object() {
                for (key, value) in fields {
                    event[key] = value.clone();
                }
            }
            log_json_at(&state.log_path, event);
        }
        OverlayOutcome::Moved => {}
        _ => {
            let signature = format!(
                "shown:{}:{}:{}:{}:{}:{}:{}:{}",
                window.token,
                outer.x,
                outer.y,
                outer.w,
                outer.h,
                topmost,
                color_hex,
                state.underlay_overlay.snapshot()["dib_checksum"],
            );
            let changed = state.underlay_last.as_deref() != Some(signature.as_str());
            if changed {
                state.underlay_last = Some(signature);
            }
            if !(changed || state.trace) {
                return;
            }
            let mut event = serde_json::json!({
                    "event": "group-underlay",
                    "tick": state.tick,
                    "outcome": match outcome {
                        OverlayOutcome::Shown => "shown",
                        OverlayOutcome::Redrew => "redrew",
                        _ => "shown",
                    },
                    "target": window.token,
                    "outer": [outer.x, outer.y, outer.w, outer.h],
                    "color": color_hex,
                    "members": found.members.len(),
                    "dib_checksum": state.underlay_overlay.snapshot()["dib_checksum"],
            });
            if let Some(fields) = geometry_debug.as_object() {
                for (key, value) in fields {
                    event[key] = value.clone();
                }
            }
            log_json_at(&state.log_path, event);
        }
    }
}

/// Drop-preview refresh for one loop wake while an accepted move gesture
/// holds the loop: a separate owned click-through nonactivating filled
/// target-slot surface ABOVE windows (KWin overlay-item analogue).
///
/// Both accepted producers feed this path: the native caption modal loop
/// (frame-following) and the stationary Win+Left hold (pointer-tracked).
/// Each 100ms pump sample coalesces on pointer movement, assembles a fresh
/// complete canonical domain observation with size hints, and routes the
/// SAME Engine `DragPreview` resolver as the final drop, carrying the exact
/// sticky group-edge hover prior across samples (and into the drop via
/// `preview_bound`). Refusals (self/centre/outside/zero/Esc) hide with no
/// rectangle; only a real moved pointer with a valid resolved target shows.
///
/// Never focuses, never writes geometry, never touches topology or the
/// reconciler: the Engine call is the read-only working-clone preview, and
/// the only effect is the owned overlay surface. Resize, floating, and
/// sticky gestures never show.
fn refresh_drag_preview(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    use crate::active_border_sys::PREVIEW_DEFAULT_ARGB;
    // Open gestures only: managed, held, with START capture. The producer
    // distinguishes the stationary Win+Left hold (frames frozen by design)
    // from the native modal loop. Resize-vs-move for the native loop is
    // decided below from live frames, never from the START hit test (a
    // top-edge caption grab hit-tests as a sizing border).
    let mut movers: Vec<u64> = state
        .active
        .iter()
        .copied()
        .filter(|hwnd| state.managed.contains(hwnd) && state.gesture_before.contains_key(hwnd))
        .collect();
    movers.sort_unstable();
    // Preview-unrelated holds (no START capture, e.g. unmanaged races)
    // never keep a rectangle.
    for hwnd in state.active.iter().copied().collect::<Vec<_>>() {
        if !movers.contains(&hwnd) && state.preview_bound.remove(&hwnd).is_some() {
            state.gesture_start_cursor.remove(&hwnd);
        }
    }
    if movers.is_empty() {
        // No open move: late samples after Finish must never render.
        if state.preview_overlay.is_visible() || !state.preview_bound.is_empty() {
            hide_preview(state, "idle");
        }
        state.preview_bound.clear();
        // Gesture-scoped diagnostic: an open managed hold with no move
        // classification (resize/unknown) explains a silent preview.
        // Change-only so a held resize logs once, never per pump.
        let held: Vec<&str> = state
            .active
            .iter()
            .filter(|hwnd| state.managed.contains(hwnd))
            .map(|hwnd| {
                if state.gesture_producer.get(hwnd).copied() == Some("windrag") {
                    "windrag-unclassified"
                } else {
                    match state.move_kind.get(hwnd) {
                        Some(MoveSizeKind::Move) => "move-unheld",
                        Some(MoveSizeKind::Resize) => "resize",
                        _ => "unknown",
                    }
                }
            })
            .collect();
        if !held.is_empty() {
            let signature = format!("idle:{}", held.join(","));
            if state.preview_last.as_deref() != Some(signature.as_str()) {
                state.preview_last = Some(signature);
                log_json_at(
                    &state.log_path,
                    serde_json::json!({
                        "event": "drag-preview",
                        "tick": state.tick,
                        "outcome": "idle",
                        "held": held,
                    }),
                );
            }
        }
        return;
    }
    // One gesture at a time by construction; extra entries fail closed
    // with no rebind until settle.
    if movers.len() > 1 {
        hide_preview(state, "multi-gesture");
        state.preview_bound.clear();
        for hwnd in movers {
            state.preview_dead.insert(hwnd);
            state.gesture_start_cursor.remove(&hwnd);
        }
        return;
    }
    let hwnd = movers[0];
    // Logical cancel hides IMMEDIATELY, never waiting for mouse-up (the
    // stationary gesture has no native modal loop to end it).
    if state.esc_latched.contains(&hwnd) {
        hide_preview(state, "esc-cancelled");
        state.preview_bound.remove(&hwnd);
        state.preview_dead.remove(&hwnd);
        state.gesture_preview_start.remove(&hwnd);
        state.gesture_start_cursor.remove(&hwnd);
        return;
    }
    // A dead gesture never rebinds and never samples again until settle.
    if state.preview_dead.contains(&hwnd) {
        return;
    }
    let is_windrag = state.gesture_producer.get(&hwnd).copied() == Some("windrag");
    // START-frozen source binding (token, output/workspace, revision): bound
    // at START, never rebound. Drift fails the whole gesture closed below.
    let Some(start) = state.gesture_preview_start.get(&hwnd).cloned() else {
        hide_preview(state, "no-start");
        state.preview_dead.insert(hwnd);
        return;
    };
    // Frame-grounded move gate on both producers: a move keeps the
    // pre-gesture size while the origin travels; a resize changes it. The
    // START hit test cannot decide this (top-edge caption grabs hit-test as
    // sizing borders), so resize holds hide here the moment any edge moves.
    // Lane care: `gesture_before` prefers the stable visible (DWM
    // extended-frame) rect with a GetWindowRect-outer fallback, so either
    // lane matching means same-size (accepted review outcome; a true resize
    // moves both).
    let probe = hwnd as isize as HWND;
    let live_frame = match observe_window(probe, me, fulls, &mut state.tokens) {
        Ok(window) => window,
        Err(_) => {
            hide_preview(state, "no-target");
            state.preview_bound.remove(&hwnd);
            state.preview_dead.insert(hwnd);
            return;
        }
    };
    let moved_size = state.gesture_before.get(&hwnd).is_some_and(|before| {
        preview_same_size(
            before.w,
            before.h,
            live_frame.outer.w,
            live_frame.outer.h,
            live_frame.visible.w,
            live_frame.visible.h,
        )
    });
    if !moved_size {
        hide_preview(state, "resize");
        state.preview_bound.remove(&hwnd);
        state.preview_dead.insert(hwnd);
        return;
    }
    let Some((start_x, start_y)) = (if is_windrag {
        state.windrag_start_cursor.get(&hwnd).copied()
    } else {
        state.gesture_start_cursor.get(&hwnd).copied()
    }) else {
        hide_preview(state, "no-start");
        state.preview_bound.remove(&hwnd);
        state.preview_dead.insert(hwnd);
        return;
    };
    let Some((x, y)) = cursor_pos() else {
        hide_preview(state, "no-cursor");
        return;
    };
    if let Some(entries) = state.allowlist.as_ref()
        && entries.iter().all(|entry| entry.hwnd != hwnd)
    {
        hide_preview(state, "allowlist-changed");
        state.preview_bound.remove(&hwnd);
        state.preview_dead.insert(hwnd);
        return;
    }
    // Fresh invalidation facts against the START binding: identity, tag,
    // remap, membership, revision, float, and same-output scope. The pure
    // gate below orders them (invalidation fails the whole gesture closed
    // even with a static pointer; zero/outside hide transiently). No
    // pointer-movement short-circuit runs before these checks, so a
    // lingering rectangle can never survive mover death, reuse, or domain
    // drift on a stationary pointer.
    let live = window_identity(probe, hwnd, me);
    let live_tag = crate::product_hide::sys::read_member_tag(hwnd);
    let start_key = state.gesture_start_key.get(&hwnd).cloned();
    let identity_ok = match (&start_key, &live) {
        (Some(key), Some(identity)) => crate::workspace_owner::member_matches(
            key,
            hwnd,
            identity.pid,
            &identity.process_creation,
        ),
        _ => false,
    };
    let tag_ok = state
        .gesture_start_tag
        .get(&hwnd)
        .is_none_or(|stored| stored == &live_tag);
    let token_ok = start_key.as_ref().is_some_and(|key| {
        state
            .member_tokens
            .get(key)
            .is_some_and(|token| token.as_str() == start.token.as_str())
    });
    let loc = start_key
        .as_ref()
        .and_then(|key| state.workspaces.member_loc(key).cloned());
    let loc_ok = loc
        .as_ref()
        .is_some_and(|loc| loc.output == start.output && loc.workspace == start.workspace);
    let revision = revision_for(state, &start.output, &start.workspace);
    let revision_ok = revision == start.revision;
    let float_hold = start_key
        .as_ref()
        .is_some_and(|key| state.sticky.contains_key(key))
        || engine_is_float(state, &start.output, &start.workspace, &start.token)
        || !workspace_mode_tiled(state, &start.output, &start.workspace);
    let domain = workspace_domain_for(
        &start.output,
        &start.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    );
    let inside = domain.as_ref().is_some_and(|(domain, _)| {
        drop_point_in_domain(
            domain.bounds.x,
            domain.bounds.y,
            domain.bounds.w,
            domain.bounds.h,
            x,
            y,
        )
    });
    match preview_sample_gate(&PreviewFresh {
        dead: false,
        zero: !PreviewBound::sample_allowed(start_x, start_y, x, y, false),
        identity_ok,
        tag_ok,
        token_ok,
        loc_ok,
        revision_ok,
        float_hold,
        inside,
    }) {
        PreviewGate::Proceed => {}
        PreviewGate::Transient(reason) => {
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = None;
            }
            hide_preview(state, reason);
            return;
        }
        PreviewGate::Dead(reason) => {
            if reason == "revision-drift" {
                // Revision drift clears the carried prior first: nothing
                // stale may render or ride into a later drop.
                if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                    bound.hover_prior = None;
                }
            }
            hide_preview(state, reason);
            state.preview_bound.remove(&hwnd);
            state.preview_dead.insert(hwnd);
            return;
        }
    }
    let Some((domain, domain_key)) = domain else {
        hide_preview(state, "no-group");
        state.preview_bound.remove(&hwnd);
        state.preview_dead.insert(hwnd);
        return;
    };
    // Fresh complete canonical domain observation with size hints: the same
    // row assembly as the settle path, so preview and drop resolve over
    // identical inputs through the shared resolver. Enumeration or row
    // failure hides (never a lingering rectangle on incomplete input).
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        hide_preview(state, "observe-failed");
        return;
    };
    let correlation = state.correlation();
    let mut hint_cx = HintCx::new();
    let Some(rows) = assemble_domain_rows(
        state,
        &start.output,
        &start.workspace,
        &observed,
        &retained,
        "drag-preview",
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        hide_preview(state, "incomplete");
        return;
    };
    if !rows.iter().any(|row| row.token == start.token) {
        hide_preview(state, "unmanaged");
        state.preview_bound.remove(&hwnd);
        state.preview_dead.insert(hwnd);
        return;
    }
    // The 100ms pump already coalesces on the latest pointer; every
    // reaching sample re-resolves so fresh size-hint changes update the
    // preview even while the pointer is static.
    state
        .preview_bound
        .entry(hwnd)
        .or_insert(PreviewBound { hover_prior: None });
    let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let fp = fingerprint(
        &rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect::<Vec<_>>(),
    );
    let mover = WindowId(start.token.clone());
    let carried = state
        .preview_bound
        .get(&hwnd)
        .and_then(|bound| bound.hover_prior.clone());
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        start.revision,
        fp,
        &domain,
        &domain_key,
        OUTER_GAP,
        &windows,
        Some(&mover),
    );
    event.command = CoreCommand::DragPreview {
        window: start.token.clone(),
        x,
        y,
        hover_prior: carried,
        source: None,
    };
    // Correlation/domain/revision/mover fence on the reply, all START
    // bound: a stale reply after Finish, drift, or a mover swap never
    // renders.
    match state.engine.handle(&event) {
        CoreReply::DragPreview(plan) => {
            let preview = &plan.preview;
            if preview.domain != domain_key
                || preview.revision != start.revision
                || preview.source_window.0 != start.token
            {
                hide_preview(state, "stale");
                state.preview_bound.remove(&hwnd);
                state.preview_dead.insert(hwnd);
                return;
            }
            let rect = preview.proposed_rect;
            if rect.w <= 0 || rect.h <= 0 {
                if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                    bound.hover_prior = None;
                }
                hide_preview(state, "geometry");
                return;
            }
            // Carry the exact sticky hover prior into the next sample and
            // the final drop (the Engine owns the 32/80 semantics; this
            // only forwards the opaque value).
            let next_prior = preview.hover_prior();
            let prior_desc = preview_prior_desc(&Some(next_prior.clone()));
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = Some(next_prior);
            }
            let outcome = state.preview_overlay.show_fill_above(
                rect,
                PREVIEW_DEFAULT_ARGB,
                &start.token,
                hwnd,
            );
            let checksum = state.preview_overlay.snapshot()["dib_checksum"].clone();
            let signature = format!(
                "shown:{}:{},{},{},{}:{checksum}",
                start.token, rect.x, rect.y, rect.w, rect.h
            );
            let changed = state.preview_last.as_deref() != Some(signature.as_str());
            if changed {
                state.preview_last = Some(signature);
            }
            if changed || state.trace {
                log_json_at(
                    &state.log_path,
                    serde_json::json!({
                        "event": "drag-preview",
                        "tick": state.tick,
                        "correlation": correlation.as_str(),
                        "outcome": match outcome {
                            OverlayOutcome::Shown => "shown",
                            OverlayOutcome::Redrew => "redrew",
                            OverlayOutcome::Moved => "moved",
                            OverlayOutcome::Unchanged => "unchanged",
                            OverlayOutcome::Hidden => "hidden",
                        },
                        "window": start.token,
                        "rect": [rect.x, rect.y, rect.w, rect.h],
                        "hover_prior": prior_desc,
                    }),
                );
            }
        }
        CoreReply::Rejected { kind, .. } => {
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = None;
            }
            // Self, centre-stack, and outside refusals show no rectangle.
            hide_preview(state, preview_hide_for_refusal(kind));
        }
        CoreReply::Diverged(reason) => {
            hide_preview(state, reason.as_str());
            state.preview_bound.remove(&hwnd);
            state.preview_dead.insert(hwnd);
        }
        CoreReply::SnapshotInvalid { detail, .. } => {
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = None;
            }
            hide_preview(state, detail);
        }
        _ => {
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = None;
            }
            hide_preview(state, "refused");
        }
    }
}

/// Revalidate one write target immediately before `SetWindowPos`: fresh
/// eligibility, scope, fresh full identity compared against the cached
/// expectation and the frozen allowlist, with the process held open across
/// the write. Returns the fresh observation plus the held process (liveness
/// guard).
struct WriteTarget {
    window: ObservedWindow,
    held: HeldProcess,
}

#[allow(clippy::too_many_arguments)]
fn revalidate_target(
    expected: &ObservedWindow,
    me: &ProcessIdentity,
    fulls: &[Rect],
    tokens: &mut TokenMap,
    proof_mode: bool,
    allowlist: Option<&Vec<AllowEntry>>,
    scope: &[String],
    member_tag: Option<&str>,
    scope_hosts: &[ScopeHostChild],
    allow_maximized: bool,
) -> std::result::Result<WriteTarget, &'static str> {
    let hwnd = expected.hwnd as isize as HWND;
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    if pid == 0 || pid != expected.identity.pid {
        return Err("pid-changed");
    }
    let held = HeldProcess::open(pid).map_err(|_| "identity-changed")?;
    let ident = held.identity().map_err(|_| "identity-changed")?;
    if ident.pid != pid
        || ident.process_creation != expected.identity.process_creation
        || ident.exe_path != expected.identity.exe_path
        || ident.user_sid != expected.identity.user_sid
        || ident.session_id != expected.identity.session_id
    {
        return Err("identity-changed");
    }
    if ident.user_sid != me.user_sid || ident.session_id != me.session_id {
        return Err("identity-changed");
    }
    let rid = held.integrity().map_err(|_| "identity-changed")?;
    if !is_medium_rid(rid) {
        return Err("integrity-changed");
    }
    let fresh = observe_window(hwnd, me, fulls, tokens).map_err(|_| "ineligible")?;
    if !scope_allows(scope, &fresh.identity.exe_path) {
        return Err("scope-excluded");
    }
    // Visible lifetime gate: the live member tag must equal the stored tag,
    // so a same-process HWND reuse (same HWND/PID/creation, fresh window)
    // authorizes no geometry write and no focus actuation. Untracked windows
    // (no stored tag) never pass, only admission stamps.
    let live_tag = crate::product_hide::sys::read_member_tag(fresh.hwnd);
    let Some(stored) = member_tag else {
        return Err("identity-changed");
    };
    if !crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref()) {
        return Err("identity-changed");
    }
    // Hosted-child fence, fresh (never from the tick's observation): a newly
    // appearing hosted app authorizes no geometry write and no focus
    // actuation under another app's membership.
    if !hosted_gate_allows(
        &fresh.identity.exe_path,
        fresh.hwnd,
        fresh.identity.pid,
        scope_hosts,
    ) {
        return Err("scope-excluded");
    }
    if proof_mode {
        let entries = allowlist.ok_or("allowlist-missing")?;
        let Some(entry) = entries
            .iter()
            .find(|entry| allow_match(entry, &fresh.identity))
        else {
            return Err("allowlist-changed");
        };
        // Owned-helper gate immediately before the setter: sibling
        // exe/class/tag, no ordinary fallback.
        verify_proof_owned(fresh.hwnd, entry, me)?;
    }
    // Focus carries no geometry write, so it stays allowed onto a maximized
    // or fullscreen member (KDE `requestFocus` overlay exemption); geometry
    // never allows either. Every other skip still refuses on both paths.
    if allow_maximized {
        crate::tiling::classify_focus(&fresh.facts).map_err(|reason| reason.as_str())?;
    } else {
        classify(&fresh.facts).map_err(|reason| reason.as_str())?;
    }
    if fresh.token != expected.token {
        return Err("identity-changed");
    }
    Ok(WriteTarget {
        window: fresh,
        held,
    })
}

/// Recheck the HWND's current pid after the write (recycled-HWND guard).
fn pid_current(hwnd_u64: u64, pid: u32) -> bool {
    let hwnd = hwnd_u64 as isize as HWND;
    let mut current: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut current);
    }
    current != 0 && current == pid
}

/// Fresh maximized read on one HWND (single `IsZoomed` query, no frame read).
fn is_zoomed_now(hwnd_u64: u64) -> bool {
    let hwnd = hwnd_u64 as isize as HWND;
    let zoomed = unsafe { IsZoomed(hwnd) };
    zoomed != 0
}

/// Live overlay flags for one HWND (item 20): maximized from `IsZoomed`,
/// fullscreen from the live caption bit plus the live extended frame against
/// the monitor full rects (same read shape as the foreground veto, without
/// cloak/visibility). Fullscreen is `None` when the frame is unreadable
/// (minimized/hidden/foreign): the caller falls back to the retained row
/// facts, never a fabricated value. Captioned windows are never fullscreen.
/// No window is touched: three bounded reads, no setters.
#[must_use]
pub fn live_overlay_flags(hwnd_u64: u64, fulls: &[Rect]) -> (bool, Option<bool>) {
    let hwnd = hwnd_u64 as isize as HWND;
    let maximized = unsafe { IsZoomed(hwnd) } != 0;
    unsafe {
        SetLastError(0);
    }
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let style_err = unsafe { GetLastError() };
    let style_ok = style != 0 || style_err == 0;
    if style_ok && style & WS_CAPTION != 0 {
        return (maximized, Some(false));
    }
    let mut visible_raw: RECT = unsafe { std::mem::zeroed() };
    let dwm_ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut visible_raw as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    } == 0;
    if !dwm_ok {
        return (maximized, None);
    }
    let covering = rect_from_win(visible_raw)
        .is_some_and(|visible| is_borderless_fullscreen(true, visible, fulls));
    if covering {
        // Fullscreen needs captionless evidence: a covering frame with an
        // unreadable style stays unknown, never fabricated.
        return (maximized, if style_ok { Some(true) } else { None });
    }
    (maximized, Some(false))
}

// Probe one armed wake: `(expired, lost, done)` for the routing seam.
fn restore_wake_probe(state: &TileLoop, wake: &RestoreWake, now: Instant) -> (bool, bool, bool) {
    let expired = now >= wake.until;
    let lost = state
        .member_tokens
        .get(&wake.key)
        .is_none_or(|live| *live != wake.token)
        || unsafe { IsWindow(wake.key.hwnd as isize as HWND) } == 0;
    (expired, lost, !is_zoomed_now(wake.key.hwnd))
}

/// Restore one maximized window to its normal placement without activating
/// it: the official `GetWindowPlacement`/`SetWindowPlacement` route with
/// `SW_SHOWNOACTIVATE` (restores in place, active window stays active) plus
/// `WPF_ASYNCWINDOWPLACEMENT` (the request posts to the owner thread, so the
/// loop never blocks on a hung window). Single bounded call pair, no wait, no
/// retry. Returns dispatch vs observed completion honestly: `restored` only
/// when the window no longer reads maximized, `dispatched` when the setter
/// accepted but the async completion is still pending (converges next tick),
/// `threw` when nothing was dispatched.
fn restore_zoom_placement(hwnd_u64: u64) -> &'static str {
    let hwnd = hwnd_u64 as isize as HWND;
    // Immediate pre-placement revalidation: a window that closed (or already
    // restored) between the tick observation and this write takes no write.
    if unsafe { IsWindow(hwnd) } == 0 {
        return "threw";
    }
    if !is_zoomed_now(hwnd_u64) {
        return "restored";
    }
    let mut placement: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
    placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    if unsafe { GetWindowPlacement(hwnd, &mut placement) } == 0 {
        return "threw";
    }
    if placement.showCmd != SW_SHOWMAXIMIZED as u32 {
        return if is_zoomed_now(hwnd_u64) {
            "dispatched"
        } else {
            "restored"
        };
    }
    placement.showCmd = SW_SHOWNOACTIVATE as u32;
    placement.flags |= WPF_ASYNCWINDOWPLACEMENT;
    if unsafe { SetWindowPlacement(hwnd, &placement) } == 0 {
        return "threw";
    }
    if is_zoomed_now(hwnd_u64) {
        "dispatched"
    } else {
        "restored"
    }
}

/// Toggle one verified managed window's native maximize state. Maximize uses
/// the official async route (`ShowWindowAsync`, never blocks on a hung
/// target); restore uses the nonactivating placement path above. Returns the
/// readback-settled outcome for the bounded lifecycle log: `dispatched`
/// whenever the setter accepted but native completion is still pending
/// (converges next tick; the waiter proves `IsZoomed` separately).
fn toggle_zoom_async(hwnd_u64: u64, wanted: bool) -> &'static str {
    let hwnd = hwnd_u64 as isize as HWND;
    if wanted {
        if unsafe { ShowWindowAsync(hwnd, SW_MAXIMIZE) } == 0 {
            return "threw";
        }
    } else {
        match restore_zoom_placement(hwnd_u64) {
            "restored" => return "restored",
            "dispatched" => return "dispatched",
            _ => return "threw",
        }
    }
    if is_zoomed_now(hwnd_u64) == wanted {
        if wanted { "maximized" } else { "restored" }
    } else {
        "dispatched"
    }
}

/// Style bits the project-owned fullscreen toggle clears on entry
/// (`WS_CAPTION` covers border+frame, `WS_THICKFRAME` the sizing border).
/// Only these bits are ever stored and restored; unrelated style changes the
/// application makes meanwhile are preserved.
const FULLSCREEN_STYLE_BITS: u32 = WS_CAPTION | WS_THICKFRAME;
/// Inert per-window ownership marker for the project-owned fullscreen toggle.
/// Nonzero magic only; never a pointer, never trusted across window lifetime
/// (the property dies with its window, so a recycled HWND never inherits it).
const FULLSCREEN_MARKER: usize = 0x4653_3131;
/// Ownership marker property: presence of the exact magic proves a
/// project-owned fullscreen frame with restoration metadata alongside.
const FULLSCREEN_PROP: &str = "OmniTilerFullscreen";
/// Stored changed style bits plus one (never zero when present, so absence
/// stays distinguishable from a no-op clear).
const FULLSCREEN_STYLE_PROP: &str = "OmniTilerFullscreenStyle";
/// Preexisting maximize state: 1 was normal, 2 was maximized. Absence means
/// corrupt metadata, never a default.
const FULLSCREEN_MAX_PROP: &str = "OmniTilerFullscreenMax";

fn prop_wide(name: &str) -> Vec<u16> {
    name.encode_utf16().chain([0]).collect()
}

/// Owned restoration metadata for one project-toggled fullscreen window: the
/// exact style bits cleared on entry plus whether the window was maximized
/// before (fullscreen wins over maximize; exit reasserts it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FullscreenMeta {
    changed: u32,
    was_maximized: bool,
}

/// Read the owned fullscreen metadata. `None` means app-owned or absent:
/// a missing marker, a wrong magic, any missing companion property, or a
/// changed-bits value outside the owned mask all read as unowned, never as
/// a default preimage. Only bits in `FULLSCREEN_STYLE_BITS` are ever owned;
/// arbitrary style bits are never trusted for restoration.
fn read_fullscreen_meta(hwnd_u64: u64) -> Option<FullscreenMeta> {
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return None;
    }
    let marker = unsafe { GetPropW(hwnd, prop_wide(FULLSCREEN_PROP).as_ptr()) };
    if marker.is_null() || marker as usize != FULLSCREEN_MARKER {
        return None;
    }
    let bits = unsafe { GetPropW(hwnd, prop_wide(FULLSCREEN_STYLE_PROP).as_ptr()) };
    if bits.is_null() {
        return None;
    }
    let changed = u32::try_from((bits as usize).checked_sub(1)?).ok()?;
    if changed & !FULLSCREEN_STYLE_BITS != 0 {
        return None;
    }
    let max = unsafe { GetPropW(hwnd, prop_wide(FULLSCREEN_MAX_PROP).as_ptr()) };
    if max.is_null() {
        return None;
    }
    let was_maximized = match max as usize {
        1 => false,
        2 => true,
        _ => return None,
    };
    Some(FullscreenMeta {
        changed,
        was_maximized,
    })
}

/// Remove one owned metadata property set best-effort. Used to clean partial
/// `SetProp` writes so a failed entry never leaves a trusted-looking residue.
fn remove_fullscreen_meta_props(hwnd: HWND) {
    for name in [FULLSCREEN_PROP, FULLSCREEN_STYLE_PROP, FULLSCREEN_MAX_PROP] {
        unsafe {
            RemovePropW(hwnd, prop_wide(name).as_ptr());
        }
    }
}

/// Store the owned metadata with readback: all three properties must read
/// back exactly, or the entry fails closed with no partial residue trusted.
/// A partial `SetProp` sequence cleans up before returning false, so a later
/// press never mistakes residue for an owned preimage.
fn write_fullscreen_meta(hwnd_u64: u64, meta: &FullscreenMeta) -> bool {
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return false;
    }
    let stored_bits = (meta.changed as usize).wrapping_add(1);
    let stored_max = if meta.was_maximized { 2usize } else { 1usize };
    if unsafe {
        SetPropW(
            hwnd,
            prop_wide(FULLSCREEN_PROP).as_ptr(),
            FULLSCREEN_MARKER as _,
        )
    } == 0
    {
        return false;
    }
    if unsafe {
        SetPropW(
            hwnd,
            prop_wide(FULLSCREEN_STYLE_PROP).as_ptr(),
            stored_bits as _,
        )
    } == 0
    {
        remove_fullscreen_meta_props(hwnd);
        return false;
    }
    if unsafe {
        SetPropW(
            hwnd,
            prop_wide(FULLSCREEN_MAX_PROP).as_ptr(),
            stored_max as _,
        )
    } == 0
    {
        remove_fullscreen_meta_props(hwnd);
        return false;
    }
    if read_fullscreen_meta(hwnd_u64).is_some_and(|back| back == *meta) {
        true
    } else {
        remove_fullscreen_meta_props(hwnd);
        false
    }
}

/// Remove the owned metadata after a verified restore. Best-effort removal
/// with an absence readback; a failed removal keeps the marker so a later
/// exit still finds its preimage rather than stranding the frame.
fn clear_fullscreen_meta(hwnd_u64: u64) -> bool {
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return false;
    }
    for name in [FULLSCREEN_PROP, FULLSCREEN_STYLE_PROP, FULLSCREEN_MAX_PROP] {
        unsafe {
            RemovePropW(hwnd, prop_wide(name).as_ptr());
        }
    }
    read_fullscreen_meta(hwnd_u64).is_none()
}

/// Enter project-owned fullscreen: store the restoration preimage first, then
/// clear the frame bits and cover the monitor without activating. No blind
/// input synthesis; the official style/frame APIs only. Re-entry never
/// overwrites an existing preimage: an owned frame always exits first, so an
/// entry that finds metadata returns without a write. Returns the
/// readback-settled outcome: `fullscreen` when the frame bits read back
/// cleared, `dispatched` when a setter accepted but completion is pending or
/// uncertain (including a `SetWindowPos` failure after the chrome changed,
/// where the preimage is preserved for a later exit), `threw` when no style
/// effect happened (the fresh preimage is cleared, nothing is left trusted).
/// No automatic retries on any path.
fn enter_fullscreen(hwnd_u64: u64, full: Rect) -> &'static str {
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return "threw";
    }
    if read_fullscreen_meta(hwnd_u64).is_some() {
        return "dispatched";
    }
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let meta = FullscreenMeta {
        changed: style & FULLSCREEN_STYLE_BITS,
        was_maximized: is_zoomed_now(hwnd_u64),
    };
    if !write_fullscreen_meta(hwnd_u64, &meta) {
        return "threw";
    }
    let next = (style & !FULLSCREEN_STYLE_BITS) as i32;
    unsafe {
        SetLastError(0);
    }
    let prev = unsafe { SetWindowLongW(hwnd, GWL_STYLE, next) };
    let style_err = unsafe { GetLastError() };
    if prev == 0 && style_err != 0 {
        clear_fullscreen_meta(hwnd_u64);
        return "threw";
    }
    let placed = unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            full.x,
            full.y,
            full.w,
            full.h,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_FRAMECHANGED,
        )
    };
    if placed == 0 {
        return "dispatched";
    }
    if unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32 & FULLSCREEN_STYLE_BITS == 0 {
        "fullscreen"
    } else {
        "dispatched"
    }
}

/// Exit project-owned fullscreen: restore exactly the owned mask bits
/// (unrelated application style changes meanwhile are preserved), reapply
/// the frame without moving (so externally moved frames still exit), reassert
/// a preexisting maximize, then drop the metadata. A failed restore keeps
/// the metadata so a later press can retry; nothing is guessed and nothing
/// retries automatically. `meta.changed` is already validated to the owned
/// mask on read, so restoration never touches arbitrary bits.
fn exit_fullscreen_owned(hwnd_u64: u64, meta: &FullscreenMeta) -> &'static str {
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return "threw";
    }
    let current = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let next = (current | meta.changed) as i32;
    unsafe {
        SetLastError(0);
    }
    let prev = unsafe { SetWindowLongW(hwnd, GWL_STYLE, next) };
    let style_err = unsafe { GetLastError() };
    if prev == 0 && style_err != 0 {
        return "threw";
    }
    let placed = unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
    };
    if placed == 0 {
        return "threw";
    }
    if unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32 & meta.changed != meta.changed {
        return "dispatched";
    }
    if meta.was_maximized && unsafe { ShowWindowAsync(hwnd, SW_MAXIMIZE) } == 0 {
        return "dispatched";
    }
    if clear_fullscreen_meta(hwnd_u64) {
        "restored"
    } else {
        "dispatched"
    }
}

/// Fresh holdability check for one fullscreen HWND: same-session,
/// medium-integrity identity plus every safety gate observation admission
/// requires (never elevated, shell, tool, owned, dialog including the own
/// Settings UI, cloaked, or no-activate; in scope with a live hosted child where listed; frozen
/// allowlist plus owned-helper verification in proof modes). Returns the
/// member key on pass; the caller decides hold vs exemption. Read-only.
fn holdable_key(
    state: &TileLoop,
    me: &ProcessIdentity,
    hwnd_u64: u64,
) -> Option<crate::workspace::WindowKey> {
    if hwnd_u64 == 0 {
        return None;
    }
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return None;
    }
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    if pid == 0 || pid == me.pid {
        return None;
    }
    let held = HeldProcess::open(pid).ok()?;
    let live = held.identity().ok()?;
    if live.pid != pid {
        return None;
    }
    if live.user_sid != me.user_sid || live.session_id != me.session_id {
        return None;
    }
    if held.integrity().ok().is_none_or(|rid| !is_medium_rid(rid)) {
        return None;
    }
    if unsafe { IsIconic(hwnd) } != 0 {
        return None;
    }
    let class = class_of(hwnd);
    if is_shell_class(&class)
        || class == DIALOG_CLASS
        || crate::tiling::is_own_settings_window(&class, &live.exe_path, &me.exe_path)
    {
        return None;
    }
    if !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null() {
        return None;
    }
    let exstyle = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    if exstyle & (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE) != 0 {
        return None;
    }
    let mut cloaked: i32 = 0;
    if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut i32).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    } != 0
    {
        return None;
    }
    if cloaked != 0 {
        return None;
    }
    if !scope_allows(&state.scope, &live.exe_path) {
        return None;
    }
    if !hosted_gate_allows(&live.exe_path, hwnd_u64, pid, &state.scope_hosts) {
        return None;
    }
    if let Some(entries) = state.allowlist.as_ref() {
        let entry = entries.iter().find(|e| e.hwnd == hwnd_u64)?;
        if verify_proof_owned(hwnd_u64, entry, me).is_err() {
            return None;
        }
    }
    Some(crate::workspace::WindowKey {
        hwnd: hwnd_u64,
        pid,
        creation: live.process_creation,
    })
}

/// Track the born-fullscreen hold after one observation (KDE
/// initial-fullscreen-hold parity): lifetime-known non-fullscreen identities
/// accumulate here so a later fullscreen transition is a managed overlay,
/// never a born hold again; held keys observed non-fullscreen release for
/// normal fresh admission; hold and seen sets prune with the enumeration.
/// New holds are admitted in `ensure_workspace_assignments` (which owns
/// workspace membership, lifetime tags, and the held lifecycle log), never
/// here, so tracking never creates a non-member hold. Every observation path
/// funnels through `observe`, so one call here covers reconcile,
/// directional, workspace, gesture, and select ticks.
fn track_fullscreen_holds(
    state: &mut TileLoop,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) {
    let mut live_nonfullscreen: BTreeSet<crate::workspace::WindowKey> = BTreeSet::new();
    for window in observed {
        live_nonfullscreen.insert(crate::workspace::WindowKey {
            hwnd: window.hwnd,
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
        });
    }
    for row in retained {
        if !row.fullscreen {
            live_nonfullscreen.insert(row.key.clone());
        }
    }
    // Lifetime-known non-fullscreen (KDE `seenNonFullscreen` parity): any
    // non-fullscreen observation, including minimized members, means a later
    // fullscreen transition is a managed overlay, never a born hold again.
    // Windows that could never verify holdable (scope, proof, safety gates)
    // suspend either way, so this never weakens a fence.
    state
        .seen_nonfullscreen
        .extend(live_nonfullscreen.iter().cloned());
    let log_path = state.log_path.clone();
    let released: Vec<crate::workspace::WindowKey> = state
        .born_fullscreen
        .iter()
        .filter(|key| live_nonfullscreen.contains(key) || !state.last_hwnds.contains(&key.hwnd))
        .cloned()
        .collect();
    for key in released {
        state.born_fullscreen.remove(&key);
        // Drop the hidden-snapshot seed so the first exit converges through
        // normal fresh admission (and the existing maximize clear when the
        // exit lands maximized) with no faked slot. The log below is the
        // once-per-lifecycle release line; held was logged at admission.
        if let Some(token) = state.member_tokens.get(&key).cloned() {
            state.member_rects.remove(&token);
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "initial-fullscreen-released",
                    "window": token,
                }),
            );
        } else if let Some(token) = retained
            .iter()
            .find(|r| r.key == key)
            .map(|r| r.token.clone())
        {
            state.member_rects.remove(&token);
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "initial-fullscreen-released",
                    "window": token,
                }),
            );
        }
    }
    state
        .seen_nonfullscreen
        .retain(|key| state.last_hwnds.contains(&key.hwnd));
}

/// Otherwise-eligible check for an admission-clear candidate: every safety
/// gate must already pass, so the clear never mutates an elevated, shell,
/// tool, owned, dialog, cloaked, or no-activate window. Maximized is the only
/// permitted overlay state here (fullscreen never clears, checked separately).
fn admission_clear_eligible(facts: WindowFacts) -> bool {
    facts.visible
        && !facts.minimized
        && !facts.cloaked
        && !facts.elevated
        && !facts.shell
        && !facts.tool_window
        && !facts.owned
        && !facts.dialog
        && !facts.no_activate
}

/// One-shot maximize clear at admission (KDE `clearMaximizeAtAdmission`
/// parity). Call after `ensure_workspace_assignments` (lifetime tags stamped)
/// and before the first Engine admission of the tick. For every retained
/// maximized, non-fullscreen row that is otherwise eligible, scoped,
/// hosted-gated, and proof-verified with fresh identity, and that holds no
/// retained tile slot (`member_rects`), restore the native maximize exactly
/// once with no automatic retry. Fullscreen never clears. The cleared window
/// converges as eligible on the next tick.
///
/// R-MAX-06: the keep-overlay policy binds to the exact fresh lifetime at
/// tiled-first slotless admission (`born_keep_overlay`, set only after every
/// ownership fence in `admit_slotless_maximized`). A latched row skips the
/// clear and converges through the existing retained-slot overlay machinery
/// instead (slotless admission seeds the slot on the tiled domain, row
/// assembly rides the canonical retained rect, and the retained-only token
/// takes no geometry write until a native restore lands it in the slot).
/// Floating-first (R-MAX-03), pre-existing, born-fullscreen-exit, and later
/// maxima rows never latch, so they keep the existing one-shot clear (or no
/// clear) under both setting values; a live flip applies only to subsequent
/// first admissions and never moves an already-bound lifetime.
///
/// Attempts key on HWND plus process creation plus the live lifetime tag, so
/// a recycled HWND re-arms for the fresh window while the same window never
/// retries. Stale attempts prune when the HWND leaves the enumeration or the
/// live occupant's pid/creation moved on.
fn clear_maximize_at_admission(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    retained: &[RetainedRow],
) {
    let log_path = state.log_path.clone();
    for row in retained {
        if row.fullscreen || !row.maximized {
            continue;
        }
        // Floating workspaces preserve the native maximum until the first
        // tiled admission (KDE floating-gate parity). Unassigned rows fail
        // closed: `ensure_workspace_assignments` owns membership and runs
        // before this clear on every path.
        let domain_tiled = state
            .workspaces
            .member_loc(&row.key)
            .and_then(|loc| workspace_mode_known(state, &loc.output, &loc.workspace));
        let tiled = matches!(domain_tiled, Some(true));
        if !row.facts.is_some_and(admission_clear_eligible) {
            continue;
        }
        // Truly no retained tile slot: a slotted member keeps its allocation
        // and never re-clears. Workspace membership alone is not a slot.
        if state.member_rects.contains_key(&row.token) {
            continue;
        }
        // Fresh identity: the live window must still be the retained one with
        // a same-session, medium-integrity identity. Scope, hosted-child, and
        // proof gates match observation admission exactly.
        let hwnd = row.key.hwnd as isize as HWND;
        let mut live_pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut live_pid);
        }
        if live_pid == 0 || live_pid != row.key.pid {
            continue;
        }
        let live = match HeldProcess::open(live_pid).and_then(|held| {
            let ident = held.identity()?;
            let rid = held.integrity()?;
            Ok((ident, rid))
        }) {
            Ok((ident, rid)) => {
                if ident.pid != live_pid
                    || ident.process_creation != row.key.creation
                    || ident.user_sid != me.user_sid
                    || ident.session_id != me.session_id
                    || !is_medium_rid(rid)
                {
                    continue;
                }
                ident
            }
            Err(_) => continue,
        };
        if !scope_allows(&state.scope, &live.exe_path) {
            continue;
        }
        if !hosted_gate_allows(&live.exe_path, row.key.hwnd, live_pid, &state.scope_hosts) {
            continue;
        }
        if let Some(entries) = state.allowlist.as_ref() {
            let Some(entry) = entries.iter().find(|e| e.hwnd == row.key.hwnd) else {
                continue;
            };
            if verify_proof_owned(row.key.hwnd, entry, me).is_err() {
                continue;
            }
        }
        let live_tag = crate::product_hide::sys::read_member_tag(row.key.hwnd);
        let attempt_key = format!(
            "{}:{}:{}:{}",
            row.key.hwnd,
            row.key.creation,
            live_pid,
            live_tag.as_deref().unwrap_or("")
        );
        if !crate::tiling::should_clear_maximize_at_admission_with_mode(
            row.fullscreen,
            row.maximized,
            false,
            state.maximize_admission_attempted.contains(&attempt_key),
            tiled,
            state.born_keep_overlay.contains(&row.key),
        ) {
            continue;
        }
        // Mark before the native call so a failed write never retries.
        state.maximize_admission_attempted.insert(attempt_key);
        let outcome = restore_zoom_placement(row.key.hwnd);
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "maximize-admission-clear",
                "window": row.token,
                "outcome": outcome,
            }),
        );
    }
    // Prune attempts whose window left the enumeration or whose HWND now hosts
    // a different pid/creation: the set stays bounded without ever re-arming
    // a live window.
    state.maximize_admission_attempted.retain(|key| {
        let mut parts = key.splitn(4, ':');
        let (Some(hwnd), Some(creation), Some(pid)) = (parts.next(), parts.next(), parts.next())
        else {
            return false;
        };
        let Ok(hwnd_u64) = hwnd.parse::<u64>() else {
            return false;
        };
        if !state.last_hwnds.contains(&hwnd_u64) {
            return false;
        }
        let Ok(pid_u32) = pid.parse::<u32>() else {
            return false;
        };
        let hwnd_ptr = hwnd_u64 as isize as HWND;
        if unsafe { IsWindow(hwnd_ptr) } == 0 {
            return false;
        }
        let mut live_pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd_ptr, &mut live_pid);
        }
        live_pid == pid_u32 && {
            let key = crate::workspace::WindowKey {
                hwnd: hwnd_u64,
                pid: pid_u32,
                creation: creation.to_owned(),
            };
            // A same-process reuse drops membership tables at assignment; a
            // pruned attempt re-arms the fresh window below.
            state.member_tags.contains_key(&key) || {
                HeldProcess::open(live_pid)
                    .and_then(|held| held.identity())
                    .is_ok_and(|ident| ident.process_creation == creation)
            }
        }
    });
}

/// Overlay refusal for one verified member before any Engine mutation (KDE
/// move/pointer isolation parity): fullscreen wins, then maximize. Returns
/// the refusal cause or `None` when the route may proceed.
fn overlay_refusal_for(row: &RetainedRow) -> Option<&'static str> {
    crate::tiling::overlay_refusal(row.fullscreen, row.maximized)
}

/// Whether one Engine token is currently an intentional-float exception in
/// its domain session (floated windows hold no tile slot). Born-fullscreen
/// holds are exceptions too; the intentional lane additionally keys off the
/// `float_topmost_prev` preimage map. `false` when the session is unknown.
fn engine_is_float(state: &TileLoop, output: &str, workspace: &str, token: &str) -> bool {
    let key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(workspace.to_owned()),
    };
    state
        .engine
        .session(&key)
        .is_some_and(|session| session.is_exception(&WindowId(token.to_owned())))
}

/// Session workspace-mode gate: tiled workspaces run every Engine/geometry
/// path; floating workspaces leave frames untouched and stop all tiled
/// geometry, group-underlay, and drop-preview operations for that domain
/// while the independent active border still marks focus. Intentional
/// per-window float/sticky and native maximize/fullscreen overlays keep
/// their own semantics on either mode.
#[must_use]
fn workspace_mode_tiled(state: &TileLoop, output: &str, workspace: &str) -> bool {
    state.workspaces.is_tiled(output, workspace)
}

/// Fresh `WS_EX_TOPMOST` read on one HWND (official Win32 topmost band).
fn read_topmost_now(hwnd_u64: u64) -> bool {
    let hwnd = hwnd_u64 as isize as HWND;
    (unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) }) as u32 & WS_EX_TOPMOST != 0
}

/// Set one window's topmost band without moving, sizing, or activating it.
/// Returns whether the setter accepted; the caller re-reads for proof.
fn set_topmost_band(hwnd_u64: u64, top: bool) -> bool {
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return false;
    }
    let after: HWND = if top { HWND_TOPMOST } else { HWND_NOTOPMOST };
    let placed = unsafe {
        SetWindowPos(
            hwnd,
            after,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
    placed != 0
}

/// Held ownership gates for one classification-marker native write (`SetProp` /
/// `RemoveProp` for sticky, float-intent, or the reserved tile override) on
/// a stored member: the held process identity must equal the stored full
/// identity with matching creation, same user/session as the owner, medium
/// integrity, live lifetime tag, plus scope, hosted-child, and proof fences.
/// The caller keeps the returned hold across the write and its readback and
/// rechecks pid plus lifetime tag immediately before the call (a stable
/// process handle never protects HWND reuse) with a consistent readback
/// after.
fn hold_marker_target(
    state: &TileLoop,
    me: &ProcessIdentity,
    key: &crate::workspace::WindowKey,
) -> Option<HeldProcess> {
    let stored = state.member_identity.get(key)?;
    let held = HeldProcess::open(key.pid).ok()?;
    let ident = held.identity().ok()?;
    if ident.pid != key.pid
        || ident.process_creation != key.creation
        || stored.pid != key.pid
        || stored.process_creation != key.creation
        || ident.user_sid != stored.user_sid
        || ident.session_id != stored.session_id
    {
        return None;
    }
    if !crate::lifecycle::exe_paths_equal(&ident.exe_path, &stored.exe_path) {
        return None;
    }
    if ident.user_sid != me.user_sid || ident.session_id != me.session_id {
        return None;
    }
    if !held.integrity().is_ok_and(is_medium_rid) {
        return None;
    }
    if !scope_allows(&state.scope, &stored.exe_path) {
        return None;
    }
    if !hosted_gate_allows(&stored.exe_path, key.hwnd, key.pid, &state.scope_hosts) {
        return None;
    }
    if let Some(entries) = state.allowlist.as_ref() {
        let entry = entries.iter().find(|e| e.hwnd == key.hwnd)?;
        if verify_proof_owned(key.hwnd, entry, me).is_err() {
            return None;
        }
    }
    let live_tag = crate::product_hide::sys::read_member_tag(key.hwnd);
    let stored_tag = state.member_tags.get(key)?;
    if !crate::workspace_owner::visible_lifetime_ok(stored_tag, live_tag.as_deref()) {
        return None;
    }
    if !pid_current(key.hwnd, key.pid) {
        return None;
    }
    Some(held)
}

/// Held ownership gates for a band-only topmost effect on a stored member
/// with no live eligible observation (retained floats, graceful stop).
/// Same bar as [`hold_marker_target`]: the caller keeps the returned hold
/// across the effect and rechecks pid plus band readback after.
/// Geometry effects use `revalidate_target` (fresh eligibility classification);
/// band effects move, size, and activate nothing.
fn hold_band_target(
    state: &TileLoop,
    me: &ProcessIdentity,
    key: &crate::workspace::WindowKey,
) -> Option<HeldProcess> {
    hold_marker_target(state, me, key)
}

/// Fresh pid plus lifetime-tag recheck for one classification-marker write:
/// the live member tag must still equal the stored tag and the HWND must
/// still resolve to the stored pid. Call immediately before the `SetProp` /
/// `RemoveProp` under the held process and again after the readback.
fn marker_tag_pid_fresh(state: &TileLoop, key: &crate::workspace::WindowKey) -> bool {
    state.member_tags.get(key).is_some_and(|stored| {
        crate::workspace_owner::visible_lifetime_ok(
            stored,
            crate::product_hide::sys::read_member_tag(key.hwnd).as_deref(),
        )
    }) && pid_current(key.hwnd, key.pid)
}

/// Install one sticky marker under the fresh held gate with the guard kept
/// across the write and its readback. Returns true only when the install
/// reads back consistently and the pid plus lifetime tag stay fresh after.
fn install_sticky_mark_held(
    state: &TileLoop,
    me: &ProcessIdentity,
    key: &crate::workspace::WindowKey,
    prior_floating: bool,
) -> bool {
    let Some(held) = hold_marker_target(state, me, key) else {
        return false;
    };
    if !marker_tag_pid_fresh(state, key) {
        let _ = &held;
        return false;
    }
    let ok = crate::product_hide::sys::install_sticky_marker(key.hwnd, prior_floating).is_ok();
    let fresh = marker_tag_pid_fresh(state, key);
    let _ = &held;
    ok && fresh
}

/// Install one intentional-float marker under the fresh held gate with the
/// guard kept across the write and its readback. Only call after the Engine
/// commit plus verified native effects hold: success-only persistence keeps
/// local intent and native state on failure with no rollback. Returns true
/// only when the install reads back consistently and the pid plus lifetime
/// tag stay fresh after.
fn install_float_mark_held(
    state: &TileLoop,
    me: &ProcessIdentity,
    key: &crate::workspace::WindowKey,
) -> bool {
    let Some(held) = hold_marker_target(state, me, key) else {
        return false;
    };
    if !marker_tag_pid_fresh(state, key) {
        let _ = &held;
        return false;
    }
    let ok = crate::product_hide::sys::install_float_intent_marker(key.hwnd).is_ok();
    let fresh = marker_tag_pid_fresh(state, key);
    let _ = &held;
    ok && fresh
}

/// Drop float and sticky runtime state whose membership is gone. No writes,
/// no ledger; the next tick re-derives float rows from the Engine.
/// Classification markers (window properties) are never pruned here: only
/// the runtime maps. A surviving marker re-adopts on the next preamble.
fn prune_float_state(state: &mut TileLoop) {
    state
        .float_topmost_prev
        .retain(|key, _| state.member_tokens.contains_key(key));
    state
        .float_rects
        .retain(|token, _| state.member_tokens.values().any(|live| live == token));
    state
        .floated
        .retain(|key| state.member_tokens.contains_key(key));
    state
        .sticky
        .retain(|key, _| state.member_tokens.contains_key(key));
}

/// Drop every runtime table for one dead member: tokens, rects, bands, sticky,
/// identity, tags, hints, and workspace membership. No writes, no ledger.
/// Classification markers (window properties) die with the window and are
/// never pruned here: only the runtime maps.
fn drop_member_state(state: &mut TileLoop, key: &crate::workspace::WindowKey) {
    if let Some(token) = state.member_tokens.remove(key) {
        state.member_rects.remove(&token);
        state.float_rects.remove(&token);
        state.hint_logged.remove(&token);
    }
    if state.restore_wake.as_ref().is_some_and(|w| w.key == *key) {
        state.restore_wake = None;
    }
    state.born_keep_overlay.remove(key);
    state.float_topmost_prev.remove(key);
    state.floated.remove(key);
    state.sticky.remove(key);
    state.member_identity.remove(key);
    state.member_tags.remove(key);
    state.workspaces.remove_window(key);
}

/// One exact-window foreground check plus a single setter with readback.
/// Already-foreground takes no setter. Token-safe outcome only.
fn retain_float_focus(hwnd_u64: u64) -> &'static str {
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    if foreground == hwnd_u64 {
        return "float-focus-retained";
    }
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { SetForegroundWindow(hwnd) } == 0 {
        return "float-focus-failed";
    }
    if unsafe { GetForegroundWindow() } as usize as u64 == hwnd_u64 {
        "float-focus-retained"
    } else {
        "float-focus-failed"
    }
}

/// Validated float focus retention for the exact toggled window: fresh
/// revalidation holds the target across the prior band calls, then at most
/// one setter with readback. Stale targets fail closed; failures report
/// without claiming success. No timers or retries.
fn retain_float_focus_validated(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    expected: &ObservedWindow,
    member_key: &crate::workspace::WindowKey,
) -> &'static str {
    let member_tag = state.member_tags.get(member_key).map(String::as_str);
    let target = match revalidate_target(
        expected,
        me,
        fulls,
        &mut state.tokens,
        state.allowlist.is_some(),
        state.allowlist.as_ref(),
        &state.scope,
        member_tag,
        &state.scope_hosts,
        false,
    ) {
        Ok(target) => target,
        Err(_) => return "float-focus-failed",
    };
    let _ = &target.held;
    if !pid_current(target.window.hwnd, target.window.identity.pid) {
        return "float-focus-failed";
    }
    retain_float_focus(target.window.hwnd)
}

type DesiredEntry = crate::workspace_owner::PlannedWrite;

fn desired_entries(reply: &CoreReply) -> Option<Vec<DesiredEntry>> {
    crate::workspace_owner::planned_writes(reply)
}

/// Assemble complete Engine rows for one `(output, workspace)` domain: the
/// union of eligible visible members, retained-occupancy members with fresh
/// frames, frame-less retained members with last-known snapshots, and hidden
/// snapshots. Refreshes the member token/rect/identity tables for every row
/// with a fresh read.
///
/// Eligible visible members carry a fresh application-declared minimum-size
/// hint: one bounded `WM_GETMINMAXINFO` query per member converted with its
/// currently measured frame insets. Verified managed hidden members carry a
/// fresh hint the same way (fresh identity fences plus fresh `GetWindowRect`
/// and `DWMWA_EXTENDED_FRAME_BOUNDS` for current insets, never shown or
/// moved); a failed hidden measurement is unknown, never a stale inset.
/// Retained tiled maximized/fullscreen members ride their last-known declared
/// hint (lifetime-bound by Engine token, never derived from the maximized
/// frame); minimized without a frame, cloaked, float, and born-hold rows
/// carry no hint. `hint_cx` shares one
/// aggregate deadline across every domain in
/// the operation, so a send's source plus target never independently blow
/// the per-operation budget; remaining members report unknown with no hint.
/// Every Engine-building path funnels through here, so reconcile, directional
/// actions, workspace send/select, and post-admission ticks all carry hints;
/// hint-only changes still reach projection because no caller skips the
/// Engine on an unchanged rectangle fingerprint.
///
/// Returns `None` when any member lacks a known snapshot: every Engine
/// handle path must defer with retained state instead of converging a
/// falsely complete observation that would drop membership and layout.
#[allow(clippy::too_many_arguments)]
fn assemble_domain_rows(
    state: &mut TileLoop,
    output: &str,
    workspace: &str,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    op: &str,
    correlation: &str,
    hint_cx: &mut HintCx,
) -> Option<Vec<crate::workspace_owner::OwnerRow>> {
    let members = state.workspaces.workspace_members(output, workspace);
    let by_token: HashMap<&str, &ObservedWindow> =
        observed.iter().map(|w| (w.token.as_str(), w)).collect();
    let mut views = Vec::new();
    let query_start = Instant::now();
    let mut stats = HintStats::default();
    let mut reasons: HashMap<String, &'static str> = HashMap::new();
    // Intentional-float tokens in this domain session: slotless floating
    // rows on the live native frame, no tiled writes, `member_rects` frozen
    // while `float_rects` tracks the draggable frame.
    let domain_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(workspace.to_owned()),
    };
    let float_tokens: HashSet<String> = state
        .engine
        .session(&domain_key)
        .map(|session| {
            members
                .iter()
                .filter_map(|key| state.member_tokens.get(key))
                .filter(|token| session.is_exception(&WindowId(token.to_string())))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    // Runtime intent carries the exception across domain releases (which drop
    // every Engine exception) and boundary-send fresh adoption: a released
    // then retiled workspace re-adopts its intentional floats on the next
    // observation instead of tiling them. Lifetime keyed, pruned with
    // membership; sticky/born-fullscreen keep their own lanes.
    let mut float_tokens = float_tokens;
    for token in
        crate::workspace_owner::float_carry_tokens(&members, &state.member_tokens, &state.floated)
    {
        float_tokens.insert(token);
    }
    // A slotless maximized row on a floating domain keeps no tile slot, so
    // the deferred admission clear still fires on retile. Unknown domains
    // seed as today (assembly implies a live domain).
    let domain_tiled = workspace_mode_known(state, output, workspace).unwrap_or(true);
    for key in &members {
        let born = state.born_fullscreen.contains(key);
        if state.workspaces.is_hidden(key) {
            let token = state.member_tokens.get(key).cloned();
            let rect = token
                .as_ref()
                .and_then(|t| {
                    crate::workspace_owner::hidden_snapshot_rect(
                        float_tokens.contains(t),
                        state.float_rects.get(t).copied(),
                        state.member_rects.get(t).copied(),
                    )
                })
                // Slotless members carry no snapshot yet: fall back to the
                // fresh retained frame so the observation stays complete
                // without seeding a tile slot. No insert here, ever.
                .or_else(|| retained.iter().find(|r| r.key == *key).and_then(|r| r.rect));
            if let (Some(token), Some(rect)) = (token, rect) {
                let is_float = float_tokens.contains(&token);
                let (hints, outcome) = hidden_hint_for(state, key, hint_cx);
                stats.note(outcome);
                reasons.insert(token.clone(), outcome.as_str());
                views.push(crate::workspace_owner::MemberView {
                    key: key.clone(),
                    token,
                    rect,
                    hints,
                    floating: born || is_float,
                });
            }
            continue;
        }
        // Eligible visible read first. Out-of-scope members still ride a row
        // so the observation stays complete: the scope fence lives in
        // `writable_tokens` (no geometry writes), `workspace_hide_one`, and
        // the select/send dispatchers (no hides), never in row assembly
        // (where a missing view would stall convergence as `deferred`).
        if let Some(token) = state.member_tokens.get(key).cloned()
            && let Some(window) = by_token.get(token.as_str())
        {
            // Slotless floating row on the live native frame, no hint. The
            // tiled allocation in `member_rects` stays frozen. Engine-known
            // floats, runtime-intent floats, and sticky members ride here;
            // adoption commits the Engine exception inline.
            if float_tokens.contains(&token) || state.sticky.contains_key(key) {
                state.float_rects.insert(token.clone(), window.visible);
                views.push(crate::workspace_owner::MemberView {
                    key: key.clone(),
                    token: token.clone(),
                    rect: window.visible,
                    hints: WindowSizeHints::none(),
                    floating: true,
                });
                continue;
            }
            state.member_rects.insert(token.clone(), window.visible);
            if let Some(identity) = state.member_identity.get_mut(key) {
                identity.pid = window.identity.pid;
                identity.process_creation = window.identity.process_creation.clone();
                identity.exe_path = window.identity.exe_path.clone();
                identity.user_sid = window.identity.user_sid.clone();
                identity.session_id = window.identity.session_id;
            }
            let (hints, outcome) = if hint_cx.over_budget() {
                (WindowSizeHints::none(), HintOutcome::Budget)
            } else {
                min_hint_for(window, hint_cx)
            };
            stats.note(outcome);
            reasons.insert(token.clone(), outcome.as_str());
            views.push(crate::workspace_owner::MemberView {
                key: key.clone(),
                token: token.clone(),
                rect: window.visible,
                hints,
                floating: false,
            });
            continue;
        }
        // Retained occupancy: an overlaid (maximized/fullscreen) member rides
        // its last-known tile rectangle, never the compositor-owned native
        // maximum frame, so tile topology and sibling shares survive the
        // overlay; first sightings fall back to the fresh frame. Retained rows
        // carry no hint and never take writes. Born-held fullscreen members
        // are the slotless exception: they ride the fresh row rect with a
        // floating Engine observation (siblings keep the full tile area) and
        // never write a canonical slot into `member_rects`; the admission
        // hidden-snapshot seed stays untouched for a later hide. Existing
        // skip semantics preserved.
        if let Some(row) = retained.iter().find(|r| r.key == *key) {
            if born && row.fullscreen {
                if let Some(rect) = row.rect {
                    views.push(crate::workspace_owner::MemberView {
                        key: key.clone(),
                        token: row.token.clone(),
                        rect,
                        hints: WindowSizeHints::none(),
                        floating: true,
                    });
                }
                continue;
            }
            if float_tokens.contains(&row.token) {
                // Overlaid float (user maximized/fullscreened it natively) or
                // frameless: slotless floating row on the fresh frame, else
                // the last float snapshot, with no hint. Unfloat refuses
                // until the frame reads normal again.
                let snapshot = state.float_rects.get(&row.token).copied();
                let kept = state.member_rects.get(&row.token).copied();
                let rect = match row.rect {
                    Some(fresh) => Some(crate::tiling::canonical_retained_rect(
                        true,
                        fresh,
                        snapshot.or(kept),
                    )),
                    None => snapshot.or(kept),
                };
                if let Some(rect) = rect {
                    state.float_rects.insert(row.token.clone(), rect);
                    views.push(crate::workspace_owner::MemberView {
                        key: key.clone(),
                        token: row.token.clone(),
                        rect,
                        hints: WindowSizeHints::none(),
                        floating: true,
                    });
                }
                continue;
            }
            let overlay = row.maximized || row.fullscreen;
            let kept = state.member_rects.get(&row.token).copied();
            let rect = match row.rect {
                Some(fresh) => Some(crate::tiling::canonical_retained_rect(overlay, fresh, kept)),
                None => kept,
            };
            if let Some(rect) = rect {
                // Slotless maxima on floating domains stay slotless (flag
                // above); every other row seeds or refreshes as before.
                if crate::tiling::should_seed_member_slot(
                    domain_tiled,
                    row.maximized,
                    state.member_rects.contains_key(&row.token),
                ) {
                    state.member_rects.insert(row.token.clone(), rect);
                }
                // Retained tiled overlay keeps its last-known declared hint
                // so min-bound siblings stay stable; minimized/frameless,
                // cloaked, float, and born-hold rows stay hintless.
                let token_matches = state
                    .member_tokens
                    .get(key)
                    .is_some_and(|live| live == &row.token);
                let hints = crate::tiling::retained_overlay_hint(
                    state.hint_logged.get(&row.token).copied(),
                    token_matches,
                    overlay,
                    float_tokens.contains(&row.token),
                    born,
                    true,
                );
                reasons.insert(
                    row.token.clone(),
                    if hints.is_empty() {
                        "retained"
                    } else {
                        "retained-hint"
                    },
                );
                views.push(crate::workspace_owner::MemberView {
                    key: key.clone(),
                    token: row.token.clone(),
                    rect,
                    hints,
                    floating: false,
                });
            }
        }
    }
    let mut rows = crate::workspace_owner::domain_rows(&members, &views)?;
    rows.sort_by(|a, b| a.token.cmp(&b.token));
    let elapsed = query_start.elapsed();
    log_min_hint_summary(
        state,
        op,
        correlation,
        output,
        workspace,
        &rows,
        &stats,
        &reasons,
        elapsed,
    );
    Some(rows)
}

/// Bounded minimum-hint observation summary for one row assembly: query
/// outcomes plus the tokens whose fresh hint differs from the last logged
/// value, threaded on the real operation correlation (`tick-N` for reconcile,
/// `act-N` for select/send/directional) with opaque output/workspace tokens
/// so the change joins to the projection adjustment and the
/// `client-clamped` skips (plus overconstrained min-clamped targets) in the
/// existing apply summaries
/// by `(correlation, output, workspace, window)`. Per-window hint values and
/// per-token query reasons ride the trace log only; the normal log carries
/// counts plus changes so steady state stays quiet. The dedupe cache is
/// pruned by actual live membership (every member token across
/// all domains), never by the current domain alone, so multi-output and send
/// domains never re-log unchanged hints. Rows feed the Engine; the cache
/// additionally feeds the overconstrained native target (`max(planned, min)`).
/// Retained overlay rows reuse the cached hint, so syncing here keeps (never
/// drops) that reused value.
#[allow(clippy::too_many_arguments)]
fn log_min_hint_summary(
    state: &mut TileLoop,
    op: &str,
    correlation: &str,
    output: &str,
    workspace: &str,
    rows: &[crate::workspace_owner::OwnerRow],
    stats: &HintStats,
    reasons: &HashMap<String, &'static str>,
    elapsed: Duration,
) {
    let tick = state.tick;
    let mut changed: Vec<String> = Vec::new();
    for row in rows {
        match state.hint_logged.get(&row.token) {
            Some(previous) if *previous == row.hints => {}
            _ => {
                if !row.hints.is_empty() || state.hint_logged.contains_key(&row.token) {
                    changed.push(row.token.clone());
                }
                if row.hints.is_empty() {
                    state.hint_logged.remove(&row.token);
                } else {
                    state.hint_logged.insert(row.token.clone(), row.hints);
                }
            }
        }
    }
    // Prune by actual live membership identity across all domains, not by the
    // current domain's rows: multi-output ticks and send source/target pairs
    // must not evict (and re-log) each other's unchanged hints.
    let live: HashSet<String> = state.member_tokens.values().cloned().collect();
    state
        .hint_logged
        .retain(|token, _| live.contains(token) || rows.iter().any(|row| row.token == *token));
    changed.sort();
    if changed.is_empty() && !state.trace {
        return;
    }
    let log_path = state.log_path.clone();
    let output_token = state.workspaces.output_token(output);
    let workspace_token = state.workspaces.workspace_token(output, workspace);
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "min-hints",
            "tick": tick,
            "correlation": correlation,
            "op": op,
            "output": output_token,
            "workspace": workspace_token,
            "queried": stats.queried,
            "with_hint": stats.with_hint,
            "unknown": stats.queried.saturating_sub(stats.with_hint).saturating_add(stats.budget_skipped),
            "timeout": stats.timeout,
            "failed": stats.failed,
            "invalid": stats.invalid,
            "budget_skipped": stats.budget_skipped,
            "query_ms": elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
            "changed": changed,
        }),
    );
    if state.trace {
        let detail: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "window": row.token,
                    "min": [row.hints.min_w, row.hints.min_h],
                    "reason": reasons.get(&row.token).copied().unwrap_or("retained"),
                })
            })
            .collect();
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "min-hints-detail",
                "tick": tick,
                "correlation": correlation,
                "op": op,
                "output": output_token,
                "workspace": workspace_token,
                "entries": detail,
            }),
        );
    }
}

/// Engine-writable tokens for one domain: eligible observed members only.
/// Retained and hidden rows converge membership but never take geometry
/// writes; hidden workspace geometry is not written until reveal.
///
/// Native HWND/PID/creation plus scope gates pre-filter here; the portable
/// hidden/fresh exclusion rides [`crate::workspace_owner::writable_subset`],
/// the same seam the portable regression covers, so production and tests
/// share one rule without weakening any native check.
fn writable_tokens(
    state: &TileLoop,
    output: &str,
    workspace: &str,
    observed: &[ObservedWindow],
) -> HashSet<String> {
    let members = state.workspaces.workspace_members(output, workspace);
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let mut eligible: std::collections::BTreeMap<crate::workspace::WindowKey, String> =
        std::collections::BTreeMap::new();
    for key in &members {
        let Some(window) = by_hwnd.get(&key.hwnd) else {
            continue;
        };
        if !crate::workspace_owner::member_matches(
            key,
            window.hwnd,
            window.identity.pid,
            &window.identity.process_creation,
        ) {
            continue;
        }
        if !scope_allows(&state.scope, &window.identity.exe_path) {
            continue;
        }
        eligible.insert(key.clone(), window.token.clone());
    }
    let fresh: HashSet<String> = observed.iter().map(|w| w.token.clone()).collect();
    // Floats converge Engine membership but never take tiled geometry writes.
    let domain_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(workspace.to_owned()),
    };
    let float_tokens: HashSet<String> = state
        .engine
        .session(&domain_key)
        .map(|session| {
            eligible
                .values()
                .filter(|token| session.is_exception(&WindowId(token.to_string())))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    eligible.retain(|_, token| !float_tokens.contains(token));
    crate::workspace_owner::writable_subset(
        &members,
        |k| state.workspaces.is_hidden(k),
        &eligible,
        &fresh,
    )
}

/// Run one reconcile tick partitioned per output-local domain: enumerate
/// once, then converge each monitor's current workspace through its own
/// retained Engine session with that monitor's `rcWork` bounds. Hidden
/// workspace sessions are never reconciled, so their membership and layout
/// survive untouched; no layout reseeds across switches.
fn reconcile_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let log_path = state.log_path.clone();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"tick-skip","tick":tick,"cause":"enum-failed"}),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    workspace_close_cleanup(state);
    // Structured observer tick before Engine dispatch: full EnumWindows
    // success with the raw enumerated count and the managed (eligible,
    // allowlisted) count, including zero. A zero managed set is a completed
    // observation, not a failure: the Engine may answer Rejected for an empty
    // domain and the nonmoving proof must not require a layout plan.
    let enumerated = state.last_enumerated;
    let managed = observed.len();
    if state.trace {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "observe",
                "tick": tick,
                "op": "reconcile",
                "enumerated": enumerated,
                "managed": managed,
            }),
        );
    }
    if let Some(audit) = state.audit_path.clone() {
        log_json_at(
            &audit,
            serde_json::json!({
                "event": "proof-observe",
                "tick": tick,
                "op": "reconcile",
                "enumerated": enumerated,
                "managed": managed,
            }),
        );
    }
    // One shared hint-query budget for the whole tick across all outputs:
    // no domain independently blows the per-operation bound.
    let mut hint_cx = HintCx::new();
    for area in areas {
        let output = area.device.clone();
        state.workspaces.ensure_output(&output);
        let Some(active) = state.workspaces.active_id(&output) else {
            continue;
        };
        // Floating workspaces leave frames untouched: no Engine reconcile
        // and no geometry writes for that domain. Membership, hide/reveal,
        // and the independent active border still run.
        if !workspace_mode_tiled(state, &output, &active) {
            continue;
        }
        if tiling_domain_bounds_with(area.work, state.outer_gap).is_none() {
            continue;
        }
        // Incomplete snapshot defers with retained Engine state: a falsely
        // complete observation would drop membership and layout.
        let Some(rows) = assemble_domain_rows(
            state,
            &output,
            &active,
            &observed,
            &retained,
            "reconcile",
            correlation.as_str(),
            &mut hint_cx,
        ) else {
            continue;
        };
        let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let fp = fingerprint(
            &rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        let focused = state
            .focused_token(&observed)
            .filter(|f| windows.iter().any(|(w, _, _, _)| w == f));
        let Some((domain, domain_key)) =
            workspace_domain_for(&output, &active, areas, state.inner_gap, state.outer_gap)
        else {
            continue;
        };
        // Retained gap drift adopts through the topology-preserving
        // update-gaps path: plain reconcile refuses domain-mismatch instead
        // of adopting it, and fresh domains still seed via reconcile.
        let gaps_adopted = crate::settings::retained_gaps_match(
            &state.engine,
            &domain_key,
            state.inner_gap,
            state.outer_gap,
        );
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            &correlation,
            revision_for(state, &output, &active),
            fp,
            &domain,
            &domain_key,
            state.outer_gap,
            &windows,
            focused.as_ref(),
        );
        if !gaps_adopted {
            event.command = CoreCommand::UpdateGaps;
        }
        let reply = state.engine.handle(&event);
        log_engine_placement_trace(state, tick, correlation.as_str(), "reconcile");
        let writable = writable_tokens(state, &output, &active, &observed);
        apply_geometry(
            state,
            ApplyInput {
                me,
                fulls,
                reply: &reply,
                observed: &observed,
                op: "reconcile",
                tick,
                correlation: correlation.as_str(),
                skipped: skipped.clone(),
                writable: &writable,
                output_token: state.workspaces.output_token(&output),
                workspace_token: state.workspaces.workspace_token(&output, &active),
                revision: revision_for(state, &output, &active),
            },
        );
    }
}

/// Bundled inputs for one geometry application pass.
struct ApplyInput<'a> {
    me: &'a ProcessIdentity,
    fulls: &'a [Rect],
    reply: &'a CoreReply,
    observed: &'a [ObservedWindow],
    op: &'a str,
    tick: u64,
    correlation: &'a str,
    skipped: Vec<(String, String)>,
    /// Eligible observed tokens in this domain. Retained and hidden rows
    /// converge Engine membership but never take geometry writes.
    writable: &'a HashSet<String>,
    /// Opaque domain tokens for the per-domain tick log (no native ids).
    output_token: String,
    workspace_token: String,
    /// Engine revision of this domain for the tick summary signature.
    revision: u64,
}

/// Settled result of one geometry application pass: verified writes,
/// readback mismatches, and whether the independent readback observation ran
/// at all, plus the first privacy-safe veto diagnostic when a write vetoed.
/// Callers map this to their action outcome vocabulary instead of assuming
/// success. `veto` is `None` when no write vetoed: the action line serializes
/// null with reason, never zeros implying applied.
#[derive(Debug, Clone, Copy)]
struct ApplySummary {
    applied: usize,
    mismatched: usize,
    readback_ok: bool,
    veto: Option<VetoDiag>,
}

/// Bounded privacy-safe veto diagnostic: reason token plus the native
/// booleans behind it. No handles, paths, PIDs, titles, or content; opaque
/// managed tokens only travel in the existing skip list.
#[derive(Debug, Clone, Copy)]
struct VetoDiag {
    reason: &'static str,
    desktop: bool,
    visible: bool,
    captioned: bool,
    dwm_readable: bool,
    cloaked: bool,
}

fn audit_json(state: &TileLoop, value: serde_json::Value) {
    if let Some(path) = state.audit_path.as_ref() {
        log_json_at(path, value);
    }
}

fn apply_geometry(state: &mut TileLoop, input: ApplyInput<'_>) -> Option<ApplySummary> {
    let ApplyInput {
        me,
        fulls,
        reply,
        observed,
        op,
        tick,
        correlation,
        mut skipped,
        writable,
        output_token,
        workspace_token,
        revision,
    } = input;
    let log_path = state.log_path.clone();
    let Some(desired) = desired_entries(reply) else {
        let outcome = match reply {
            CoreReply::Rejected { kind, .. } => ("rejected", *kind),
            CoreReply::Diverged(reason) => ("diverged", reason.as_str()),
            CoreReply::SnapshotInvalid { detail, .. } => ("snapshot-invalid", *detail),
            _ => ("unexpected-reply", "unhandled-variant"),
        };
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "reply",
                "tick": tick,
                "correlation": correlation,
                "op": op,
                "output": output_token,
                "workspace": workspace_token,
                "outcome": outcome.0,
                "kind": outcome.1,
            }),
        );
        return None;
    };
    let by_token: HashMap<&str, &ObservedWindow> =
        observed.iter().map(|w| (w.token.as_str(), w)).collect();
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let proof_mode = state.allowlist.is_some();
    let now = Instant::now();
    let mut applied = 0usize;
    // First veto diagnostic in this pass for the bounded action summary; the
    // per-window skip list keeps the existing `fullscreen-foreground` token.
    let mut veto_diag: Option<VetoDiag> = None;
    // Tokens whose setter succeeded this tick. Only these may enter the
    // app-clamp lane on readback mismatch; failed or skipped setters keep
    // the transient/backoff lanes intact.
    let mut written: HashSet<String> = HashSet::new();
    for entry in &desired {
        if entry.client_clamped {
            skipped.push((entry.window.0.clone(), "client-clamped".to_owned()));
            continue;
        }
        if !writable.contains(entry.window.0.as_str()) {
            // Retained or hidden rows converge Engine membership but never
            // take geometry writes: hidden workspace geometry waits for
            // reveal and minimized/maximized/fullscreen frames stay native.
            // Overconstrained non-writable rows stay here too; only admitted
            // writable overconstrained windows take the min-clamped target
            // below.
            skipped.push((entry.window.0.clone(), "retained".to_owned()));
            continue;
        }
        // Overconstrained writables take the min-clamped tile origin instead
        // of skipping while reserving the tile. All checks below use the
        // effective target so clamping converges instead of retrying.
        let effective = if entry.overconstrained {
            let hints = state
                .hint_logged
                .get(entry.window.0.as_str())
                .copied()
                .unwrap_or(WindowSizeHints::none());
            crate::tiling::overconstrained_effective(entry.rect, hints)
        } else {
            entry.rect
        };
        let Some(expected) = by_token.get(entry.window.0.as_str()) else {
            skipped.push((entry.window.0.clone(), "vanished".to_owned()));
            continue;
        };
        if expected.visible == effective {
            state.refused.note_match(&entry.window.0);
            continue;
        }
        if state
            .refused
            .should_skip(&entry.window.0, &effective, &expected.visible, now)
        {
            skipped.push((entry.window.0.clone(), "refused-intent".to_owned()));
            continue;
        }
        // Snapshot the pre-write identity for the proof audit before the
        // revalidation borrows `state.tokens` mutably.
        let audit_pre = if proof_mode {
            by_hwnd.get(&expected.hwnd).map(|w| {
                (
                    w.identity.pid,
                    w.identity.process_creation.clone(),
                    w.identity.exe_path.clone(),
                    w.identity.user_sid.clone(),
                    w.identity.session_id,
                    w.identity.tag.clone(),
                )
            })
        } else {
            None
        };
        // Stored lifetime tag for the write gate: disjoint-field borrows of
        // `state` coexist with the `&mut state.tokens` below.
        let member_key = crate::workspace::WindowKey {
            hwnd: expected.hwnd,
            pid: expected.identity.pid,
            creation: expected.identity.process_creation.clone(),
        };
        let stored_tag = state.member_tags.get(&member_key).map(String::as_str);
        let target = match revalidate_target(
            expected,
            me,
            fulls,
            &mut state.tokens,
            proof_mode,
            state.allowlist.as_ref(),
            &state.scope,
            stored_tag,
            &state.scope_hosts,
            false,
        ) {
            Ok(target) => target,
            Err(reason) => {
                skipped.push((entry.window.0.clone(), reason.to_owned()));
                if proof_mode {
                    audit_json(
                        state,
                        serde_json::json!({
                            "event": "proof-skip",
                            "tick": tick,
                            "op": op,
                            "window": entry.window.0,
                            "requested": [effective.x, effective.y, effective.w, effective.h],
                            "reason": reason,
                        }),
                    );
                }
                continue;
            }
        };
        let Some(outer) = target.window.insets.visible_to_outer(effective) else {
            skipped.push((entry.window.0.clone(), "frame-overflow".to_owned()));
            continue;
        };
        // User safety across the tick: a vetoing foreground that arrived
        // after the loop guard must veto this write, not ride along. The
        // first veto pins the bounded diagnostic for the action summary.
        // Verified managed overlays bypass like a managed maximize.
        let read = suspend_read(state, me, fulls);
        if read.veto.block {
            if veto_diag.is_none() {
                veto_diag = Some(VetoDiag {
                    reason: read.veto.reason.as_str(),
                    desktop: read.desktop,
                    visible: read.visible,
                    captioned: read.captioned,
                    dwm_readable: read.dwm_readable,
                    cloaked: read.cloaked,
                });
            }
            skipped.push((entry.window.0.clone(), "fullscreen-foreground".to_owned()));
            continue;
        }
        let hwnd = target.window.hwnd as isize as HWND;
        let ok = unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                outer.x,
                outer.y,
                outer.w,
                outer.h,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        };
        // The held process keeps liveness authority across the write; recheck
        // the HWND pid afterward against recycling.
        let _ = &target.held;
        let pid_ok = pid_current(target.window.hwnd, target.window.identity.pid);
        if proof_mode {
            let (pid, creation, exe, sid, session, tag) = audit_pre.unwrap_or_else(|| {
                (
                    target.window.identity.pid,
                    target.window.identity.process_creation.clone(),
                    target.window.identity.exe_path.clone(),
                    target.window.identity.user_sid.clone(),
                    target.window.identity.session_id,
                    target.window.identity.tag.clone(),
                )
            });
            audit_json(
                state,
                serde_json::json!({
                    "event": "proof-write",
                    "tick": tick,
                    "op": op,
                    "window": entry.window.0,
                    "requested": [effective.x, effective.y, effective.w, effective.h],
                    "outer": [outer.x, outer.y, outer.w, outer.h],
                    "target": {
                        "hwnd": target.window.hwnd,
                        "pid": pid,
                        "process_creation": creation,
                        "exe_path": exe,
                        "user_sid": sid,
                        "session_id": session,
                        "tag": tag,
                    },
                    "flags": "SWP_NOACTIVATE|SWP_NOZORDER",
                    "outcome": if ok != 0 && pid_ok { "written" } else if ok == 0 { "setter-failed" } else { "pid-changed" },
                }),
            );
        }
        if !pid_ok {
            skipped.push((entry.window.0.clone(), "pid-changed".to_owned()));
            continue;
        }
        if ok == 0 {
            skipped.push((entry.window.0.clone(), "setter-failed".to_owned()));
            state
                .refused
                .note_transient(&entry.window.0, &effective, &target.window.visible, now);
            continue;
        }
        applied += 1;
        written.insert(entry.window.0.clone());
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "write",
                    "tick": tick,
                    "correlation": correlation,
                    "op": op,
                    "window": entry.window.0,
                    "desired": [effective.x, effective.y, effective.w, effective.h],
                }),
            );
        }
    }
    // Complete Engine plan for this tick: every desired entry, not only the
    // changed writes, so proof can compare the full requested set.
    if state.trace {
        let plan: Vec<serde_json::Value> = desired
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "window": entry.window.0,
                    "rect": [entry.rect.x, entry.rect.y, entry.rect.w, entry.rect.h],
                })
            })
            .collect();
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "plan",
                "tick": tick,
                "correlation": correlation,
                "op": op,
                "entries": plan,
            }),
        );
    }
    // Independent readback: a second full observation pass, never the setter
    // result. Successful writes that still mismatch are app-held sizes (clamp
    // lane); failed setters already took the transient lane above. The
    // readback pass reuses the same full-monitor list so DWM pre/post frame
    // math stays coherent within the tick.
    let reread_fulls = fulls.to_vec();
    let mut reread_skipped = Vec::new();
    let mut reread_retained: Vec<RetainedRow> = Vec::new();
    let reread = state.observe(me, &reread_fulls, &mut reread_skipped, &mut reread_retained);
    let mut mismatched = 0usize;
    let mut readback_ok = false;
    if let Some(reread) = reread {
        readback_ok = true;
        // Stable pre-gesture state follows the AFTER-actuation observation,
        // never the pre-apply frame the Engine just consumed.
        publish_managed(state, me, &reread, &reread_retained);
        let reread_by_token: HashMap<&str, Rect> = reread
            .iter()
            .map(|w| (w.token.as_str(), w.visible))
            .collect();
        for entry in &desired {
            if !writable.contains(entry.window.0.as_str()) {
                // Retained rows never took writes: no readback verdict.
                continue;
            }
            // Same effective target as the write pass so clamping reads back
            // as a match and refused tracking stays stable.
            let effective = if entry.overconstrained {
                let hints = state
                    .hint_logged
                    .get(entry.window.0.as_str())
                    .copied()
                    .unwrap_or(WindowSizeHints::none());
                crate::tiling::overconstrained_effective(entry.rect, hints)
            } else {
                entry.rect
            };
            match reread_by_token.get(entry.window.0.as_str()) {
                Some(visible) => {
                    match readback_outcome(
                        written.contains(entry.window.0.as_str()),
                        &effective,
                        visible,
                    ) {
                        ReadbackOutcome::Match => {
                            state.refused.note_match(&entry.window.0);
                        }
                        ReadbackOutcome::Clamp => {
                            mismatched += 1;
                            state
                                .refused
                                .note_clamp(&entry.window.0, &effective, visible);
                        }
                        ReadbackOutcome::Pending => {
                            // No write behind this mismatch (skipped or
                            // refused intent): count it, but leave the
                            // tracker lanes alone so a later matching
                            // re-read still converges instead of sticking
                            // as a permanent clamp.
                            mismatched += 1;
                        }
                    }
                }
                None => {
                    skipped.push((entry.window.0.clone(), "vanished".to_owned()));
                }
            }
        }
        let live: Vec<&str> = reread_by_token.keys().copied().collect();
        state.refused.retain(&live);
        if state.trace {
            let detail: Vec<serde_json::Value> = desired
                .iter()
                .map(|entry| {
                    let readback = reread_by_token.get(entry.window.0.as_str());
                    let effective = if entry.overconstrained {
                        let hints = state
                            .hint_logged
                            .get(entry.window.0.as_str())
                            .copied()
                            .unwrap_or(WindowSizeHints::none());
                        crate::tiling::overconstrained_effective(entry.rect, hints)
                    } else {
                        entry.rect
                    };
                    serde_json::json!({
                        "window": entry.window.0,
                        "desired": [effective.x, effective.y, effective.w, effective.h],
                        "readback": readback.map(|r| [r.x, r.y, r.w, r.h]),
                        "matched": readback.is_some_and(|r| r == &effective),
                    })
                })
                .collect();
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "readback-detail",
                    "tick": tick,
                    "correlation": correlation,
                    "op": op,
                    "entries": detail,
                }),
            );
        }
    }
    skipped.extend(reread_skipped);
    if mismatched > 0 {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "readback",
                "tick": tick,
                "correlation": correlation,
                "op": op,
                "mismatched": mismatched,
            }),
        );
    }
    // Routine zero-change ticks stay quiet; anything else logs one summary.
    // The dedupe signature carries no tick (stable-noop): an identical
    // observation must stay quiet no matter how many ticks pass. Bounded: the
    // sorted skip list is the full signal, no arbitrary caps.
    let mut summary_skipped = skipped.clone();
    summary_skipped.sort();
    let signature = tick_summary_signature(
        observed.len(),
        applied,
        &summary_skipped,
        mismatched,
        op,
        revision,
    );
    let signature = format!("{output_token}/{workspace_token}|{signature}");
    let changed = state.last_summary.as_ref() != Some(&signature);
    if changed || state.trace {
        state.last_summary = Some(signature);
        let skipped_json: Vec<serde_json::Value> = summary_skipped
            .into_iter()
            .map(|(window, reason)| serde_json::json!({"window": window, "reason": reason}))
            .collect();
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "tick",
                "tick": tick,
                "op": op,
                "correlation": correlation,
                "output": output_token,
                "workspace": workspace_token,
                "windows": observed.len(),
                "applied": applied,
                "mismatched": mismatched,
                "skipped": skipped_json,
            }),
        );
    }
    Some(ApplySummary {
        applied,
        mismatched,
        readback_ok,
        veto: veto_diag,
    })
}

/// Closed outcome vocabulary for a directional Engine reply that is not a
/// success plan: edge no-ops and refusals actuate nothing.
fn reply_outcome(reply: &CoreReply) -> &'static str {
    match reply {
        CoreReply::Rejected { kind, .. } => kind,
        CoreReply::Diverged(reason) => reason.as_str(),
        CoreReply::SnapshotInvalid { detail, .. } => detail,
        _ => "unexpected-reply",
    }
}

/// Outcome of one Engine focus actuation plus setter/readback trace.
/// `setter_accepted` is the raw `SetForegroundWindow` BOOL (never trusted as
/// proof); `prime_inserted` is the bounded `SendInput` accepted-event count
/// for the E8 last-input prime (0..=2, never trusted as proof);
/// `attach_ok` reports the bounded `AttachThreadInput` coupling to the
/// pre-call foreground thread (never trusted as proof); `eventual` is true
/// only when the exact-foreground readback matched after the pumped settle
/// rather than immediately (never trusted beyond the readback itself);
/// `deferred` stays false: no synchronous foreign message waits, the
/// callback remains promptly available. Token-safe ints/bools for the
/// production log: no native ids.
struct FocusActuation {
    outcome: &'static str,
    setter_accepted: bool,
    prime_inserted: u8,
    attach_ok: bool,
    eventual: bool,
    deferred: bool,
}

/// Acquire last-input rights immediately before `SetForegroundWindow`: one
/// unassigned E8 down/up pair (`VK_MASK`, `dwExtraInfo` 0) via `SendInput`.
/// The OS grants foreground rights to the last input provider, and E8 carries
/// no modifier, character, or Start-menu side effect (same technique as the
/// accepted Win-up mask). Zero means the prime contributed nothing; the caller
/// still attempts the setter and the exact foreground readback decides.
/// A partial single insert runs one bounded E8-up release so no virtual key
/// stays held. Runs on the owner loop thread, never in the LL callback, with
/// no wait, no `AttachThreadInput`, no Alt, and no policy/registry effect.
fn prime_foreground_rights() -> u8 {
    let inserted = unsafe {
        let mut pair: [INPUT; 2] = std::mem::zeroed();
        pair[0].r#type = INPUT_KEYBOARD;
        pair[0].Anonymous.ki = KEYBDINPUT {
            wVk: VK_MASK as u16,
            wScan: 0,
            dwFlags: 0,
            time: 0,
            dwExtraInfo: 0,
        };
        pair[1].r#type = INPUT_KEYBOARD;
        pair[1].Anonymous.ki = KEYBDINPUT {
            wVk: VK_MASK as u16,
            wScan: 0,
            dwFlags: KEYEVENTF_KEYUP,
            time: 0,
            dwExtraInfo: 0,
        };
        let size = std::mem::size_of::<INPUT>() as i32;
        SendInput(2, pair.as_ptr(), size)
    };
    if inserted == 1 {
        // Bounded release of the lone E8 down; the count stays 1 (no clean
        // pair) so evidence never claims a full prime.
        unsafe {
            let mut up: INPUT = std::mem::zeroed();
            up.r#type = INPUT_KEYBOARD;
            up.Anonymous.ki = KEYBDINPUT {
                wVk: VK_MASK as u16,
                wScan: 0,
                dwFlags: KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            };
            let size = std::mem::size_of::<INPUT>() as i32;
            SendInput(1, &up, size);
        }
    }
    inserted.min(u32::from(u8::MAX)) as u8
}

/// Actuate one Engine focus plan on the exact target window: fresh
/// revalidation with the process held open across the call, one bounded E8
/// `SendInput` last-input prime, a bounded `AttachThreadInput` coupling of
/// the owner thread to the pre-call foreground thread, then exactly one
/// `SetForegroundWindow` with immediate detach, an exact immediate
/// `GetForegroundWindow` readback, and - on an immediate miss - a bounded
/// pumped settle (own queue only, hook stays live) distinguishing deferred
/// delivery from no transfer. The prime count, the attach result, and the
/// setter return are never trusted; only an exact foreground readback settles
/// `focus-ok`. There is no Alt injection and no `SendMessageTimeoutW` wait.
/// Proof mode keeps the frozen-allowlist gate; normal mode skips it.
///
/// This runs on the loop thread, never inside the low-level hook callback.
/// No call here waits on foreign input: `AttachThreadInput` only links
/// queues, `SetForegroundWindow` returns without waiting, detach runs
/// immediately after the single setter, and the settle only pumps the owner's
/// own queue. Revalidation is identity/liveness, not responsiveness - a hung
/// target cannot stall these calls because none of them blocks on it.
fn actuate_focus(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    to_token: &str,
) -> FocusActuation {
    let unsettled = |outcome: &'static str| FocusActuation {
        outcome,
        setter_accepted: false,
        prime_inserted: 0,
        attach_ok: false,
        eventual: false,
        deferred: false,
    };
    // Eligible observed target first; otherwise a verified retained overlay
    // member (KDE `requestFocus` overlay exemption): maximized and fullscreen
    // members are retained, never eligible, so eligible-only lookup never
    // focuses them. The expected identity below still revalidates fresh with
    // the full identity/scope/hosted/proof/lifetime gates; only the geometry
    // classifier is focus-relaxed (overlays allowed, everything else still
    // refused). Reuses the retained-aware origin set's membership rule.
    let owned_expected: Option<ObservedWindow>;
    let expected = if let Some(found) = observed.iter().find(|w| w.token == to_token) {
        found
    } else {
        let Some(row) = retained.iter().find(|r| r.token == to_token) else {
            return unsettled("vanished");
        };
        let Some(stored) = state.member_identity.get(&row.key).cloned() else {
            return unsettled("vanished");
        };
        owned_expected = Some(ObservedWindow {
            hwnd: row.key.hwnd,
            token: row.token.clone(),
            outer: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            visible: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            insets: FrameInsets::default(),
            identity: crate::tiling::ObservedTarget {
                hwnd: row.key.hwnd,
                pid: row.key.pid,
                process_creation: row.key.creation.clone(),
                exe_path: stored.exe_path,
                user_sid: stored.user_sid,
                session_id: stored.session_id,
                tag: String::new(),
            },
            facts: row.facts.unwrap_or(crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: true,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                dialog: false,
            }),
        });
        owned_expected.as_ref().expect("retained expected built")
    };
    let proof_mode = state.allowlist.is_some();
    let member_key = crate::workspace::WindowKey {
        hwnd: expected.hwnd,
        pid: expected.identity.pid,
        creation: expected.identity.process_creation.clone(),
    };
    let stored_tag = state.member_tags.get(&member_key).map(String::as_str);
    let target = match revalidate_target(
        expected,
        me,
        fulls,
        &mut state.tokens,
        proof_mode,
        state.allowlist.as_ref(),
        &state.scope,
        stored_tag,
        &state.scope_hosts,
        true,
    ) {
        Ok(target) => target,
        Err(reason) => return unsettled(reason),
    };
    let hwnd = target.window.hwnd as isize as HWND;
    if unsafe { GetForegroundWindow() } as usize as u64 == target.window.hwnd {
        // Already exact foreground: no input and no setter needed.
        return FocusActuation {
            outcome: "focus-ok",
            setter_accepted: false,
            prime_inserted: 0,
            attach_ok: false,
            eventual: false,
            deferred: false,
        };
    }
    let prime_inserted = prime_foreground_rights();
    // Real fullscreen arrival fence immediately before the native setter: a
    // vetoing foreground (or an elevated foreground) that arrived during
    // validation/priming is never stolen from. The geometry path below then
    // vetoes honestly instead of riding along. No wait, no retry. Verified
    // managed overlays bypass like a managed maximize.
    if suspend_read(state, me, fulls).veto.block || foreground_elevated(me) {
        return FocusActuation {
            outcome: "focus-skipped-fence",
            setter_accepted: false,
            prime_inserted,
            attach_ok: false,
            eventual: false,
            deferred: false,
        };
    }
    // Couple the owner input queue to the pre-call foreground thread so the
    // single setter below runs attached (AHK ladder rung one). Skipped when
    // there is no foreground thread or it is already ours. No wait, no Alt.
    // `GetWindowThreadProcessId` RETURNS the thread id and writes the process
    // id to its out-param: the tid comes from the return value, never from
    // the pid slot (a pid-as-tid attach was the 024139 false negative).
    let current_tid = unsafe { GetCurrentThreadId() };
    let foreground_before = unsafe { GetForegroundWindow() };
    let foreground_tid = if foreground_before.is_null() {
        0
    } else {
        unsafe { GetWindowThreadProcessId(foreground_before, std::ptr::null_mut()) }
    };
    let attach_ok = foreground_tid != 0
        && foreground_tid != current_tid
        && unsafe { AttachThreadInput(current_tid, foreground_tid, 1) } != 0;
    let setter_accepted = unsafe { SetForegroundWindow(hwnd) } != 0;
    if attach_ok {
        unsafe {
            AttachThreadInput(current_tid, foreground_tid, 0);
        }
    }
    // The held process keeps liveness authority across the call; recheck the
    // HWND pid afterward against recycling.
    let _ = &target.held;
    if !pid_current(target.window.hwnd, target.window.identity.pid) {
        return FocusActuation {
            outcome: "pid-changed",
            setter_accepted,
            prime_inserted,
            attach_ok,
            eventual: false,
            deferred: false,
        };
    }
    if unsafe { GetForegroundWindow() } as usize as u64 == target.window.hwnd {
        return FocusActuation {
            outcome: "focus-ok",
            setter_accepted,
            prime_inserted,
            attach_ok,
            eventual: false,
            deferred: false,
        };
    }
    // Immediate miss with an accepted setter does not establish no transfer:
    // the system can deliver foreground after the call returns. Pump the
    // owner's own queue (new intents queue bounded for the next drain; the
    // hook callback stays prompt) and re-read, bounded with no foreign wait.
    let mut eventual = false;
    for _ in 0..FOCUS_SETTLE_ROUNDS {
        pump_wait(FOCUS_SETTLE_POLL_MS);
        if unsafe { GetForegroundWindow() } as usize as u64 == target.window.hwnd {
            eventual = true;
            break;
        }
    }
    // Recycled-HWND guard covers the settle window too.
    if !pid_current(target.window.hwnd, target.window.identity.pid) {
        return FocusActuation {
            outcome: "pid-changed",
            setter_accepted,
            prime_inserted,
            attach_ok,
            eventual: false,
            deferred: false,
        };
    }
    if eventual {
        return FocusActuation {
            outcome: "focus-ok",
            setter_accepted,
            prime_inserted,
            attach_ok,
            eventual,
            deferred: false,
        };
    }
    if !setter_accepted {
        return FocusActuation {
            outcome: "focus-unverified",
            setter_accepted,
            prime_inserted,
            attach_ok,
            eventual: false,
            deferred: false,
        };
    }
    FocusActuation {
        outcome: "focus-unverified",
        setter_accepted,
        prime_inserted,
        attach_ok,
        eventual: false,
        deferred: false,
    }
}

/// R-MOV-03 mode for the next keyboard move: the last-good live setting,
/// read per move so a saved change applies to subsequent moves only. Proof
/// owners carry no live settings (`None`) and keep the default wrap.
fn same_axis_move_for(live: Option<&LiveSettings>) -> tiler_core::directional::SameAxisMove {
    live.map(|live| live.settings.core.same_axis_move_mode())
        .unwrap_or_default()
}

/// R-MAX-06 mode for the next first-seen maximized tiled admission: the
/// last-good live setting, read per tick so a saved change applies to
/// subsequent admissions only. Proof owners carry no live settings (`None`)
/// and keep the default tile-on-launch clear. Previously admitted
/// clients and Engine topology are untouched: the flag only gates the
/// one-shot clear below, never existing slots.
fn born_maximized_keep_overlay_for(live: Option<&LiveSettings>) -> bool {
    live.is_some_and(|live| live.settings.core.born_maximized_keep_overlay())
}

/// Drain one bounded batch of product keyboard intents against fresh complete
/// observations through the retained Engine.
///
/// While `blocked` is set (fullscreen suspension, an open managed gesture, or
/// a just-ended gesture settling) every intent drops safely with one bounded
/// stale line and nothing dispatches. Otherwise each consumed down/repeat
/// carries its chord-time origin (opaque token plus HWND/process-lifetime
/// evidence bound by the callback) and must re-resolve against fresh
/// observation: a chord consumed for A never acts on unrelated B after an
/// external focus change. The only retarget is an explicitly verified
/// owner-caused continuation within the same batch (a prior actuation moved
/// focus and the fresh foreground is exactly that target), so rapid repeated
/// navigation survives while external focus moves reject loudly. Focus intents
/// actuate the Engine `FocusDirectional` target with an exact foreground
/// readback; move intents apply the `MoveDirectional` geometry through the
/// shared write path and report its settled summary. Key-ups close the pair
/// and passed intents never dispatch: both are trace-only so held-key traffic
/// stays out of normal logs. Every settled action carries its correlation;
/// enumeration failure settles the action as failed. Log vocabulary is tokens
/// plus op/direction/edge/disposition/outcome/correlation only: no titles, raw
/// native ids, or keys.
fn keyboard_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<QueuedSnapEvent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    let dropped = crate::snapkey::sys::queue_dropped();
    if dropped > state.snap_dropped {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"snap-drop","lost": dropped - state.snap_dropped}),
        );
        state.snap_dropped = dropped;
    }
    if let Some(cause) = blocked {
        // Suspension and gestures invalidate any own-focus chain: the next
        // batch starts without continuation.
        state.snap_advance = None;
        let mut stale = 0u32;
        for ev in events {
            match ev {
                QueuedSnapEvent::Mask(mask) => {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap-mask",
                            "mask": crate::snapkey::sys::mask_evidence(&mask),
                        }),
                    );
                }
                QueuedSnapEvent::Intent(_) => stale += 1,
                QueuedSnapEvent::Resize(_) => stale += 1,
                QueuedSnapEvent::Workspace(_) => stale += 1,
                QueuedSnapEvent::WorkspaceSend(_) => stale += 1,
                QueuedSnapEvent::WorkspaceHistory(_) => stale += 1,
                QueuedSnapEvent::Maximize(_) => stale += 1,
                QueuedSnapEvent::Orientation(_) => stale += 1,
                QueuedSnapEvent::Fullscreen(_) => stale += 1,
                QueuedSnapEvent::Float(_) => stale += 1,
                QueuedSnapEvent::Sticky(_) => stale += 1,
            }
        }
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"snap-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    // Owner-caused focus advances verified across batches live on the loop:
    // the only retarget a chord origin may take, so rapid repeated
    // navigation survives bounded drains while external focus moves reject.
    // Cleared on external focus, lifetime mismatch, suspension, or gesture.
    for ev in events {
        match ev {
            QueuedSnapEvent::Mask(mask) => {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "snap-mask",
                        "mask": crate::snapkey::sys::mask_evidence(&mask),
                    }),
                );
            }
            QueuedSnapEvent::Intent(intent) => {
                if !intent.consumed || !intent.announce {
                    // Key-ups close the pair and passed chords never dispatch:
                    // trace-only, so held-key traffic stays out of normal logs.
                    if state.trace {
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "snap",
                                "tick": state.tick,
                                "op": intent.op.as_str(),
                                "direction": direction_name(intent.direction),
                                "edge": intent.edge.as_str(),
                                "disposition": if intent.consumed { "consumed" } else { "passed" },
                                "outcome": if intent.consumed { "key-up" } else { "passed" },
                            }),
                        );
                    }
                    continue;
                }
                // Fresh complete observation per intent. The chord-time origin
                // must re-resolve: stale intents die here, never on a cached
                // rectangle.
                state.tick += 1;
                let tick = state.tick;
                let correlation = state.correlation();
                let Some(origin) = intent.origin.clone() else {
                    // Authentic interception swallows unmanaged/unadmitted/
                    // shell/suspended/elevated chords with no origin; settle
                    // defensively without dispatch (never acts on protected
                    // windows).
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "origin-vanished",
                        }),
                    );
                    continue;
                };
                let mut skipped: Vec<(String, String)> = Vec::new();
                let mut retained: Vec<RetainedRow> = Vec::new();
                let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained)
                else {
                    // Enumeration failure settles the action as failed; the
                    // Engine keeps retained state and the next tick retries.
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "observation-failed",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                };
                publish_managed(state, me, &observed, &retained);
                ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
                // First-seen maximized windows restore once here: lifetime tags are
                // stamped, and the cleared window converges as eligible next tick.
                clear_maximize_at_admission(state, me, &retained);
                let fresh: Vec<SnapOrigin> = state.snap_origins.values().cloned().collect();
                let foreground_hwnd = Some(unsafe { GetForegroundWindow() } as usize as u64);
                let (from, continued) = match resolve_origin(
                    &origin,
                    foreground_hwnd,
                    &fresh,
                    state.snap_advance.as_ref(),
                ) {
                    OriginVerdict::Dispatch { token, continued } => (token, continued),
                    OriginVerdict::Reject(reason) => {
                        // External focus or lifetime mismatch invalidates the
                        // chain; never carry a stale advance forward.
                        state.snap_advance = None;
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "snap",
                                "tick": tick,
                                "correlation": correlation.as_str(),
                                "op": intent.op.as_str(),
                                "direction": direction_name(intent.direction),
                                "edge": intent.edge.as_str(),
                                "disposition": "consumed",
                                "outcome": reason,
                                "origin": origin.token,
                            }),
                        );
                        continue;
                    }
                };
                // Route through the origin window's own (output, workspace)
                // session: directional ops run on the retained per-workspace
                // domain the mover actually lives in, never a parallel
                // single-display family.
                let Some(member_key) = state
                    .member_tokens
                    .iter()
                    .find(|(_, token)| token.as_str() == from.as_str())
                    .map(|(key, _)| key.clone())
                else {
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "unmanaged",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                };
                if !crate::workspace_owner::member_matches(
                    &member_key,
                    origin.hwnd,
                    origin.pid,
                    &origin.creation,
                ) {
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "origin-vanished",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                }
                let Some(loc) = state.workspaces.member_loc(&member_key).cloned() else {
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "unmanaged",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                };
                let Some((domain, domain_key)) = workspace_domain_for(
                    &loc.output,
                    &loc.workspace,
                    areas,
                    state.inner_gap,
                    state.outer_gap,
                ) else {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "unknown-output",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                };
                // A focused float never starts directional navigation; floats
                // hold no tile slot so the Engine observation already excludes
                // them as targets. Sticky rides the same slotless float plus
                // an explicit subject gate so a pruned-domain sticky (no
                // Engine session) still refuses instead of navigating.
                if state.sticky.contains_key(&member_key) {
                    let outcome = match intent.op {
                        SnapOp::Focus => "focus-refused-sticky",
                        SnapOp::Move => "move-refused-sticky",
                    };
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": outcome,
                            "origin": origin.token,
                            "window": from,
                        }),
                    );
                    continue;
                }
                if engine_is_float(state, &loc.output, &loc.workspace, &from) {
                    let outcome = match intent.op {
                        SnapOp::Focus => "focus-refused-floating",
                        SnapOp::Move => "move-refused-floating",
                    };
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": outcome,
                            "origin": origin.token,
                            "window": from,
                        }),
                    );
                    continue;
                }
                // Floating workspaces run no tile layout: both focus and move
                // refuse with the matching workspace-floating vocabulary (KDE
                // plan-adapter parity). Frames stay native.
                if let Some(outcome) = crate::workspace_owner::directional_workspace_refusal(
                    intent.op,
                    workspace_mode_tiled(state, &loc.output, &loc.workspace),
                ) {
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": outcome,
                            "origin": origin.token,
                            "window": from,
                        }),
                    );
                    continue;
                }
                // KDE maximize/fullscreen isolation parity: a maximized
                // focused window keeps its tile slot but a directional move
                // would change its retained position/share, so refuse
                // fail-closed before any Engine mutation. Focus carries no
                // geometry write and stays allowed. Fullscreen wins.
                if intent.op == SnapOp::Move
                    && let Some(row) = retained.iter().find(|r| r.key == member_key)
                    && let Some(cause) = overlay_refusal_for(row)
                {
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": if cause == "fullscreen" { "move-refused-fullscreen" } else { "move-refused-maximize" },
                            "origin": origin.token,
                            "window": from,
                        }),
                    );
                    continue;
                }
                let mut hint_cx = HintCx::new();
                let Some(rows) = assemble_domain_rows(
                    state,
                    &loc.output,
                    &loc.workspace,
                    &observed,
                    &retained,
                    "directional",
                    correlation.as_str(),
                    &mut hint_cx,
                ) else {
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "deferred",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                };
                if !rows.iter().any(|r| r.token == from) {
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "unmanaged",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                }
                let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
                    .iter()
                    .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
                    .collect();
                let fp = fingerprint(
                    &rows
                        .iter()
                        .map(|r| (r.token.clone(), r.rect))
                        .collect::<Vec<_>>(),
                );
                // `from` is the origin-verified token, never blind foreground.
                let from = WindowId(from);
                // Live gap edits adopt here so the focus/move convergence
                // below sees matching gaps instead of refusing the keypress.
                {
                    let revision = revision_for(state, &loc.output, &loc.workspace);
                    let outer_gap = state.outer_gap;
                    adopt_gaps_for_route(
                        state,
                        &domain,
                        &domain_key,
                        outer_gap,
                        &windows,
                        Some(&from),
                        revision,
                        fp,
                        &correlation,
                    );
                }
                let mut event = crate::tiling::build_reconcile_event_for_floating(
                    &state.owner,
                    &state.generation,
                    &correlation,
                    revision_for(state, &loc.output, &loc.workspace),
                    fp,
                    &domain,
                    &domain_key,
                    state.outer_gap,
                    &windows,
                    Some(&from),
                );
                let direction = direction_name(intent.direction).to_owned();
                event.command = match intent.op {
                    SnapOp::Focus => CoreCommand::Focus {
                        window: from.0.clone(),
                        direction,
                        cross_output_transfer: false,
                        float_subject: false,
                    },
                    SnapOp::Move => CoreCommand::Move {
                        window: from.0.clone(),
                        direction,
                        cross_output_transfer: false,
                        // R-MOV-03: last-good live setting, read per move so
                        // Apply changes subsequent moves only. Proof owners
                        // carry no live settings and keep the default wrap.
                        same_axis_move: same_axis_move_for(state.settings_live.as_ref()),
                    },
                };
                // Single-domain observations run the local retained
                // propose/commit path; Core owns direction semantics.
                let reply = state.engine.handle(&event);
                let outcome = match intent.op {
                    SnapOp::Focus => {
                        if let CoreReply::FocusDirectional(plan) = &reply {
                            let to = plan.to_window.0.clone();
                            let actuation =
                                actuate_focus(state, me, fulls, &observed, &retained, &to);
                            let outcome = actuation.outcome;
                            if outcome == "focus-ok" {
                                // Verified own advance: later intents in this
                                // batch and across bounded drains may continue
                                // from it; cleared on external focus,
                                // mismatch, suspension, or gesture.
                                // Retained-aware: a focus-ok into a maximized
                                // member advances from its retained row, never
                                // a stale None.
                                state.snap_advance = observed
                                    .iter()
                                    .find(|w| w.token == to)
                                    .map(snap_origin_of)
                                    .or_else(|| {
                                        retained.iter().find(|r| r.token == to).map(|row| {
                                            crate::snapkey::SnapOrigin {
                                                hwnd: row.key.hwnd,
                                                token: row.token.clone(),
                                                pid: row.key.pid,
                                                creation: row.key.creation.clone(),
                                            }
                                        })
                                    });
                            } else {
                                state.snap_advance = None;
                            }
                            log_json_at(
                                &log_path,
                                serde_json::json!({
                                    "event": "snap",
                                    "tick": tick,
                                    "correlation": correlation.as_str(),
                                    "op": intent.op.as_str(),
                                    "direction": direction_name(intent.direction),
                                    "edge": intent.edge.as_str(),
                                    "disposition": "consumed",
                                    "outcome": outcome,
                                    "setter_accepted": actuation.setter_accepted,
                                    "prime_inserted": actuation.prime_inserted,
                                    "attach_ok": actuation.attach_ok,
                                    "eventual": actuation.eventual,
                                    "deferred": actuation.deferred,
                                    "origin": origin.token,
                                    "window": from.0,
                                    "continued": continued,
                                }),
                            );
                            continue;
                        }
                        reply_outcome(&reply)
                    }
                    SnapOp::Move => {
                        if let CoreReply::MoveDirectional(_) = &reply {
                            let writable =
                                writable_tokens(state, &loc.output, &loc.workspace, &observed);
                            let summary = apply_geometry(
                                state,
                                ApplyInput {
                                    me,
                                    fulls,
                                    reply: &reply,
                                    observed: &observed,
                                    op: "move",
                                    tick,
                                    correlation: correlation.as_str(),
                                    skipped,
                                    writable: &writable,
                                    output_token: state.workspaces.output_token(&loc.output),
                                    workspace_token: state
                                        .workspaces
                                        .workspace_token(&loc.output, &loc.workspace),
                                    revision: revision_for(state, &loc.output, &loc.workspace),
                                },
                            );
                            match summary {
                                Some(s) if !s.readback_ok => "move-unverified",
                                Some(s) if s.mismatched > 0 => "move-mismatch",
                                Some(s) if s.applied > 0 => "move-applied",
                                Some(_) => "move-noop",
                                None => reply_outcome(&reply),
                            }
                        } else {
                            reply_outcome(&reply)
                        }
                    }
                };
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "snap",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "op": intent.op.as_str(),
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": outcome,
                        "origin": origin.token,
                        "window": from.0,
                        "continued": continued,
                    }),
                );
            }
            QueuedSnapEvent::Maximize(intent) => {
                // Win+M toggle (KDE Meta+M parity): key-ups close the pair
                // and passed chords never dispatch, trace-only like the
                // directional arms.
                if !intent.consumed || !intent.announce {
                    if state.trace {
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "maximize-toggle",
                                "tick": state.tick,
                                "edge": intent.edge.as_str(),
                                "disposition": if intent.consumed { "consumed" } else { "passed" },
                                "outcome": if intent.consumed { "key-up" } else { "passed" },
                            }),
                        );
                    }
                    continue;
                }
                state.tick += 1;
                let tick = state.tick;
                let correlation = state.correlation();
                let Some(origin) = intent.origin.clone() else {
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "maximize-toggle",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "origin-vanished",
                        }),
                    );
                    continue;
                };
                // Fresh per-intent suspension/elevation fence (workspace_tick
                // parity): newly intercepted suspended/elevated chords settle
                // here with no side effects. The suspend read exempts verified
                // managed overlays, and the target revalidation below refuses
                // protected windows, so only a live managed member toggles.
                if let Some(outcome) = crate::tiling::toggle_gate_outcome(
                    suspend_read(state, me, fulls).veto.block,
                    foreground_elevated(me),
                ) {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "maximize-toggle",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": outcome,
                            "origin": origin.token,
                        }),
                    );
                    continue;
                };
                let mut skipped: Vec<(String, String)> = Vec::new();
                let mut retained: Vec<RetainedRow> = Vec::new();
                let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained)
                else {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "maximize-toggle",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "observation-failed",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                };
                publish_managed(state, me, &observed, &retained);
                ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
                // First-seen maximized windows restore once here: lifetime tags are
                // stamped, and the cleared window converges as eligible next tick.
                clear_maximize_at_admission(state, me, &retained);
                let fresh: Vec<SnapOrigin> = state.snap_origins.values().cloned().collect();
                let foreground_hwnd = Some(unsafe { GetForegroundWindow() } as usize as u64);
                let (from, _) = match resolve_origin(
                    &origin,
                    foreground_hwnd,
                    &fresh,
                    state.snap_advance.as_ref(),
                ) {
                    OriginVerdict::Dispatch { token, continued } => (token, continued),
                    OriginVerdict::Reject(reason) => {
                        state.snap_advance = None;
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "maximize-toggle",
                                "tick": tick,
                                "correlation": correlation.as_str(),
                                "edge": intent.edge.as_str(),
                                "disposition": "consumed",
                                "outcome": reason,
                                "origin": origin.token,
                            }),
                        );
                        continue;
                    }
                };
                let Some(member_key) = state
                    .member_tokens
                    .iter()
                    .find(|(_, token)| token.as_str() == from.as_str())
                    .map(|(key, _)| key.clone())
                else {
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "maximize-toggle",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "unmanaged",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                };
                if !crate::workspace_owner::member_matches(
                    &member_key,
                    origin.hwnd,
                    origin.pid,
                    &origin.creation,
                ) {
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "maximize-toggle",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "foreground-changed",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                }
                // Fresh fullscreen/state fences before the native write (KDE
                // `maximize-refused-fullscreen` parity): fullscreen wins and
                // refuses, never toggles. A maximized member restores, any
                // other managed member maximizes.
                let retained_row = retained.iter().find(|r| r.key == member_key).cloned();
                if retained_row.as_ref().is_some_and(|r| r.fullscreen) {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "maximize-toggle",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "maximize-refused-fullscreen",
                            "origin": origin.token,
                            "window": from,
                        }),
                    );
                    continue;
                }
                // Visible lifetime gate: the live member tag must equal the
                // stored tag, so a same-process HWND reuse authorizes no
                // native write.
                let live_tag = crate::product_hide::sys::read_member_tag(member_key.hwnd);
                let lifetime_ok = state.member_tags.get(&member_key).is_some_and(|stored| {
                    crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
                });
                if !lifetime_ok {
                    drop_member_state(state, &member_key);
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "maximize-toggle",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "identity-changed",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                }
                let Some(stored) = state.member_identity.get(&member_key).cloned() else {
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "maximize-toggle",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "unmanaged",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                };
                // Proof-mode ownership gate before the native write: helpers
                // re-verify against the frozen allowlist exactly like the
                // hide path.
                if let Some(entries) = state.allowlist.as_ref() {
                    let owned = entries
                        .iter()
                        .find(|e| e.hwnd == member_key.hwnd)
                        .is_some_and(|entry| {
                            verify_proof_owned(member_key.hwnd, entry, me).is_ok()
                        });
                    if !owned {
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "maximize-toggle",
                                "tick": tick,
                                "correlation": correlation.as_str(),
                                "edge": intent.edge.as_str(),
                                "disposition": "consumed",
                                "outcome": "identity-changed",
                                "origin": origin.token,
                            }),
                        );
                        continue;
                    }
                }
                // Explicit scope fences before the native write: out-of-scope
                // members and listed hosts without a live matching child
                // refuse with no writes.
                if !scope_allows(&state.scope, &stored.exe_path)
                    || !hosted_gate_allows(
                        &stored.exe_path,
                        member_key.hwnd,
                        stored.pid,
                        &state.scope_hosts,
                    )
                {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "maximize-toggle",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "scope-excluded",
                            "origin": origin.token,
                            "window": from,
                        }),
                    );
                    continue;
                }
                let zoomed = is_zoomed_now(member_key.hwnd);
                let wanted = !zoomed;
                // Discrete-toggle parity (provisional): each fresh discrete
                // Win+M down dispatches exactly one native toggle attempt.
                // Held repeats never reach this drain (the classifier emits
                // them as non-announce trace-only), so no persistent fence is
                // kept: a press after a native restore simply dispatches
                // again. Matches KDE's visible toggle while fixing the sticky
                // `attempted` refusal after a native restore.
                let outcome = toggle_zoom_async(member_key.hwnd, wanted);
                if crate::tiling::restore_wake_arm(outcome, !wanted) {
                    state.restore_wake = Some(RestoreWake {
                        key: member_key.clone(),
                        token: from.clone(),
                        until: Instant::now()
                            + Duration::from_millis(crate::tiling::RESTORE_WAKE_MS),
                    });
                }
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "maximize-toggle",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": outcome,
                        "target": if wanted { "maximized" } else { "restored" },
                        "origin": origin.token,
                        "window": from,
                    }),
                );
            }
            QueuedSnapEvent::Fullscreen(intent) => {
                // Win+F11 toggle (KDE Meta+F11 parity): key-ups close the pair
                // and passed chords never dispatch, trace-only like the
                // maximize arm.
                if !intent.consumed || !intent.announce {
                    if state.trace {
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "fullscreen-toggle",
                                "tick": state.tick,
                                "edge": intent.edge.as_str(),
                                "disposition": if intent.consumed { "consumed" } else { "passed" },
                                "outcome": if intent.consumed { "key-up" } else { "passed" },
                            }),
                        );
                    }
                    continue;
                }
                state.tick += 1;
                let tick = state.tick;
                let correlation = state.correlation();
                let Some(origin) = intent.origin.clone() else {
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "fullscreen-toggle",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "origin-vanished",
                        }),
                    );
                    continue;
                };
                // Fresh per-intent suspension/elevation fence (workspace_tick
                // parity): newly intercepted suspended/elevated chords settle
                // here with no side effects. The suspend read exempts the
                // verified managed foreground, so exiting our owned
                // fullscreen still proceeds; app-owned frames refuse below
                // via the metadata decision, never guessing restoration.
                if let Some(outcome) = crate::tiling::toggle_gate_outcome(
                    suspend_read(state, me, fulls).veto.block,
                    foreground_elevated(me),
                ) {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "fullscreen-toggle",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": outcome,
                            "origin": origin.token,
                        }),
                    );
                    continue;
                };
                let mut skipped: Vec<(String, String)> = Vec::new();
                let mut retained: Vec<RetainedRow> = Vec::new();
                let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained)
                else {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "fullscreen-toggle",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "observation-failed",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                };
                publish_managed(state, me, &observed, &retained);
                ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
                // First-seen maximized windows restore once here: lifetime tags are
                // stamped, and the cleared window converges as eligible next tick.
                clear_maximize_at_admission(state, me, &retained);
                let fresh: Vec<SnapOrigin> = state.snap_origins.values().cloned().collect();
                let foreground_hwnd = Some(unsafe { GetForegroundWindow() } as usize as u64);
                let (from, _) = match resolve_origin(
                    &origin,
                    foreground_hwnd,
                    &fresh,
                    state.snap_advance.as_ref(),
                ) {
                    OriginVerdict::Dispatch { token, continued } => (token, continued),
                    OriginVerdict::Reject(reason) => {
                        state.snap_advance = None;
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "fullscreen-toggle",
                                "tick": tick,
                                "correlation": correlation.as_str(),
                                "edge": intent.edge.as_str(),
                                "disposition": "consumed",
                                "outcome": reason,
                                "origin": origin.token,
                            }),
                        );
                        continue;
                    }
                };
                let Some(member_key) = state
                    .member_tokens
                    .iter()
                    .find(|(_, token)| token.as_str() == from.as_str())
                    .map(|(key, _)| key.clone())
                else {
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "fullscreen-toggle",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "unmanaged",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                };
                if !crate::workspace_owner::member_matches(
                    &member_key,
                    origin.hwnd,
                    origin.pid,
                    &origin.creation,
                ) {
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "fullscreen-toggle",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "foreground-changed",
                            "origin": origin.token,
                        }),
                    );
                    continue;
                }
                // Fresh pre-effect revalidation through the production target
                // gate (focus/overlay classifier, held process
                // integrity/identity, lifetime tag, host-child, proof scope)
                // immediately before any style/`SetWindowPos` effect, using
                // the held handle across effects. Replaces the stale
                // tick-observation gates: the expected window is the fresh
                // eligible observation when present, otherwise a retained
                // overlay construction exactly like `actuate_focus`, and only
                // the geometry classifier is focus-relaxed (overlays allowed).
                let owned_expected: Option<ObservedWindow>;
                let expected =
                    if let Some(found) = observed.iter().find(|w| w.hwnd == member_key.hwnd) {
                        found
                    } else {
                        let Some(row) = retained.iter().find(|r| r.key == member_key) else {
                            state.snap_advance = None;
                            log_json_at(
                                &log_path,
                                serde_json::json!({
                                    "event": "fullscreen-toggle",
                                    "tick": tick,
                                    "correlation": correlation.as_str(),
                                    "edge": intent.edge.as_str(),
                                    "disposition": "consumed",
                                    "outcome": "deferred",
                                    "origin": origin.token,
                                }),
                            );
                            continue;
                        };
                        let Some(stored) = state.member_identity.get(&row.key).cloned() else {
                            state.snap_advance = None;
                            log_json_at(
                                &log_path,
                                serde_json::json!({
                                    "event": "fullscreen-toggle",
                                    "tick": tick,
                                    "correlation": correlation.as_str(),
                                    "edge": intent.edge.as_str(),
                                    "disposition": "consumed",
                                    "outcome": "unmanaged",
                                    "origin": origin.token,
                                }),
                            );
                            continue;
                        };
                        owned_expected = Some(ObservedWindow {
                            hwnd: row.key.hwnd,
                            token: row.token.clone(),
                            outer: Rect {
                                x: 0,
                                y: 0,
                                w: 0,
                                h: 0,
                            },
                            visible: Rect {
                                x: 0,
                                y: 0,
                                w: 0,
                                h: 0,
                            },
                            insets: FrameInsets::default(),
                            identity: crate::tiling::ObservedTarget {
                                hwnd: row.key.hwnd,
                                pid: row.key.pid,
                                process_creation: row.key.creation.clone(),
                                exe_path: stored.exe_path,
                                user_sid: stored.user_sid,
                                session_id: stored.session_id,
                                tag: String::new(),
                            },
                            facts: row.facts.unwrap_or(crate::tiling::WindowFacts {
                                visible: true,
                                minimized: false,
                                maximized: true,
                                cloaked: false,
                                elevated: false,
                                shell: false,
                                tool_window: false,
                                owned: false,
                                captionless_fullscreen: false,
                                no_activate: false,
                                dialog: false,
                            }),
                        });
                        owned_expected.as_ref().expect("retained expected built")
                    };
                let proof_mode = state.allowlist.is_some();
                let member_tag = state.member_tags.get(&member_key).map(String::as_str);
                let target = match revalidate_target(
                    expected,
                    me,
                    fulls,
                    &mut state.tokens,
                    proof_mode,
                    state.allowlist.as_ref(),
                    &state.scope,
                    member_tag,
                    &state.scope_hosts,
                    true,
                ) {
                    Ok(target) => target,
                    Err(reason) => {
                        if reason == "identity-changed" {
                            drop_member_state(state, &member_key);
                        }
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "fullscreen-toggle",
                                "tick": tick,
                                "correlation": correlation.as_str(),
                                "edge": intent.edge.as_str(),
                                "disposition": "consumed",
                                "outcome": reason,
                                "origin": origin.token,
                            }),
                        );
                        continue;
                    }
                };
                // The held process keeps liveness authority across the style
                // and frame effects below; the HWND pid is rechecked by the
                // setters' readbacks, never trusted from the tick.
                let _held = &target.held;
                // Fresh pre-effect state plus our restoration metadata decide
                // the direction: owned metadata wins over geometry, so a press
                // on any owned frame exits even when the frame no longer
                // covers a monitor (external move or partial restore).
                // Entering is always project-owned; an app-owned frame
                // refuses with a bounded reason instead of guessing.
                let now_fullscreen = target.window.facts.captionless_fullscreen;
                let meta = read_fullscreen_meta(member_key.hwnd);
                let (target, outcome) =
                    match fullscreen_toggle_decision(now_fullscreen, meta.is_some()) {
                        crate::tiling::FullscreenToggle::RefuseAppOwned => {
                            ("fullscreen", "fullscreen-refused-app-owned")
                        }
                        crate::tiling::FullscreenToggle::Enter => {
                            // The revalidation above already proved fresh
                            // eligibility (or a maximized overlay, where
                            // fullscreen wins): enter with the fresh frame as
                            // the output anchor, never a stale tick read.
                            let full = output_for_rect(areas, &target.window.visible);
                            let full = areas
                                .iter()
                                .find(|a| a.device == full)
                                .map(|area| area.full)
                                .or_else(|| areas.first().map(|area| area.full));
                            match full {
                                Some(full) => {
                                    ("fullscreen", enter_fullscreen(member_key.hwnd, full))
                                }
                                None => ("fullscreen", "unknown-output"),
                            }
                        }
                        crate::tiling::FullscreenToggle::ExitOwned => {
                            let meta = meta.expect("decision owns metadata");
                            let outcome = exit_fullscreen_owned(member_key.hwnd, &meta);
                            // A clean exit to a normal frame restores the current
                            // Engine allocation in the same action; a reasserted
                            // maximize converges as a retained overlay next tick.
                            if outcome == "restored" && !meta.was_maximized {
                                let fulls_owned = fulls.to_vec();
                                reconcile_tick(state, me, &fulls_owned, areas);
                            }
                            ("restored", outcome)
                        }
                    };
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "fullscreen-toggle",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": outcome,
                        "target": target,
                        "origin": origin.token,
                        "window": from,
                    }),
                );
            }
            QueuedSnapEvent::Workspace(intent) => {
                // Routed to workspace_tick by the caller; defensive drop here
                // stays trace-only so held-key traffic never pollutes logs.
                if state.trace {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "workspace",
                            "tick": state.tick,
                            "op": intent.op.as_str(),
                            "index": intent.index,
                            "edge": intent.edge.as_str(),
                            "disposition": "rerouted",
                            "outcome": "workspace-tick",
                        }),
                    );
                }
            }
            QueuedSnapEvent::WorkspaceHistory(intent) => {
                // Routed to workspace_history_tick by the caller; defensive
                // drop here stays trace-only like digits.
                if state.trace {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "workspace-history",
                            "tick": state.tick,
                            "op": intent.op.as_str(),
                            "edge": intent.edge.as_str(),
                            "disposition": "rerouted",
                            "outcome": "workspace-history-tick",
                        }),
                    );
                }
            }
            QueuedSnapEvent::WorkspaceSend(intent) => {
                // Routed to workspace_relative_send_tick by the caller;
                // defensive drop here stays trace-only like digits.
                if state.trace {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "workspace-send",
                            "tick": state.tick,
                            "delta": intent.delta,
                            "follow": intent.follow,
                            "edge": intent.edge.as_str(),
                            "disposition": "rerouted",
                            "outcome": "workspace-send-tick",
                        }),
                    );
                }
            }
            QueuedSnapEvent::Float(intent) => {
                dispatch_float_intent(state, me, fulls, areas, intent);
            }
            QueuedSnapEvent::Resize(intent) => {
                dispatch_resize_intent(state, me, fulls, areas, intent);
            }
            QueuedSnapEvent::Orientation(intent) => {
                dispatch_orientation_intent(state, me, fulls, areas, intent);
            }
            QueuedSnapEvent::Sticky(intent) => {
                dispatch_sticky_intent(state, me, fulls, areas, intent);
            }
        }
    }
}

/// Keyboard resize dispatch (Win+Alt grow outwards, Win+Shift+Alt shrink
/// inwards, over H/J/K/L plus arrow aliases): fresh observation, exact
/// origin re-resolution, full gates, one Engine [`CoreCommand::Resize`] on
/// the retained session with the exact KDE `requestResize` repeat tracker
/// (`repeatFocused`/`repeatDirection`/`repeatMode`/`repeatNext` via
/// [`crate::tiling::resize_repeat_next`]; `press_index` 0 is the initial
/// 12px press, then 14/16/18/20px capped), then sibling reflow through the
/// shared write path. No focus, order, or topology mutation beyond the two
/// adjacent shares: focus stays on the same window. Never Move semantics:
/// the command is always `CoreCommand::Resize` carrying the pinned mode, and
/// the subject is always the origin-verified focused mover.
///
/// Pre-dispatch refusals mirror KDE `requestResize` exactly (disabled, busy,
/// floating subject, floating workspace, fullscreen/maximized overlay) plus
/// the Windows production fences (suspend/elevated, exact origin
/// re-resolution, lifetime, proof, scope). Refusals settle with no writes
/// and no repeat-tracker update; only a fence-passing press advances the
/// tracker, before the Engine dispatch (KDE order).
fn dispatch_resize_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: crate::snapkey::QueuedResizeIntent,
) {
    let log_path = state.log_path.clone();
    // Key-ups close the pair and passed chords never dispatch: trace-only so
    // held-key traffic stays out of normal logs.
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "resize",
                    "tick": state.tick,
                    "mode": intent.mode.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": if intent.consumed { "consumed" } else { "passed" },
                    "outcome": if intent.consumed { "key-up" } else { "passed" },
                }),
            );
        }
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "resize",
            "tick": tick,
            "correlation": correlation.as_str(),
            "mode": intent.mode.as_str(),
            "direction": direction_name(intent.direction),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    // KDE `!enabled` parity: chords arriving while keyboard takeover is off
    // never dispatch (the classifier already passes them through; this
    // covers the exact-owner test-needed route, which bypasses it).
    if !state.keyboard.takeover {
        let line = settle("resize-refused-disabled");
        log_json_at(&log_path, line);
        return;
    }
    let Some(origin) = intent.origin.clone() else {
        state.snap_advance = None;
        let line = settle("origin-vanished");
        log_json_at(&log_path, line);
        return;
    };
    // Fresh per-intent suspension/elevation fence (workspace_tick parity):
    // newly intercepted suspended/elevated chords settle here with no side
    // effects.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        let mut line = settle(outcome);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let Some(chord) = observe_chord_intent(state, me, fulls, areas) else {
        let mut line = settle("observation-failed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let ChordObserved {
        observed,
        retained,
        skipped,
    } = chord;
    let ChordTarget {
        from,
        member_key,
        loc,
    } = match resolve_chord_target(state, me, areas, &origin) {
        Ok(target) => target,
        Err(reject) => {
            settle_chord_reject(&log_path, &settle, &origin, &reject);
            return;
        }
    };
    // Float/sticky subjects hold no tile slot: the resize settles with no
    // writes, never retiles them. Ordered ahead of the workspace fence to
    // match the Snap Move arm and KDE refusal priority (subject before
    // domain).
    if state.sticky.contains_key(&member_key) {
        let mut line = settle("resize-refused-sticky");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    if engine_is_float(state, &loc.output, &loc.workspace, &from) {
        let mut line = settle("resize-refused-floating");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Floating workspaces run no tile layout: the resize settles with no
    // writes and every frame stays native (KDE plan-adapter parity).
    if !workspace_mode_tiled(state, &loc.output, &loc.workspace) {
        let mut line = settle("resize-refused-workspace-floating");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Focused overlays refuse before any Engine mutation: a maximized or
    // fullscreen focused window keeps its native state and its reserved
    // slot (KDE maximize/fullscreen isolation parity; fullscreen wins).
    if let Some(row) = retained.iter().find(|r| r.key == member_key)
        && let Some(cause) = overlay_refusal_for(row)
    {
        let mut line = settle(if cause == "fullscreen" {
            "resize-refused-fullscreen"
        } else {
            "resize-refused-maximize"
        });
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    let Some((domain, domain_key)) = workspace_domain_for(
        &loc.output,
        &loc.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        let mut line = settle("unknown-output");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    // KDE `inFlight`/`r4Flight` parity: a staged-but-unverified plan or an
    // open drag holds the domain. The Engine would refuse `pending-exists`
    // anyway; the pre-dispatch refusal keeps the outcome vocabulary exact
    // and the repeat tracker untouched.
    let busy = state.engine.session(&domain_key).is_some_and(|session| {
        session.has_pending() || session.has_pending_desired() || session.has_drag()
    });
    if busy {
        let mut line = settle("resize-refused-busy");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    let mut hint_cx = HintCx::new();
    let Some(rows) = assemble_domain_rows(
        state,
        &loc.output,
        &loc.workspace,
        &observed,
        &retained,
        "resize",
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        let mut line = settle("deferred");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    if !rows.iter().any(|r| r.token == from) {
        let mut line = settle("unmanaged");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    }
    let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let fp = fingerprint(
        &rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect::<Vec<_>>(),
    );
    let from_id = WindowId(from.clone());
    // Live gap edits adopt here so the resize converges against matching
    // gaps instead of refusing the keypress.
    {
        let revision = revision_for(state, &loc.output, &loc.workspace);
        let outer_gap = state.outer_gap;
        adopt_gaps_for_route(
            state,
            &domain,
            &domain_key,
            outer_gap,
            &windows,
            Some(&from_id),
            revision,
            fp,
            &correlation,
        );
    }
    // Exact KDE repeat tracker: continued only on a focused/direction/mode
    // match, restarted otherwise, advanced before the Engine dispatch.
    // `from` is the origin-verified token, never blind foreground.
    let direction = direction_name(intent.direction).to_owned();
    let mode = intent.mode.as_str().to_owned();
    let (press_index, repeat) =
        crate::tiling::resize_repeat_next(state.resize_repeat.clone(), &from, &direction, &mode);
    state.resize_repeat = Some(repeat);
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        revision_for(state, &loc.output, &loc.workspace),
        fp,
        &domain,
        &domain_key,
        state.outer_gap,
        &windows,
        Some(&from_id),
    );
    event.command = CoreCommand::Resize {
        window: from.clone(),
        direction,
        mode,
        press_index,
    };
    // Single-domain observations run the local retained propose/commit
    // path; Core owns direction semantics. The reply carries complete
    // desired geometry through `planned_writes`, exactly like Move.
    let reply = state.engine.handle(&event);
    let outcome = if let CoreReply::Resize(_) = &reply {
        let writable = writable_tokens(state, &loc.output, &loc.workspace, &observed);
        let summary = apply_geometry(
            state,
            ApplyInput {
                me,
                fulls,
                reply: &reply,
                observed: &observed,
                op: "resize",
                tick,
                correlation: correlation.as_str(),
                skipped,
                writable: &writable,
                output_token: state.workspaces.output_token(&loc.output),
                workspace_token: state
                    .workspaces
                    .workspace_token(&loc.output, &loc.workspace),
                revision: revision_for(state, &loc.output, &loc.workspace),
            },
        );
        match summary {
            Some(s) if !s.readback_ok => "resize-unverified",
            Some(s) if s.mismatched > 0 => "resize-mismatch",
            Some(s) if s.applied > 0 => "resize-applied",
            Some(_) => "resize-noop",
            None => reply_outcome(&reply),
        }
    } else {
        reply_outcome(&reply)
    };
    let mut line = settle(outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    line["window"] = serde_json::Value::from(from);
    line["press_index"] = serde_json::Value::from(press_index);
    log_json_at(&log_path, line);
}

struct ChordObserved {
    observed: Vec<ObservedWindow>,
    retained: Vec<RetainedRow>,
    skipped: Vec<(String, String)>,
}

fn observe_chord_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
) -> Option<ChordObserved> {
    let mut skipped = Vec::new();
    let mut retained = Vec::new();
    let mut observed = state.observe(me, fulls, &mut skipped, &mut retained)?;
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    clear_maximize_at_admission(state, me, &retained);
    Some(ChordObserved {
        observed,
        retained,
        skipped,
    })
}

/// Resolved chord target: the Engine token, member key, and domain.
struct ChordTarget {
    from: String,
    member_key: crate::workspace::WindowKey,
    loc: crate::workspace::MemberLoc,
}

/// Chord rejection: the outcome to log and the window token when the log
/// line carries one.
struct ChordReject {
    outcome: &'static str,
    window: Option<String>,
}

/// Origin-to-member resolution shared by the float and sticky chords: origin
/// re-resolution, member binding, domain, lifetime drop, proof, and scope
/// fences. Drops stale membership as a side effect; the caller logs.
fn resolve_chord_target(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    areas: &[MonitorArea],
    origin: &SnapOrigin,
) -> std::result::Result<ChordTarget, ChordReject> {
    let reject = |outcome: &'static str| ChordReject {
        outcome,
        window: None,
    };
    let fresh: Vec<SnapOrigin> = state.snap_origins.values().cloned().collect();
    let foreground_hwnd = Some(unsafe { GetForegroundWindow() } as usize as u64);
    let from = match resolve_origin(origin, foreground_hwnd, &fresh, state.snap_advance.as_ref()) {
        OriginVerdict::Dispatch { token, .. } => token,
        OriginVerdict::Reject(reason) => {
            state.snap_advance = None;
            return Err(reject(reason));
        }
    };
    let Some(member_key) = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == from.as_str())
        .map(|(key, _)| key.clone())
    else {
        state.snap_advance = None;
        return Err(reject("unmanaged"));
    };
    if !crate::workspace_owner::member_matches(
        &member_key,
        origin.hwnd,
        origin.pid,
        &origin.creation,
    ) {
        state.snap_advance = None;
        return Err(reject("foreground-changed"));
    }
    let Some(loc) = state.workspaces.member_loc(&member_key).cloned() else {
        state.snap_advance = None;
        return Err(reject("unmanaged"));
    };
    if workspace_domain_for(
        &loc.output,
        &loc.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    )
    .is_none()
    {
        return Err(reject("unknown-output"));
    }
    let live_tag = crate::product_hide::sys::read_member_tag(member_key.hwnd);
    let lifetime_ok = state.member_tags.get(&member_key).is_some_and(|stored| {
        crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
    });
    if !lifetime_ok {
        drop_member_state(state, &member_key);
        return Err(reject("identity-changed"));
    }
    let Some(stored) = state.member_identity.get(&member_key).cloned() else {
        state.snap_advance = None;
        return Err(reject("unmanaged"));
    };
    if let Some(entries) = state.allowlist.as_ref() {
        let owned = entries
            .iter()
            .find(|e| e.hwnd == member_key.hwnd)
            .is_some_and(|entry| verify_proof_owned(member_key.hwnd, entry, me).is_ok());
        if !owned {
            return Err(reject("identity-changed"));
        }
    }
    if !scope_allows(&state.scope, &stored.exe_path)
        || !hosted_gate_allows(
            &stored.exe_path,
            member_key.hwnd,
            stored.pid,
            &state.scope_hosts,
        )
    {
        return Err(ChordReject {
            outcome: "scope-excluded",
            window: Some(from.clone()),
        });
    }
    Ok(ChordTarget {
        from,
        member_key,
        loc,
    })
}

/// Settle one rejected chord with its origin (and window when carried).
fn settle_chord_reject(
    log_path: &Path,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    origin: &SnapOrigin,
    reject: &ChordReject,
) {
    let mut line = settle(reject.outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    if let Some(window) = &reject.window {
        line["window"] = serde_json::Value::from(window.clone());
    }
    log_json_at(log_path, line);
}

/// Overlay refusal for one chord target, float or sticky vocabulary.
fn chord_overlay_refusal(
    sticky_vocab: bool,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    member_key: &crate::workspace::WindowKey,
) -> Option<&'static str> {
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let zoomed = is_zoomed_now(member_key.hwnd);
    if let Some(window) = by_hwnd.get(&member_key.hwnd) {
        if sticky_vocab {
            crate::tiling::sticky_toggle_refusal(window.facts.captionless_fullscreen, zoomed)
        } else {
            crate::tiling::float_toggle_refusal(window.facts.captionless_fullscreen, zoomed)
        }
    } else if let Some(row) = retained.iter().find(|r| r.key == *member_key) {
        if sticky_vocab {
            crate::tiling::sticky_toggle_refusal(row.fullscreen, zoomed)
        } else {
            crate::tiling::float_toggle_refusal(row.fullscreen, zoomed)
        }
    } else {
        None
    }
}

/// Overlay refusal for one orientation target, orientation vocabulary.
/// Same live-state sourcing as the float/sticky overlay fence: the live
/// observed fullscreen flag plus a fresh zoom read when visible, else the
/// retained overlay flags plus a fresh zoom read.
fn orientation_overlay_refusal(
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    member_key: &crate::workspace::WindowKey,
) -> Option<&'static str> {
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let zoomed = is_zoomed_now(member_key.hwnd);
    if let Some(window) = by_hwnd.get(&member_key.hwnd) {
        crate::tiling::orientation_toggle_refusal(window.facts.captionless_fullscreen, zoomed)
    } else if let Some(row) = retained.iter().find(|r| r.key == *member_key) {
        crate::tiling::orientation_toggle_refusal(row.fullscreen, zoomed)
    } else {
        None
    }
}

/// Accept only the matching orientation plan from one Engine reply: a `Tiled`
/// plan of exactly `TiledKind::ToggleOrientation`. Lone root (`unchanged`),
/// float/no-focus (`not-tiled`), and every other refusal return `None` so the
/// caller settles with no writes and no focus mutation. Called by the native
/// orientation route; pinned against real Engine replies below.
fn orientation_plan(reply: &CoreReply) -> Option<&tiler_core::boundary::TiledPlan> {
    match reply {
        CoreReply::Tiled(plan)
            if plan.kind == tiler_core::boundary::TiledKind::ToggleOrientation =>
        {
            Some(plan)
        }
        _ => None,
    }
}

/// Shared `ToggleFloat` candidate for one domain member: assemble rows and
/// evaluate on a local candidate. The caller commits the Engine only after
/// its native effects verify, so a failed placement never strands state.
#[allow(clippy::too_many_arguments)]
fn prepare_toggle_float(
    state: &mut TileLoop,
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    op: &'static str,
    from: &str,
    loc: &crate::workspace::MemberLoc,
    float_rect: Option<Rect>,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
    correlation: &CorrelationId,
    target: &'static str,
) -> Option<(CoreReply, tiler_core::engine::Engine)> {
    let mut hint_cx = HintCx::new();
    let Some(rows) = assemble_domain_rows(
        state,
        &loc.output,
        &loc.workspace,
        observed,
        retained,
        op,
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        let mut line = settle("deferred");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(log_path, line);
        return None;
    };
    if !rows.iter().any(|r| r.token == from) {
        let mut line = settle("unmanaged");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(log_path, line);
        return None;
    }
    let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let fp = fingerprint(
        &rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect::<Vec<_>>(),
    );
    let from_id = WindowId(from.to_owned());
    let Some((domain, domain_key)) = workspace_domain_for(
        &loc.output,
        &loc.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        let mut line = settle("unknown-output");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(log_path, line);
        return None;
    };
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        correlation,
        revision_for(state, &loc.output, &loc.workspace),
        fp,
        &domain,
        &domain_key,
        OUTER_GAP,
        &windows,
        Some(&from_id),
    );
    event.command = CoreCommand::ToggleFloat {
        window: from.to_owned(),
        float_rect,
    };
    let mut candidate = state.engine.clone();
    let reply = candidate.handle(&event);
    if !matches!(reply, CoreReply::Tiled(_)) {
        let mut line = settle(reply_outcome(&reply));
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return None;
    }
    Some((reply, candidate))
}

/// Shared tiled-to-float transition: native placement plus raised band with
/// verified readback, Engine commit, sibling reflow, exact focus retained.
/// With `mark_sticky` the sticky mark lands after the float verifies; a failed
/// mark leaves a normal float for a later retry.
#[allow(clippy::too_many_arguments)]
fn apply_float_from_tiled(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    skipped: Vec<(String, String)>,
    member_key: &crate::workspace::WindowKey,
    from: &str,
    loc: &crate::workspace::MemberLoc,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
    tick: u64,
    correlation: &CorrelationId,
    op: &'static str,
    target: &'static str,
    applied: &'static str,
    mark_sticky: bool,
) {
    let Some((reply, candidate)) = prepare_toggle_float(
        state,
        areas,
        observed,
        retained,
        op,
        from,
        loc,
        None,
        origin,
        settle,
        log_path,
        correlation,
        target,
    ) else {
        return;
    };
    let CoreReply::Tiled(plan) = &reply else {
        return;
    };
    let Some(effective) = plan.float_rect else {
        let mut line = settle("float-missing-placement");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    };
    let Some(expected) = observed.iter().find(|w| w.hwnd == member_key.hwnd) else {
        let mut line = settle("origin-vanished");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    };
    let proof_mode = state.allowlist.is_some();
    let audit_pre = (
        expected.identity.pid,
        expected.identity.process_creation.clone(),
        expected.identity.exe_path.clone(),
        expected.identity.user_sid.clone(),
        expected.identity.session_id,
        expected.identity.tag.clone(),
    );
    let member_tag = state.member_tags.get(member_key).map(String::as_str);
    let placement = match revalidate_target(
        expected,
        me,
        fulls,
        &mut state.tokens,
        proof_mode,
        state.allowlist.as_ref(),
        &state.scope,
        member_tag,
        &state.scope_hosts,
        false,
    ) {
        Ok(target) => target,
        Err(reason) => {
            let mut line = settle(reason);
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            log_json_at(log_path, line);
            return;
        }
    };
    let Some(outer) = placement.window.insets.visible_to_outer(effective) else {
        let mut line = settle("frame-overflow");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    };
    if suspend_read(state, me, fulls).veto.block {
        let mut line = settle("fullscreen-foreground");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    }
    let hwnd_u64 = placement.window.hwnd;
    let current_topmost = read_topmost_now(hwnd_u64);
    let prior = state
        .float_topmost_prev
        .get(member_key)
        .copied()
        .unwrap_or(current_topmost);
    let (anchor, flags) = if current_topmost {
        (std::ptr::null_mut(), SWP_NOACTIVATE | SWP_NOZORDER)
    } else {
        (HWND_TOPMOST, SWP_NOACTIVATE)
    };
    let placed = unsafe {
        SetWindowPos(
            hwnd_u64 as isize as HWND,
            anchor,
            outer.x,
            outer.y,
            outer.w,
            outer.h,
            flags,
        )
    };
    let _ = &placement.held;
    let pid_ok = pid_current(hwnd_u64, placement.window.identity.pid);
    if proof_mode {
        let (pid, creation, exe, sid, session, tag) = audit_pre;
        audit_json(
            state,
            serde_json::json!({
                "event": "proof-write",
                "tick": tick,
                "op": op,
                "window": from,
                "requested": [effective.x, effective.y, effective.w, effective.h],
                "outer": [outer.x, outer.y, outer.w, outer.h],
                "target": {
                    "hwnd": placement.window.hwnd,
                    "pid": pid,
                    "process_creation": creation,
                    "exe_path": exe,
                    "user_sid": sid,
                    "session_id": session,
                    "tag": tag,
                },
                "flags": if current_topmost { "SWP_NOACTIVATE|SWP_NOZORDER" } else { "SWP_NOACTIVATE|TOPMOST" },
                "outcome": if placed != 0 && pid_ok { "written" } else if placed == 0 { "setter-failed" } else { "pid-changed" },
            }),
        );
    }
    if placed == 0 || !pid_ok {
        let mut line = settle(if placed == 0 {
            "setter-failed"
        } else {
            "pid-changed"
        });
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    }
    state.float_topmost_prev.insert(member_key.clone(), prior);
    let actual = match observe_window(hwnd_u64 as isize as HWND, me, fulls, &mut state.tokens) {
        Ok(fresh)
            if fresh.token == placement.window.token
                && pid_current(hwnd_u64, fresh.identity.pid) =>
        {
            if readback_outcome(true, &effective, &fresh.visible) == ReadbackOutcome::Match {
                effective
            } else {
                fresh.visible
            }
        }
        _ => {
            let mut line = settle("float-unverified");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            log_json_at(log_path, line);
            return;
        }
    };
    if !prior && !read_topmost_now(hwnd_u64) {
        let mut line = settle("topmost-unverified");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    }
    state.engine = candidate;
    state.float_rects.insert(from.to_owned(), actual);
    // Intentional-float lifetime: row assembly rides this token floating
    // across domain releases and boundary sends even when the Engine session
    // carries no exception yet. Sticky-on keeps its own lane instead. The
    // on-window marker persists only after this verified success so a later
    // owner restart re-adopts the classification; a failed install keeps
    // local intent and native state with the degraded outcome below.
    if !mark_sticky {
        state.floated.insert(member_key.clone());
    }
    if !mark_sticky && !install_float_mark_held(state, me, member_key) {
        let focus = retain_float_focus_validated(state, me, fulls, expected, member_key);
        let writable = writable_tokens(state, &loc.output, &loc.workspace, observed);
        let summary = apply_geometry(
            state,
            ApplyInput {
                me,
                fulls,
                reply: &reply,
                observed,
                op,
                tick,
                correlation: correlation.as_str(),
                skipped,
                writable: &writable,
                output_token: state.workspaces.output_token(&loc.output),
                workspace_token: state
                    .workspaces
                    .workspace_token(&loc.output, &loc.workspace),
                revision: revision_for(state, &loc.output, &loc.workspace),
            },
        );
        let outcome: &'static str = match summary {
            Some(s) if !s.readback_ok => "float-unverified",
            Some(s) if s.mismatched > 0 || actual != effective => "float-mismatch",
            Some(_) => "float-unverified",
            None => reply_outcome(&reply),
        };
        let mut line = settle(outcome);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        line["focus"] = serde_json::Value::from(focus);
        log_json_at(log_path, line);
        return;
    }
    if mark_sticky && !install_sticky_mark_held(state, me, member_key, false) {
        let focus = retain_float_focus_validated(state, me, fulls, expected, member_key);
        let writable = writable_tokens(state, &loc.output, &loc.workspace, observed);
        let summary = apply_geometry(
            state,
            ApplyInput {
                me,
                fulls,
                reply: &reply,
                observed,
                op,
                tick,
                correlation: correlation.as_str(),
                skipped,
                writable: &writable,
                output_token: state.workspaces.output_token(&loc.output),
                workspace_token: state
                    .workspaces
                    .workspace_token(&loc.output, &loc.workspace),
                revision: revision_for(state, &loc.output, &loc.workspace),
            },
        );
        let outcome: &'static str = match summary {
            Some(s) if !s.readback_ok => "float-unverified",
            Some(s) if s.mismatched > 0 || actual != effective => "float-mismatch",
            Some(_) => "sticky-unverified",
            None => reply_outcome(&reply),
        };
        let mut line = settle(outcome);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        line["focus"] = serde_json::Value::from(focus);
        log_json_at(log_path, line);
        return;
    }
    if mark_sticky {
        state.sticky.insert(member_key.clone(), false);
    }
    let focus = retain_float_focus_validated(state, me, fulls, expected, member_key);
    let writable = writable_tokens(state, &loc.output, &loc.workspace, observed);
    let summary = apply_geometry(
        state,
        ApplyInput {
            me,
            fulls,
            reply: &reply,
            observed,
            op,
            tick,
            correlation: correlation.as_str(),
            skipped,
            writable: &writable,
            output_token: state.workspaces.output_token(&loc.output),
            workspace_token: state
                .workspaces
                .workspace_token(&loc.output, &loc.workspace),
            revision: revision_for(state, &loc.output, &loc.workspace),
        },
    );
    let outcome: &'static str = match summary {
        Some(s) if !s.readback_ok => "float-unverified",
        Some(s) if s.mismatched > 0 || actual != effective => "float-mismatch",
        Some(_) => applied,
        None => reply_outcome(&reply),
    };
    let mut line = settle(outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    line["window"] = serde_json::Value::from(from.to_owned());
    line["target"] = serde_json::Value::from(target);
    line["focus"] = serde_json::Value::from(focus);
    log_json_at(log_path, line);
}

/// Restore a project-raised topmost band under held gates before an unfloat.
/// Returns `None` after logging a failure; otherwise whether a restore ran.
#[allow(clippy::too_many_arguments)]
fn restore_raised_band(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    observed: &[ObservedWindow],
    member_key: &crate::workspace::WindowKey,
    from: &str,
    target: &str,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
    tick: u64,
    op: &'static str,
) -> Option<bool> {
    let Some(prior) = state.float_topmost_prev.get(member_key).copied() else {
        return Some(false);
    };
    let current = read_topmost_now(member_key.hwnd);
    if !crate::tiling::float_topmost_restore_needed(prior, current) {
        state.float_topmost_prev.remove(member_key);
        return Some(true);
    }
    let held = match observed.iter().find(|w| w.hwnd == member_key.hwnd) {
        Some(expected) => {
            let member_tag = state.member_tags.get(member_key).map(String::as_str);
            match revalidate_target(
                expected,
                me,
                fulls,
                &mut state.tokens,
                state.allowlist.is_some(),
                state.allowlist.as_ref(),
                &state.scope,
                member_tag,
                &state.scope_hosts,
                false,
            ) {
                Ok(target) => target.held,
                Err(reason) => {
                    let mut line = settle(reason);
                    line["origin"] = serde_json::Value::from(origin.token.clone());
                    line["window"] = serde_json::Value::from(from.to_owned());
                    line["target"] = serde_json::Value::from(target);
                    log_json_at(log_path, line);
                    return None;
                }
            }
        }
        None => match hold_band_target(state, me, member_key) {
            Some(held) => held,
            None => {
                let mut line = settle("identity-changed");
                line["origin"] = serde_json::Value::from(origin.token.clone());
                line["window"] = serde_json::Value::from(from.to_owned());
                line["target"] = serde_json::Value::from(target);
                log_json_at(log_path, line);
                return None;
            }
        },
    };
    let restored = set_topmost_band(member_key.hwnd, prior)
        && read_topmost_now(member_key.hwnd) == prior
        && pid_current(member_key.hwnd, member_key.pid);
    let _ = &held;
    if state.allowlist.is_some() {
        audit_json(
            state,
            serde_json::json!({
                "event": "proof-topmost",
                "tick": tick,
                "op": op,
                "window": from,
                "restored": restored,
            }),
        );
    }
    if !restored {
        let mut line = settle("topmost-unverified");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        line["topmost_restored"] = serde_json::Value::from(false);
        log_json_at(log_path, line);
        return None;
    }
    state.float_topmost_prev.remove(member_key);
    Some(true)
}

/// Clear one sticky mark under the fresh held gate with the guard kept
/// across the `RemoveProp` and its readback. The live value must still
/// equal the pre-sticky state and the pid plus lifetime tag must stay fresh
/// before and after. A verifiably absent marker (destroyed HWND or no
/// property) needs no native write, so runtime may still drop without the
/// hold; a present marker without the hold stays for later recovery. Logs
/// the failure and returns false without mutating.
#[allow(clippy::too_many_arguments)]
fn clear_sticky_mark(
    state: &TileLoop,
    me: &ProcessIdentity,
    member_key: &crate::workspace::WindowKey,
    expected_prior: bool,
    from: &str,
    target: &str,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
) -> bool {
    let Some(held) = hold_marker_target(state, me, member_key) else {
        if unsafe { IsWindow(member_key.hwnd as isize as HWND) } == 0
            || crate::product_hide::sys::read_sticky_marker(member_key.hwnd).is_none()
        {
            return true;
        }
        let mut line = settle("identity-changed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return false;
    };
    if !marker_tag_pid_fresh(state, member_key) {
        let _ = &held;
        let mut line = settle("identity-changed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return false;
    }
    let result = crate::product_hide::sys::remove_sticky_marker(member_key.hwnd, expected_prior);
    let fresh = marker_tag_pid_fresh(state, member_key);
    let _ = &held;
    match result {
        Ok(true) if fresh => true,
        Ok(true) => {
            let mut line = settle("identity-changed");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            log_json_at(log_path, line);
            false
        }
        Ok(false) => {
            let mut line = settle("identity-changed");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            log_json_at(log_path, line);
            false
        }
        Err(_) => {
            let mut line = settle("sticky-unverified");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            log_json_at(log_path, line);
            false
        }
    }
}

/// Clear one intentional-float mark under the fresh held gate with the guard
/// kept across the `RemoveProp` and its readback, on settled unfloat
/// (ordinary unfloat, sticky-off to tile, cross-domain rehome to tile).
/// Corrupt residue on our own property name cleans with the same call (it is
/// inert either way) under the same full identity plus lifetime fences. A
/// verifiably absent marker (destroyed HWND or no property) needs no native
/// write, so runtime may still drop without the hold; a present marker
/// without the hold stays for later recovery. Logs the failure and returns
/// false without mutating.
#[allow(clippy::too_many_arguments)]
fn clear_float_mark(
    state: &TileLoop,
    me: &ProcessIdentity,
    member_key: &crate::workspace::WindowKey,
    from: &str,
    target: &str,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
) -> bool {
    let Some(held) = hold_marker_target(state, me, member_key) else {
        if unsafe { IsWindow(member_key.hwnd as isize as HWND) } == 0
            || !crate::product_hide::sys::read_float_intent_marker(member_key.hwnd)
        {
            return true;
        }
        let mut line = settle("identity-changed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return false;
    };
    if !marker_tag_pid_fresh(state, member_key) {
        let _ = &held;
        let mut line = settle("identity-changed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return false;
    }
    let result = crate::product_hide::sys::remove_float_intent_marker(member_key.hwnd);
    let fresh = marker_tag_pid_fresh(state, member_key);
    let _ = &held;
    match result {
        Ok(()) if fresh => true,
        Ok(()) => {
            let mut line = settle("identity-changed");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            log_json_at(log_path, line);
            false
        }
        Err(_) => {
            let mut line = settle("float-unverified");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            log_json_at(log_path, line);
            false
        }
    }
}

/// Shared float-to-tiled transition: project-raised band restored first under
/// held gates, classification markers cleared between the band and the
/// commit, Engine commit, sibling reflow, exact focus retained. With
/// `unmark_sticky` the sticky mark clears first (sticky-off to tile); the
/// intentional-float mark always clears here (settled unfloat, including a
/// prior-float sticky tiling).
#[allow(clippy::too_many_arguments)]
fn apply_unfloat_to_tiled(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    skipped: Vec<(String, String)>,
    member_key: &crate::workspace::WindowKey,
    from: &str,
    loc: &crate::workspace::MemberLoc,
    live_rect: Option<Rect>,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
    tick: u64,
    correlation: &CorrelationId,
    op: &'static str,
    target: &'static str,
    unmark_sticky: bool,
) {
    let float_rect = live_rect;
    let Some((reply, candidate)) = prepare_toggle_float(
        state,
        areas,
        observed,
        retained,
        op,
        from,
        loc,
        float_rect,
        origin,
        settle,
        log_path,
        correlation,
        target,
    ) else {
        return;
    };
    let Some(topmost_restored) = restore_raised_band(
        state, me, fulls, observed, member_key, from, target, origin, settle, log_path, tick, op,
    ) else {
        return;
    };
    if unmark_sticky {
        let expected_prior = state.sticky.get(member_key).copied().unwrap_or(false);
        if !clear_sticky_mark(
            state,
            me,
            member_key,
            expected_prior,
            from,
            target,
            origin,
            settle,
            log_path,
        ) {
            return;
        }
    }
    if !clear_float_mark(
        state, me, member_key, from, target, origin, settle, log_path,
    ) {
        return;
    }
    state.engine = candidate;
    state.sticky.remove(member_key);
    state.floated.remove(member_key);
    state.float_rects.remove(from);
    let focus = match observed.iter().find(|w| w.hwnd == member_key.hwnd) {
        Some(expected) => retain_float_focus_validated(state, me, fulls, expected, member_key),
        None => "float-focus-failed",
    };
    let writable = writable_tokens(state, &loc.output, &loc.workspace, observed);
    let summary = apply_geometry(
        state,
        ApplyInput {
            me,
            fulls,
            reply: &reply,
            observed,
            op,
            tick,
            correlation: correlation.as_str(),
            skipped,
            writable: &writable,
            output_token: state.workspaces.output_token(&loc.output),
            workspace_token: state
                .workspaces
                .workspace_token(&loc.output, &loc.workspace),
            revision: revision_for(state, &loc.output, &loc.workspace),
        },
    );
    let outcome: &'static str = match summary {
        Some(s) if !s.readback_ok => "unfloat-unverified",
        Some(s) if s.mismatched > 0 => "unfloat-mismatch",
        Some(_) => "unfloat-applied",
        None => reply_outcome(&reply),
    };
    let mut line = settle(outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    line["window"] = serde_json::Value::from(from.to_owned());
    line["target"] = serde_json::Value::from(target);
    line["focus"] = serde_json::Value::from(focus);
    line["topmost_restored"] = serde_json::Value::from(topmost_restored);
    log_json_at(log_path, line);
}

/// Win+G float toggle (KDE Meta+G parity): fresh observation, exact origin
/// re-resolution, full gates, one Engine `ToggleFloat` evaluated on a local
/// candidate that commits only after the target native effects verify, then
/// sibling reflow through the shared write path. No compensating toggle.
fn dispatch_float_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: crate::snapkey::QueuedFloatIntent,
) {
    let log_path = state.log_path.clone();
    // Key-ups close the pair and passed chords never dispatch: trace-only so
    // held-key traffic stays out of normal logs.
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "float-toggle",
                    "tick": state.tick,
                    "edge": intent.edge.as_str(),
                    "disposition": if intent.consumed { "consumed" } else { "passed" },
                    "outcome": if intent.consumed { "key-up" } else { "passed" },
                }),
            );
        }
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "float-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    let Some(origin) = intent.origin.clone() else {
        state.snap_advance = None;
        let line = settle("origin-vanished");
        log_json_at(&log_path, line);
        return;
    };
    // Fresh per-intent suspension/elevation fence (workspace_tick parity):
    // newly intercepted suspended/elevated chords settle here with no side
    // effects. The suspend read exempts verified managed overlays; the target
    // revalidation below refuses protected windows.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        let mut line = settle(outcome);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let Some(chord) = observe_chord_intent(state, me, fulls, areas) else {
        let mut line = settle("observation-failed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let ChordObserved {
        observed,
        retained,
        skipped,
    } = chord;
    let ChordTarget {
        from,
        member_key,
        loc,
        ..
    } = match resolve_chord_target(state, me, areas, &origin) {
        Ok(target) => target,
        Err(reject) => {
            settle_chord_reject(&log_path, &settle, &origin, &reject);
            return;
        }
    };
    let is_float = engine_is_float(state, &loc.output, &loc.workspace, &from);
    // Overlay fences on live state: overlay targets never float, and a
    // natively overlaid float never unfloats until it reads normal again.
    // Sticky origins refuse with the sticky vocabulary (KDE parity).
    // Floating workspaces take no per-window float toggles: the domain is
    // released and every frame stays native.
    if !workspace_mode_tiled(state, &loc.output, &loc.workspace) {
        state.snap_advance = None;
        let mut line = settle("workspace-floating");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Overlay fences on live state: overlay targets never float, and a
    // natively overlaid float never unfloats until it reads normal again.
    // Sticky origins refuse with the sticky vocabulary (KDE parity).
    let is_sticky_origin = state.sticky.contains_key(&member_key);
    if let Some(refusal) =
        chord_overlay_refusal(is_sticky_origin, &observed, &retained, &member_key)
    {
        let mut line = settle(refusal);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let live_visible = by_hwnd.get(&member_key.hwnd).map(|w| w.visible);
    // Win+G on any sticky origin clears sticky then tiles the CURRENT
    // workspace. Prior float or tiled both tile.
    if is_sticky_origin {
        sticky_off_to_current(
            state,
            me,
            fulls,
            areas,
            &observed,
            &retained,
            &member_key,
            &from,
            &loc,
            &origin,
            &settle,
            &log_path,
            &correlation,
            StickyOffMode::Tile,
        );
        return;
    }
    // Unfloat carries the live frame rect so a moved/resized float is
    // retained; float carries no rect so the Engine picks the retained
    // placement, else the centered fallback. Frameless floats unfloat with no
    // rect and keep the Engine-retained placement.
    let live_rect = live_visible.or_else(|| {
        retained
            .iter()
            .find(|r| r.key == member_key)
            .and_then(|row| row.rect)
    });
    if is_float {
        apply_unfloat_to_tiled(
            state,
            me,
            fulls,
            areas,
            &observed,
            &retained,
            skipped,
            &member_key,
            &from,
            &loc,
            live_rect,
            &origin,
            &settle,
            &log_path,
            tick,
            &correlation,
            "float",
            "unfloated",
            false,
        );
        return;
    }
    apply_float_from_tiled(
        state,
        me,
        fulls,
        areas,
        &observed,
        &retained,
        skipped,
        &member_key,
        &from,
        &loc,
        &origin,
        &settle,
        &log_path,
        tick,
        &correlation,
        "float",
        "floated",
        "float-applied",
        false,
    );
}

/// Win+O orientation toggle (KDE Meta+O parity, item 4 R-LAY-01): fresh
/// observation, exact origin re-resolution, full gates, one Engine
/// `ToggleOrientation` on the retained session, then sibling reflow through
/// the shared write path. No focus or view mutation: focus stays on the same
/// window and the workspace never switches. Lone root, float/sticky
/// subjects, missing focus, floating workspaces, and focused overlays settle
/// with no writes; sibling overlays reproject as reserved slots and skip
/// native writes through the existing writable fence.
fn dispatch_orientation_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: crate::snapkey::QueuedOrientationIntent,
) {
    let log_path = state.log_path.clone();
    // Key-ups close the pair and passed chords never dispatch: trace-only so
    // held-key traffic stays out of normal logs.
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "orientation-toggle",
                    "tick": state.tick,
                    "edge": intent.edge.as_str(),
                    "disposition": if intent.consumed { "consumed" } else { "passed" },
                    "outcome": if intent.consumed { "key-up" } else { "passed" },
                }),
            );
        }
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "orientation-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    let Some(origin) = intent.origin.clone() else {
        state.snap_advance = None;
        let line = settle("origin-vanished");
        log_json_at(&log_path, line);
        return;
    };
    // Fresh per-intent suspension/elevation fence (workspace_tick parity):
    // newly intercepted suspended/elevated chords settle here with no side
    // effects. The suspend read exempts verified managed overlays, and the
    // target revalidation below refuses protected windows.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        let mut line = settle(outcome);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let Some(chord) = observe_chord_intent(state, me, fulls, areas) else {
        let mut line = settle("observation-failed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let ChordObserved {
        observed,
        retained,
        skipped,
    } = chord;
    let ChordTarget {
        from,
        member_key,
        loc,
        ..
    } = match resolve_chord_target(state, me, areas, &origin) {
        Ok(target) => target,
        Err(reject) => {
            settle_chord_reject(&log_path, &settle, &origin, &reject);
            return;
        }
    };
    // Floating workspaces run no tile layout: the toggle settles with no
    // writes and every frame stays native.
    if !workspace_mode_tiled(state, &loc.output, &loc.workspace) {
        state.snap_advance = None;
        let mut line = settle("workspace-floating");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Float/sticky subjects hold no tile slot: the toggle settles with no
    // writes, never retiles them.
    if state.sticky.contains_key(&member_key) {
        let mut line = settle("orientation-refused-sticky");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    if engine_is_float(state, &loc.output, &loc.workspace, &from) {
        let mut line = settle("orientation-refused-floating");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Focused overlays refuse before any Engine mutation: a maximized or
    // fullscreen focused window keeps its native state and its reserved slot.
    if let Some(refusal) = orientation_overlay_refusal(&observed, &retained, &member_key) {
        let mut line = settle(refusal);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    let mut hint_cx = HintCx::new();
    let Some(rows) = assemble_domain_rows(
        state,
        &loc.output,
        &loc.workspace,
        &observed,
        &retained,
        "orientation",
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        let mut line = settle("deferred");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    if !rows.iter().any(|r| r.token == from) {
        let mut line = settle("unmanaged");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    }
    let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let fp = fingerprint(
        &rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect::<Vec<_>>(),
    );
    let from_id = WindowId(from.clone());
    let Some((domain, domain_key)) = workspace_domain_for(
        &loc.output,
        &loc.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        let mut line = settle("unknown-output");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    // Live gap edits adopt here so the toggle converges against matching gaps
    // instead of refusing the keypress.
    {
        let revision = revision_for(state, &loc.output, &loc.workspace);
        let outer_gap = state.outer_gap;
        adopt_gaps_for_route(
            state,
            &domain,
            &domain_key,
            outer_gap,
            &windows,
            Some(&from_id),
            revision,
            fp,
            &correlation,
        );
    }
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        revision_for(state, &loc.output, &loc.workspace),
        fp,
        &domain,
        &domain_key,
        state.outer_gap,
        &windows,
        Some(&from_id),
    );
    event.command = CoreCommand::ToggleOrientation {
        window: from.clone(),
    };
    let reply = state.engine.handle(&event);
    if orientation_plan(&reply).is_none() {
        // Lone root (`unchanged`), float/no-focus (`not-tiled`), and every
        // other refusal settle here with no writes and no focus mutation.
        let mut line = settle(reply_outcome(&reply));
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    let writable = writable_tokens(state, &loc.output, &loc.workspace, &observed);
    let summary = apply_geometry(
        state,
        ApplyInput {
            me,
            fulls,
            reply: &reply,
            observed: &observed,
            op: "orientation",
            tick,
            correlation: correlation.as_str(),
            skipped,
            writable: &writable,
            output_token: state.workspaces.output_token(&loc.output),
            workspace_token: state
                .workspaces
                .workspace_token(&loc.output, &loc.workspace),
            revision: revision_for(state, &loc.output, &loc.workspace),
        },
    );
    // Sibling overlays reprojected as reserved slots skip native writes
    // through the writable fence above; only verified writes count here.
    // Focus stays on the same window: no focus setter, no view switch.
    let outcome = match summary {
        Some(s) if !s.readback_ok => "orientation-unverified",
        Some(s) if s.mismatched > 0 => "orientation-mismatch",
        Some(s) if s.applied > 0 => "orientation-applied",
        Some(_) => "orientation-noop",
        None => reply_outcome(&reply),
    };
    let mut line = settle(outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    line["window"] = serde_json::Value::from(from.clone());
    log_json_at(&log_path, line);
}

/// Cross-domain sticky-off to the current workspace: drop the float exception
/// from the backing domain and admit on the current domain via two reconciles
/// on a local candidate, with no stale duplicate. With `floating` the window
/// stays a normal float preserving the live frame (no geometry writes);
/// otherwise it tiles current. Band restores under held gates for the tiled
/// destination; focus is retained in both.
#[allow(clippy::too_many_arguments)]
fn sticky_rehome_to_current(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    member_key: &crate::workspace::WindowKey,
    from: &str,
    loc: &crate::workspace::MemberLoc,
    current_ws: &str,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
    correlation: &CorrelationId,
    floating: bool,
) {
    let target = if floating { "float" } else { "unfloated" };
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let live_frame = by_hwnd
        .get(&member_key.hwnd)
        .map(|w| w.visible)
        .or_else(|| state.float_rects.get(from).copied())
        .or_else(|| {
            retained
                .iter()
                .find(|r| r.key == *member_key)
                .and_then(|row| row.rect)
        });
    let Some(live_frame) = live_frame else {
        let mut line = settle("deferred");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        log_json_at(log_path, line);
        return;
    };
    let mut hint_cx = HintCx::new();
    let Some(old_rows) = assemble_domain_rows(
        state,
        &loc.output,
        &loc.workspace,
        observed,
        retained,
        "sticky",
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        let mut line = settle("deferred");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(log_path, line);
        return;
    };
    let Some((old_domain, old_key)) = workspace_domain_for(
        &loc.output,
        &loc.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        let mut line = settle("unknown-output");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(log_path, line);
        return;
    };
    let old_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = old_rows
        .iter()
        .filter(|r| r.token != from)
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let old_fp = fingerprint(
        &old_windows
            .iter()
            .map(|(w, r, _, _)| (w.0.clone(), *r))
            .collect::<Vec<_>>(),
    );
    let Some((cur_domain, cur_key)) = workspace_domain_for(
        &loc.output,
        current_ws,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        let mut line = settle("unknown-output");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(log_path, line);
        return;
    };
    let mut hint_cx = HintCx::new();
    let Some(cur_rows) = assemble_domain_rows(
        state,
        &loc.output,
        current_ws,
        observed,
        retained,
        "sticky",
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        let mut line = settle("deferred");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(log_path, line);
        return;
    };
    let mut cur_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = cur_rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    cur_windows.push((
        WindowId(from.to_owned()),
        live_frame,
        WindowSizeHints::none(),
        floating,
    ));
    let cur_fp = fingerprint(
        &cur_windows
            .iter()
            .map(|(w, r, _, _)| (w.0.clone(), *r))
            .collect::<Vec<_>>(),
    );
    let mut candidate = state.engine.clone();
    let old_event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        correlation,
        revision_for(state, &loc.output, &loc.workspace),
        old_fp,
        &old_domain,
        &old_key,
        OUTER_GAP,
        &old_windows,
        None,
    );
    let old_reply = candidate.handle(&old_event);
    if matches!(
        old_reply,
        CoreReply::Rejected { .. } | CoreReply::Diverged(_) | CoreReply::SnapshotInvalid { .. }
    ) {
        let mut line = settle(reply_outcome(&old_reply));
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    }
    let cur_event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        correlation,
        revision_for(state, &loc.output, current_ws),
        cur_fp,
        &cur_domain,
        &cur_key,
        OUTER_GAP,
        &cur_windows,
        None,
    );
    let cur_reply = candidate.handle(&cur_event);
    if matches!(
        cur_reply,
        CoreReply::Rejected { .. } | CoreReply::Diverged(_) | CoreReply::SnapshotInvalid { .. }
    ) {
        let mut line = settle(reply_outcome(&cur_reply));
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    }
    let topmost_restored = if floating {
        true
    } else {
        let Some(restored) = restore_raised_band(
            state, me, fulls, observed, member_key, from, target, origin, settle, log_path,
            state.tick, "sticky",
        ) else {
            return;
        };
        restored
    };
    let expected_prior = state.sticky.get(member_key).copied().unwrap_or(false);
    if !clear_sticky_mark(
        state,
        me,
        member_key,
        expected_prior,
        from,
        target,
        origin,
        settle,
        log_path,
    ) {
        return;
    }
    // Settled rehome to tile clears a carried float marker too (a prior-float
    // sticky tiling); rehome to float keeps (or repairs) it below.
    if !floating
        && !clear_float_mark(
            state, me, member_key, from, target, origin, settle, log_path,
        )
    {
        return;
    }
    state.engine = candidate;
    if floating {
        // Sticky-off preserving a prior float: the float marker rides on.
        // Repair it when a foreign value crept onto our property; a failed
        // repair keeps local intent and native state with a degraded log.
        let float_ok = crate::product_hide::sys::read_float_intent_marker(member_key.hwnd)
            || install_float_mark_held(state, me, member_key);
        state.float_rects.insert(from.to_owned(), live_frame);
        state.floated.insert(member_key.clone());
        if !float_ok {
            let focus = match observed.iter().find(|w| w.hwnd == member_key.hwnd) {
                Some(expected) => {
                    retain_float_focus_validated(state, me, fulls, expected, member_key)
                }
                None => "float-focus-failed",
            };
            let mut line = settle("float-unverified");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            line["focus"] = serde_json::Value::from(focus);
            log_json_at(log_path, line);
            state.sticky.remove(member_key);
            state.workspaces.remove_window(member_key);
            state.workspaces.ensure_output(&loc.output);
            state
                .workspaces
                .assign(member_key.clone(), &loc.output, current_ws, false);
            return;
        }
    } else {
        state.float_topmost_prev.remove(member_key);
        state.floated.remove(member_key);
        state.float_rects.remove(from);
    }
    state.sticky.remove(member_key);
    state.workspaces.remove_window(member_key);
    state.workspaces.ensure_output(&loc.output);
    state
        .workspaces
        .assign(member_key.clone(), &loc.output, current_ws, false);
    let focus = match observed.iter().find(|w| w.hwnd == member_key.hwnd) {
        Some(expected) => retain_float_focus_validated(state, me, fulls, expected, member_key),
        None => "float-focus-failed",
    };
    if floating {
        let mut line = settle("sticky-applied");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        line["focus"] = serde_json::Value::from(focus);
        log_json_at(log_path, line);
        return;
    }
    let writable = writable_tokens(state, &loc.output, current_ws, observed);
    let summary = apply_geometry(
        state,
        ApplyInput {
            me,
            fulls,
            reply: &cur_reply,
            observed,
            op: "sticky",
            tick: state.tick,
            correlation: correlation.as_str(),
            skipped: Vec::new(),
            writable: &writable,
            output_token: state.workspaces.output_token(&loc.output),
            workspace_token: state.workspaces.workspace_token(&loc.output, current_ws),
            revision: revision_for(state, &loc.output, current_ws),
        },
    );
    let outcome: &'static str = match summary {
        Some(s) if !s.readback_ok => "unfloat-unverified",
        Some(s) if s.mismatched > 0 => "unfloat-mismatch",
        Some(_) => "unfloat-applied",
        None => reply_outcome(&cur_reply),
    };
    let mut line = settle(outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    line["window"] = serde_json::Value::from(from.to_owned());
    line["target"] = serde_json::Value::from(target);
    line["focus"] = serde_json::Value::from(focus);
    line["topmost_restored"] = serde_json::Value::from(topmost_restored);
    log_json_at(log_path, line);
}

/// Win+Shift+G sticky toggle: origin-sensitive off to the current workspace
/// (prior float stays float, prior tiled tiles), sticky-on from float marks
/// only, sticky-on from tiled floats first via the shared transition.
/// Fullscreen/maximized refuse; failures commit nothing and never retry.
fn dispatch_sticky_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: crate::snapkey::QueuedStickyIntent,
) {
    let log_path = state.log_path.clone();
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "sticky-toggle",
                    "tick": state.tick,
                    "edge": intent.edge.as_str(),
                    "disposition": if intent.consumed { "consumed" } else { "passed" },
                    "outcome": if intent.consumed { "key-up" } else { "passed" },
                }),
            );
        }
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "sticky-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    let Some(origin) = intent.origin.clone() else {
        state.snap_advance = None;
        let line = settle("origin-vanished");
        log_json_at(&log_path, line);
        return;
    };
    // Fresh per-intent suspension/elevation fence (workspace_tick parity):
    // newly intercepted suspended/elevated chords settle here with no side
    // effects; the sticky mark and Engine commit below only run for a live
    // managed member.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        let mut line = settle(outcome);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let Some(chord) = observe_chord_intent(state, me, fulls, areas) else {
        let mut line = settle("observation-failed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let ChordObserved {
        observed,
        retained,
        skipped,
    } = chord;
    let ChordTarget {
        from,
        member_key,
        loc,
        ..
    } = match resolve_chord_target(state, me, areas, &origin) {
        Ok(target) => target,
        Err(reject) => {
            settle_chord_reject(&log_path, &settle, &origin, &reject);
            return;
        }
    };
    if let Some(refusal) = chord_overlay_refusal(true, &observed, &retained, &member_key) {
        let mut line = settle(refusal);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Floating workspaces take no per-window sticky toggles: the domain is
    // released and every frame stays native.
    if !workspace_mode_tiled(state, &loc.output, &loc.workspace) {
        state.snap_advance = None;
        let mut line = settle("workspace-floating");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    if state.sticky.contains_key(&member_key) {
        sticky_off_to_current(
            state,
            me,
            fulls,
            areas,
            &observed,
            &retained,
            &member_key,
            &from,
            &loc,
            &origin,
            &settle,
            &log_path,
            &correlation,
            StickyOffMode::PreservePrior,
        );
        return;
    }
    let is_float = engine_is_float(state, &loc.output, &loc.workspace, &from);
    if is_float {
        // Sticky-on from a normal float: no Engine change, live frame
        // preserved, only the mark. Marker install verifies before the
        // runtime map changes, so a failed write leaves no partial state.
        if !install_sticky_mark_held(state, me, &member_key, true) {
            let mut line = settle("sticky-unverified");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.clone());
            log_json_at(&log_path, line);
            return;
        }
        state.sticky.insert(member_key.clone(), true);
        let focus = match observed.iter().find(|w| w.hwnd == member_key.hwnd) {
            Some(expected) => retain_float_focus_validated(state, me, fulls, expected, &member_key),
            None => "float-focus-failed",
        };
        let mut line = settle("sticky-applied");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        line["target"] = serde_json::Value::from("sticky-float");
        line["focus"] = serde_json::Value::from(focus);
        log_json_at(&log_path, line);
        return;
    }
    apply_float_from_tiled(
        state,
        me,
        fulls,
        areas,
        &observed,
        &retained,
        skipped,
        &member_key,
        &from,
        &loc,
        &origin,
        &settle,
        &log_path,
        tick,
        &correlation,
        "sticky",
        "sticky-float",
        "sticky-applied",
        true,
    );
}

/// Sticky-off destination: Win+Shift+G preserves a prior float on the current
/// workspace, while Win+G always tiles the current workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StickyOffMode {
    PreservePrior,
    Tile,
}

/// Sticky-off to the current workspace: with `PreservePrior` a prior float
/// stays a normal float preserving the live frame, otherwise the window tiles
/// current. Same-domain tiles via the shared unfloat; cross-domain rehomes.
#[allow(clippy::too_many_arguments)]
fn sticky_off_to_current(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    member_key: &crate::workspace::WindowKey,
    from: &str,
    loc: &crate::workspace::MemberLoc,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
    correlation: &CorrelationId,
    mode: StickyOffMode,
) {
    let prior = state.sticky.get(member_key).copied().unwrap_or(false);
    let to_float = mode == StickyOffMode::PreservePrior && prior;
    let Some(current_ws) = state.workspaces.active_id(&loc.output) else {
        let mut line = settle("unknown-output");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        log_json_at(log_path, line);
        return;
    };
    if to_float {
        if current_ws == loc.workspace {
            // Settling to an ordinary float keeps the float marker: repair
            // it first under held gates, so a failed repair mutates nothing
            // and the window stays sticky with its marker intact.
            if !crate::product_hide::sys::read_float_intent_marker(member_key.hwnd)
                && !install_float_mark_held(state, me, member_key)
            {
                let focus = match observed.iter().find(|w| w.hwnd == member_key.hwnd) {
                    Some(expected) => {
                        retain_float_focus_validated(state, me, fulls, expected, member_key)
                    }
                    None => "float-focus-failed",
                };
                let mut line = settle("float-unverified");
                line["origin"] = serde_json::Value::from(origin.token.clone());
                line["window"] = serde_json::Value::from(from.to_owned());
                line["target"] = serde_json::Value::from("float");
                line["focus"] = serde_json::Value::from(focus);
                log_json_at(log_path, line);
                return;
            }
            if clear_sticky_mark(
                state, me, member_key, true, from, "float", origin, settle, log_path,
            ) {
                state.sticky.remove(member_key);
                let focus = match observed.iter().find(|w| w.hwnd == member_key.hwnd) {
                    Some(expected) => {
                        retain_float_focus_validated(state, me, fulls, expected, member_key)
                    }
                    None => "float-focus-failed",
                };
                let mut line = settle("sticky-applied");
                line["origin"] = serde_json::Value::from(origin.token.clone());
                line["window"] = serde_json::Value::from(from.to_owned());
                line["target"] = serde_json::Value::from("float");
                line["focus"] = serde_json::Value::from(focus);
                log_json_at(log_path, line);
            }
            return;
        }
        sticky_rehome_to_current(
            state,
            me,
            fulls,
            areas,
            observed,
            retained,
            member_key,
            from,
            loc,
            &current_ws,
            origin,
            settle,
            log_path,
            correlation,
            true,
        );
        return;
    }
    if current_ws == loc.workspace {
        let live_rect = observed
            .iter()
            .find(|w| w.hwnd == member_key.hwnd)
            .map(|w| w.visible)
            .or_else(|| {
                retained
                    .iter()
                    .find(|r| r.key == *member_key)
                    .and_then(|row| row.rect)
            });
        apply_unfloat_to_tiled(
            state,
            me,
            fulls,
            areas,
            observed,
            retained,
            Vec::new(),
            member_key,
            from,
            loc,
            live_rect,
            origin,
            settle,
            log_path,
            state.tick,
            correlation,
            "sticky",
            "unfloated",
            true,
        );
        return;
    }
    sticky_rehome_to_current(
        state,
        me,
        fulls,
        areas,
        observed,
        retained,
        member_key,
        from,
        loc,
        &current_ws,
        origin,
        settle,
        log_path,
        correlation,
        false,
    );
}

/// Stable output key for a rectangle center: the monitor containing the
/// center wins; otherwise the largest overlap; otherwise the primary.
/// Rectangles decide placement, device names decide session identity.
fn output_for_rect(areas: &[MonitorArea], rect: &Rect) -> String {
    let cx = rect.x + rect.w / 2;
    let cy = rect.y + rect.h / 2;
    for area in areas {
        let r = area.full;
        if cx >= r.x && cx < r.x + r.w && cy >= r.y && cy < r.y + r.h {
            return area.device.clone();
        }
    }
    let mut best: Option<(&MonitorArea, i64)> = None;
    for area in areas {
        let r = area.full;
        let ox = (rect.x.min(r.x + r.w) - rect.x.max(r.x)).max(0) as i64;
        let oy = (rect.y.min(r.y + r.h) - rect.y.max(r.y)).max(0) as i64;
        let overlap = ox * oy;
        if best.as_ref().is_none_or(|(_, d)| overlap > *d) {
            best = Some((area, overlap));
        }
    }
    best.map(|(a, _)| a.device.clone())
        .or_else(|| areas.first().map(|a| a.device.clone()))
        .unwrap_or_else(|| "display-?".to_owned())
}

fn output_for_point(areas: &[MonitorArea], x: i32, y: i32) -> Option<String> {
    for area in areas {
        let r = area.full;
        if x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h {
            return Some(area.device.clone());
        }
    }
    None
}

fn foreground_output(areas: &[MonitorArea]) -> Option<String> {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return None;
    }
    let mut raw: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(foreground, &mut raw) } == 0 {
        return None;
    }
    rect_from_win(raw).map(|r| output_for_rect(areas, &r))
}

/// Exact output context for a digit chord: cached active output when known,
/// else the foreground monitor, else the pointer monitor, else ordering.
fn chord_output(state: &TileLoop, areas: &[MonitorArea]) -> Option<String> {
    let cached = if state.active_output.is_empty() {
        None
    } else {
        Some(state.active_output.as_str())
    };
    let foreground = foreground_output(areas);
    let pointer = cursor_pos().and_then(|(x, y)| output_for_point(areas, x, y));
    crate::workspace_owner::output_context(
        &state.workspaces,
        cached,
        foreground.as_deref(),
        pointer.as_deref(),
    )
}

/// Engine revision for one `(output, workspace)` domain.
fn revision_for(state: &TileLoop, output: &str, workspace: &str) -> u64 {
    let key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(workspace.to_owned()),
    };
    state
        .engine
        .session(&key)
        .map(|s| s.accepted_revision())
        .unwrap_or(0)
}

/// Portable domain value for one workspace on its monitor's work area.
/// Gaps ride the owner's live settings (inner gap in the domain, outer gap
/// in both the inset bounds and the carried event value).
fn workspace_domain_for(
    output: &str,
    workspace: &str,
    areas: &[MonitorArea],
    inner_gap: i32,
    outer_gap: i32,
) -> Option<(
    tiler_core::session::OutputDomain,
    tiler_core::session::DomainKey,
)> {
    let area = areas.iter().find(|a| a.device == output)?;
    let bounds = tiling_domain_bounds_with(area.work, outer_gap)?;
    Some(crate::workspace_owner::workspace_domain(
        output, workspace, bounds, inner_gap,
    ))
}

/// Adopt drifted live gaps through the retained topology-preserving
/// update-gaps path before a domain operation (focus/move/select/send/
/// gesture). A live gap edit between the last reconcile and this intent would
/// otherwise make the operation's own convergence refuse with
/// `domain-mismatch` (safe, but the keypress is lost) or, on a domain the
/// Engine has never seen, seed fresh with mixed gap generations. Best effort:
/// returns true when the retained gaps now match the carried values; false
/// leaves the caller's Engine outcome (refusal or fresh seed) as the honest
/// evidence and topology is never destroyed either way.
#[allow(clippy::too_many_arguments)]
fn adopt_gaps_for_route(
    state: &mut TileLoop,
    domain: &tiler_core::session::OutputDomain,
    domain_key: &tiler_core::session::DomainKey,
    outer_gap: i32,
    windows: &[(WindowId, Rect, WindowSizeHints, bool)],
    focused: Option<&WindowId>,
    revision: u64,
    fingerprint: u64,
    correlation: &CorrelationId,
) -> bool {
    if crate::settings::retained_gaps_match(&state.engine, domain_key, state.inner_gap, outer_gap) {
        return true;
    }
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        correlation,
        revision,
        fingerprint,
        domain,
        domain_key,
        outer_gap,
        windows,
        focused,
    );
    event.command = CoreCommand::UpdateGaps;
    let adopted = matches!(
        state.engine.handle(&event),
        CoreReply::Projection(_) | CoreReply::Tiled(_)
    );
    adopted
        && crate::settings::retained_gaps_match(
            &state.engine,
            domain_key,
            state.inner_gap,
            outer_gap,
        )
}

/// Admit first-seen fullscreen windows as born-held workspace members (KDE
/// initial-fullscreen-hold parity): managed membership with a slotless
/// synthetic floating Engine observation, never a tile slot. First-seen
/// applies regardless of app/project properties after a restart: a
/// restart-owned frame (our restoration metadata still on the window) is a
/// born hold like any first observed fullscreen, and the Win+F11 shortcut
/// still exits through those properties. A later fullscreen on a slotted
/// member or a lifetime-known non-fullscreen window is a managed overlay
/// transition, never born again. Membership carries the lifetime tag,
/// identity, and Engine token (supporting hide/reveal/recovery with no
/// native writes); floating observation is true only for the born hold, and
/// no `member_rects` slot is faked (the row rect / hidden snapshot carries
/// the native frame). Release happens on the first verified non-fullscreen
/// observation (or close), when normal fresh admission takes over; a first
/// exit to maximized converges through the existing admission clear.
fn admit_born_fullscreen(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    retained: &[RetainedRow],
    areas: &[MonitorArea],
) {
    let log_path = state.log_path.clone();
    for row in retained {
        if !row.fullscreen {
            continue;
        }
        let Some(frame) = row.rect else {
            continue;
        };
        if state.workspaces.member_loc(&row.key).is_some()
            || state.member_tokens.contains_key(&row.key)
            || state.hidden_claims.keys().any(|k| k.hwnd == row.key.hwnd)
        {
            continue;
        }
        let known_slot = state.member_rects.contains_key(&row.token);
        if !should_hold_born_fullscreen(
            true,
            known_slot,
            state.seen_nonfullscreen.contains(&row.key),
        ) {
            continue;
        }
        if holdable_key(state, me, row.key.hwnd).is_none_or(|key| key != row.key) {
            continue;
        }
        let live = match HeldProcess::open(row.key.pid).and_then(|held| held.identity()) {
            Ok(live) => live,
            Err(_) => continue,
        };
        if live.pid != row.key.pid || live.process_creation != row.key.creation {
            continue;
        }
        let stale = crate::workspace_owner::reused_hwnd_stale(
            &state.member_tokens.keys().cloned().collect::<Vec<_>>(),
            &state.hidden_claims.keys().cloned().collect(),
            &row.key,
        );
        for dead in stale {
            drop_member_state(state, &dead);
        }
        if state.workspaces.member_loc(&row.key).is_some()
            || state.member_tokens.contains_key(&row.key)
        {
            continue;
        }
        let fresh_tag = match crate::product_hide::sys::install_member_tag(row.key.hwnd, me.pid) {
            Ok(tag) => tag,
            Err(_) => continue,
        };
        let post_ok = crate::product_hide::sys::hold_target_verified(live.pid, &live).is_ok()
            && pid_current(row.key.hwnd, live.pid)
            && crate::product_hide::sys::read_member_tag(row.key.hwnd).as_deref()
                == Some(fresh_tag.as_str());
        if !post_ok {
            continue;
        }
        let token = row.token.clone();
        let output = output_for_rect(areas, &frame);
        state.workspaces.ensure_output(&output);
        let Some(active) = state.workspaces.active_id(&output) else {
            continue;
        };
        if state
            .workspaces
            .assign(row.key.clone(), &output, &active, false)
        {
            state.member_tokens.insert(row.key.clone(), token.clone());
            state.member_identity.insert(row.key.clone(), live);
            state.member_tags.insert(row.key.clone(), fresh_tag);
            // Hidden-snapshot seed only (never a tile slot): a born hold that
            // is later hidden needs an existing snapshot for row assembly,
            // while visible rows always use the fresh row rect. `known_slot`
            // checks treat born keys as slotless regardless of this seed, and
            // release drops it so a first exit to maximized clears normally.
            state.member_rects.insert(token.clone(), frame);
            if state.born_fullscreen.insert(row.key.clone()) {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "initial-fullscreen-held",
                        "window": token,
                    }),
                );
            }
        }
    }
}

/// Slotless maximized admission (R-MAX-03 / KDE floating parity): first-seen
/// maximized, non-fullscreen retained windows without workspace membership
/// and without a retained tile slot join the active workspace slotless:
/// membership plus stable token, identity, and lifetime tag for hide/reveal
/// and focus, but no `member_rects` tile slot, no Engine exception, and no
/// hold set. A floating workspace therefore preserves the native frame (the
/// mode-gated clear skips) while select-away/back still hides and reveals;
/// the first tiled admission clears once through the existing gate and the
/// refetched normal observation tiles normally. Same exact-lifetime, scope,
/// hosted-child, and proof fences as the born-fullscreen admission.
fn admit_slotless_maximized(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    retained: &[RetainedRow],
    areas: &[MonitorArea],
) {
    let log_path = state.log_path.clone();
    for row in retained {
        let is_member = state.workspaces.member_loc(&row.key).is_some()
            || state.member_tokens.contains_key(&row.key)
            || state.hidden_claims.keys().any(|k| k.hwnd == row.key.hwnd);
        if !crate::tiling::should_admit_slotless_maximized(
            row.maximized,
            row.fullscreen,
            is_member,
            state.member_rects.contains_key(&row.token),
        ) {
            continue;
        }
        let Some(frame) = row.rect else {
            continue;
        };
        if holdable_key(state, me, row.key.hwnd).is_none_or(|key| key != row.key) {
            continue;
        }
        let live = match HeldProcess::open(row.key.pid).and_then(|held| held.identity()) {
            Ok(live) => live,
            Err(_) => continue,
        };
        if live.pid != row.key.pid || live.process_creation != row.key.creation {
            continue;
        }
        let stale = crate::workspace_owner::reused_hwnd_stale(
            &state.member_tokens.keys().cloned().collect::<Vec<_>>(),
            &state.hidden_claims.keys().cloned().collect(),
            &row.key,
        );
        for dead in stale {
            drop_member_state(state, &dead);
        }
        if state.workspaces.member_loc(&row.key).is_some()
            || state.member_tokens.contains_key(&row.key)
        {
            continue;
        }
        let fresh_tag = match crate::product_hide::sys::install_member_tag(row.key.hwnd, me.pid) {
            Ok(tag) => tag,
            Err(_) => continue,
        };
        let post_ok = crate::product_hide::sys::hold_target_verified(live.pid, &live).is_ok()
            && pid_current(row.key.hwnd, live.pid)
            && crate::product_hide::sys::read_member_tag(row.key.hwnd).as_deref()
                == Some(fresh_tag.as_str());
        if !post_ok {
            continue;
        }
        let token = row.token.clone();
        let output = output_for_rect(areas, &frame);
        state.workspaces.ensure_output(&output);
        let Some(active) = state.workspaces.active_id(&output) else {
            continue;
        };
        if state
            .workspaces
            .assign(row.key.clone(), &output, &active, false)
        {
            state.member_tokens.insert(row.key.clone(), token.clone());
            state.member_identity.insert(row.key.clone(), live);
            state.member_tags.insert(row.key.clone(), fresh_tag);
            // R-MAX-06 latch, bound here after every ownership fence above
            // (exact-lifetime, scope, hosted-child, proof, fresh tag): only a
            // tiled-first admission under the live overlay value keeps the
            // overlay. Floating-first rows take the unchanged R-MAX-03 lane
            // (no latch, one-shot clear on retile under both values), and a
            // later live flip cannot move this lifetime.
            if crate::tiling::born_keep_latch(
                state.workspaces.is_tiled(&output, &active),
                born_maximized_keep_overlay_for(state.settings_live.as_ref()),
            ) {
                state.born_keep_overlay.insert(row.key.clone());
            }
            // No tile seed here: tiled row assembly reserves the overlay slot,
            // or the one-shot clear permits ordinary observed admission.
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "maximize-admitted-slotless",
                    "window": token,
                }),
            );
        }
    }
}

/// Restart marker adoption: the next owner re-adopts every surviving sticky
/// marker as sticky (keeping its pre-sticky origin) and every surviving
/// intentional-float marker as an ordinary float, each on its current
/// workspace with the live frame preserved, before any tiling. Runs in the
/// shared admission preamble, so every tick and chord adopts before any tile
/// write.
///
/// All admitted valid markers commit in ONE candidate event per domain:
/// sequential per-window commits admit the other marker windows as ordinary
/// tiles first, and flipping the Engine's focused tile to float then rejects
/// with `focus-mismatch` while the window stays tiled. Batching keeps every
/// flip on first contact, where no retained focus binding can reject. The
/// event carries the first tiled row as focus (the fresh-seed anchor rule)
/// so retained-session commits converge focus onto a surviving tiled member.
///
/// Markers restore classification only: admission plus the exact-lifetime
/// fences below must already hold, so a marker alone never grants
/// membership, writes, or recovery authority. Adoption performs no native
/// writes and keeps every durable marker for later restarts; only settled
/// unfloat/sticky-off removes them. Missing means empty; a wrong value logs
/// `marker-corrupt` and a failed read logs `marker-unreadable`, both with no
/// intent. The reserved tile-override marker never adopts here.
fn adopt_restart_markers(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    areas: &[MonitorArea],
) {
    /// One staged marker window: `prior` carries the pre-sticky float state
    /// for sticky markers and is `None` for an ordinary float.
    struct Staged {
        key: crate::workspace::WindowKey,
        token: String,
        loc: crate::workspace::MemberLoc,
        prior: Option<bool>,
        frame: Rect,
    }
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    // Stage every admitted valid marker before the first domain commit, so
    // no marker window is ever admitted as an ordinary tile first. No Engine
    // commits and no map writes in this pass (the already-floating fast path
    // below excepted).
    let mut staged: Vec<Staged> = Vec::new();
    for window in observed {
        let key = crate::workspace::WindowKey {
            hwnd: window.hwnd,
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
        };
        if state.sticky.contains_key(&key) || state.floated.contains(&key) {
            continue;
        }
        if state.hidden_claims.keys().any(|k| k.hwnd == window.hwnd) {
            continue;
        }
        let Some(token) = state.member_tokens.get(&key).cloned() else {
            continue;
        };
        let Some(loc) = state.workspaces.member_loc(&key).cloned() else {
            continue;
        };
        let Some(stored) = state.member_identity.get(&key).cloned() else {
            continue;
        };
        if !scope_allows(&state.scope, &stored.exe_path)
            || !hosted_gate_allows(&stored.exe_path, key.hwnd, key.pid, &state.scope_hosts)
        {
            continue;
        }
        if let Some(entries) = state.allowlist.as_ref() {
            let owned = entries
                .iter()
                .find(|e| e.hwnd == key.hwnd)
                .is_some_and(|entry| verify_proof_owned(key.hwnd, entry, me).is_ok());
            if !owned {
                continue;
            }
        }
        // Shared portable classifier (sticky wins over float and the
        // reserved tile override); absent is silent, corrupt/unreadable log
        // no intent.
        let marker = crate::workspace_owner::classify_restart_marker(
            crate::product_hide::sys::peek_float_intent_raw(window.hwnd),
            crate::product_hide::sys::peek_sticky_raw(window.hwnd),
            // Reserved peek: keeps the priority honest without hydrating.
            crate::product_hide::sys::peek_tile_override_raw(window.hwnd),
        );
        let prior = match marker {
            crate::workspace_owner::RestartMarker::Sticky(prior) => Some(prior),
            crate::workspace_owner::RestartMarker::FloatIntent => None,
            crate::workspace_owner::RestartMarker::Absent
            | crate::workspace_owner::RestartMarker::TileOverride => continue,
            crate::workspace_owner::RestartMarker::Corrupt => {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "sticky-adopt",
                        "window": token,
                        "outcome": "marker-corrupt",
                    }),
                );
                continue;
            }
            crate::workspace_owner::RestartMarker::Unreadable => {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "sticky-adopt",
                        "window": token,
                        "outcome": "marker-unreadable",
                    }),
                );
                continue;
            }
        };
        let live_tag = crate::product_hide::sys::read_member_tag(key.hwnd);
        let lifetime_ok = state.member_tags.get(&key).is_some_and(|stored| {
            crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
        });
        // Portable hydration gate: admitted (checked above) plus exact
        // identity plus live lifetime tag. A mismatch fails closed with no
        // intent and no log: the window is simply not ours.
        let eligible = match prior {
            Some(prior) => {
                crate::workspace_owner::restart_sticky_eligible(true, true, lifetime_ok, marker)
                    == Some(prior)
            }
            None => crate::workspace_owner::restart_float_eligible(true, true, lifetime_ok, marker),
        };
        if !eligible || !lifetime_ok || !pid_current(key.hwnd, key.pid) {
            continue;
        }
        if engine_is_float(state, &loc.output, &loc.workspace, &token) {
            // The Engine already floats this token: track the runtime lane
            // with the live frame. No marker write, no commit.
            let lane = match prior {
                Some(prior) => {
                    state.sticky.insert(key.clone(), prior);
                    "sticky-adopted"
                }
                None => {
                    state.floated.insert(key.clone());
                    "float-adopted"
                }
            };
            state.float_rects.insert(token.clone(), window.visible);
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": lane,
                    "window": token,
                }),
            );
            continue;
        }
        staged.push(Staged {
            key,
            token,
            loc,
            prior,
            frame: window.visible,
        });
    }
    // One candidate commit per domain over the complete staged set, in
    // first-seen domain order. Every marker window rides floating in a
    // single first-contact event; the event focus names the first tiled row
    // so retained sessions converge focus onto a survivor.
    let mut domains: Vec<(String, String)> = Vec::new();
    for s in &staged {
        if !domains
            .iter()
            .any(|(o, w)| o == &s.loc.output && w == &s.loc.workspace)
        {
            domains.push((s.loc.output.clone(), s.loc.workspace.clone()));
        }
    }
    for (output, workspace) in domains {
        for s in staged
            .iter()
            .filter(|s| s.loc.output == output && s.loc.workspace == workspace)
        {
            match s.prior {
                Some(prior) => {
                    state.sticky.insert(s.key.clone(), prior);
                }
                None => {
                    state.floated.insert(s.key.clone());
                }
            }
        }
        let rollback = |state: &mut TileLoop| {
            for s in staged
                .iter()
                .filter(|s| s.loc.output == output && s.loc.workspace == workspace)
            {
                state.sticky.remove(&s.key);
                state.floated.remove(&s.key);
                state.float_rects.remove(&s.token);
            }
        };
        let mut hint_cx = HintCx::new();
        let Some(rows) = assemble_domain_rows(
            state,
            &output,
            &workspace,
            observed,
            retained,
            "restart-adopt",
            correlation.as_str(),
            &mut hint_cx,
        ) else {
            rollback(state);
            continue;
        };
        let Some((domain, domain_key)) =
            workspace_domain_for(&output, &workspace, areas, state.inner_gap, state.outer_gap)
        else {
            rollback(state);
            continue;
        };
        let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let fp = fingerprint(
            &rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        let focused = rows
            .iter()
            .find(|r| !r.floating)
            .map(|r| WindowId(r.token.clone()));
        let mut candidate = state.engine.clone();
        let event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            &correlation,
            revision_for(state, &output, &workspace),
            fp,
            &domain,
            &domain_key,
            state.outer_gap,
            &windows,
            focused.as_ref(),
        );
        let reply = candidate.handle(&event);
        if matches!(
            reply,
            CoreReply::Rejected { .. } | CoreReply::Diverged(_) | CoreReply::SnapshotInvalid { .. }
        ) {
            rollback(state);
            for s in staged
                .iter()
                .filter(|s| s.loc.output == output && s.loc.workspace == workspace)
            {
                let lane = match s.prior {
                    Some(_) => "sticky-adopt",
                    None => "float-adopt",
                };
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": lane,
                        "window": s.token,
                        "correlation": correlation.as_str(),
                        "outcome": "candidate-rejected",
                        "reason": reply_outcome(&reply),
                    }),
                );
            }
            continue;
        }
        let mut missing = false;
        for s in staged
            .iter()
            .filter(|s| s.loc.output == output && s.loc.workspace == workspace)
        {
            if !candidate
                .session(&domain_key)
                .is_some_and(|sess| sess.is_exception(&WindowId(s.token.clone())))
            {
                missing = true;
                let lane = match s.prior {
                    Some(_) => "sticky-adopt",
                    None => "float-adopt",
                };
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": lane,
                        "window": s.token,
                        "correlation": correlation.as_str(),
                        "outcome": "candidate-unfloated",
                    }),
                );
            }
        }
        if missing {
            rollback(state);
            continue;
        }
        state.engine = candidate;
        // Kept: the temporary lane entries above are the re-adopted
        // origins (sticky keeps its prior, never a normal float). Every
        // durable marker stays for later restarts; only settled
        // unfloat/sticky-off removes them.
        for s in staged
            .iter()
            .filter(|s| s.loc.output == output && s.loc.workspace == workspace)
        {
            state.float_rects.insert(s.token.clone(), s.frame);
            let lane = match s.prior {
                Some(_) => "sticky-adopted",
                None => "float-adopted",
            };
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": lane,
                    "window": s.token,
                }),
            );
        }
    }
}

/// Assign newly seen eligible windows to the active workspace of their
/// monitor output. Hidden-claim HWNDs are never re-admitted into another
/// domain here. A nonempty scope admits only the named executables.
///
/// Visible-membership lifetime: every admitted window carries our inert
/// window-lifetime tag, verified live against the stored copy on every tick.
/// A same-process HWND reuse (same HWND/PID/creation, fresh window without
/// our tag) repairs here as a brand-new window: stale membership, Engine
/// token, identity, tag, and focus memory drop first, then a fresh tag is
/// stamped, a fresh Engine token is issued, and the current tick's row is
/// patched to it, so the new generation inherits no membership, focus, or
/// layout. Stamping writes only our own property name (dies with the window,
/// never trusted across runs, never removed by us); a failed stamp skips
/// admission fail-closed for the tick.
fn ensure_workspace_assignments(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    areas: &[MonitorArea],
) {
    for window in observed.iter_mut() {
        if !scope_allows(&state.scope, &window.identity.exe_path) {
            continue;
        }
        let key = crate::workspace::WindowKey {
            hwnd: window.hwnd,
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
        };
        // Never re-admit a claimed HWND nonce into another domain: any
        // same-HWND hidden claim blocks assignment regardless of key.
        if state.hidden_claims.keys().any(|k| k.hwnd == window.hwnd) {
            continue;
        }
        // Same-HWND reuse repair (cross-process): the fresh enumeration read
        // is authoritative, so stale same-HWND members (a destroyed window
        // whose number the OS recycled while visible) drop here instead of
        // stalling Engine convergence with a viewless member. Live hidden
        // claims never drop here; they retire through the ledger/audit path.
        let stale = crate::workspace_owner::reused_hwnd_stale(
            &state.member_tokens.keys().cloned().collect::<Vec<_>>(),
            &state.hidden_claims.keys().cloned().collect(),
            &key,
        );
        for dead in stale {
            drop_member_state(state, &dead);
        }
        // Visible lifetime gate: the live tag must equal the stored tag. A
        // mismatch with the same key is a same-process reuse (or an untracked
        // window): drop every stale table so the admission below treats the
        // live window as brand new with no inheritance.
        let live_tag = crate::product_hide::sys::read_member_tag(window.hwnd);
        let lifetime_ok = state.member_tags.get(&key).is_some_and(|stored| {
            crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
        });
        if !lifetime_ok {
            drop_member_state(state, &key);
        }
        if !lifetime_ok || state.workspaces.member_loc(&key).is_none() {
            // Fresh admission binds the stamp to the expected full identity:
            // the live window is re-read (identity, eligibility, hosted
            // children) and exact proof ownership is re-verified before any
            // SetProp effect. Anything stale or foreign skips fail-closed.
            let identity = ProcessIdentity {
                pid: window.identity.pid,
                process_creation: window.identity.process_creation.clone(),
                user_sid: window.identity.user_sid.clone(),
                session_id: window.identity.session_id,
                exe_path: window.identity.exe_path.clone(),
            };
            if let Some(entries) = state.allowlist.as_ref() {
                let Some(entry) = entries.iter().find(|e| e.hwnd == key.hwnd) else {
                    continue;
                };
                if verify_proof_owned(key.hwnd, entry, me).is_err() {
                    continue;
                }
            }
            let admitted = match crate::product_hide::sys::classify_candidate(window.hwnd, me) {
                crate::product_hide::sys::CandidateStatus::Admissible(snap)
                | crate::product_hide::sys::CandidateStatus::Retained(snap) => snap,
                _ => continue,
            };
            if admitted.pid != identity.pid || admitted.process != identity {
                continue;
            }
            if !hosted_gate_allows(
                &identity.exe_path,
                window.hwnd,
                identity.pid,
                &state.scope_hosts,
            ) {
                continue;
            }
            let fresh_tag = match crate::product_hide::sys::install_member_tag(window.hwnd, me.pid)
            {
                Ok(tag) => tag,
                Err(_) => continue,
            };
            // Post-stamp postcheck: the expected identity must still be live
            // with the fresh tag reading back, or no membership is stored.
            let post_ok = crate::product_hide::sys::hold_target_verified(identity.pid, &identity)
                .is_ok()
                && pid_current(window.hwnd, identity.pid)
                && crate::product_hide::sys::read_member_tag(window.hwnd).as_deref()
                    == Some(fresh_tag.as_str());
            if !post_ok {
                continue;
            }
            let fresh_token = state
                .tokens
                .reissue(window.hwnd, &window.identity.process_creation);
            window.token = fresh_token.clone();
            let output = output_for_rect(areas, &window.visible);
            state.workspaces.ensure_output(&output);
            let Some(active) = state.workspaces.active_id(&output) else {
                continue;
            };
            if state
                .workspaces
                .assign(key.clone(), &output, &active, false)
            {
                state.member_tokens.insert(key.clone(), fresh_token.clone());
                state.member_rects.insert(fresh_token, window.visible);
                state.member_identity.insert(key.clone(), identity);
                state.member_tags.insert(key, fresh_tag);
            }
            continue;
        }
        let identity = ProcessIdentity {
            pid: window.identity.pid,
            process_creation: window.identity.process_creation.clone(),
            user_sid: window.identity.user_sid.clone(),
            session_id: window.identity.session_id,
            exe_path: window.identity.exe_path.clone(),
        };
        state
            .member_tokens
            .insert(key.clone(), window.token.clone());
        state
            .member_rects
            .insert(window.token.clone(), window.visible);
        state.member_identity.insert(key, identity);
    }
    // Born-held first-seen fullscreen members join the workspace slotless
    // (floating Engine observation, no tile slot); later fullscreen on a
    // slotted or lifetime-known window stays a managed overlay transition.
    admit_born_fullscreen(state, me, retained, areas);
    // First-seen maximized members join slotless the same way (membership
    // without a tile slot); the mode-gated clear restores them on tiled.
    admit_slotless_maximized(state, me, retained, areas);
    adopt_restart_markers(state, me, observed, retained, areas);
    if state.active_output.is_empty()
        && let Some(first) = state.workspaces.output_keys().into_iter().next()
    {
        state.active_output = first;
    }
}

/// Periodic hidden-claim audit: destroyed/recycled claims retire through the
/// identity-safe reveal path (no foreign writes), then drop session
/// membership, tokens, identity, and prune emptied workspaces. A merely
/// invisible window never retires anything: only `query_recovery` decides.
/// Unknown identities retain with the ledger kept.
fn audit_hidden_claims(state: &mut TileLoop, me: &ProcessIdentity, store: &LedgerStore) {
    let keys: Vec<crate::workspace::WindowKey> = state.hidden_claims.keys().cloned().collect();
    let mut retired_any = false;
    for key in keys {
        let Some(record) = state.hidden_claims.get(&key).cloned() else {
            continue;
        };
        let dead = match crate::product_hide::sys::query_recovery(key.hwnd, me) {
            Ok(snap) => {
                snap.process != record.claim.process
                    || snap.nonce.as_deref() != Some(record.claim.tag.as_str())
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.starts_with("absent:") || msg.contains("sid/session mismatch") {
                    true
                } else {
                    // Unknown: retain with the ledger kept.
                    continue;
                }
            }
        };
        if !dead {
            continue;
        }
        let _ = crate::product_hide::sys::reveal_product_claim_to(
            store,
            me,
            &record.claim,
            record.iconic,
        );
        state.hidden_claims.remove(&key);
        drop_member_state(state, &key);
        retired_any = true;
    }
    if retired_any {
        for output in state.workspaces.output_keys() {
            let Some(active_id) = state.workspaces.active_id(&output) else {
                continue;
            };
            let displaced: Vec<String> = state
                .workspaces
                .displaced_snapshot()
                .values()
                .flat_map(|r| r.workspace_ids.clone())
                .collect();
            let sticky_keys: BTreeSet<crate::workspace::WindowKey> =
                state.sticky.keys().cloned().collect();
            let (removed, append) = state.workspaces.plan_cleanup_excluding(
                &output,
                std::slice::from_ref(&active_id),
                &displaced,
                &sticky_keys,
            );
            state.workspaces.apply_cleanup(&output, &removed, append);
        }
    }
}

/// Drop membership for windows that vanished from the raw enumeration
/// inventory. Retained-occupancy members (minimized, maximized, fullscreen)
/// stay enumerated with identity, so only a truly absent HWND cleans up;
/// hidden claims retire through the reveal path, never here. Unknown
/// identities (enumeration without resolution) retain membership. Born holds
/// clean here too: a closed born window releases its hold with the
/// once-per-lifecycle line and drops its snapshot seed.
fn workspace_close_cleanup(state: &mut TileLoop) {
    let gone: Vec<crate::workspace::WindowKey> = state
        .member_tokens
        .keys()
        .filter(|k| !state.last_hwnds.contains(&k.hwnd) && !state.hidden_claims.contains_key(k))
        .cloned()
        .collect();
    let log_path = state.log_path.clone();
    for key in gone {
        let born = state.born_fullscreen.remove(&key);
        let token = state.member_tokens.get(&key).cloned();
        if born && let Some(token) = token {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "initial-fullscreen-released",
                    "window": token,
                }),
            );
        }
        drop_member_state(state, &key);
    }
    state
        .born_fullscreen
        .retain(|key| state.last_hwnds.contains(&key.hwnd));
    state
        .born_keep_overlay
        .retain(|key| state.member_tokens.contains_key(key));
    state
        .seen_nonfullscreen
        .retain(|key| state.last_hwnds.contains(&key.hwnd));
    prune_float_state(state);
}

/// Hide one managed member bound to its stored full identity: the live
/// window must equal the stored [`ProcessIdentity`] exactly, so a recycled
/// HWND refuses with no writes. Fresh proof gate on every write in proof
/// modes; commit-before-hide through the central watcher/ledger. Outcomes
/// are typed at the source, never selected by error strings. The detail
/// carries the managed-admission refusal code when the outcome is
/// `refused`, so the select hide loop can log the exact failure.
///
/// Bounded per-window hide diagnostics cap for one select transition.
const MAX_HIDE_DIAG_PER_SELECT: usize = 64;
fn workspace_hide_one(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    key: &crate::workspace::WindowKey,
    expected: &ProcessIdentity,
    iconic: bool,
) -> (&'static str, Option<&'static str>) {
    use crate::product_hide::sys::ManagedAdmitError;
    if state.allowlist.is_some() && !state.workspace_proof {
        return ("workspace-disabled", None);
    }
    if !scope_allows(&state.scope, &expected.exe_path) {
        return ("scope-excluded", None);
    }
    // Listed hosts hide only with a live matching hosted child, re-verified
    // fresh here (not from the tick's observation): a newly appearing hosted
    // app is never hidden under another app's membership.
    if !hosted_gate_allows(
        &expected.exe_path,
        key.hwnd,
        expected.pid,
        &state.scope_hosts,
    ) {
        return ("scope-excluded", None);
    }
    // Visible lifetime gate: the live member tag must equal the stored tag.
    // A same-process HWND reuse (same HWND/PID/creation, fresh window)
    // drops its stale membership here with no writes; the next tick
    // re-admits the live window as brand new.
    let live_tag = crate::product_hide::sys::read_member_tag(key.hwnd);
    let stored_tag = state.member_tags.get(key).cloned().unwrap_or_default();
    if !crate::workspace_owner::visible_lifetime_ok(&stored_tag, live_tag.as_deref()) {
        drop_member_state(state, key);
        return ("identity-changed", None);
    }
    if let Some(entries) = state.allowlist.as_ref() {
        let Some(entry) = entries.iter().find(|e| e.hwnd == key.hwnd) else {
            return ("allowlist-changed", None);
        };
        if verify_proof_owned(key.hwnd, entry, me).is_err() {
            return ("identity-changed", None);
        }
    }
    // Project-raised topmost exception: the ordinary intentional float
    // raises the keep-above band from a non-topmost prior, recorded in
    // `float_topmost_prev` as exact-key `Some(false)`. Only that fresh
    // runtime evidence plus the exact member key matching the stored full
    // identity authorizes the narrow hide-admission bypass below; the float
    // marker alone never authorizes, preexisting topmost stays refused, and
    // a replaced member (key/identity drift) refuses in admission as
    // `identity-changed`. The band is never cleared merely to hide.
    let allow_project_topmost = crate::product_hide::project_topmost_hide_allowed(
        state.float_topmost_prev.get(key).copied(),
        read_topmost_now(key.hwnd),
    ) && key.pid == expected.pid
        && key.creation == expected.process_creation;
    let claim = match crate::product_hide::sys::admit_managed_claim_for_hide(
        key.hwnd,
        me,
        expected,
        &stored_tag,
        allow_project_topmost,
    ) {
        Ok(claim) => claim,
        Err(ManagedAdmitError::Absent) => return ("origin-vanished", None),
        Err(ManagedAdmitError::Uncertain) => return ("uncertain", None),
        Err(ManagedAdmitError::WrongIdentity) => return ("identity-changed", None),
        Err(ManagedAdmitError::Refused(code)) => return ("refused", Some(code)),
    };
    match crate::product_hide::sys::hide_managed_claim(store, me, &claim, dir) {
        Ok(committed) => {
            state.hidden_claims.insert(
                key.clone(),
                HiddenRecord {
                    claim: committed,
                    iconic,
                },
            );
            ("hidden", None)
        }
        Err(e) => {
            // Commit-before-hide appends the durable enriched claim BEFORE the
            // async visibility post: an already-committed claim must register
            // in the owner table even when the post-hide readback fails, or
            // the window is lost (hidden with no claim). Re-read the ledger
            // for the exact key (full identity plus tag); never re-admit the
            // claimed HWND nonce and never fabricate a rollback.
            if let Ok(Some(record)) = store.committed()
                && record.owner == *me
                && let Some(durable) = record.windows.iter().find(|w| {
                    w.hwnd == claim.hwnd
                        && w.tag == claim.tag
                        && w.process == claim.process
                        && w.kind == crate::model::WindowClaimKind::Product
                })
            {
                state.hidden_claims.insert(
                    key.clone(),
                    HiddenRecord {
                        claim: durable.clone(),
                        iconic,
                    },
                );
            }
            let msg = e.to_string();
            if msg.starts_with("absent:") {
                ("origin-vanished", None)
            } else if msg.starts_with("uncertain:") {
                ("uncertain", None)
            } else {
                ("refused", None)
            }
        }
    }
}

/// Fresh proof-owned gate for one reveal write: in proof modes the helper
/// must verify against the frozen allowlist immediately before the reveal
/// effect, exactly like the hide path.
fn workspace_proof_reveal_gate(
    state: &TileLoop,
    me: &ProcessIdentity,
    hwnd: u64,
) -> std::result::Result<(), &'static str> {
    let Some(entries) = state.allowlist.as_ref() else {
        return Ok(());
    };
    if !state.workspace_proof {
        return Err("workspace-disabled");
    }
    let Some(entry) = entries.iter().find(|e| e.hwnd == hwnd) else {
        return Err("allowlist-changed");
    };
    verify_proof_owned(hwnd, entry, me).map_err(|_| "identity-changed")
}

/// Dispatch one exact shared-Engine domain release for a workspace-mode
/// toggle: the current observed rows ride the event with `ReleaseDomain`
/// (no geometry), so nothing is written and no stale layout is retained.
/// Returns true exactly when the Engine confirms the release.
fn dispatch_workspace_release(
    state: &mut TileLoop,
    output: &str,
    workspace: &str,
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    correlation: &str,
) -> bool {
    let Some((domain, domain_key)) =
        workspace_domain_for(output, workspace, areas, state.inner_gap, state.outer_gap)
    else {
        return false;
    };
    let mut hint_cx = HintCx::new();
    let Some(rows) = assemble_domain_rows(
        state,
        output,
        workspace,
        observed,
        retained,
        "release",
        correlation,
        &mut hint_cx,
    ) else {
        return false;
    };
    let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let fp = fingerprint(
        &rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect::<Vec<_>>(),
    );
    let Some(correlation_id) = CorrelationId::parse(correlation) else {
        return false;
    };
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation_id,
        revision_for(state, output, workspace),
        fp,
        &domain,
        &domain_key,
        state.outer_gap,
        &windows,
        None,
    );
    event.command = CoreCommand::ReleaseDomain;
    match state.engine.handle(&event) {
        CoreReply::Released => {
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-released",
                    "output": state.workspaces.output_token(output),
                    "workspace": state.workspaces.workspace_token(output, workspace),
                    "outcome": "released",
                    "members": rows.len(),
                }),
            );
            true
        }
        reply => {
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-released",
                    "output": state.workspaces.output_token(output),
                    "workspace": state.workspaces.workspace_token(output, workspace),
                    "outcome": reply_outcome(&reply),
                    "recovery": "retry-on-event",
                }),
            );
            false
        }
    }
}

/// Cancel stale drag and queued-action effects for one workspace after a
/// mode toggle: in-flight gesture, preview, and Win+Left bindings anchored
/// to its members can never settle into geometry writes afterwards. Queued
/// keyboard intents re-resolve the live mode at dispatch and refuse there.
fn cancel_workspace_effects(state: &mut TileLoop, output: &str, workspace: &str) {
    let members = state.workspaces.workspace_members(output, workspace);
    let hwnds: HashSet<u64> = members.iter().map(|k| k.hwnd).collect();
    let mut cancelled = 0u32;
    let mut drop_entries = |before: usize, after: usize| {
        cancelled += (before - after) as u32;
    };
    let before = state.gesture_before.len();
    state.gesture_before.retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_before.len());
    let before = state.gesture_end_cursor.len();
    state
        .gesture_end_cursor
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_end_cursor.len());
    let before = state.windrag_start_cursor.len();
    state
        .windrag_start_cursor
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.windrag_start_cursor.len());
    let before = state.gesture_start_key.len();
    state
        .gesture_start_key
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_start_key.len());
    let before = state.gesture_start_tag.len();
    state
        .gesture_start_tag
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_start_tag.len());
    let before = state.gesture_preview_start.len();
    state
        .gesture_preview_start
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_preview_start.len());
    let before = state.gesture_esc_seq.len();
    state
        .gesture_esc_seq
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_esc_seq.len());
    let before = state.gesture_end_seq.len();
    state
        .gesture_end_seq
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_end_seq.len());
    let before = state.windrag_bound.len();
    state.windrag_bound.retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.windrag_bound.len());
    let before = state.move_kind.len();
    state.move_kind.retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.move_kind.len());
    let before = state.gesture_start_cursor.len();
    state
        .gesture_start_cursor
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_start_cursor.len());
    let before = state.preview_bound.len();
    state.preview_bound.retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.preview_bound.len());
    let before = state.gesture_producer.len();
    state
        .gesture_producer
        .retain(|hwnd, _| !hwnds.contains(hwnd));
    drop_entries(before, state.gesture_producer.len());
    let before = state.active.len();
    state.active.retain(|hwnd| !hwnds.contains(hwnd));
    drop_entries(before, state.active.len());
    let before = state.preview_dead.len();
    state.preview_dead.retain(|hwnd| !hwnds.contains(hwnd));
    drop_entries(before, state.preview_dead.len());
    let before = state.esc_latched.len();
    state.esc_latched.retain(|hwnd| !hwnds.contains(hwnd));
    drop_entries(before, state.esc_latched.len());
    if let Some(advance) = state.snap_advance.clone()
        && hwnds.contains(&advance.hwnd)
    {
        state.snap_advance = None;
        cancelled += 1;
    }
    hide_preview(state, "workspace-mode");
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "workspace-mode",
            "op": "effects-cancelled",
            "output": state.workspaces.output_token(output),
            "workspace": state.workspaces.workspace_token(output, workspace),
            "cancelled": cancelled,
        }),
    );
}

/// Toggle the tiling mode of the owner's current workspace (tray edge).
/// Floating applies immediately with frames untouched, then the exact shared
/// Engine domain releases with no writes. Retile releases first and stays
/// floating until the release confirms, then marks tiled with a fresh
/// complete reconcile on current observed geometry. Release failures queue
/// for retry on later topology edges; per-window float/sticky and native
/// maximize/fullscreen exceptions are never touched.
fn toggle_workspace_tiling(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    output: &str,
) {
    state.workspaces.ensure_output(output);
    let Some(current) = state.workspaces.active_id(output) else {
        return;
    };
    state.tick += 1;
    let tick = state.tick;
    let correlation = format!("ws-mode-{tick}");
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "workspace-mode",
                "op": "toggle",
                "outcome": "observation-failed",
            }),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    let current_clone = current.clone();
    if state.workspaces.is_tiled(output, &current_clone) {
        state.workspaces.set_tiled(output, &current_clone, false);
        cancel_workspace_effects(state, output, &current_clone);
        let confirmed = dispatch_workspace_release(
            state,
            output,
            &current_clone,
            areas,
            &observed,
            &retained,
            &correlation,
        );
        if !confirmed {
            state.pending_releases.queue(output, &current_clone, false);
        } else {
            // A confirming toggle supersedes any earlier intent for the
            // domain (e.g. a failed float queued before a successful retile):
            // without this the stale entry would later fire at live layout.
            state.pending_releases.confirm(output, &current_clone);
        }
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "workspace-mode",
                "op": "toggle",
                "output": state.workspaces.output_token(output),
                "workspace": state.workspaces.workspace_token(output, &current),
                "tiled": false,
                "outcome": "applied",
            }),
        );
    } else {
        let confirmed = dispatch_workspace_release(
            state,
            output,
            &current_clone,
            areas,
            &observed,
            &retained,
            &correlation,
        );
        if confirmed {
            // The successful retile supersedes every earlier intent for the
            // domain: drop the entry so no stale float can fire later.
            state.pending_releases.confirm(output, &current_clone);
            state.workspaces.set_tiled(output, &current_clone, true);
            cancel_workspace_effects(state, output, &current_clone);
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-mode",
                    "op": "toggle",
                    "output": state.workspaces.output_token(output),
                    "workspace": state.workspaces.workspace_token(output, &current),
                    "tiled": true,
                    "outcome": "applied",
                }),
            );
            reconcile_tick(state, me, fulls, areas);
            refresh_active_border(state, me, fulls);
        } else {
            state.pending_releases.queue(output, &current_clone, true);
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-mode",
                    "op": "toggle",
                    "output": state.workspaces.output_token(output),
                    "workspace": state.workspaces.workspace_token(output, &current),
                    "tiled": false,
                    "outcome": "pending-retile",
                }),
            );
        }
    }
}

/// Retry unconfirmed workspace-domain releases on a topology edge with the
/// maintenance observation (no second enumeration). Confirmed retiles mark
/// tiled with a fresh complete reconcile; confirmed float releases stay
/// floating. Entries whose workspace reads tiled again drop stale without
/// dispatch: releasing would destroy the live tiled layout. Failures stay
/// queued and the fingerprint gate retries them on the next topology edge
/// only, so quiet ticks never poll or log.
fn retry_pending_releases(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) {
    if state.pending_releases.is_empty() {
        return;
    }
    let fingerprint_now = pending_topology_fingerprint(state, areas);
    if fingerprint_now == state.pending_retry_fp {
        return;
    }
    state.pending_retry_fp = fingerprint_now;
    let mut retiled = false;
    for (output, workspace, retile) in state.pending_releases.snapshot() {
        let current = workspace_mode_known(state, &output, &workspace);
        if crate::workspace::PendingReleases::is_stale(current) {
            state.pending_releases.confirm(&output, &workspace);
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-mode",
                    "op": "retry",
                    "output": state.workspaces.output_token(&output),
                    "workspace": state.workspaces.workspace_token(&output, &workspace),
                    "outcome": "stale-dropped",
                }),
            );
            continue;
        }
        state.tick += 1;
        let tick = state.tick;
        let correlation = format!("ws-retry-{tick}");
        if dispatch_workspace_release(
            state,
            &output,
            &workspace,
            areas,
            observed,
            retained,
            &correlation,
        ) {
            state.pending_releases.confirm(&output, &workspace);
            if retile {
                state.workspaces.set_tiled(&output, &workspace, true);
                cancel_workspace_effects(state, &output, &workspace);
                retiled = true;
            }
        }
    }
    if retiled {
        reconcile_tick(state, me, fulls, areas);
        refresh_active_border(state, me, fulls);
    }
}

/// Session tiled state for one workspace, or `None` when the workspace is
/// unknown on the output. Stale-intent detection must distinguish unknown
/// (still releasable with empty rows) from tiled (never releasable).
fn workspace_mode_known(state: &TileLoop, output: &str, workspace: &str) -> Option<bool> {
    state
        .workspaces
        .workspace_ids(output)
        .iter()
        .any(|id| id == workspace)
        .then(|| state.workspaces.is_tiled(output, workspace))
}

/// Topology fingerprint behind the pending-release retry gate: monitor
/// devices plus work/full rects, workspace order per output, member
/// identities, and the raw HWND inventory size.
fn pending_topology_fingerprint(state: &TileLoop, areas: &[MonitorArea]) -> u64 {
    fn pack(rect: &Rect) -> (i32, i32, i32, i32) {
        (rect.x, rect.y, rect.w, rect.h)
    }
    let area_tuples: Vec<crate::workspace::TopologyArea> = areas
        .iter()
        .map(|area| (area.device.clone(), pack(&area.work), pack(&area.full)))
        .collect();
    let workspace_tuples: Vec<(String, Vec<String>)> = state
        .workspaces
        .output_keys()
        .into_iter()
        .map(|output| {
            let ids = state.workspaces.workspace_ids(&output);
            (output, ids)
        })
        .collect();
    let members: Vec<crate::workspace::WindowKey> = state.member_tokens.keys().cloned().collect();
    crate::workspace::topology_fingerprint(
        &area_tuples,
        &workspace_tuples,
        &members,
        state.last_hwnds.len(),
    )
}

/// REQ-WS-09 departure memory for an explicit workspace select: the
/// chord-time origin still holds live foreground at dispatch, so the source
/// workspace remembers it before hiding. Exact member key (HWND/PID/creation)
/// plus source membership plus stay-mirrored liveness (fresh observed match,
/// else a verified retained maximized/fullscreen row). Returns the remembered
/// opaque token for one bounded log line; no windows, geometry, or
/// suppression change.
fn remember_select_departure(
    state: &mut TileLoop,
    origin: &crate::snapkey::SnapOrigin,
    output: &str,
    source_id: &str,
    foreground_hwnd: u64,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) -> Option<String> {
    if origin.token.is_empty() || foreground_hwnd != origin.hwnd {
        return None;
    }
    let key = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == origin.token.as_str())
        .map(|(key, _)| key.clone())?;
    if !crate::workspace_owner::member_matches(&key, origin.hwnd, origin.pid, &origin.creation) {
        return None;
    }
    let in_source = state
        .workspaces
        .member_loc(&key)
        .is_some_and(|loc| loc.output == output && loc.workspace == source_id);
    if !in_source {
        return None;
    }
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let live = match by_hwnd.get(&key.hwnd) {
        Some(fresh) => {
            fresh.token == origin.token
                && crate::workspace_owner::member_matches(
                    &key,
                    fresh.hwnd,
                    fresh.identity.pid,
                    &fresh.identity.process_creation,
                )
        }
        None => retained
            .iter()
            .any(|row| row.key == key && (row.maximized || row.fullscreen)),
    };
    if !live {
        return None;
    }
    state.workspaces.note_foreground(&key);
    Some(origin.token.clone())
}

/// Switch the visible set from the output's active workspace to `target`:
/// hide the prior set, reveal the target set, preserve Engine sessions for
/// both, prune trailing empties, then re-enumerate fresh, establish the
/// appropriate target focus BEFORE geometry (with fullscreen/elevated fences
/// retained so a real arrival is never stolen from), then reconcile the
/// selected domain geometry with verified readback. Empty targets hide the
/// prior set and take no focus.
/// The pre-switch `observed` serves only the hide phase; focus never uses it
/// because the hidden target set is absent from it by construction.
/// `ctx` carries the stable action correlation even when inner ticks
/// increment; `focus_hint` (send-follow mover) wins when eligible so the
/// caller can reuse the fresh focus without a redundant enumeration.
#[allow(clippy::too_many_arguments)]
fn workspace_do_select(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    output: &str,
    target: &str,
    ctx: &ActionCtx,
    focus_hint: Option<&crate::workspace::WindowKey>,
    suppress_focus: bool,
) -> SelectEffect {
    let none =
        |outcome: &'static str, source_workspace: String, target_workspace: String| SelectEffect {
            outcome,
            geometry: None,
            focus: "none",
            transition_ms: 0,
            observation_ms: 0,
            geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            plan_ms: 0,
            source_workspace,
            target_workspace,
        };
    let Some(current) = state.workspaces.active_id(output) else {
        return none("unknown-output", "ws?".to_owned(), "ws?".to_owned());
    };
    if current == target {
        state.active_output = output.to_owned();
        let token = state.workspaces.workspace_token(output, target);
        return none("already-active", token.clone(), token);
    }
    let source_token = state.workspaces.workspace_token(output, &current);
    let target_token = state.workspaces.workspace_token(output, target);
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let leaving = state.workspaces.workspace_members(output, &current);
    let mut hide_failed = false;
    // Windows newly committed to hidden in this transition: on any failure
    // before activation these are safely revealed back (exact identity) so a
    // partial never leaves a doubled visible set or a lost claim behind.
    let mut newly_hidden: Vec<crate::workspace::WindowKey> = Vec::new();
    let hide_start = Instant::now();
    // Bounded per-window hide diagnostics for this select: one
    // `workspace-hide` line per leaving member up to the cap below.
    let mut hide_diag_logged: usize = 0;
    for key in &leaving {
        if state.workspaces.is_hidden(key) {
            continue;
        }
        let Some(expected) = state.member_identity.get(key).cloned() else {
            continue;
        };
        // Explicit scope fences every hide: out-of-scope members stay
        // visible and keep membership, never hidden.
        if !scope_allows(&state.scope, &expected.exe_path) {
            continue;
        }
        // Bind the hide to the stored full member identity: a recycled HWND
        // (same number, fresh process) reconciles membership with no writes.
        // Members absent from the tiled observation hide through a fresh
        // identity-only read so retained minimized/maximized/fullscreen
        // members still leave the visible set. Sticky liveness verifies here
        // too: a dead sticky drops instead of hiding, a live one skips below.
        let (live_ok, iconic) = match by_hwnd.get(&key.hwnd) {
            Some(window) => {
                let live_ok = crate::workspace_owner::member_matches(
                    key,
                    window.hwnd,
                    window.identity.pid,
                    &window.identity.process_creation,
                ) && window.identity.exe_path == expected.exe_path
                    && window.identity.user_sid == expected.user_sid
                    && window.identity.session_id == expected.session_id;
                (live_ok, window.facts.minimized)
            }
            None => match crate::product_hide::sys::query_recovery(key.hwnd, me) {
                Ok(snap) => {
                    let live_ok = snap.pid == expected.pid
                        && snap.process == expected
                        && snap.nonce.is_none();
                    (live_ok, snap.iconic)
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.starts_with("absent:") || msg.contains("sid/session mismatch") {
                        drop_member_state(state, key);
                    } else {
                        hide_failed = true;
                    }
                    continue;
                }
            },
        };
        if !live_ok {
            // Recycled HWND: drop the stale membership, never touch the new
            // owner, and retire any same-HWND claim residue without writes.
            drop_member_state(state, key);
            continue;
        }
        // Verified-live sticky members never hide: they stay visible through
        // workspace selects with no `SW_HIDE`, no claim, no backing occupancy.
        if state.sticky.contains_key(key) {
            continue;
        }
        let had_claim = state.hidden_claims.contains_key(key);
        let project_topmost = crate::product_hide::project_topmost_hide_allowed(
            state.float_topmost_prev.get(key).copied(),
            read_topmost_now(key.hwnd),
        );
        let (outcome, detail) = workspace_hide_one(state, me, store, dir, key, &expected, iconic);
        // Correlated bounded per-window hide outcome: opaque token plus
        // outcome and refusal detail only, no titles, paths, or content.
        // Proves the exact failing window when a select goes partial.
        if hide_diag_logged < MAX_HIDE_DIAG_PER_SELECT {
            let token = state.member_tokens.get(key).cloned().unwrap_or_default();
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-hide",
                    "correlation": ctx.correlation,
                    "window": if token.is_empty() { serde_json::Value::Null } else { serde_json::Value::from(token) },
                    "outcome": outcome,
                    "detail": detail,
                    "project_topmost": project_topmost,
                }),
            );
            hide_diag_logged += 1;
        }
        // A committed claim (exact full identity plus tag) is owned even when
        // the post-hide readback reports uncertain: the ledger and the owner
        // table keep it, never a lost window.
        if state.hidden_claims.contains_key(key) || outcome == "hidden" {
            state.workspaces.set_hidden(key, true);
            if !had_claim {
                newly_hidden.push(key.clone());
            }
        } else if outcome == "origin-vanished" || outcome == "identity-changed" {
            drop_member_state(state, key);
        } else if outcome == "scope-excluded" {
            // Explicit scope fence: out-of-scope members stay visible with
            // membership intact, never hidden, never a failure.
        } else {
            hide_failed = true;
        }
    }
    // A failed hide phase never activates: revealing the target beside a
    // still-visible current set would double the visible set and lose claims.
    // Narrow recovery reveals exactly the claims committed above (exact
    // identity, no blind replay, no fabricated rollback) and keeps current.
    let hide_ms = hide_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    if hide_failed {
        for key in &newly_hidden {
            let Some(record) = state.hidden_claims.get(key).cloned() else {
                continue;
            };
            match crate::product_hide::sys::reveal_product_claim_to(
                store,
                me,
                &record.claim,
                record.iconic,
            ) {
                Ok(crate::product_hide::ProductTeardown::Retired) => {
                    state.hidden_claims.remove(key);
                    drop_member_state(state, key);
                }
                Ok(_) => {
                    state.hidden_claims.remove(key);
                    state.workspaces.set_hidden(key, false);
                }
                Err(_) => {}
            }
        }
        return SelectEffect {
            outcome: "partial",
            hide_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
            ..none("partial", source_token.clone(), target_token.clone())
        };
    }
    let entering = state.workspaces.workspace_members(output, target);
    let reveal_start = Instant::now();
    for key in &entering {
        let Some(record) = state.hidden_claims.get(key).cloned() else {
            state.workspaces.set_hidden(key, false);
            continue;
        };
        if workspace_proof_reveal_gate(state, me, key.hwnd).is_err() {
            hide_failed = true;
            continue;
        }
        match crate::product_hide::sys::reveal_product_claim_to(
            store,
            me,
            &record.claim,
            record.iconic,
        ) {
            Ok(crate::product_hide::ProductTeardown::Retired) => {
                state.hidden_claims.remove(key);
                if let Some(token) = state.member_tokens.remove(key) {
                    state.member_rects.remove(&token);
                    state.float_rects.remove(&token);
                    state.hint_logged.remove(&token);
                }
                if state.restore_wake.as_ref().is_some_and(|w| w.key == *key) {
                    state.restore_wake = None;
                }
                state.float_topmost_prev.remove(key);
                state.floated.remove(key);
                state.member_identity.remove(key);
                state.member_tags.remove(key);
                state.workspaces.remove_window(key);
            }
            Ok(_) => {
                state.hidden_claims.remove(key);
                state.workspaces.set_hidden(key, false);
            }
            Err(_) => {
                hide_failed = true;
            }
        }
    }
    // A failed reveal phase keeps current as well: the leaving set was
    // hidden successfully, so the safest known view is restored by revealing
    // exactly those affected claims (exact identity only). The switch is not
    // pretended: the outcome stays partial with current kept.
    let reveal_ms = reveal_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    if hide_failed {
        for key in &newly_hidden {
            let Some(record) = state.hidden_claims.get(key).cloned() else {
                continue;
            };
            match crate::product_hide::sys::reveal_product_claim_to(
                store,
                me,
                &record.claim,
                record.iconic,
            ) {
                Ok(crate::product_hide::ProductTeardown::Retired) => {
                    state.hidden_claims.remove(key);
                    drop_member_state(state, key);
                }
                Ok(_) => {
                    state.hidden_claims.remove(key);
                    state.workspaces.set_hidden(key, false);
                }
                Err(_) => {}
            }
        }
        return SelectEffect {
            outcome: "partial",
            hide_ms,
            reveal_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
            ..none("partial", source_token.clone(), target_token.clone())
        };
    }
    // Move active to the target id without disturbing order, including
    // appended trailing ids past the Win1..9 ordinal range. This is the only
    // activation point: callers resolve targets without preactivating so the
    // `current == target` guard above stays accurate. Reached only after the
    // hide and reveal effects verified, so the switch is real.
    state.workspaces.activate(output, target);
    state.active_output = output.to_owned();
    // Item 1 history: record the actual completed view transition here,
    // including when later focus/geometry fails below. View evidence (the
    // verified hide/reveal above), not the final action success label,
    // controls history. Same-view activation returned early above and never
    // records; failed hide/reveal returned partial before activation.
    state.workspaces.observe_workspace_change(output, target);
    // Trailing maintenance: keep one empty, minimum two, active preserved.
    // Sticky members never occupy, so a sticky-only workspace prunes like an
    // empty one while the sticky float state itself is retained.
    let active_id = state.workspaces.active_id(output).unwrap_or_default();
    let displaced: Vec<String> = state
        .workspaces
        .displaced_snapshot()
        .values()
        .flat_map(|r| r.workspace_ids.clone())
        .collect();
    let sticky_keys: BTreeSet<crate::workspace::WindowKey> = state.sticky.keys().cloned().collect();
    let (removed, append) = state.workspaces.plan_cleanup_excluding(
        output,
        std::slice::from_ref(&active_id),
        &displaced,
        &sticky_keys,
    );
    state.workspaces.apply_cleanup(output, &removed, append);
    // Post-reveal fresh observation: the pre-switch `observed` cannot contain
    // the hidden target set, so focusing from it always misses as `vanished`.
    // Re-enumerate, republish origins, establish the appropriate target focus
    // BEFORE geometry (fenced), then reconcile the selected domain geometry
    // with verified readback. Empty targets hide the prior set and take no
    // focus.
    let transition_ms = ctx.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let observation_start = Instant::now();
    let mut fresh_skipped: Vec<(String, String)> = Vec::new();
    let mut fresh_retained: Vec<RetainedRow> = Vec::new();
    let Some(mut fresh_observed) =
        state.observe(me, fulls, &mut fresh_skipped, &mut fresh_retained)
    else {
        let observation_ms = observation_start
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        return SelectEffect {
            outcome: "observation-failed",
            transition_ms,
            observation_ms,
            hide_ms,
            reveal_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
            ..none(
                "observation-failed",
                source_token.clone(),
                target_token.clone(),
            )
        };
    };
    let observation_ms = observation_start
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    publish_managed(state, me, &fresh_observed, &fresh_retained);
    ensure_workspace_assignments(state, me, &mut fresh_observed, &fresh_retained, areas);
    clear_maximize_at_admission(state, me, &fresh_retained);
    workspace_close_cleanup(state);
    // Focus-before-geometry on the verified transition: the revealed target
    // must hold foreground before writes so a transient foreground cannot
    // veto eligible writes on the next tick. All fences retained: a real
    // fullscreen or elevated arrival is never stolen from, and identity,
    // lifetime, scope, and observation gates inside `actuate_focus` still
    // apply. Skipping focus there lets geometry veto honestly.
    // Item 9 fullscreen-carry transitions suppress ALL explicit focus
    // actuation here (never `SetForegroundWindow` on a fullscreen mover):
    // the select still reveals/reconciles the target, and a fresh foreground
    // readback reports observed focus honestly below.
    let mut focus_outcome: &'static str = "no-focus";
    let mut focus_window: Option<String> = None;
    // Focus duration covers the bounded pumped settle when it runs, so a
    // dominating 500 ms settle attributes to focus, not geometry.
    let mut focus_ms: u64 = 0;
    if suppress_focus {
        // No setter, no priming, no MRU fallback actuation: compare the fresh
        // foreground against the mover hint only. Native reveal may have
        // focused the mover; anything else stays explicitly suppressed.
        let foreground = unsafe { GetForegroundWindow() } as usize as u64;
        let observed = focus_hint.is_some_and(|hint| hint.hwnd != 0 && foreground == hint.hwnd);
        if observed {
            focus_outcome = "focus-ok";
            if let Some(hint) = focus_hint {
                focus_window = state.member_tokens.get(hint).cloned();
                state.workspaces.note_foreground(hint);
            }
        } else {
            focus_outcome = "focus-suppressed";
        }
    } else {
        // Retained-aware focus set: maximized and fullscreen movers are
        // retained, never eligible-observed, so their tokens ride along for focus
        // only (geometry still excludes them via `writable_tokens`; focus carries
        // no write, KDE `requestFocus` parity). The foreground fence below still
        // never steals from a real arrival.
        let members = state.workspaces.workspace_members(output, target);
        let mut fresh_tokens: HashSet<String> =
            fresh_observed.iter().map(|w| w.token.clone()).collect();
        for row in &fresh_retained {
            if row.maximized || row.fullscreen {
                fresh_tokens.insert(row.token.clone());
            }
        }
        let eligible =
            state
                .workspaces
                .eligible_focus_set(&members, &state.member_tokens, &fresh_tokens);
        let focus_key = focus_hint
            .filter(|hint| eligible.contains(*hint))
            .cloned()
            .or_else(|| state.workspaces.focus_target(output, target, &eligible));
        if let Some(ref focus_key) = focus_key {
            let token = state
                .member_tokens
                .get(focus_key)
                .cloned()
                .unwrap_or_default();
            if !token.is_empty() {
                focus_window = Some(token.clone());
                if crate::workspace_owner::focus_before_geometry(
                    true,
                    suspend_read(state, me, fulls).veto.block,
                    foreground_elevated(me),
                ) {
                    let focus_start = Instant::now();
                    let actuation =
                        actuate_focus(state, me, fulls, &fresh_observed, &fresh_retained, &token);
                    focus_ms = focus_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                    focus_outcome = actuation.outcome;
                    if actuation.outcome == "focus-ok" {
                        state.workspaces.note_foreground(focus_key);
                    }
                } else {
                    // Fenced: a real fullscreen/elevated foreground arrived
                    // during the transition. Never steal; geometry below vetoes
                    // honestly instead of riding along.
                    focus_outcome = "focus-skipped-fence";
                }
            }
        }
    }
    // Bounded focus reconstruction for the next trace: opaque chosen token
    // (null when none) plus outcome under the action correlation.
    log_json_at(
        &state.log_path.clone(),
        serde_json::json!({"event":"select-focus","correlation":ctx.correlation,"window":focus_window,"focus":focus_outcome}),
    );
    let mut geometry: Option<ApplySummary> = None;
    // One shared hint-query budget for the select's row assembly. Floating
    // targets take no geometry: hide/reveal and focus above already ran, and
    // frames stay untouched.
    let mut hint_cx = HintCx::new();
    if workspace_mode_tiled(state, output, target)
        && let Some((domain, domain_key)) =
            workspace_domain_for(output, target, areas, state.inner_gap, state.outer_gap)
        && let Some(rows) = assemble_domain_rows(
            state,
            output,
            target,
            &fresh_observed,
            &fresh_retained,
            "select",
            ctx.correlation.as_str(),
            &mut hint_cx,
        )
    {
        let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let fp = fingerprint(
            &rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        // Foreground read after the focus above, so the Engine sees the
        // established target focus when eligible.
        let focused = state
            .focused_token(&fresh_observed)
            .filter(|f| windows.iter().any(|(w, _, _, _)| w == f));
        state.tick += 1;
        let tick = state.tick;
        let correlation =
            CorrelationId::parse(&ctx.correlation).expect("action correlation is a valid token");
        // Live gap edits adopt here so the select convergence below sees
        // matching gaps instead of refusing.
        {
            let revision = revision_for(state, output, target);
            let outer_gap = state.outer_gap;
            adopt_gaps_for_route(
                state,
                &domain,
                &domain_key,
                outer_gap,
                &windows,
                focused.as_ref(),
                revision,
                fp,
                &correlation,
            );
        }
        let plan_start = Instant::now();
        let event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            &correlation,
            revision_for(state, output, target),
            fp,
            &domain,
            &domain_key,
            state.outer_gap,
            &windows,
            focused.as_ref(),
        );
        let reply = state.engine.handle(&event);
        let plan_ms = plan_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        let writable = writable_tokens(state, output, target, &fresh_observed);
        let geometry_start = Instant::now();
        let summary = apply_geometry(
            state,
            ApplyInput {
                me,
                fulls,
                reply: &reply,
                observed: &fresh_observed,
                op: "reconcile",
                tick,
                correlation: ctx.correlation.as_str(),
                skipped: fresh_skipped,
                writable: &writable,
                output_token: state.workspaces.output_token(output),
                workspace_token: state.workspaces.workspace_token(output, target),
                revision: revision_for(state, output, target),
            },
        );
        let geometry_ms = geometry_start
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        geometry = summary;
        return SelectEffect {
            outcome: "ok",
            geometry,
            focus: focus_outcome,
            transition_ms,
            observation_ms,
            geometry_ms,
            hide_ms,
            reveal_ms,
            focus_ms,
            plan_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
        };
    }
    SelectEffect {
        outcome: "ok",
        geometry,
        focus: focus_outcome,
        transition_ms,
        observation_ms,
        geometry_ms: 0,
        hide_ms,
        reveal_ms,
        focus_ms,
        plan_ms: 0,
        source_workspace: source_token.clone(),
        target_workspace: target_token.clone(),
    }
}

/// Send target selector resolved once before transfer (item 2): numbered
/// digits resolve through the existing ordinal/trailing resolvers (index 0
/// reuses or appends the trailing empty); relative steps resolve from the
/// mover's source workspace through the item 1 scoped ring (wrapping through
/// the trailing empty and ordinals beyond 9, never MRU, never creating).
/// Filling the trailing empty invokes ordinary lifecycle for the next spare
/// after transfer; the frozen target never re-resolves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendTarget {
    Numbered(u8),
    Relative(i32),
}

impl SendTarget {
    fn log_index(self) -> u8 {
        match self {
            Self::Numbered(index) => index,
            Self::Relative(_) => 0,
        }
    }

    fn log_op(self) -> &'static str {
        match self {
            Self::Numbered(_) => "send",
            Self::Relative(delta) if delta < 0 => "send-prev",
            Self::Relative(_) => "send-next",
        }
    }
}

/// Carried pre-transfer send fences (item 2 + item 20): the pure fence
/// snapshot (both gaps, both modes, overlay flags) plus the retained-row
/// fullscreen fallback for unreadable live frames (never fabricated).
/// Tails re-verify before source writes AND hide/focus; any drift refuses
/// with no replay.
#[derive(Debug, Clone, Copy)]
struct SendPreflight {
    fences: crate::workspace_owner::SendFenceSnapshot,
    fallback_fullscreen: bool,
}

impl SendPreflight {
    /// Live tail-time readings for [`crate::workspace_owner::send_fences_hold`]:
    /// fresh view/mode/gap/scope/lifetime/membership/overlay state. Scope and
    /// membership are in-memory session reads; lifetime re-reads the live
    /// member tag; overlay re-reads the live OS flags against the monitor
    /// full rects with the carried fallback for unreadable frames.
    #[allow(clippy::too_many_arguments)]
    fn live(
        &self,
        state: &TileLoop,
        stored: &ProcessIdentity,
        mover_key: &crate::workspace::WindowKey,
        mover_hwnd: u64,
        output: &str,
        source_id: &str,
        target_id: &str,
        fulls: &[Rect],
    ) -> crate::workspace_owner::SendFenceLive {
        let active = state.workspaces.active_id(output);
        let (live_max, live_full_opt) = live_overlay_flags(mover_hwnd, fulls);
        crate::workspace_owner::SendFenceLive {
            inner_gap: state.inner_gap,
            outer_gap: state.outer_gap,
            source_tiled: state.workspaces.is_tiled(output, source_id),
            target_tiled: state.workspaces.is_tiled(output, target_id),
            active_is_source: active.as_deref() == Some(source_id),
            target_is_active: active.as_deref() == Some(target_id),
            scope_ok: scope_allows(&state.scope, &stored.exe_path)
                && hosted_gate_allows(&stored.exe_path, mover_hwnd, stored.pid, &state.scope_hosts),
            lifetime_ok: {
                let live_tag = crate::product_hide::sys::read_member_tag(mover_hwnd);
                state.member_tags.get(mover_key).is_some_and(|tag| {
                    crate::workspace_owner::visible_lifetime_ok(tag, live_tag.as_deref())
                })
            },
            membership_ok: state
                .workspaces
                .member_loc(mover_key)
                .is_some_and(|loc| loc.workspace == target_id),
            overlay_maximized: live_max,
            overlay_fullscreen: live_full_opt.unwrap_or(self.fallback_fullscreen),
        }
    }

    /// Live overlay-only recheck before native effects (item 9): a newly
    /// arrived fullscreen refuses, other drift defers. Narrower than
    /// [`crate::workspace_owner::send_fences_hold`]; follow tails (whose
    /// selects re-verify everything else) use this.
    fn overlay_hold(
        &self,
        mover_hwnd: u64,
        fulls: &[Rect],
    ) -> std::result::Result<(), &'static str> {
        let (live_max, live_full_opt) = live_overlay_flags(mover_hwnd, fulls);
        crate::workspace_owner::send_overlay_gate(
            self.fences.overlay_maximized,
            self.fences.overlay_fullscreen,
            live_max,
            live_full_opt,
            self.fallback_fullscreen,
        )
    }
}

/// Native cross-boundary send for any boundary touching a floating
/// workspace: transfer project native membership (verified exactly like the
/// Engine route), reflow the tiled source survivors when the source is tiled,
/// then follow through the shared tail so the target reveals and reconciles
/// when tiled (`follow=true`), or hide the mover and keep the source view
/// with its existing native boundary focus (`follow=false`, no target
/// selection, no history record). No two-domain Engine plan, no
/// floating-side geometry; the tiled target admits normally through the
/// ordinary follow select. Sticky movers never transfer as a
/// single-desktop write; intentional per-window float movers ride along only
/// from an actually floating source.
#[allow(clippy::too_many_arguments)]
fn workspace_do_send_native(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    output: &str,
    target_id: &str,
    mover_key: &crate::workspace::WindowKey,
    mover_hwnd: u64,
    source_token: &str,
    target_token: &str,
    source_tiled: bool,
    target_tiled: bool,
    ctx: &ActionCtx,
    follow: bool,
) -> SendEffect {
    let fail_at = |outcome: &'static str| SendEffect {
        outcome,
        focus: "none",
        source: None,
        target: None,
        transition_ms: 0,
        observation_ms: 0,
        source_geometry_ms: 0,
        target_geometry_ms: 0,
        hide_ms: 0,
        reveal_ms: 0,
        focus_ms: 0,
        source_plan_ms: 0,
        target_plan_ms: 0,
        source_workspace: source_token.to_owned(),
        target_workspace: target_token.to_owned(),
    };
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let Some(stored) = state.member_identity.get(mover_key).cloned() else {
        return fail_at("unmanaged");
    };
    let source_workspace = state
        .workspaces
        .member_loc(mover_key)
        .map(|loc| loc.workspace.clone())
        .unwrap_or_default();
    let mut snap_overlay = (false, false);
    let mut fallback_fullscreen = false;
    let mover_minimized = if let Some(fresh) = by_hwnd.get(&mover_hwnd) {
        if !crate::workspace_owner::member_matches(
            mover_key,
            fresh.hwnd,
            fresh.identity.pid,
            &fresh.identity.process_creation,
        ) || fresh.identity.exe_path != stored.exe_path
            || fresh.identity.user_sid != stored.user_sid
            || fresh.identity.session_id != stored.session_id
        {
            return fail_at("origin-vanished");
        }
        fresh.facts.minimized
    } else if let Some(row) = retained.iter().find(|r| r.key == *mover_key) {
        if !crate::workspace_owner::send_retained_overlay_admits(row.maximized, row.fullscreen) {
            return fail_at("origin-vanished");
        }
        // Item 9 live snapshot (not the retained row): a stable
        // maximized/fullscreen snapshot carries without restoring; a newly
        // arrived live fullscreen refuses outright, other drift defers, and
        // an unreadable live flag defers a fullscreen snapshot (never trust
        // the fallback before effects). Maximized/fullscreen carry itself
        // never restores.
        let (live_max, live_full_opt) = live_overlay_flags(mover_hwnd, fulls);
        if let Err(outcome) = crate::workspace_owner::send_overlay_gate(
            row.maximized,
            row.fullscreen,
            live_max,
            live_full_opt,
            row.fullscreen,
        ) {
            return fail_at(outcome);
        }
        snap_overlay = (row.maximized, row.fullscreen);
        fallback_fullscreen = row.fullscreen;
        if stored.pid != mover_key.pid || stored.process_creation != mover_key.creation {
            return fail_at("origin-vanished");
        }
        false
    } else {
        return fail_at("origin-vanished");
    };
    let post_tag = crate::product_hide::sys::read_member_tag(mover_hwnd);
    if !state
        .member_tags
        .get(mover_key)
        .is_some_and(|tag| crate::workspace_owner::visible_lifetime_ok(tag, post_tag.as_deref()))
    {
        return fail_at("identity-changed");
    }
    // Item 9 pre-transfer overlay gate with live reads (mirrors the Engine
    // route's pre-assign check): an observed mover that went borderless
    // fullscreen live refuses here with NO membership change; other drift
    // from the snapshot defers the same way. The post-transfer recheck below
    // stays for races across the transfer itself.
    {
        let (live_max, live_full_opt) = live_overlay_flags(mover_hwnd, fulls);
        if let Err(outcome) = crate::workspace_owner::send_overlay_gate(
            snap_overlay.0,
            snap_overlay.1,
            live_max,
            live_full_opt,
            fallback_fullscreen,
        ) {
            return fail_at(outcome);
        }
    }
    if !state
        .workspaces
        .assign(mover_key.clone(), output, target_id, true)
    {
        return fail_at("refused");
    }
    let source_now = state
        .workspaces
        .workspace_members(output, &source_workspace);
    let target_now = state.workspaces.workspace_members(output, target_id);
    if !crate::workspace_owner::verify_membership_transfer(mover_key, &source_now, &target_now) {
        return fail_at("unverified");
    }
    let preflight = SendPreflight {
        fences: crate::workspace_owner::SendFenceSnapshot {
            inner_gap: state.inner_gap,
            outer_gap: state.outer_gap,
            source_tiled,
            target_tiled,
            overlay_maximized: snap_overlay.0,
            overlay_fullscreen: snap_overlay.1,
        },
        fallback_fullscreen,
    };
    // Item 20 pre-effect recheck with live reads: refuse fullscreen, defer
    // drift, before any reflow/hide/focus.
    if let Err(outcome) = preflight.overlay_hold(mover_hwnd, fulls) {
        return fail_at(outcome);
    }
    // Tiled-source survivor reflow before hide/follow: the transfer above
    // already moved the mover out, so a complete reconcile on current
    // observed geometry converges the survivors (no layout hole left in the
    // retained Engine session). No two-domain plan, no floating-side writes:
    // floating sources skip entirely (frames untouched), and the follow
    // select admits the target normally below. Best-effort: an incomplete
    // source observation skips the reflow and the follow still runs, with
    // convergence on the next return select.
    if source_tiled {
        let mut hint_cx = HintCx::new();
        if let Some(source_rows) = assemble_domain_rows(
            state,
            output,
            &source_workspace,
            observed,
            retained,
            "send-source",
            ctx.correlation.as_str(),
            &mut hint_cx,
        ) && let Some((source_domain, source_key)) = workspace_domain_for(
            output,
            &source_workspace,
            areas,
            state.inner_gap,
            state.outer_gap,
        ) {
            let source_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = source_rows
                .iter()
                .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
                .collect();
            let source_fp = fingerprint(
                &source_rows
                    .iter()
                    .map(|r| (r.token.clone(), r.rect))
                    .collect::<Vec<_>>(),
            );
            let focused = state
                .focused_token(observed)
                .filter(|f| source_windows.iter().any(|(w, _, _, _)| w == f));
            let revision = revision_for(state, output, &source_workspace);
            let action_correlation = CorrelationId::parse(&ctx.correlation)
                .expect("action correlation is a valid token");
            adopt_gaps_for_route(
                state,
                &source_domain,
                &source_key,
                state.outer_gap,
                &source_windows,
                focused.as_ref(),
                revision,
                source_fp,
                &action_correlation,
            );
            let event = crate::tiling::build_reconcile_event_for_floating(
                &state.owner,
                &state.generation,
                &action_correlation,
                revision,
                source_fp,
                &source_domain,
                &source_key,
                state.outer_gap,
                &source_windows,
                focused.as_ref(),
            );
            let reply = state.engine.handle(&event);
            let writable = writable_tokens(state, output, &source_workspace, observed);
            state.tick += 1;
            let source_tick = state.tick;
            apply_geometry(
                state,
                ApplyInput {
                    me,
                    fulls,
                    reply: &reply,
                    observed,
                    op: "send-source",
                    tick: source_tick,
                    correlation: ctx.correlation.as_str(),
                    skipped: Vec::new(),
                    writable: &writable,
                    output_token: state.workspaces.output_token(output),
                    workspace_token: state.workspaces.workspace_token(output, &source_workspace),
                    revision: revision_for(state, output, &source_workspace),
                },
            );
        }
    }
    let effect = if follow {
        workspace_send_follow(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            observed,
            mover_key,
            &stored,
            mover_minimized,
            output,
            target_id,
            source_token,
            target_token,
            ctx,
            None,
            0,
            0,
            preflight,
        )
    } else {
        workspace_send_stay_native(
            state,
            me,
            store,
            dir,
            fulls,
            mover_key,
            &stored,
            mover_minimized,
            mover_hwnd,
            output,
            &source_workspace,
            target_id,
            source_token,
            target_token,
            preflight,
        )
    };
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "workspace-send-native",
            "source_tiled": source_tiled,
            "target_tiled": target_tiled,
            "follow": follow,
            "outcome": effect.outcome,
        }),
    );
    effect
}

/// Ordinary trailing lifecycle after a stay send filled the trailing empty:
/// keep one empty with a minimum of two, preserving the active view. Runs
/// without activation or selection, so no history records; removal
/// invalidation still applies (a removed previous clears like any cleanup).
/// Shared by the Engine and native stay tails.
fn ensure_trailing_spare(state: &mut TileLoop, output: &str) {
    let active_id = state.workspaces.active_id(output).unwrap_or_default();
    let displaced: Vec<String> = state
        .workspaces
        .displaced_snapshot()
        .values()
        .flat_map(|r| r.workspace_ids.clone())
        .collect();
    let sticky_keys: BTreeSet<crate::workspace::WindowKey> = state.sticky.keys().cloned().collect();
    let (removed, append) = state.workspaces.plan_cleanup_excluding(
        output,
        std::slice::from_ref(&active_id),
        &displaced,
        &sticky_keys,
    );
    state.workspaces.apply_cleanup(output, &removed, append);
}

/// Native stay tail for a floating-boundary send: the mover is already
/// transferred (hidden membership) and the tiled source already reflowed by
/// the caller. Re-verifies the carried fences before hide (item 2 + item 20),
/// then hides the mover from the source view, keeps the source selected and
/// visible with its existing native boundary focus (no target selection, no
/// focus setter, no history record), then runs ordinary trailing lifecycle
/// for the next spare. Floating frames stay untouched; only the tiled side
/// reflowed. Partial progress never replays: a failed hide reports without
/// focus or reveal attempts.
#[allow(clippy::too_many_arguments)]
fn workspace_send_stay_native(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    mover_key: &crate::workspace::WindowKey,
    stored: &ProcessIdentity,
    mover_minimized: bool,
    mover_hwnd: u64,
    output: &str,
    source_id: &str,
    target_id: &str,
    source_token: &str,
    target_token: &str,
    preflight: SendPreflight,
) -> SendEffect {
    // Carried pre-transfer fences before hide (item 2 + item 20). The source
    // id rides the caller (membership already points at the target).
    if let Some(outcome) = crate::workspace_owner::send_fences_hold(
        preflight.fences,
        preflight.live(
            state, stored, mover_key, mover_hwnd, output, source_id, target_id, fulls,
        ),
    ) {
        if outcome == "identity-changed" {
            drop_member_state(state, mover_key);
        }
        return SendEffect {
            outcome,
            focus: "none",
            source: None,
            target: None,
            transition_ms: 0,
            observation_ms: 0,
            source_geometry_ms: 0,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms: 0,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        };
    }
    let (hide_outcome, _) =
        workspace_hide_one(state, me, store, dir, mover_key, stored, mover_minimized);
    if state.hidden_claims.contains_key(mover_key) {
        state.workspaces.set_hidden(mover_key, true);
    }
    if hide_outcome != "hidden" {
        return SendEffect {
            outcome: hide_outcome,
            focus: "none",
            source: None,
            target: None,
            transition_ms: 0,
            observation_ms: 0,
            source_geometry_ms: 0,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms: 0,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        };
    }
    // Ordinary lifecycle supplies the next spare when the transfer filled the
    // trailing empty; the source view stays selected throughout.
    ensure_trailing_spare(state, output);
    SendEffect {
        outcome: "ok",
        focus: "none",
        source: None,
        target: None,
        transition_ms: 0,
        observation_ms: 0,
        source_geometry_ms: 0,
        target_geometry_ms: 0,
        hide_ms: 0,
        reveal_ms: 0,
        focus_ms: 0,
        source_plan_ms: 0,
        target_plan_ms: 0,
        source_workspace: source_token.to_owned(),
        target_workspace: target_token.to_owned(),
    }
}

/// Send the focused window to a same-output workspace, then follow or stay.
/// Tiled-to-tiled sends run the retained Engine `MoveToWorkspace` route with
/// source reflow; any boundary touching a floating workspace transfers
/// project native membership instead (no two-domain Engine plan, no
/// floating-side geometry) and reflows only the tiled side through the
/// ordinary follow select. `target` resolves once before transfer (numbered
/// ordinal/trailing, or relative ring step from the mover source); `follow`
/// pins the explicit intent (CLI sends follow). Follow selects/reveals the
/// target and focuses the mover; stay hides the mover without selecting or
/// revealing the target, keeps the source selected/visible, applies the
/// source-bound plan focus (MRU, none for null) and reflows only writable
/// source rows. Refuses unmanaged focus, no-op/foreign transfers, and proof
/// modes without workspace hides.
///
/// Source reflow consumes the existing `SendWorkspace` plan while the source
/// survivors are still visible and eligible in the same action (scoped by the
/// source writable set; hidden/retained rows never take writes). The follow
/// select then reveals the target, establishes mover focus before geometry
/// with all fences retained, and reconciles the destination in the same
/// action. The select's fresh focus/observation is reused: no redundant
/// post-follow enumeration. Real fullscreen/unreadable/identity/incomplete
/// observation continues to block writes honestly; `ok` never implies
/// applied.
#[allow(clippy::too_many_arguments)]
fn workspace_do_send(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    output: &str,
    target: SendTarget,
    follow: bool,
    mover_hwnd: u64,
    origin_token: &str,
    origin_pid: u32,
    origin_creation: &str,
    ctx: &ActionCtx,
) -> SendEffect {
    let fail = |outcome: &'static str| SendEffect {
        outcome,
        focus: "none",
        source: None,
        target: None,
        transition_ms: 0,
        observation_ms: 0,
        source_geometry_ms: 0,
        target_geometry_ms: 0,
        hide_ms: 0,
        reveal_ms: 0,
        focus_ms: 0,
        source_plan_ms: 0,
        target_plan_ms: 0,
        source_workspace: "ws?".to_owned(),
        target_workspace: "ws?".to_owned(),
    };
    let mover_key = state
        .member_tokens
        .iter()
        .find(|(_, t)| t.as_str() == origin_token)
        .map(|(k, _)| k.clone());
    let Some(mover_key) = mover_key else {
        return fail("unmanaged");
    };
    // Full chord-time binding: token plus HWND, PID, and process creation.
    // A recycled HWND or a retargeted token never dispatches.
    if !crate::workspace_owner::member_matches(&mover_key, mover_hwnd, origin_pid, origin_creation)
    {
        return fail("foreground-changed");
    }
    let Some(loc) = state.workspaces.member_loc(&mover_key).cloned() else {
        return fail("unmanaged");
    };
    if loc.output != output || state.workspaces.is_hidden(&mover_key) {
        return fail("unmanaged");
    }
    // Floats never send as movers on the Engine route; floating survivors
    // ride the carried rows. Sticky rides the same slotless float plus an
    // explicit subject gate so a pruned-domain sticky still refuses.
    // Floating-source boundary sends skip this refusal (eligibility derives
    // from the actual source below): their movers ride the native route.
    if state.sticky.contains_key(&mover_key) {
        return fail("send-refused-sticky");
    }
    if state.workspaces.is_tiled(&loc.output, &loc.workspace)
        && engine_is_float(state, &loc.output, &loc.workspace, origin_token)
    {
        return fail("send-refused-floating");
    }
    // Explicit scope fences every send before the Engine mutation: an
    // out-of-scope mover reports without touching Engine sessions,
    // membership, or layout. The hide path re-fences independently.
    if let Some(stored) = state.member_identity.get(&mover_key) {
        if !scope_allows(&state.scope, &stored.exe_path) {
            return fail("scope-excluded");
        }
        // Listed hosts send only with a live matching hosted child: a newly
        // appearing hosted app never dispatches under another app's
        // membership. The hide path re-fences fresh independently.
        if !hosted_gate_allows(&stored.exe_path, mover_hwnd, stored.pid, &state.scope_hosts) {
            return fail("scope-excluded");
        }
    }
    // Visible lifetime gate: the live member tag must equal the stored tag,
    // so a same-process HWND reuse (same HWND/PID/creation, fresh window)
    // dispatches nothing. The stale membership drops here; the next tick
    // re-admits the live window as brand new with no inheritance.
    let live_tag = crate::product_hide::sys::read_member_tag(mover_hwnd);
    let lifetime_ok = state.member_tags.get(&mover_key).is_some_and(|stored| {
        crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
    });
    if !lifetime_ok {
        drop_member_state(state, &mover_key);
        return fail("identity-changed");
    }
    if state.allowlist.is_some() && !state.workspace_proof {
        return fail("workspace-disabled");
    }
    let target_id = match target {
        SendTarget::Numbered(0) => {
            // Trailing send reuses or creates without switching first.
            match state.workspaces.resolve_send_trailing(output) {
                Some((id, _)) => id,
                None => return fail("unknown-output"),
            }
        }
        SendTarget::Numbered(index) => match state.workspaces.resolve_send(output, index) {
            Some(id) => id,
            None => return fail("unknown-target"),
        },
        SendTarget::Relative(delta) => {
            // Frozen once before transfer: the item 1 scoped ring step from
            // the mover's source workspace (wrapping through the trailing
            // empty and ordinals beyond 9, never MRU, never creating).
            // Filling the trailing empty invokes ordinary lifecycle for the
            // next spare after transfer.
            match state
                .workspaces
                .resolve_relative_from(output, &loc.workspace, delta)
            {
                Some(id) => id,
                None => return fail("unknown-target"),
            }
        }
    };
    if target_id == loc.workspace {
        return fail("no-op");
    }
    let source_token = state.workspaces.workspace_token(output, &loc.workspace);
    let target_token = state.workspaces.workspace_token(output, &target_id);
    let fail_at = |outcome: &'static str| SendEffect {
        outcome,
        focus: "none",
        source: None,
        target: None,
        transition_ms: 0,
        observation_ms: 0,
        source_geometry_ms: 0,
        target_geometry_ms: 0,
        hide_ms: 0,
        reveal_ms: 0,
        focus_ms: 0,
        source_plan_ms: 0,
        target_plan_ms: 0,
        source_workspace: source_token.clone(),
        target_workspace: target_token.clone(),
    };
    // Cross-boundary route from the actual workspace modes: tiled-to-tiled
    // runs the shared two-domain Engine plan below; any floating boundary
    // transfers project native membership instead with no plan and no
    // floating-side geometry, reflows only the tiled side, and follows only
    // after the existing exact membership confirmation.
    let source_tiled = state.workspaces.is_tiled(output, &loc.workspace);
    let target_tiled = state.workspaces.is_tiled(output, &target_id);
    if crate::workspace_owner::send_route(source_tiled, target_tiled)
        == crate::workspace_owner::SendRoute::Native
    {
        return workspace_do_send_native(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            observed,
            retained,
            output,
            &target_id,
            &mover_key,
            mover_hwnd,
            &source_token,
            &target_token,
            source_tiled,
            target_tiled,
            ctx,
            follow,
        );
    }
    // Full source+target observations including hidden snapshots, so the
    // planned mutation reuses topology instead of remove/reseed. Either side
    // incomplete defers with retained state, never a falsely complete pair.
    // One shared hint-query budget across both assemblies: source and target
    // never independently blow the per-operation bound.
    //
    // Pre-dispatch overlay gate (item 9): a retained maximized/fullscreen
    // member is retained, never eligible, so it must read its overlay flags
    // live (not the retained row) before the Engine plans; a stable
    // maximized/fullscreen snapshot carries without restoring, with
    // retained-row fallback for unreadable frames, never a fabricated value.
    // Observed movers ride the ordinary (false, false) expectation from
    // observation; the post-plan recheck below gates any live race before
    // effects. A tiled maximized/fullscreen member sends (carry, never
    // restore); a newly fullscreened ordinary mover refuses with no writes.
    let mover_observed = observed.iter().any(|w| w.hwnd == mover_hwnd);
    let (snap_overlay, fallback_fullscreen) = if mover_observed {
        ((false, false), false)
    } else {
        let Some(row) = retained.iter().find(|r| r.key == mover_key) else {
            return fail_at("origin-vanished");
        };
        // Actual KDE wrapper behavior (workspace-send `non-tiled-focus`):
        // overlay movers never send as ordinary.
        if !crate::workspace_owner::send_retained_overlay_admits(row.maximized, row.fullscreen) {
            return fail_at("origin-vanished");
        }
        let (live_max, live_full_opt) = live_overlay_flags(mover_hwnd, fulls);
        if let Err(outcome) = crate::workspace_owner::send_overlay_gate(
            row.maximized,
            row.fullscreen,
            live_max,
            live_full_opt,
            row.fullscreen,
        ) {
            return fail_at(outcome);
        }
        ((row.maximized, row.fullscreen), row.fullscreen)
    };
    let preflight = SendPreflight {
        fences: crate::workspace_owner::SendFenceSnapshot {
            inner_gap: state.inner_gap,
            outer_gap: state.outer_gap,
            source_tiled,
            target_tiled,
            overlay_maximized: snap_overlay.0,
            overlay_fullscreen: snap_overlay.1,
        },
        fallback_fullscreen,
    };
    let mut hint_cx = HintCx::new();
    let Some(source_rows) = assemble_domain_rows(
        state,
        output,
        &loc.workspace,
        observed,
        retained,
        "send",
        ctx.correlation.as_str(),
        &mut hint_cx,
    ) else {
        return fail_at("deferred");
    };
    let Some(target_rows) = assemble_domain_rows(
        state,
        output,
        &target_id,
        observed,
        retained,
        "send",
        ctx.correlation.as_str(),
        &mut hint_cx,
    ) else {
        return fail_at("deferred");
    };
    if !source_rows.iter().any(|r| r.token == origin_token) {
        return fail_at("unmanaged");
    }
    let Some((source_domain, source_key)) = workspace_domain_for(
        output,
        &loc.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        return fail_at("unknown-output");
    };
    let Some((target_domain, target_key)) =
        workspace_domain_for(output, &target_id, areas, state.inner_gap, state.outer_gap)
    else {
        return fail_at("unknown-output");
    };
    let fingerprint = {
        let mut pairs: Vec<(String, Rect)> = source_rows
            .iter()
            .chain(target_rows.iter())
            .map(|r| (r.token.clone(), r.rect))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        fingerprint(
            &pairs
                .iter()
                .map(|(t, r)| (t.clone(), *r))
                .collect::<Vec<_>>(),
        )
    };
    let action_correlation =
        CorrelationId::parse(&ctx.correlation).expect("action correlation is a valid token");
    let revision = revision_for(state, output, &loc.workspace);
    // Live gap edits adopt on both domains first, so the pair convergence
    // below never straddles gap generations (an unseen target would
    // otherwise seed fresh with new gaps beside a retained stale source).
    {
        let outer_gap = state.outer_gap;
        let mover = WindowId(origin_token.to_owned());
        let source_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = source_rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let source_fp = crate::tiling::fingerprint(
            &source_rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        adopt_gaps_for_route(
            state,
            &source_domain,
            &source_key,
            outer_gap,
            &source_windows,
            Some(&mover),
            revision,
            source_fp,
            &action_correlation,
        );
        let target_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = target_rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let target_fp = crate::tiling::fingerprint(
            &target_rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        let target_revision = revision_for(state, output, &target_id);
        adopt_gaps_for_route(
            state,
            &target_domain,
            &target_key,
            outer_gap,
            &target_windows,
            None,
            target_revision,
            target_fp,
            &action_correlation,
        );
    }
    let Some(mut event) = crate::workspace_owner::build_send_event(
        &state.owner,
        &state.generation,
        &action_correlation,
        revision,
        fingerprint,
        (source_domain, source_key.clone()),
        (target_domain, target_key.clone()),
        &source_rows,
        &target_rows,
        origin_token,
        state.outer_gap,
        follow,
    ) else {
        return fail_at("refused");
    };
    crate::workspace_owner::stamp_send_target(&mut event, &target_key);
    let source_plan_start = Instant::now();
    let reply = state.engine.handle(&event);
    log_engine_placement_trace(
        state,
        ctx.tick,
        ctx.correlation.as_str(),
        "send-to-workspace",
    );
    let source_plan_ms = source_plan_start
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let tiler_core::boundary::CoreReply::SendWorkspace(plan) = &reply else {
        let outcome: &'static str = match reply {
            tiler_core::boundary::CoreReply::Rejected { kind, .. } => kind,
            tiler_core::boundary::CoreReply::Diverged(reason) => reason.as_str(),
            tiler_core::boundary::CoreReply::SnapshotInvalid { detail, .. } => detail,
            _ => "refused",
        };
        return fail_at(outcome);
    };
    // Project-owned membership change after revalidation and the planned
    // Engine mutation: exact identity table update, never HWND alone. A
    // tiled maximized/fullscreen member sends too (KDE parity): it is
    // retained rather than eligible, so it resolves through its retained row
    // with a live flag recheck (not the retained row) instead of the eligible
    // observation. Its target allocation is kept by the plan while overlay
    // geometry never writes.
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let Some(stored) = state.member_identity.get(&mover_key).cloned() else {
        return fail_at("unmanaged");
    };
    // Item 9 post-plan recheck with live OS reads (not the retained row):
    // a newly arrived fullscreen refuses, other drift defers. Before any
    // membership change or native effect, for follow and stay alike.
    {
        let (live_max, live_full_opt) = live_overlay_flags(mover_hwnd, fulls);
        if let Err(outcome) = crate::workspace_owner::send_overlay_gate(
            preflight.fences.overlay_maximized,
            preflight.fences.overlay_fullscreen,
            live_max,
            live_full_opt,
            preflight.fallback_fullscreen,
        ) {
            return fail_at(outcome);
        }
    }
    let mover_minimized = if let Some(fresh) = by_hwnd.get(&mover_hwnd) {
        if !crate::workspace_owner::member_matches(
            &mover_key,
            fresh.hwnd,
            fresh.identity.pid,
            &fresh.identity.process_creation,
        ) || fresh.identity.exe_path != stored.exe_path
            || fresh.identity.user_sid != stored.user_sid
            || fresh.identity.session_id != stored.session_id
        {
            return fail_at("origin-vanished");
        }
        fresh.facts.minimized
    } else if let Some(row) = retained.iter().find(|r| r.key == mover_key) {
        // Retained maximized/fullscreen mover: the live overlay gate above
        // already refused a newly arrived fullscreen and deferred drift; only
        // a maximized/fullscreen member proceeds, with no restore and no
        // remaximize (overlay carry).
        if !crate::workspace_owner::send_retained_overlay_admits(row.maximized, row.fullscreen) {
            return fail_at("origin-vanished");
        }
        if stored.pid != mover_key.pid || stored.process_creation != mover_key.creation {
            return fail_at("origin-vanished");
        }
        false
    } else {
        return fail_at("origin-vanished");
    };
    // Post-Engine lifetime recheck: a same-process reuse across the in-memory
    // mutation still refuses before any membership change or hide.
    let post_tag = crate::product_hide::sys::read_member_tag(mover_hwnd);
    if !state
        .member_tags
        .get(&mover_key)
        .is_some_and(|tag| crate::workspace_owner::visible_lifetime_ok(tag, post_tag.as_deref()))
    {
        return fail_at("identity-changed");
    }
    if !state
        .workspaces
        .assign(mover_key.clone(), output, &target_id, true)
    {
        return fail_at("refused");
    }
    // Verified transfer before follow/stay: absent source, present target.
    let source_now = state.workspaces.workspace_members(output, &loc.workspace);
    let target_now = state.workspaces.workspace_members(output, &target_id);
    if !crate::workspace_owner::verify_membership_transfer(&mover_key, &source_now, &target_now) {
        return fail_at("unverified");
    }
    // Carried pre-plan fences before source writes for both intents
    // (item 2 + item 20): view/mode/gaps/scope/lifetime/membership/overlay
    // re-verified live; any drift refuses with no further effects and no
    // replay. A lifetime refusal drops the stale membership like the
    // dispatch-time gate.
    if let Some(outcome) = crate::workspace_owner::send_fences_hold(
        preflight.fences,
        preflight.live(
            state,
            &stored,
            &mover_key,
            mover_hwnd,
            output,
            &loc.workspace,
            &target_id,
            fulls,
        ),
    ) {
        if outcome == "identity-changed" {
            drop_member_state(state, &mover_key);
        }
        return fail_at(outcome);
    }
    // The Engine echoes the explicit intent: follow consumes the source plan
    // while survivors are visible, then selects/reveals the target; stay
    // fences first, then reflows only writable source rows, hides without
    // selecting, and applies the source-bound plan focus (never the target).
    if !plan.follow {
        return workspace_send_stay(
            state,
            me,
            store,
            dir,
            fulls,
            observed,
            retained,
            &mover_key,
            &stored,
            mover_minimized,
            mover_hwnd,
            output,
            &loc.workspace,
            &target_id,
            &source_token,
            &target_token,
            ctx,
            plan,
            preflight,
            source_plan_ms,
        );
    }
    // Consume the existing source plan while the survivors are still visible
    // and eligible in the same action. The writable set scopes to the source
    // domain (mover already transferred out, hidden/retained rows never
    // writable), so target entries skip honestly as retained and hidden
    // geometry is never written. All write fences stay inside
    // `apply_geometry` (identity/lifetime/scope, fullscreen veto, readback).
    // The internal tick reuses the parent action correlation.
    let source_writable = writable_tokens(state, output, &loc.workspace, observed);
    state.tick += 1;
    let source_tick = state.tick;
    let source_start = Instant::now();
    let source_summary = apply_geometry(
        state,
        ApplyInput {
            me,
            fulls,
            reply: &reply,
            observed,
            op: "send-source",
            tick: source_tick,
            correlation: ctx.correlation.as_str(),
            skipped: Vec::new(),
            writable: &source_writable,
            output_token: state.workspaces.output_token(output),
            workspace_token: state.workspaces.workspace_token(output, &loc.workspace),
            revision: revision_for(state, output, &loc.workspace),
        },
    );
    let source_ms = source_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let source = source_summary;
    // Hide the mover from the source view (commit-before-hide), then follow
    // by selecting the target (which reveals it). The select reuses the mover
    // as its focus hint and returns its fresh focus/observation: no redundant
    // post-follow enumeration here. A committed claim is owned even on
    // uncertain post-hide readback: flag it hidden and report the stall
    // without pretending follow success.
    workspace_send_follow(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        observed,
        &mover_key,
        &stored,
        mover_minimized,
        output,
        &target_id,
        &source_token,
        &target_token,
        ctx,
        source,
        source_ms,
        source_plan_ms,
        preflight,
    )
}

/// Shared send tail for both the Engine plan route and the native
/// cross-boundary route: commit-before-hide the mover, then follow by
/// selecting the target (which reveals it and reconciles it when tiled).
/// The carried overlay snapshot re-verifies live before hide (item 9):
/// a newly arrived fullscreen refuses, other drift defers, with no restore
/// and no replay. Broader view/mode/gap races stay with the select's own
/// fresh fences below.
#[allow(clippy::too_many_arguments)]
fn workspace_send_follow(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    mover_key: &crate::workspace::WindowKey,
    stored: &ProcessIdentity,
    mover_minimized: bool,
    output: &str,
    target_id: &str,
    source_token: &str,
    target_token: &str,
    ctx: &ActionCtx,
    source: Option<ApplySummary>,
    source_ms: u64,
    source_plan_ms: u64,
    preflight: SendPreflight,
) -> SendEffect {
    if let Err(outcome) = preflight.overlay_hold(mover_key.hwnd, fulls) {
        return SendEffect {
            outcome,
            focus: "none",
            source,
            target: None,
            transition_ms: 0,
            observation_ms: 0,
            source_geometry_ms: source_ms,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        };
    }
    let (hide_outcome, _) =
        workspace_hide_one(state, me, store, dir, mover_key, stored, mover_minimized);
    if state.hidden_claims.contains_key(mover_key) {
        state.workspaces.set_hidden(mover_key, true);
    }
    if hide_outcome != "hidden" {
        return SendEffect {
            outcome: hide_outcome,
            focus: "none",
            source,
            target: None,
            transition_ms: 0,
            observation_ms: 0,
            source_geometry_ms: source_ms,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        };
    }
    // Item 9 follow-focus policy: a fullscreen snapshot suppresses ALL
    // explicit focus actuation for this transition (select still reveals the
    // target; observed focus reports honestly, suppressed reports success).
    let suppress_focus =
        crate::workspace_owner::send_focus_suppressed(preflight.fences.overlay_fullscreen);
    let select = workspace_do_select(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        observed,
        output,
        target_id,
        ctx,
        Some(mover_key),
        suppress_focus,
    );
    if select.outcome != "ok" {
        return SendEffect {
            outcome: select.outcome,
            focus: select.focus,
            source,
            target: select.geometry,
            transition_ms: select.transition_ms,
            observation_ms: select.observation_ms,
            source_geometry_ms: source_ms,
            target_geometry_ms: select.geometry_ms,
            hide_ms: select.hide_ms,
            reveal_ms: select.reveal_ms,
            focus_ms: select.focus_ms,
            source_plan_ms,
            target_plan_ms: select.plan_ms,
            source_workspace: source_token.to_owned(),
            target_workspace: select.target_workspace.clone(),
        };
    }
    // Item 9: a suppressed fullscreen follow reports success honestly when no
    // explicit focus was required (`focus-suppressed`); observed focus still
    // reports `focus-ok`. Ordinary/maximized follows keep the verified-only
    // mapping below.
    let suppressed_ok = suppress_focus && select.focus == "focus-suppressed";
    let outcome: &'static str = if select.focus == "focus-ok" || suppressed_ok {
        "ok"
    } else {
        "focus-unverified"
    };
    SendEffect {
        outcome,
        focus: select.focus,
        source,
        target: select.geometry,
        transition_ms: select.transition_ms,
        observation_ms: select.observation_ms,
        source_geometry_ms: source_ms,
        target_geometry_ms: select.geometry_ms,
        hide_ms: select.hide_ms,
        reveal_ms: select.reveal_ms,
        focus_ms: select.focus_ms,
        source_plan_ms,
        target_plan_ms: select.plan_ms,
        source_workspace: source_token.to_owned(),
        target_workspace: select.target_workspace.clone(),
    }
}

/// Source-bound stay focus resolution from the Engine plan (item 2): the
/// plan's `focus_domain`/`focus_leaf` names the tiled layout focus after a
/// stay send (source focused-removal MRU, null when no source tiled entry
/// remains). Gathers the Engine snapshot mapping plus exact source/member
/// lifetime and fresh focus eligibility, then routes through
/// [`crate::workspace_owner::classify_stay_focus`]: `Null` is a genuine null
/// plan (no setter; the native removal focus stands, never a guessed
/// fallback), `Stale` is a some-but-unusable mapping (no setter either), and
/// `Ready` carries the MRU token for the single actuation attempt.
fn stay_focus_token(
    state: &TileLoop,
    output: &str,
    source_id: &str,
    plan: &tiler_core::boundary::SendWorkspacePlan,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) -> (crate::workspace_owner::StayFocusClass, Option<String>) {
    use crate::workspace_owner::{StayFocusClass, classify_stay_focus};
    let plan_null = plan.focus_domain.is_none() || plan.focus_leaf.is_none();
    // Single gather pass (no early returns): every classifier input is real
    // state, so the decision table below stays causal.
    let mut mapped_key: Option<crate::workspace::WindowKey> = None;
    let mut mapped_window: Option<String> = None;
    if !plan_null {
        let focus_domain = plan.focus_domain.clone().expect("plan focus domain");
        let focus_leaf = plan.focus_leaf.clone().expect("plan focus leaf");
        if focus_domain.output.0 == output
            && focus_domain.workspace.0 == source_id
            && let Some(session) = state.engine.session(&focus_domain)
        {
            let snapshot = session.snapshot();
            if let Some(link) = snapshot.windows.iter().find(|link| {
                link.leaf == focus_leaf
                    && link.output == focus_domain.output
                    && link.workspace == focus_domain.workspace
            }) {
                let window = link.window.0.clone();
                if let Some(key) = state
                    .member_tokens
                    .iter()
                    .find(|(_, token)| token.as_str() == window.as_str())
                    .map(|(key, _)| key.clone())
                {
                    mapped_key = Some(key);
                    mapped_window = Some(window);
                }
            }
        }
    }
    let source_members = state.workspaces.workspace_members(output, source_id);
    let in_source = mapped_key
        .as_ref()
        .is_some_and(|key| source_members.contains(key));
    let hidden = mapped_key
        .as_ref()
        .is_some_and(|key| state.workspaces.is_hidden(key));
    // Exact member lifetime: visible MRU matches its fresh observation plus
    // the stored tag; a retained-overlay MRU (maximized/fullscreen) matches
    // its exact session key and rides the actuation's own revalidation.
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let lifetime_ok = match mapped_key.as_ref() {
        Some(key) => match by_hwnd.get(&key.hwnd) {
            Some(fresh) => {
                crate::workspace_owner::member_matches(
                    key,
                    fresh.hwnd,
                    fresh.identity.pid,
                    &fresh.identity.process_creation,
                ) && {
                    let live_tag = crate::product_hide::sys::read_member_tag(key.hwnd);
                    state.member_tags.get(key).is_some_and(|tag| {
                        crate::workspace_owner::visible_lifetime_ok(tag, live_tag.as_deref())
                    })
                }
            }
            None => retained
                .iter()
                .any(|row| row.key == *key && (row.maximized || row.fullscreen)),
        },
        None => false,
    };
    // Fresh focus eligibility mirrors the select path: revealed target focus
    // set over fresh tokens plus verified retained overlay tokens (focus
    // carries no write, KDE `requestFocus` parity).
    let mut fresh_tokens: HashSet<String> = observed.iter().map(|w| w.token.clone()).collect();
    for row in retained {
        if row.maximized || row.fullscreen {
            fresh_tokens.insert(row.token.clone());
        }
    }
    let eligible = mapped_key.as_ref().is_some_and(|key| {
        state
            .workspaces
            .eligible_focus_set(&source_members, &state.member_tokens, &fresh_tokens)
            .contains(key)
    });
    let class = classify_stay_focus(
        plan_null,
        mapped_key.is_some(),
        in_source,
        hidden,
        lifetime_ok,
        eligible,
    );
    match (class, mapped_window) {
        (StayFocusClass::Ready, Some(window)) => (class, Some(window)),
        (StayFocusClass::Null, _) => (StayFocusClass::Null, None),
        _ => (StayFocusClass::Stale, None),
    }
}

/// Engine stay tail for an explicit send-and-stay: the Engine mutation is
/// committed and membership verified by the caller. Fences
/// (lifetime/view/mode/gaps/source-selected) before writes AND focus, then
/// reflows only writable source rows, hides the mover without selecting or
/// revealing the target (hidden target takes no writes), keeps the source
/// selected/visible, applies the source-bound plan focus (MRU; no setter for
/// null), and runs ordinary trailing lifecycle for the next spare. History
/// never records (only verified selects record). Destination admission and
/// structural geometry are identical to follow: only focus and visibility
/// differ. Partial progress never replays: a fenced refusal reports without
/// further setters, and a failed focus reports without retry.
#[allow(clippy::too_many_arguments)]
fn workspace_send_stay(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    mover_key: &crate::workspace::WindowKey,
    stored: &ProcessIdentity,
    mover_minimized: bool,
    mover_hwnd: u64,
    output: &str,
    source_id: &str,
    target_id: &str,
    source_token: &str,
    target_token: &str,
    ctx: &ActionCtx,
    plan: &tiler_core::boundary::SendWorkspacePlan,
    preflight: SendPreflight,
    source_plan_ms: u64,
) -> SendEffect {
    let fail_at = |outcome: &'static str| SendEffect {
        outcome,
        focus: "none",
        source: None,
        target: None,
        transition_ms: 0,
        observation_ms: 0,
        source_geometry_ms: 0,
        target_geometry_ms: 0,
        hide_ms: 0,
        reveal_ms: 0,
        focus_ms: 0,
        source_plan_ms,
        target_plan_ms: 0,
        source_workspace: source_token.to_owned(),
        target_workspace: target_token.to_owned(),
    };
    // Carried pre-plan fences before source writes: view/mode/gaps/scope/
    // lifetime/membership/overlay re-verified live (item 2 + item 20). Any
    // drift refuses with no writes and no focus attempt (the committed
    // membership stands like the follow hide-failure path; convergence
    // without replay, never a guessed rollback). A lifetime refusal drops
    // the stale membership like the dispatch-time gate.
    if let Some(outcome) = crate::workspace_owner::send_fences_hold(
        preflight.fences,
        preflight.live(
            state, stored, mover_key, mover_hwnd, output, source_id, target_id, fulls,
        ),
    ) {
        if outcome == "identity-changed" {
            drop_member_state(state, mover_key);
        }
        return fail_at(outcome);
    }
    // Source reflow while survivors are still visible and eligible, scoped by
    // the source writable set exactly like follow (mover transferred out,
    // hidden/retained rows never writable): target entries skip honestly as
    // retained, and hidden geometry is never written.
    let source_writable = writable_tokens(state, output, source_id, observed);
    state.tick += 1;
    let source_tick = state.tick;
    let source_start = Instant::now();
    let source = apply_geometry(
        state,
        ApplyInput {
            me,
            fulls,
            reply: &tiler_core::boundary::CoreReply::SendWorkspace(plan.clone()),
            observed,
            op: "send-source-stay",
            tick: source_tick,
            correlation: ctx.correlation.as_str(),
            skipped: Vec::new(),
            writable: &source_writable,
            output_token: state.workspaces.output_token(output),
            workspace_token: state.workspaces.workspace_token(output, source_id),
            revision: revision_for(state, output, source_id),
        },
    );
    let source_ms = source_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    // Carried fences again before hide (mover still visible: the tag read is
    // exact). Partial progress never replays.
    if let Some(outcome) = crate::workspace_owner::send_fences_hold(
        preflight.fences,
        preflight.live(
            state, stored, mover_key, mover_hwnd, output, source_id, target_id, fulls,
        ),
    ) {
        if outcome == "identity-changed" {
            drop_member_state(state, mover_key);
        }
        return SendEffect {
            outcome,
            focus: "none",
            source,
            target: None,
            transition_ms: 0,
            observation_ms: 0,
            source_geometry_ms: source_ms,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        };
    }
    // Hide the mover from the source view (commit-before-hide) without
    // selecting or revealing the target. A committed claim is owned even on
    // uncertain post-hide readback: flag it hidden and report the stall
    // without pretending stay success.
    let (hide_outcome, _) =
        workspace_hide_one(state, me, store, dir, mover_key, stored, mover_minimized);
    if state.hidden_claims.contains_key(mover_key) {
        state.workspaces.set_hidden(mover_key, true);
    }
    if hide_outcome != "hidden" {
        return SendEffect {
            outcome: hide_outcome,
            focus: "none",
            source,
            target: None,
            transition_ms: 0,
            observation_ms: 0,
            source_geometry_ms: source_ms,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        };
    }
    // Ordinary lifecycle supplies the next spare when the transfer filled the
    // trailing empty; the source view stays selected throughout (no history
    // record: only verified selects record; trailing maintenance retains the
    // null focus by never touching focus).
    ensure_trailing_spare(state, output);
    // Exact-lifetime focus plan through the tested classifier: genuine null
    // (sole-mover stay emptied the source) resolves Null with ok/no-focus
    // and no setter; a some-but-stale mapping resolves Stale with
    // focus-unverified and no setter either; only Ready actuates once.
    let (class, focus_token) = stay_focus_token(state, output, source_id, plan, observed, retained);
    if class == crate::workspace_owner::StayFocusClass::Null {
        return SendEffect {
            outcome: "ok",
            focus: "no-focus",
            source,
            target: None,
            transition_ms: 0,
            observation_ms: 0,
            source_geometry_ms: source_ms,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        };
    }
    if class != crate::workspace_owner::StayFocusClass::Ready {
        return SendEffect {
            outcome: "focus-unverified",
            focus: "vanished",
            source,
            target: None,
            transition_ms: 0,
            observation_ms: 0,
            source_geometry_ms: source_ms,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        };
    }
    let focus_token = focus_token.expect("ready stay focus carries its token");
    // Explicit source-selected fence again before focus, then the actuation's
    // own fresh fences (identity/scope/fullscreen/elevated) decide the
    // single setter attempt: no replay on refusal.
    if state.workspaces.active_id(output).as_deref() != Some(source_id) {
        return SendEffect {
            outcome: "focus-unverified",
            focus: "vanished",
            source,
            target: None,
            transition_ms: 0,
            observation_ms: 0,
            source_geometry_ms: source_ms,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        };
    }
    let (focus, focus_ms, outcome) = if crate::workspace_owner::focus_before_geometry(
        true,
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        let focus_start = Instant::now();
        let actuation = actuate_focus(state, me, fulls, observed, retained, &focus_token);
        let focus_ms = focus_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        if actuation.outcome == "focus-ok" {
            if let Some(key) = state
                .member_tokens
                .iter()
                .find(|(_, token)| token.as_str() == focus_token.as_str())
                .map(|(key, _)| key.clone())
            {
                state.workspaces.note_foreground(&key);
            }
            ("focus-ok", focus_ms, "ok")
        } else {
            (actuation.outcome, focus_ms, "focus-unverified")
        }
    } else {
        ("focus-skipped-fence", 0, "focus-unverified")
    };
    SendEffect {
        outcome,
        focus,
        source,
        target: None,
        transition_ms: 0,
        observation_ms: 0,
        source_geometry_ms: source_ms,
        target_geometry_ms: 0,
        hide_ms: 0,
        reveal_ms: 0,
        focus_ms,
        source_plan_ms,
        target_plan_ms: 0,
        source_workspace: source_token.to_owned(),
        target_workspace: target_token.to_owned(),
    }
}

/// Test-needed exact-owner keyboard-resize poll for the normal `tile` loop
/// only (tentative, pending user review). Consume-once, exact-owner, and
/// proof refusal ride the shared `poll_workspace_cli_request` prefix shape.
/// The live foreground resolves through the same `snap_origins` map the hook
/// binds chords against (never a carried HWND, never a retarget), and the
/// built intent dispatches through the real `keyboard_tick` resize arm, so
/// every production fence (takeover, suspend/elevated, origin, lifetime,
/// proof, scope, float/sticky, workspace, overlay, busy) and the exact
/// repeat tracker apply unchanged. Logs one `resize` line with edge `cli`
/// for the queued request; the dispatch itself logs the production `resize`
/// line with the scheduled `press_index`.
fn poll_resize_cli_request(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    use tiler_core::directional::Direction;
    let path = dir.join(RESIZE_REQUEST_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => return,
    };
    let log_path = state.log_path.clone();
    // Best-effort labels for the pre-dispatch logs below: the actual parsed
    // direction/mode when the body parses, `unknown` otherwise. Never fake
    // precision for an unparseable payload.
    let parsed = parse_resize_request(text.trim()).ok();
    let direction_hint = parsed
        .as_ref()
        .map(|request| request.direction.clone())
        .unwrap_or_else(|| "unknown".to_owned());
    let mode_hint = parsed
        .as_ref()
        .map(|request| request.mode.clone())
        .unwrap_or_else(|| "unknown".to_owned());
    // Proof owners never serve the normal control: consume once with an
    // honest refusal and no native effect so the queue cannot wedge.
    if state.allowlist.is_some() {
        let _ = std::fs::remove_file(&path);
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "resize",
                "tick": tick,
                "mode": mode_hint,
                "direction": direction_hint,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": "refused-proof",
            }),
        );
        return;
    }
    // Consume-before-dispatch: exactly once, never replayed, never timed out
    // and retried. A bad body is still consumed with a refused outcome.
    let _ = std::fs::remove_file(&path);
    let Some(request) = parsed else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "resize",
                "tick": tick,
                "mode": mode_hint,
                "direction": direction_hint,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": "refused",
            }),
        );
        return;
    };
    let cli_line = |tick: u64, outcome: &'static str| {
        serde_json::json!({
            "event": "resize",
            "tick": tick,
            "mode": request.mode,
            "direction": request.direction,
            "edge": "cli",
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    if request.creation != me.process_creation
        || request.pid != me.pid
        || !exe_paths_equal(&request.exe_path, &me.exe_path)
        || request.user_sid != me.user_sid
        || request.session_id != me.session_id
    {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(&log_path, cli_line(tick, "refused"));
        return;
    }
    // Production per-intent fence first, before any observation: a suspended
    // session or an elevated foreground settles here with no side effects.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(&log_path, cli_line(tick, outcome));
        return;
    }
    // Direction/mode typed decode onto the classifier vocabulary
    // (`parse_resize_request` already bounded the literals; this match is
    // the exhaustive mapping, never a default).
    let direction = match request.direction.as_str() {
        "left" => Direction::Left,
        "right" => Direction::Right,
        "up" => Direction::Up,
        "down" => Direction::Down,
        _ => {
            state.tick += 1;
            let tick = state.tick;
            log_json_at(&log_path, cli_line(tick, "refused"));
            return;
        }
    };
    let mode = match request.mode.as_str() {
        "outwards" => crate::snapkey::ResizeMode::Outwards,
        "inwards" => crate::snapkey::ResizeMode::Inwards,
        _ => {
            state.tick += 1;
            let tick = state.tick;
            log_json_at(&log_path, cli_line(tick, "refused"));
            return;
        }
    };
    // Live-foreground origin through the same map the hook binds chords
    // against; never a carried HWND, never a retarget. The dispatch
    // re-resolves it against its own fresh observation, so a foreground
    // change between here and there refuses instead of misacting.
    let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
    let origin = state.snap_origins.get(&foreground_hwnd).cloned();
    let Some(origin) = origin.filter(|o| !o.token.is_empty()) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(&log_path, cli_line(tick, "unmanaged"));
        return;
    };
    state.tick += 1;
    let tick = state.tick;
    let mut queued = cli_line(tick, "dispatched");
    queued["origin"] = serde_json::Value::from(origin.token.clone());
    log_json_at(&log_path, queued);
    keyboard_tick(
        state,
        me,
        fulls,
        areas,
        vec![QueuedSnapEvent::Resize(
            crate::snapkey::QueuedResizeIntent {
                mode,
                direction,
                edge: crate::snapkey::SnapEdge::Down,
                origin: Some(origin),
                consumed: true,
                announce: true,
                tick: Instant::now(),
            },
        )],
        None,
    );
}

/// Exact-owner out-of-hook fullscreen toggle for the normal `tile` loop only
/// (item 9 test-needed route; the hook still filters injected Win+F11).
/// Reuses the production Win+F11 authority verbatim: the `toggle_gate_outcome`
/// suspend/elevated fence (owned-fullscreen exemption preserved), a fresh
/// observation, the live-foreground `snap_origins` origin (never a carried
/// HWND), the `member_matches` origin binding, the `revalidate_target`
/// focus/identity/scope/lifetime/proof gates with the focus-relaxed overlay
/// allowance, and the `fullscreen_toggle_decision` direction (including the
/// app-owned R-MAX-05 refusal) plus `enter_fullscreen`/`exit_fullscreen_owned`
/// with the same post-restore reconcile. Consume-once, exact-owner, and proof
/// refusal ride the shared `poll_workspace_cli_request` prefix; this runs only
/// after those pass. Logs `fullscreen-toggle` with edge `cli`.
fn dispatch_fullscreen_cli_toggle(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "fullscreen-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": "cli",
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    // Production per-intent fence first, before any observation.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        log_json_at(&state.log_path, settle(outcome));
        return;
    }
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(&state.log_path, settle("observation-failed"));
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    clear_maximize_at_admission(state, me, &retained);
    // Live-foreground origin through the same map the hook binds chords
    // against; never a carried HWND, never a retarget.
    let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
    let origin = state.snap_origins.get(&foreground_hwnd).cloned();
    let Some(origin) = origin.filter(|o| !o.token.is_empty()) else {
        log_json_at(&state.log_path, settle("unmanaged"));
        return;
    };
    let Some(member_key) = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == origin.token.as_str())
        .map(|(key, _)| key.clone())
    else {
        let mut line = settle("unmanaged");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&state.log_path, line);
        return;
    };
    if !crate::workspace_owner::member_matches(
        &member_key,
        origin.hwnd,
        origin.pid,
        &origin.creation,
    ) {
        let mut line = settle("foreground-changed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&state.log_path, line);
        return;
    };
    // Fresh pre-effect revalidation through the production target gate,
    // identical to the hook path (retained overlay construction included).
    let owned_expected: Option<ObservedWindow>;
    let expected = if let Some(found) = observed.iter().find(|w| w.hwnd == member_key.hwnd) {
        found
    } else {
        let Some(row) = retained.iter().find(|r| r.key == member_key) else {
            let mut line = settle("deferred");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            log_json_at(&state.log_path, line);
            return;
        };
        let Some(stored) = state.member_identity.get(&row.key).cloned() else {
            let mut line = settle("unmanaged");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            log_json_at(&state.log_path, line);
            return;
        };
        owned_expected = Some(ObservedWindow {
            hwnd: row.key.hwnd,
            token: row.token.clone(),
            outer: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            visible: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            insets: FrameInsets::default(),
            identity: crate::tiling::ObservedTarget {
                hwnd: row.key.hwnd,
                pid: row.key.pid,
                process_creation: row.key.creation.clone(),
                exe_path: stored.exe_path,
                user_sid: stored.user_sid,
                session_id: stored.session_id,
                tag: String::new(),
            },
            facts: row.facts.unwrap_or(crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: true,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                dialog: false,
            }),
        });
        owned_expected.as_ref().expect("retained expected built")
    };
    let from = origin.token.clone();
    let proof_mode = state.allowlist.is_some();
    let member_tag = state.member_tags.get(&member_key).map(String::as_str);
    let target = match revalidate_target(
        expected,
        me,
        fulls,
        &mut state.tokens,
        proof_mode,
        state.allowlist.as_ref(),
        &state.scope,
        member_tag,
        &state.scope_hosts,
        true,
    ) {
        Ok(target) => target,
        Err(reason) => {
            if reason == "identity-changed" {
                drop_member_state(state, &member_key);
            }
            let mut line = settle(reason);
            line["origin"] = serde_json::Value::from(origin.token.clone());
            log_json_at(&state.log_path, line);
            return;
        }
    };
    let _held = &target.held;
    let now_fullscreen = target.window.facts.captionless_fullscreen;
    let meta = read_fullscreen_meta(member_key.hwnd);
    let (target_label, outcome) = match fullscreen_toggle_decision(now_fullscreen, meta.is_some()) {
        crate::tiling::FullscreenToggle::RefuseAppOwned => {
            ("fullscreen", "fullscreen-refused-app-owned")
        }
        crate::tiling::FullscreenToggle::Enter => {
            let full = output_for_rect(areas, &target.window.visible);
            let full = areas
                .iter()
                .find(|a| a.device == full)
                .map(|area| area.full)
                .or_else(|| areas.first().map(|area| area.full));
            match full {
                Some(full) => ("fullscreen", enter_fullscreen(member_key.hwnd, full)),
                None => ("fullscreen", "unknown-output"),
            }
        }
        crate::tiling::FullscreenToggle::ExitOwned => {
            let meta = meta.expect("decision owns metadata");
            let outcome = exit_fullscreen_owned(member_key.hwnd, &meta);
            if outcome == "restored" && !meta.was_maximized {
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
            }
            ("restored", outcome)
        }
    };
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "fullscreen-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": "cli",
            "disposition": "consumed",
            "outcome": outcome,
            "target": target_label,
            "origin": origin.token,
            "window": from,
        }),
    );
}

/// Exact-owner out-of-hook float toggle for the normal `tile` loop only
/// (item 8 test-needed route, tentative pending user review; the hook still
/// filters injected Win+G). Reuses the production Win+G authority verbatim
/// through `dispatch_float_intent`: the suspend/elevated fence, a fresh
/// observation, the live-foreground `snap_origins` origin (never a carried
/// HWND), origin re-resolution with member/lifetime/proof/scope fences, the
/// overlay and workspace-mode fences, and the Engine-candidate commit with
/// marker persistence. Consume-once, exact-owner, and proof refusal ride the
/// shared `poll_workspace_cli_request` prefix; this runs only after those
/// pass. The outer `dispatched` line carries edge `cli`; the dispatched arm
/// logs its own `float-toggle` outcome lines. No synthetic input accepted.
fn dispatch_float_cli_toggle(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    // Production per-intent fence first, before any observation.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "float-toggle",
                "tick": tick,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": outcome,
            }),
        );
        return;
    }
    // Live-foreground origin through the same map the hook binds chords
    // against; never a carried HWND, never a retarget. The dispatch
    // re-resolves it against its own fresh observation, so a foreground
    // change between here and there refuses instead of misacting.
    let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
    let origin = state.snap_origins.get(&foreground_hwnd).cloned();
    let Some(origin) = origin.filter(|o| !o.token.is_empty()) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "float-toggle",
                "tick": tick,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": "unmanaged",
            }),
        );
        return;
    };
    state.tick += 1;
    let tick = state.tick;
    let mut queued = serde_json::json!({
        "event": "float-toggle",
        "tick": tick,
        "edge": "cli",
        "disposition": "consumed",
        "outcome": "dispatched",
    });
    queued["origin"] = serde_json::Value::from(origin.token.clone());
    log_json_at(&state.log_path, queued);
    dispatch_float_intent(
        state,
        me,
        fulls,
        areas,
        QueuedFloatIntent {
            edge: SnapEdge::Down,
            origin: Some(origin),
            consumed: true,
            announce: true,
            tick: Instant::now(),
        },
    );
}

/// Exact-owner out-of-hook sticky toggle for the normal `tile` loop only
/// (item 8 test-needed route, tentative pending user review; the hook still
/// filters injected Win+Shift+G). Reuses the production Win+Shift+G
/// authority verbatim through `dispatch_sticky_intent` with the same fences
/// and origin contract as the float arm above. Logs `sticky-toggle`.
fn dispatch_sticky_cli_toggle(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "sticky-toggle",
                "tick": tick,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": outcome,
            }),
        );
        return;
    }
    let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
    let origin = state.snap_origins.get(&foreground_hwnd).cloned();
    let Some(origin) = origin.filter(|o| !o.token.is_empty()) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "sticky-toggle",
                "tick": tick,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": "unmanaged",
            }),
        );
        return;
    };
    state.tick += 1;
    let tick = state.tick;
    let mut queued = serde_json::json!({
        "event": "sticky-toggle",
        "tick": tick,
        "edge": "cli",
        "disposition": "consumed",
        "outcome": "dispatched",
    });
    queued["origin"] = serde_json::Value::from(origin.token.clone());
    log_json_at(&state.log_path, queued);
    dispatch_sticky_intent(
        state,
        me,
        fulls,
        areas,
        QueuedStickyIntent {
            edge: SnapEdge::Down,
            origin: Some(origin),
            consumed: true,
            announce: true,
            tick: Instant::now(),
        },
    );
}

/// Exact-owner out-of-hook workspace control: one bounded `workspace.request`
/// file consumed once through the existing `workspace_do_select` /
/// `workspace_do_send` resolvers (plus the pure history resolvers for the
/// previous/relative forms), through the existing project-owned fullscreen
/// toggle below for `fullscreen`, or through the existing hook float/sticky
/// dispatch arms below for `float`/`sticky`. Normal `tile` only (proof
/// owners refuse without effect); workspace ops gate fullscreen and elevated
/// foreground like the hook path while the fullscreen/float/sticky ops route
/// through the production `toggle_gate_outcome` (owned-fullscreen exemption
/// preserved); the keyboard takeover switch never gates this (out-of-hook
/// dogfood/recovery). The request is deleted before dispatch so there is no
/// replay; a malformed or mismatched body is consumed the same way with a
/// `refused` outcome. Sends carry no chord origin: the mover is the live
/// foreground managed window at dispatch, resolved through the same
/// `snap_origins` map the hook binds chords against, then the exact same
/// `workspace_do_send` focus/identity/scope/owner gates with the request's
/// explicit follow/stay intent. Fullscreen/float/sticky carry no HWND either:
/// they toggle the live foreground managed window through the same
/// `revalidate_target` + toggle authority as their hook chords (including
/// the app-owned R-MAX-05 refusal for fullscreen). Production log
/// carries op/index/edge/outcome only (no HWNDs, tokens, or identity bytes);
/// the client correlation is opaque and never logged.
fn poll_workspace_cli_request(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    let path = dir.join(WORKSPACE_REQUEST_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => return,
    };
    // Honest op label for the pre-parse refusal: the parsed op when the body
    // is well-formed JSON carrying a known action, else `select`.
    let op_hint = |body: &str| -> String {
        serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v.get("action").and_then(|a| a.as_str().map(str::to_owned)))
            .filter(|a| {
                a == "select"
                    || a == "send"
                    || a == "stay"
                    || a == "previous"
                    || a == "fullscreen"
                    || a == "float"
                    || a == "sticky"
                    || a == "relative"
                    || a == "send-relative"
                    || a == "stay-relative"
            })
            .unwrap_or_else(|| "select".to_owned())
    };
    // Proof owners never serve the normal control: consume once with an
    // honest refusal and no native effect so the queue cannot wedge.
    if state.allowlist.is_some() {
        let op = op_hint(text.trim());
        let _ = std::fs::remove_file(&path);
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, &op, 0, "cli", "refused-proof"),
        );
        return;
    }
    let parsed = parse_workspace_request(text.trim());
    // Consume-before-dispatch: exactly once, never replayed, never timed out
    // and retried. A bad body is still consumed with a refused outcome.
    let _ = std::fs::remove_file(&path);
    let request = match parsed {
        Ok(request) => request,
        Err(_) => {
            let op = op_hint(text.trim());
            state.tick += 1;
            let tick = state.tick;
            log_json_at(
                &state.log_path,
                workspace_log(state, tick, &op, 0, "cli", "refused"),
            );
            return;
        }
    };
    let op = request.action.as_str();
    if request.creation != me.process_creation
        || request.pid != me.pid
        || !exe_paths_equal(&request.exe_path, &me.exe_path)
        || request.user_sid != me.user_sid
        || request.session_id != me.session_id
    {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "refused"),
        );
        return;
    }
    if request.action == WorkspaceAction::Fullscreen {
        dispatch_fullscreen_cli_toggle(state, me, fulls, areas);
        return;
    }
    if request.action == WorkspaceAction::Float {
        dispatch_float_cli_toggle(state, me, fulls, areas);
        return;
    }
    if request.action == WorkspaceAction::Sticky {
        dispatch_sticky_cli_toggle(state, me, fulls, areas);
        return;
    }
    if suspend_read(state, me, fulls).veto.block {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "suspended"),
        );
        return;
    }
    if foreground_elevated(me) {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "elevated-foreground"),
        );
        return;
    }
    let Some(output) = chord_output(state, areas) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "unknown-output"),
        );
        return;
    };
    state.workspaces.ensure_output(&output);
    state.tick += 1;
    let tick = state.tick;
    // Dispatch beginning for the CLI path: the clock starts before the
    // initial observation. No queued chord here, so the queue wait stays
    // zero by construction.
    let start = Instant::now();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "observation-failed"),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    workspace_close_cleanup(state);
    let ctx = ActionCtx {
        correlation: format!("act-{tick}"),
        tick,
        queued_at: start,
        start,
    };
    if request.action.is_send() {
        // Boundary-send testing affordance: the normal loop deliberately
        // rejects injected keys, so the mover is the live foreground managed
        // window at dispatch (same `snap_origins` map the hook binds chords
        // against), never a carried HWND. The request's explicit follow/stay
        // intent threads the exact same `workspace_do_send` gates; old
        // `--send` keeps following. Every other gate stays inside the
        // existing `workspace_do_send`.
        let (target, follow, dispatch_op): (SendTarget, bool, &'static str) = match request.action {
            WorkspaceAction::Send => (SendTarget::Numbered(request.index), true, "send"),
            WorkspaceAction::Stay => (SendTarget::Numbered(request.index), false, "stay"),
            WorkspaceAction::RelativeSend => match request.direction {
                Some(direction) => {
                    let target = SendTarget::Relative(direction.delta());
                    (target, true, target.log_op())
                }
                None => {
                    log_json_at(
                        &state.log_path,
                        workspace_log(state, tick, op, 0, "cli", "refused"),
                    );
                    return;
                }
            },
            WorkspaceAction::RelativeStay => match request.direction {
                Some(direction) => {
                    let delta = direction.delta();
                    (
                        SendTarget::Relative(delta),
                        false,
                        if delta < 0 { "stay-prev" } else { "stay-next" },
                    )
                }
                None => {
                    log_json_at(
                        &state.log_path,
                        workspace_log(state, tick, op, 0, "cli", "refused"),
                    );
                    return;
                }
            },
            WorkspaceAction::Select
            | WorkspaceAction::Previous
            | WorkspaceAction::Fullscreen
            | WorkspaceAction::Float
            | WorkspaceAction::Sticky
            | WorkspaceAction::RelativeHistory => {
                log_json_at(
                    &state.log_path,
                    workspace_log(state, tick, op, request.index, "cli", "refused"),
                );
                return;
            }
        };
        let dispatch_index = match request.action {
            WorkspaceAction::Send | WorkspaceAction::Stay => request.index,
            _ => 0,
        };
        let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
        let origin = state.snap_origins.get(&foreground_hwnd).cloned();
        match origin.filter(|o| !o.token.is_empty()) {
            Some(o) => {
                let effect = workspace_do_send(
                    state,
                    me,
                    store,
                    dir,
                    fulls,
                    areas,
                    &mut observed,
                    &retained,
                    &output,
                    target,
                    follow,
                    o.hwnd,
                    &o.token,
                    o.pid,
                    &o.creation,
                    &ctx,
                );
                log_json_at(
                    &state.log_path,
                    workspace_log(
                        state,
                        tick,
                        dispatch_op,
                        dispatch_index,
                        "cli",
                        effect.outcome,
                    ),
                );
                log_workspace_action(
                    state,
                    &state.log_path.clone(),
                    &ActionLine {
                        ctx: &ctx,
                        op: dispatch_op,
                        index: dispatch_index,
                        edge: "cli",
                        outcome: effect.outcome,
                        focus: effect.focus,
                        timings: &ActionTimings {
                            transition_ms: effect.transition_ms,
                            observation_ms: effect.observation_ms,
                            source_plan_ms: effect.source_plan_ms,
                            target_plan_ms: effect.target_plan_ms,
                            source_geometry_ms: effect.source_geometry_ms,
                            target_geometry_ms: effect.target_geometry_ms,
                            hide_ms: effect.hide_ms,
                            reveal_ms: effect.reveal_ms,
                            focus_ms: effect.focus_ms,
                        },
                        source_workspace: &effect.source_workspace,
                        target_workspace: &effect.target_workspace,
                        source: effect.source.as_ref(),
                        target: effect.target.as_ref(),
                    },
                );
            }
            _ => {
                log_json_at(
                    &state.log_path,
                    workspace_log(state, tick, dispatch_op, dispatch_index, "cli", "unmanaged"),
                );
            }
        }
        return;
    }
    // Resolve without preactivating (same as the hook select path):
    // resolvers never touch ACTIVE; only the transition activates. History
    // forms resolve purely like the hook history path.
    let (target, none_outcome, dispatch_op, dispatch_index): (
        Option<String>,
        &'static str,
        &'static str,
        u8,
    ) = match request.action {
        WorkspaceAction::Select => {
            let target = if request.index == 0 {
                state
                    .workspaces
                    .resolve_send_trailing(&output)
                    .map(|(id, _)| id)
            } else {
                state.workspaces.resolve_send(&output, request.index)
            };
            let none_outcome = if request.index == 0 {
                "unknown-output"
            } else {
                "unknown-target"
            };
            (target, none_outcome, "select", request.index)
        }
        WorkspaceAction::Previous => (
            state.workspaces.resolve_previous(&output),
            "unknown-target",
            "previous",
            0,
        ),
        WorkspaceAction::RelativeHistory => match request.direction {
            Some(direction) => {
                let delta = direction.delta();
                (
                    state.workspaces.resolve_relative(&output, delta),
                    "unknown-target",
                    if delta < 0 {
                        "relative-prev"
                    } else {
                        "relative-next"
                    },
                    0,
                )
            }
            None => (None, "refused", "relative", 0),
        },
        WorkspaceAction::Send
        | WorkspaceAction::Stay
        | WorkspaceAction::Fullscreen
        | WorkspaceAction::Float
        | WorkspaceAction::Sticky
        | WorkspaceAction::RelativeSend
        | WorkspaceAction::RelativeStay => (None, "refused", op, request.index),
    };
    let Some(target) = target else {
        log_json_at(
            &state.log_path,
            workspace_log(
                state,
                tick,
                dispatch_op,
                dispatch_index,
                "cli",
                none_outcome,
            ),
        );
        return;
    };
    // REQ-WS-09 departure memory for the CLI select/previous/relative path:
    // the live foreground resolves through the same `snap_origins` map the
    // hook binds chords against (never a carried HWND).
    let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
    if let Some(source_id) = state.workspaces.active_id(&output)
        && let Some(origin) = state.snap_origins.get(&foreground_hwnd).cloned()
        && !origin.token.is_empty()
        && let Some(token) = remember_select_departure(
            state,
            &origin,
            &output,
            &source_id,
            foreground_hwnd,
            &observed,
            &retained,
        )
    {
        log_json_at(
            &state.log_path.clone(),
            serde_json::json!({"event":"select-departure","correlation":ctx.correlation,"window":token}),
        );
    }
    let effect = workspace_do_select(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        &mut observed,
        &output,
        &target,
        &ctx,
        None,
        false,
    );
    log_json_at(
        &state.log_path,
        workspace_log(
            state,
            tick,
            dispatch_op,
            dispatch_index,
            "cli",
            effect.outcome,
        ),
    );
    log_workspace_action(
        state,
        &state.log_path.clone(),
        &ActionLine {
            ctx: &ctx,
            op: dispatch_op,
            index: dispatch_index,
            edge: "cli",
            outcome: effect.outcome,
            focus: effect.focus,
            timings: &ActionTimings {
                transition_ms: effect.transition_ms,
                observation_ms: effect.observation_ms,
                source_plan_ms: 0,
                target_plan_ms: effect.plan_ms,
                source_geometry_ms: 0,
                target_geometry_ms: effect.geometry_ms,
                hide_ms: effect.hide_ms,
                reveal_ms: effect.reveal_ms,
                focus_ms: effect.focus_ms,
            },
            source_workspace: &effect.source_workspace,
            target_workspace: &effect.target_workspace,
            source: None,
            target: effect.geometry.as_ref(),
        },
    );
}

/// Drain one bounded batch of workspace digit intents. Select works on empty
/// workspaces and unmanaged foreground; send refuses unmanaged focus. The
/// `--no-keyboard-snap-takeover` off switch disables all product
/// interception (the hook is never installed; this is the defensive second
/// fence). Fullscreen and elevated foreground gate both ops.
#[allow(clippy::too_many_arguments)]
fn workspace_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<crate::snapkey::QueuedWorkspaceIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-stale","cause":"takeover-off","dropped": events.len()}),
            );
        }
        return;
    }
    for intent in events {
        if !intent.consumed || !intent.announce {
            if state.trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "workspace",
                        "tick": state.tick,
                        "op": intent.op.as_str(),
                        "index": intent.index,
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        // Dispatch beginning: the action clock starts here, before the
        // initial observation, so queue-to-effect latency includes the
        // observation itself. `queued_at` stays the cheap callback stamp.
        // Verified managed overlays bypass suspension like a managed maximize.
        let dispatch_start = Instant::now();
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace",
                    "op": intent.op.as_str(),
                    "index": intent.index,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "suspended",
                }),
            );
            continue;
        }
        if foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace",
                    "op": intent.op.as_str(),
                    "index": intent.index,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        let Some(output) = chord_output(state, areas) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace",
                    "op": intent.op.as_str(),
                    "index": intent.index,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        state.workspaces.ensure_output(&output);
        state.tick += 1;
        let tick = state.tick;
        let mut skipped: Vec<(String, String)> = Vec::new();
        let mut retained: Vec<RetainedRow> = Vec::new();
        let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace",
                    "tick": tick,
                    "op": intent.op.as_str(),
                    "index": intent.index,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "observation-failed",
                }),
            );
            continue;
        };
        publish_managed(state, me, &observed, &retained);
        ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
        // First-seen maximized windows restore once here: lifetime tags are
        // stamped, and the cleared window converges as eligible next tick.
        clear_maximize_at_admission(state, me, &retained);
        workspace_close_cleanup(state);
        // Stable per-action context: one opaque correlation for the whole
        // select/send/follow lifecycle even when inner geometry ticks
        // increment. `queued_at` is the cheap callback stamp (no
        // logging/syscalls in the callback); `start` is the dispatch
        // beginning on the loop thread, captured before observation.
        let op = intent.op;
        let index = intent.index;
        let edge = intent.edge;
        let ctx = ActionCtx {
            correlation: format!("act-{tick}"),
            tick,
            queued_at: intent.tick,
            start: dispatch_start,
        };
        match op {
            WorkspaceOp::Select => {
                // Resolve without preactivating: `select`/`select_trailing`
                // mutate ACTIVE, which makes `workspace_do_select` see
                // `current == target` and return `already-active` with no
                // native effects. `resolve_send`/`resolve_send_trailing`
                // resolve (and append trailing when needed) without touching
                // ACTIVE; only the transition activates after hide/reveal.
                let target = if index == 0 {
                    state
                        .workspaces
                        .resolve_send_trailing(&output)
                        .map(|(id, _)| id)
                } else {
                    state.workspaces.resolve_send(&output, index)
                };
                match target {
                    Some(id) => {
                        // REQ-WS-09 departure memory: the chord-time origin
                        // still holds live foreground, so the source
                        // remembers it before hiding.
                        let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
                        if let Some(source_id) = state.workspaces.active_id(&output)
                            && let Some(origin) = intent.origin.as_ref()
                            && let Some(token) = remember_select_departure(
                                state,
                                origin,
                                &output,
                                &source_id,
                                foreground_hwnd,
                                &observed,
                                &retained,
                            )
                        {
                            log_json_at(
                                &state.log_path.clone(),
                                serde_json::json!({"event":"select-departure","correlation":ctx.correlation,"window":token}),
                            );
                        }
                        let effect = workspace_do_select(
                            state,
                            me,
                            store,
                            dir,
                            fulls,
                            areas,
                            &mut observed,
                            &output,
                            &id,
                            &ctx,
                            None,
                            false,
                        );
                        log_json_at(
                            &log_path,
                            workspace_log(
                                state,
                                tick,
                                op.as_str(),
                                index,
                                edge.as_str(),
                                effect.outcome,
                            ),
                        );
                        log_workspace_action(
                            state,
                            &log_path,
                            &ActionLine {
                                ctx: &ctx,
                                op: op.as_str(),
                                index,
                                edge: edge.as_str(),
                                outcome: effect.outcome,
                                focus: effect.focus,
                                timings: &ActionTimings {
                                    transition_ms: effect.transition_ms,
                                    observation_ms: effect.observation_ms,
                                    source_plan_ms: 0,
                                    target_plan_ms: effect.plan_ms,
                                    source_geometry_ms: 0,
                                    target_geometry_ms: effect.geometry_ms,
                                    hide_ms: effect.hide_ms,
                                    reveal_ms: effect.reveal_ms,
                                    focus_ms: effect.focus_ms,
                                },
                                source_workspace: &effect.source_workspace,
                                target_workspace: &effect.target_workspace,
                                source: None,
                                target: effect.geometry.as_ref(),
                            },
                        );
                    }
                    None => {
                        let outcome = if index == 0 {
                            "unknown-output"
                        } else {
                            "unknown-target"
                        };
                        log_json_at(
                            &log_path,
                            workspace_log(state, tick, op.as_str(), index, edge.as_str(), outcome),
                        );
                    }
                }
            }
            WorkspaceOp::Send => {
                let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
                let origin = intent.origin.clone();
                match origin.filter(|o| o.hwnd == foreground_hwnd) {
                    Some(o) if !o.token.is_empty() => {
                        let effect = workspace_do_send(
                            state,
                            me,
                            store,
                            dir,
                            fulls,
                            areas,
                            &mut observed,
                            &retained,
                            &output,
                            SendTarget::Numbered(index),
                            intent.follow,
                            o.hwnd,
                            &o.token,
                            o.pid,
                            &o.creation,
                            &ctx,
                        );
                        log_json_at(
                            &log_path,
                            workspace_log(
                                state,
                                tick,
                                op.as_str(),
                                index,
                                edge.as_str(),
                                effect.outcome,
                            ),
                        );
                        log_workspace_action(
                            state,
                            &log_path,
                            &ActionLine {
                                ctx: &ctx,
                                op: op.as_str(),
                                index,
                                edge: edge.as_str(),
                                outcome: effect.outcome,
                                focus: effect.focus,
                                timings: &ActionTimings {
                                    transition_ms: effect.transition_ms,
                                    observation_ms: effect.observation_ms,
                                    source_plan_ms: effect.source_plan_ms,
                                    target_plan_ms: effect.target_plan_ms,
                                    source_geometry_ms: effect.source_geometry_ms,
                                    target_geometry_ms: effect.target_geometry_ms,
                                    hide_ms: effect.hide_ms,
                                    reveal_ms: effect.reveal_ms,
                                    focus_ms: effect.focus_ms,
                                },
                                source_workspace: &effect.source_workspace,
                                target_workspace: &effect.target_workspace,
                                source: effect.source.as_ref(),
                                target: effect.target.as_ref(),
                            },
                        );
                    }
                    _ => {
                        log_json_at(
                            &log_path,
                            workspace_log(
                                state,
                                tick,
                                op.as_str(),
                                index,
                                edge.as_str(),
                                "unmanaged",
                            ),
                        );
                    }
                }
            }
        }
    }
}

/// Drain one bounded batch of item 2 relative workspace-send intents.
/// Previous/next steps resolve once from the mover's source workspace through
/// the item 1 scoped ring (wrapping through the trailing empty and ordinals
/// beyond 9, never MRU, never creating); filling the trailing empty invokes
/// ordinary lifecycle for the next spare after transfer. Follow/stay ride the
/// explicit intent exactly like numbered sends (`--no-keyboard-snap-takeover`
/// off, fullscreen/elevated gates, unmanaged-focus refusal all apply). Only
/// verified follow selects record history; stay never does.
#[allow(clippy::too_many_arguments)]
fn workspace_relative_send_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<QueuedWorkspaceSendIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-send-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-send-stale","cause":"takeover-off","dropped": events.len()}),
            );
        }
        return;
    }
    for intent in events {
        if !intent.consumed || !intent.announce {
            if state.trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "workspace-send",
                        "tick": state.tick,
                        "delta": intent.delta,
                        "follow": intent.follow,
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        let dispatch_start = Instant::now();
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-send",
                    "delta": intent.delta,
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "suspended",
                }),
            );
            continue;
        }
        if foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-send",
                    "delta": intent.delta,
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        let Some(output) = chord_output(state, areas) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-send",
                    "delta": intent.delta,
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        state.workspaces.ensure_output(&output);
        state.tick += 1;
        let tick = state.tick;
        let mut skipped: Vec<(String, String)> = Vec::new();
        let mut retained: Vec<RetainedRow> = Vec::new();
        let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-send",
                    "tick": tick,
                    "delta": intent.delta,
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "observation-failed",
                }),
            );
            continue;
        };
        publish_managed(state, me, &observed, &retained);
        ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
        clear_maximize_at_admission(state, me, &retained);
        workspace_close_cleanup(state);
        let target = SendTarget::Relative(intent.delta);
        let op = target.log_op();
        let edge = intent.edge;
        let ctx = ActionCtx {
            correlation: format!("act-{tick}"),
            tick,
            queued_at: intent.tick,
            start: dispatch_start,
        };
        let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
        let origin = intent.origin.clone();
        match origin.filter(|o| o.hwnd == foreground_hwnd) {
            Some(o) if !o.token.is_empty() => {
                let effect = workspace_do_send(
                    state,
                    me,
                    store,
                    dir,
                    fulls,
                    areas,
                    &mut observed,
                    &retained,
                    &output,
                    target,
                    intent.follow,
                    o.hwnd,
                    &o.token,
                    o.pid,
                    &o.creation,
                    &ctx,
                );
                log_json_at(
                    &log_path,
                    workspace_log(
                        state,
                        tick,
                        op,
                        target.log_index(),
                        edge.as_str(),
                        effect.outcome,
                    ),
                );
                log_workspace_action(
                    state,
                    &log_path,
                    &ActionLine {
                        ctx: &ctx,
                        op,
                        index: target.log_index(),
                        edge: edge.as_str(),
                        outcome: effect.outcome,
                        focus: effect.focus,
                        timings: &ActionTimings {
                            transition_ms: effect.transition_ms,
                            observation_ms: effect.observation_ms,
                            source_plan_ms: effect.source_plan_ms,
                            target_plan_ms: effect.target_plan_ms,
                            source_geometry_ms: effect.source_geometry_ms,
                            target_geometry_ms: effect.target_geometry_ms,
                            hide_ms: effect.hide_ms,
                            reveal_ms: effect.reveal_ms,
                            focus_ms: effect.focus_ms,
                        },
                        source_workspace: &effect.source_workspace,
                        target_workspace: &effect.target_workspace,
                        source: effect.source.as_ref(),
                        target: effect.target.as_ref(),
                    },
                );
            }
            _ => {
                log_json_at(
                    &log_path,
                    workspace_log(
                        state,
                        tick,
                        op,
                        target.log_index(),
                        edge.as_str(),
                        "unmanaged",
                    ),
                );
            }
        }
    }
}

/// Drain one bounded batch of workspace history intents (item 1). Toggle and
/// relative steps work on empty workspaces and unmanaged foreground: they
/// select views, never movers, so no managed-focus gate applies. The
/// `--no-keyboard-snap-takeover` off switch disables all product
/// interception (defensive second fence). Fullscreen and elevated foreground
/// gate all history ops like digits. Targets resolve without preactivating
/// (pure `resolve_previous`/`resolve_relative`: no activation, append, or
/// creation); only the verified `workspace_do_select` transition activates,
/// and its observation recording supplies history. Toggle with no recorded
/// change, or a removed/unassigned/out-of-scope previous, settles as
/// `unknown-target` (no-op until the next recorded change, never
/// recreation/ordinal reinterpretation).
#[allow(clippy::too_many_arguments)]
fn workspace_history_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<QueuedWorkspaceHistoryIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-history-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-history-stale","cause":"takeover-off","dropped": events.len()}),
            );
        }
        return;
    }
    for intent in events {
        if !intent.consumed || !intent.announce {
            if state.trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "workspace-history",
                        "tick": state.tick,
                        "op": intent.op.as_str(),
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        let dispatch_start = Instant::now();
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "suspended",
                }),
            );
            continue;
        }
        if foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        let Some(output) = chord_output(state, areas) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        state.workspaces.ensure_output(&output);
        state.tick += 1;
        let tick = state.tick;
        let mut skipped: Vec<(String, String)> = Vec::new();
        let mut retained: Vec<RetainedRow> = Vec::new();
        let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "tick": tick,
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "observation-failed",
                }),
            );
            continue;
        };
        publish_managed(state, me, &observed, &retained);
        ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
        clear_maximize_at_admission(state, me, &retained);
        workspace_close_cleanup(state);
        let op = intent.op;
        let edge = intent.edge;
        let ctx = ActionCtx {
            correlation: format!("act-{tick}"),
            tick,
            queued_at: intent.tick,
            start: dispatch_start,
        };
        // Pure resolution: no activation, append, or creation. Previous
        // validates its stable id (clears removed/out-of-scope); relative
        // wraps the full scoped ring including trailing empty and >9.
        let target: Option<String> = match op {
            WorkspaceHistoryOp::Previous => state.workspaces.resolve_previous(&output),
            WorkspaceHistoryOp::Prev => state.workspaces.resolve_relative(&output, -1),
            WorkspaceHistoryOp::Next => state.workspaces.resolve_relative(&output, 1),
        };
        match target {
            Some(id) => {
                // REQ-WS-09 departure memory, same as the numbered select.
                let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
                if let Some(source_id) = state.workspaces.active_id(&output)
                    && let Some(origin) = intent.origin.as_ref()
                    && let Some(token) = remember_select_departure(
                        state,
                        origin,
                        &output,
                        &source_id,
                        foreground_hwnd,
                        &observed,
                        &retained,
                    )
                {
                    log_json_at(
                        &state.log_path.clone(),
                        serde_json::json!({"event":"select-departure","correlation":ctx.correlation,"window":token}),
                    );
                }
                let effect = workspace_do_select(
                    state,
                    me,
                    store,
                    dir,
                    fulls,
                    areas,
                    &mut observed,
                    &output,
                    &id,
                    &ctx,
                    None,
                    false,
                );
                log_json_at(
                    &log_path,
                    workspace_history_log(state, tick, op.as_str(), edge.as_str(), effect.outcome),
                );
                log_workspace_action(
                    state,
                    &log_path,
                    &ActionLine {
                        ctx: &ctx,
                        op: op.as_str(),
                        index: 0,
                        edge: edge.as_str(),
                        outcome: effect.outcome,
                        focus: effect.focus,
                        timings: &ActionTimings {
                            transition_ms: effect.transition_ms,
                            observation_ms: effect.observation_ms,
                            source_plan_ms: 0,
                            target_plan_ms: effect.plan_ms,
                            source_geometry_ms: 0,
                            target_geometry_ms: effect.geometry_ms,
                            hide_ms: effect.hide_ms,
                            reveal_ms: effect.reveal_ms,
                            focus_ms: effect.focus_ms,
                        },
                        source_workspace: &effect.source_workspace,
                        target_workspace: &effect.target_workspace,
                        source: None,
                        target: effect.geometry.as_ref(),
                    },
                );
            }
            None => {
                log_json_at(
                    &log_path,
                    workspace_history_log(
                        state,
                        tick,
                        op.as_str(),
                        edge.as_str(),
                        "unknown-target",
                    ),
                );
            }
        }
    }
}

/// Bounded history log line: opaque output/workspace tokens plus
/// op/edge/outcome only. No HWNDs, tokens, titles, or app data.
fn workspace_history_log(
    state: &TileLoop,
    tick: u64,
    op: &str,
    edge: &str,
    outcome: &str,
) -> serde_json::Value {
    let output = if state.active_output.is_empty() {
        "o?".to_owned()
    } else {
        state.workspaces.output_token(&state.active_output)
    };
    let workspace = state
        .workspaces
        .active_id(&state.active_output)
        .map(|id| state.workspaces.workspace_token(&state.active_output, &id))
        .unwrap_or_else(|| "ws?".to_owned());
    serde_json::json!({
        "event": "workspace-history",
        "tick": tick,
        "output": output,
        "workspace": workspace,
        "op": op,
        "edge": edge,
        "outcome": outcome,
    })
}

/// Bounded workspace log line: opaque output/workspace tokens plus
/// op/index/edge/outcome only. No HWNDs, tokens, titles, or app data.
fn workspace_log(
    state: &TileLoop,
    tick: u64,
    op: &str,
    index: u8,
    edge: &str,
    outcome: &str,
) -> serde_json::Value {
    let output = if state.active_output.is_empty() {
        "o?".to_owned()
    } else {
        state.workspaces.output_token(&state.active_output)
    };
    let workspace = state
        .workspaces
        .active_id(&state.active_output)
        .map(|id| state.workspaces.workspace_token(&state.active_output, &id))
        .unwrap_or_else(|| "ws?".to_owned());
    serde_json::json!({
        "event": "workspace",
        "tick": tick,
        "output": output,
        "workspace": workspace,
        "op": op,
        "index": index,
        "edge": edge,
        "outcome": outcome,
    })
}

/// Stable per-action context: one opaque correlation for the whole select /
/// send / follow lifecycle, even when inner reconcile ticks increment. Times
/// are monotonic `Instant`s: `queued_at` is the cheap callback stamp from the
/// existing hook event (no logging/syscalls in the callback), `start` is the
/// dispatch instant on the loop thread.
struct ActionCtx {
    correlation: String,
    tick: u64,
    queued_at: Instant,
    start: Instant,
}

/// Settled effect of one verified select transition: membership outcome
/// plus honest per-phase geometry and focus evidence. `geometry` is `None`
/// when no geometry pass ran (empty target, observation failure, early
/// refusal, non-plan Engine reply with readback not run); otherwise it
/// carries the verified verdict so `ok` never implies applied.
/// `transition_ms` covers dispatch to activation (hide+reveal+activate,
/// measured before the post-reveal observation); `observation_ms` covers the
/// post-reveal observation alone. `action_ms` on the log line is the terminal
/// dispatch-to-geometry-completion offset.
struct SelectEffect {
    outcome: &'static str,
    geometry: Option<ApplySummary>,
    focus: &'static str,
    transition_ms: u64,
    observation_ms: u64,
    geometry_ms: u64,
    hide_ms: u64,
    reveal_ms: u64,
    focus_ms: u64,
    plan_ms: u64,
    source_workspace: String,
    target_workspace: String,
}

/// Settled effect of one send+follow action: membership outcome plus honest
/// source (pre-hide reflow) and target (post-reveal) geometry evidence.
/// `source`/`target` are `None` when that pass never ran. Plan and geometry
/// durations stay split per domain, never collapsed. Tokens are captured
/// before cleanup so a removed source still logs its opaque identity.
struct SendEffect {
    outcome: &'static str,
    focus: &'static str,
    source: Option<ApplySummary>,
    target: Option<ApplySummary>,
    transition_ms: u64,
    observation_ms: u64,
    source_geometry_ms: u64,
    target_geometry_ms: u64,
    hide_ms: u64,
    reveal_ms: u64,
    focus_ms: u64,
    source_plan_ms: u64,
    target_plan_ms: u64,
    source_workspace: String,
    target_workspace: String,
}

/// Grouped phase timings for one bounded action line: dispatch offsets plus
/// per-phase durations. Source/target plan and geometry stay distinct.
struct ActionTimings {
    transition_ms: u64,
    observation_ms: u64,
    source_plan_ms: u64,
    target_plan_ms: u64,
    source_geometry_ms: u64,
    target_geometry_ms: u64,
    hide_ms: u64,
    reveal_ms: u64,
    focus_ms: u64,
}

/// One bounded action summary: opaque correlation plus monotonic
/// queue/action/transition/geometry durations and per-phase
/// applied/mismatched/readback states. A phase that never ran serializes as
/// null (outcome carries empty vs deferred vs failed), never zeros implying
/// applied; a vetoed pass carries its bounded `veto` reason.
struct ActionLine<'a> {
    ctx: &'a ActionCtx,
    op: &'a str,
    index: u8,
    edge: &'a str,
    outcome: &'a str,
    focus: &'a str,
    timings: &'a ActionTimings,
    source_workspace: &'a str,
    target_workspace: &'a str,
    source: Option<&'a ApplySummary>,
    target: Option<&'a ApplySummary>,
}

fn log_workspace_action(_state: &TileLoop, log_path: &Path, line: &ActionLine<'_>) {
    let queue_wait_ms = line
        .ctx
        .start
        .duration_since(line.ctx.queued_at)
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let action_ms = line
        .ctx
        .start
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let phase = |summary: Option<&ApplySummary>| match summary {
        Some(s) => {
            let mut v = serde_json::json!({
                "applied": s.applied,
                "mismatched": s.mismatched,
                "readback_ok": s.readback_ok,
            });
            match s.veto {
                Some(diag) => {
                    v["veto"] = serde_json::json!({
                        "reason": diag.reason,
                        "desktop": diag.desktop,
                        "visible": diag.visible,
                        "captioned": diag.captioned,
                        "dwm_readable": diag.dwm_readable,
                        "cloaked": diag.cloaked,
                    });
                }
                None => {
                    v["veto"] = serde_json::Value::Null;
                }
            }
            v
        }
        None => serde_json::Value::Null,
    };
    log_json_at(
        log_path,
        serde_json::json!({
            "event": "workspace-action",
            "tick": line.ctx.tick,
            "correlation": line.ctx.correlation,
            "op": line.op,
            "index": line.index,
            "edge": line.edge,
            "outcome": line.outcome,
            "focus": line.focus,
            "queue_wait_ms": queue_wait_ms,
            "action_ms": action_ms,
            "transition_ms": line.timings.transition_ms,
            "observation_ms": line.timings.observation_ms,
            "source_plan_ms": line.timings.source_plan_ms,
            "target_plan_ms": line.timings.target_plan_ms,
            "source_geometry_ms": line.timings.source_geometry_ms,
            "target_geometry_ms": line.timings.target_geometry_ms,
            "hide_ms": line.timings.hide_ms,
            "reveal_ms": line.timings.reveal_ms,
            "focus_ms": line.timings.focus_ms,
            "source_workspace": line.source_workspace,
            "target_workspace": line.target_workspace,
            "source": phase(line.source),
            "target": phase(line.target),
        }),
    );
}

/// True when the foreground window's process is not medium integrity:
/// elevated foreground gates workspace dispatch.
fn foreground_elevated(me: &ProcessIdentity) -> bool {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return false;
    }
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(foreground, &mut pid);
    }
    if pid == 0 || pid == me.pid {
        return false;
    }
    let Ok(held) = HeldProcess::open(pid) else {
        return true;
    };
    match held.integrity() {
        Ok(rid) => !is_medium_rid(rid),
        Err(_) => true,
    }
}

/// Actual-foreground hidden member selects its workspace. Only a real
/// foreground change to an exactly verified hidden member switches; the
/// `Wake` event alone never does. Returns the settled select effect or `None`.
fn poll_foreground_workspace(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
) -> Option<SelectEffect> {
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    if foreground == 0 || foreground == state.last_foreground {
        return None;
    }
    state.last_foreground = foreground;
    // Keyed by full member identity: find the HWND's key, then guard full
    // equality before any lookup so a recycled HWND never selects.
    let key = state
        .hidden_claims
        .keys()
        .find(|k| k.hwnd == foreground)
        .cloned()?;
    let record = state.hidden_claims.get(&key).cloned()?;
    // Exact verification: live PID, full process identity, exact nonce.
    let snap = crate::product_hide::sys::query_recovery(foreground, me).ok()?;
    if !crate::workspace_owner::member_matches(
        &key,
        foreground,
        snap.pid,
        &snap.process.process_creation,
    ) || snap.process != record.claim.process
        || snap.nonce.as_deref() != Some(record.claim.tag.as_str())
    {
        return None;
    }
    // Externally foregrounded: this is genuinely the member's workspace.
    let loc = state.workspaces.member_loc(&key).cloned()?;
    if !loc.hidden {
        return None;
    }
    let output = loc.output.clone();
    let workspace = loc.workspace.clone();
    // No queued chord here: the external foreground is the trigger, so the
    // queue wait is zero by construction.
    state.tick += 1;
    let tick = state.tick;
    let start = Instant::now();
    let ctx = ActionCtx {
        correlation: format!("act-{tick}"),
        tick,
        queued_at: start,
        start,
    };
    let effect = workspace_do_select(
        state, me, store, dir, fulls, areas, observed, &output, &workspace, &ctx, None, false,
    );
    log_workspace_action(
        state,
        &state.log_path.clone(),
        &ActionLine {
            ctx: &ctx,
            op: "select",
            index: 0,
            edge: "foreground",
            outcome: effect.outcome,
            focus: effect.focus,
            timings: &ActionTimings {
                transition_ms: effect.transition_ms,
                observation_ms: effect.observation_ms,
                source_plan_ms: 0,
                target_plan_ms: effect.plan_ms,
                source_geometry_ms: 0,
                target_geometry_ms: effect.geometry_ms,
                hide_ms: effect.hide_ms,
                reveal_ms: effect.reveal_ms,
                focus_ms: effect.focus_ms,
            },
            source_workspace: &effect.source_workspace,
            target_workspace: &effect.target_workspace,
            source: None,
            target: effect.geometry.as_ref(),
        },
    );
    Some(effect)
}

/// Externally revealed hidden claims without foreground return to hidden
/// under the same claim and identity: never re-admitted, never moved to the
/// wrong domain. Destroyed/recycled claims retire their ledger notes.
fn rehide_pass(state: &mut TileLoop, me: &ProcessIdentity, store: &LedgerStore) {
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    let keys: Vec<crate::workspace::WindowKey> = state.hidden_claims.keys().cloned().collect();
    for key in keys {
        if key.hwnd == foreground {
            continue;
        }
        let Some(record) = state.hidden_claims.get(&key).cloned() else {
            continue;
        };
        let visible = unsafe { IsWindowVisible(key.hwnd as isize as HWND) } != 0;
        if !visible {
            continue;
        }
        // Same-claim guard: only the exact stored identity returns to hidden.
        let snap = match crate::product_hide::sys::query_recovery(key.hwnd, me) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if !crate::workspace_owner::member_matches(
            &key,
            key.hwnd,
            snap.pid,
            &snap.process.process_creation,
        ) || snap.process != record.claim.process
            || snap.nonce.as_deref() != Some(record.claim.tag.as_str())
        {
            continue;
        }
        match crate::product_hide::sys::rehide_known_claim(me, &record.claim) {
            Ok(true) => {}
            Ok(false) => {
                // Retired: drop the ledger note through the identity-safe
                // reveal path and forget session membership safely.
                let _ = crate::product_hide::sys::reveal_product_claim_to(
                    store,
                    me,
                    &record.claim,
                    record.iconic,
                );
                state.hidden_claims.remove(&key);
            }
            Err(_) => {}
        }
    }
}
/// Sync monitor arrivals/removals with whole-workspace displacement/return:
/// displaced layouts are never merged, hidden windows never lost, and on
/// return the workspaces come back with then-current contents. Focus
/// preserves the surviving view unless the removed monitor held focus.
fn sync_monitor_outputs(state: &mut TileLoop, areas: &[MonitorArea]) {
    let current: Vec<String> = areas.iter().map(|a| a.device.clone()).collect();
    // Returning displaced origins are recreated by `reconnect_output` without
    // baseline empties below: never `ensure_output` them here, or a duplicate
    // trailing/minimum pair appears beside the returning workspaces.
    let displaced_origins: Vec<String> = state
        .workspaces
        .displaced_snapshot()
        .keys()
        .cloned()
        .collect();
    for key in &current {
        if displaced_origins.contains(key) {
            continue;
        }
        state.workspaces.ensure_output(key);
    }
    let gone: Vec<String> = state
        .known_outputs
        .iter()
        .filter(|k| !current.contains(k))
        .cloned()
        .collect();
    // Actual-foreground member view at disconnect time: when the removed
    // monitor held focus, the survivor shows that member's workspace instead
    // of an arbitrary one. Otherwise the surviving view keeps focus untouched.
    // The foreground key must also pass the visible lifetime gate, so a
    // same-process HWND reuse racing the disconnect never activates the wrong
    // workspace.
    let fg_workspace: Option<(String, String)> = {
        let fg = unsafe { GetForegroundWindow() } as usize as u64;
        let fg_tag = crate::product_hide::sys::read_member_tag(fg);
        state
            .member_tokens
            .iter()
            .find(|(k, _)| {
                k.hwnd == fg
                    && state.member_tags.get(*k).is_some_and(|stored| {
                        crate::workspace_owner::visible_lifetime_ok(stored, fg_tag.as_deref())
                    })
            })
            .and_then(|(k, _)| state.workspaces.member_loc(k).cloned())
            .map(|loc| (loc.output, loc.workspace))
    };
    let mut deferred: Vec<String> = Vec::new();
    for origin in gone {
        // Disconnect-time geometry decides the survivor: the origin's own
        // last-seen rectangle first, never post-disconnect frames as proxy;
        // then active output, then ordering. Never merge into a new split.
        let reference = state
            .last_areas
            .iter()
            .find(|a| a.device == origin)
            .or_else(|| state.last_areas.first())
            .map(|a| tiler_core::workspace::DisplacedRect {
                x: a.full.x,
                y: a.full.y,
                w: a.full.w,
                h: a.full.h,
            });
        let survivors: Vec<tiler_core::workspace::Survivor> = current
            .iter()
            .map(|k| {
                let rect = areas.iter().find(|a| &a.device == k).map(|a| {
                    tiler_core::workspace::DisplacedRect {
                        x: a.full.x,
                        y: a.full.y,
                        w: a.full.w,
                        h: a.full.h,
                    }
                });
                tiler_core::workspace::Survivor {
                    key: k.clone(),
                    rect,
                }
            })
            .collect();
        let active = if state.active_output.is_empty() {
            None
        } else {
            Some(state.active_output.as_str())
        };
        if let Some(dest) =
            tiler_core::workspace::choose_displaced_destination(&survivors, active, reference)
        {
            // Relocate retained Engine sessions first so layout trees,
            // contents, and focus move with the workspaces instead of
            // reseeding on the survivor. Empty or unusable sessions stay;
            // the outcome counts below stay opaque.
            let mut relocated = 0u32;
            let mut retained_count = 0u32;
            // Membership entries move below; Engine sessions follow per
            // workspace id through the existing relocation operation.
            let before: Vec<String> = state.workspaces.workspace_ids(&origin);
            for ws in &before {
                let target_key = tiler_core::session::DomainKey {
                    output: tiler_core::directional::OutputId(dest.clone()),
                    workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                };
                let bounds = areas
                    .iter()
                    .find(|a| a.device == dest)
                    .and_then(|a| tiling_domain_bounds_with(a.work, state.outer_gap));
                let Some(bounds) = bounds else {
                    retained_count += 1;
                    continue;
                };
                let target_domain = tiler_core::session::OutputDomain {
                    id: tiler_core::directional::OutputId(dest.clone()),
                    workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                    bounds,
                    gap: state.inner_gap,
                    adjacent: std::collections::BTreeMap::new(),
                };
                if state.engine.try_relocate_for_target(
                    &target_key,
                    &target_domain,
                    state.outer_gap,
                ) {
                    relocated += 1;
                } else {
                    retained_count += 1;
                }
            }
            state.workspaces.displace_output_to(&origin, &dest);
            if state.active_output == origin {
                state.active_output = dest.clone();
            }
            // The removed monitor held focus: show that member's workspace on
            // the survivor with its actual focus preserved. Any other case
            // keeps the survivor view with no stealing.
            if let Some((fg_output, fg_workspace)) = fg_workspace.clone()
                && fg_output == origin
            {
                state.workspaces.activate(&dest, &fg_workspace);
                if let Some(fg_key) = state
                    .member_tokens
                    .keys()
                    .find(|k| {
                        state
                            .workspaces
                            .member_loc(k)
                            .is_some_and(|l| l.output == dest && l.workspace == fg_workspace)
                    })
                    .cloned()
                {
                    state.workspaces.note_foreground(&fg_key);
                }
            }
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-displace",
                    "output": state.workspaces.output_token(&dest),
                    "relocated": relocated,
                    "retained": retained_count,
                }),
            );
            // Hotplug-driven observed change records like any other: observe
            // the survivor's post-actuation view so a later toggle returns
            // to the actual preceding view. `displace_output_to` already
            // discarded the removed origin's history.
            if let Some(active) = state.workspaces.active_id(&dest) {
                state.workspaces.observe_workspace_change(&dest, &active);
            }
        } else {
            // No survivor: defer with known origins kept, never drop the
            // workspaces or their Engine sessions. The disconnected output's
            // history is still discarded with the output.
            state.workspaces.discard_output_history(&origin);
            deferred.push(origin);
        }
    }
    // Reconnect returns displaced workspaces to the exact origin key with
    // then-current contents; Engine sessions relocate back first so the
    // returning monitor shows then-current trees, not saved snapshots.
    // The active view shows the returning workspace only when it holds the
    // focused window, otherwise the surviving view keeps focus with no
    // stealing (best-effort: preserve active output).
    let displaced: Vec<String> = state
        .workspaces
        .displaced_snapshot()
        .keys()
        .cloned()
        .collect();
    for origin in displaced {
        if current.contains(&origin) {
            let dest_key = state
                .workspaces
                .displaced_snapshot()
                .get(&origin)
                .map(|r| r.dest_key.clone());
            if let Some(record) = state.workspaces.displaced_snapshot().get(&origin).cloned() {
                let bounds = areas
                    .iter()
                    .find(|a| a.device == origin)
                    .and_then(|a| tiling_domain_bounds_with(a.work, state.outer_gap));
                if let Some(bounds) = bounds {
                    for ws in &record.workspace_ids {
                        let target_key = tiler_core::session::DomainKey {
                            output: tiler_core::directional::OutputId(origin.clone()),
                            workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                        };
                        let target_domain = tiler_core::session::OutputDomain {
                            id: tiler_core::directional::OutputId(origin.clone()),
                            workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                            bounds,
                            gap: state.inner_gap,
                            adjacent: std::collections::BTreeMap::new(),
                        };
                        state.engine.try_relocate_for_target(
                            &target_key,
                            &target_domain,
                            OUTER_GAP,
                        );
                    }
                }
            }
            state.workspaces.reconnect_output(&origin);
            // Reconnect primes a fresh baseline at the returning view and
            // never consults history for selection (see `reconnect_output`).
            // Record any subsequently observed survivor displacement here so
            // two changes in one handler leave the actual immediately
            // preceding view: the survivor's post-return view observes
            // against its pre-return baseline.
            let dest_key = dest_key.unwrap_or_else(|| state.active_output.clone());
            if !dest_key.is_empty()
                && let Some(active) = state.workspaces.active_id(&dest_key)
            {
                state
                    .workspaces
                    .observe_workspace_change(&dest_key, &active);
            }
        }
    }
    // Deferred no-survivor origins stay known with their last geometry until
    // a survivor exists; their workspaces and Engine sessions are untouched.
    let mut known = current.clone();
    for origin in &deferred {
        if !known.contains(origin) {
            known.push(origin.clone());
        }
    }
    state.known_outputs = known;
    let mut last = areas.to_vec();
    for origin in &deferred {
        if !last.iter().any(|a| &a.device == origin)
            && let Some(area) = state
                .last_areas
                .iter()
                .find(|a| &a.device == origin)
                .cloned()
        {
            last.push(area);
        }
    }
    state.last_areas = last;
    if state.active_output.is_empty()
        && let Some(first) = state.known_outputs.first().cloned()
    {
        state.active_output = first;
    }
}

/// Per-tick workspace maintenance on quiet ticks: monitor sync, new-window
/// assignment, close cleanup, foreground-driven select, and same-claim
/// rehide of externally revealed hidden windows.
fn workspace_maintenance(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    sync_monitor_outputs(state, areas);
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    workspace_close_cleanup(state);
    // Unconfirmed toggle releases retry here on the maintenance observation
    // (no second enumeration): the fingerprint gate inside fires only on a
    // real topology edge, so quiet ticks never poll or log.
    retry_pending_releases(state, me, fulls, areas, &observed, &retained);
    // Periodic hidden-claim retirement before any foreground-driven switch so
    // dead claims never select.
    audit_hidden_claims(state, me, store);
    // Foreground-driven select only on an actual foreground change to an
    // exactly verified hidden member; event-only wakes never switch.
    if poll_foreground_workspace(state, me, store, dir, fulls, areas, &mut observed).is_some() {
        return;
    }
    rehide_pass(state, me, store);
}

/// END-bound Esc latch decision: a physical Esc down cancels the gesture
/// only when the observed sequence differs from the START snapshot. Held
/// gestures observe the live counter; just-ended gestures observe the
/// END-callback snapshot, so an Esc that lands after the END (before the
/// owner drain) observes an equal snapshot and never cancels the completed
/// drop. Pure sequence comparison (wrapping-aware by construction: the
/// counter only moves forward one step per physical Esc down).
#[must_use]
fn esc_edge_cancels(start_seq: u64, observed_seq: u64) -> bool {
    observed_seq != start_seq
}

/// Shared START capture for both gesture producers (native WinEvent and
/// project Win+Left): the Esc-sequence snapshot, the PRE-gesture rectangle,
/// and the START-time identity snapshot. PRE comes from the last stable
/// observation, never from a mid-gesture frame; live `GetWindowRect` is the
/// fallback only when `stable` has no entry. Probe failure leaves no
/// snapshot and settle falls back to the stored-member check only.
fn capture_gesture_start(state: &mut TileLoop, me: &ProcessIdentity, hwnd: u64, start_seq: u64) {
    // Sequence-bound Esc coordination: the snapshot rode with the START
    // event from callback time instead of being re-read here. Only edges
    // with a newer sequence latch later, so a stale pre-gesture tap never
    // cancels and a same-batch edge is ordered rather than discarded.
    if let std::collections::hash_map::Entry::Vacant(e) = state.gesture_esc_seq.entry(hwnd) {
        e.insert(start_seq);
    }
    if !state.gesture_before.contains_key(&hwnd) {
        let pre = state
            .stable
            .get(&hwnd)
            .copied()
            .or_else(|| live_outer_rect(hwnd).or_else(|| state.stable.get(&hwnd).copied()));
        if let Some(pre) = pre {
            state.gesture_before.insert(hwnd, pre);
        }
    }
    // START-time identity snapshot for the settle-time reuse fence: a
    // same-process HWND reuse between START and END fails closed even when
    // HWND/PID/creation still agree.
    if let std::collections::hash_map::Entry::Vacant(e) = state.gesture_start_key.entry(hwnd) {
        let probe = hwnd as isize as HWND;
        if let Some(live) = window_identity(probe, hwnd, me) {
            e.insert(crate::workspace::WindowKey {
                hwnd,
                pid: live.pid,
                creation: live.process_creation.clone(),
            });
            // Member lifetime tag, not the owned-helper tag (empty for
            // ordinary windows): the settle fence compares this against a
            // fresh member read, exactly like the admission gate.
            state
                .gesture_start_tag
                .insert(hwnd, crate::product_hide::sys::read_member_tag(hwnd));
        }
    }
    // START-frozen preview source binding (first START wins, never
    // rebound): the exact member token, source output/workspace, and
    // accepted revision the preview samples and the final drop resolve
    // against. Unmanaged STARTs store nothing and fail closed in the
    // refresh; drift after START fails the whole gesture closed.
    if !state.gesture_preview_start.contains_key(&hwnd) {
        let start_binding = state
            .member_tokens
            .iter()
            .find(|(key, _)| key.hwnd == hwnd)
            .and_then(|(key, token)| {
                state.workspaces.member_loc(key).cloned().map(|loc| {
                    let revision = revision_for(state, &loc.output, &loc.workspace);
                    PreviewStartBinding {
                        token: token.clone(),
                        output: loc.output,
                        workspace: loc.workspace,
                        revision,
                    }
                })
            });
        match start_binding {
            Some(binding) => {
                state.gesture_preview_start.insert(hwnd, binding);
            }
            None => {
                state.gesture_preview_start.remove(&hwnd);
            }
        }
    }
}

/// One drained project Win+Left down edge: validate the callback-bound
/// candidate against fresh evidence BEFORE any effect or focus, then arm
/// the shared gesture maps so the hold pauses tiling and the release
/// settles through the shared route. After all START fences pass and the
/// arm captures `active`, the mover is focused at press through the
/// existing [`actuate_focus`] authority (R-DRAG-08): a suspend/elevation
/// precheck refuses first (no prime against a known vetoing arrival), then
/// a fresh exact foreground readback avoids a redundant setter when the
/// mover already holds foreground, otherwise one `actuate_focus` attempt
/// decides (its late fence stays the second race guard). A refused press
/// clears only this HWND's arm with no Engine plan and no geometry change;
/// a failed actuation may still have attempted the E8 prime/setter, so only
/// geometry/plan silence is claimed, never zero focus writes. No geometry
/// and no Engine call here. Returns the bound token when the gesture armed.
///
/// The callback snapshot (published origin plus member tag cloned at Down
/// time) must still match the current published entry AND a fresh live
/// probe on every field, closing the callback-to-drain HWND-reuse gap: a
/// different-process reuse fails PID/creation, a same-process reuse fails
/// the lifetime tag, and a retokened member fails the token. The validated
/// snapshot is stored as the bound generation; the Up requires it and never
/// rebinds.
///
/// A refused Down carries no identity: only the closed refusal reason
/// enters the log, never HWNDs or tokens.
fn windrag_down(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    edge: &WinDragEdge,
    fulls: &[Rect],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) -> std::result::Result<String, &'static str> {
    let hwnd = edge.hwnd;
    let Some(snapshot) = edge.snapshot.as_ref() else {
        return Err("no-snapshot");
    };
    // Tiled-only published origins plus current management: floating,
    // sticky, unmanaged, and unknown subjects never arm (their Win+Left
    // passed through natively hook-side).
    if !state.managed.contains(&hwnd) {
        return Err("unmanaged");
    }
    let Some(published) = state.windrag_origins.get(&hwnd) else {
        return Err("unknown-subject");
    };
    if state.active.contains(&hwnd)
        || state.gesture_before.contains_key(&hwnd)
        || state.windrag_bound.contains_key(&hwnd)
    {
        return Err("busy");
    }
    // Fresh live evidence on the owner thread: the snapshot must still
    // match the live window, not just the last publish.
    let probe = hwnd as isize as HWND;
    let Some(live) = window_identity(probe, hwnd, me) else {
        return Err("identity-changed");
    };
    let live_tag = crate::product_hide::sys::read_member_tag(hwnd);
    if !validate_windrag_down(
        snapshot,
        &published.origin,
        &published.tag,
        live.pid,
        &live.process_creation,
        &live_tag,
    ) {
        return Err("stale-snapshot");
    }
    // The snapshot token must still name this exact live member.
    let live_key = crate::workspace::WindowKey {
        hwnd,
        pid: live.pid,
        creation: live.process_creation.clone(),
    };
    if state
        .member_tokens
        .get(&live_key)
        .is_none_or(|token| token != &snapshot.origin.token)
    {
        return Err("no-token");
    }
    capture_gesture_start(state, me, hwnd, edge.esc_seq);
    if !state.gesture_before.contains_key(&hwnd) {
        // No PRE frame (not stable and unreadable live): fail closed with
        // no arm rather than settling without a source allocation.
        state.gesture_esc_seq.remove(&hwnd);
        state.gesture_start_key.remove(&hwnd);
        state.gesture_start_tag.remove(&hwnd);
        state.gesture_preview_start.remove(&hwnd);
        return Err("no-pre");
    }
    state.gesture_producer.insert(hwnd, "windrag");
    state.windrag_start_cursor.insert(hwnd, (edge.x, edge.y));
    state.windrag_bound.insert(hwnd, snapshot.clone());
    // A/B move arm: the project hold classifies as a move, so a focused
    // Win+Left hold reveals the projected group underlay while held (no
    // stage C, no unfocused Engine focus substitution). Cleaned on
    // Up/cancel/suspend/settle like every other gesture map.
    state.move_kind.insert(hwnd, MoveSizeKind::Move);
    state.active.insert(hwnd);
    // R-DRAG-08 press focus: the validated mover activates at press through
    // the same authority as the drop path, after every START fence and the
    // `active` capture above. A suspend/elevation precheck refuses first so
    // a known vetoing fullscreen arrival (or an elevated foreground) is
    // never primed against; the late fence inside `actuate_focus` stays the
    // second guard for arrivals racing this drain. Failure refuses the whole
    // arm via the scoped clear (this HWND's maps only: a concurrently armed
    // neighbour's preview is never disturbed) with no Engine plan and no
    // geometry change. A failed actuation may still have attempted the E8
    // prime/setter, so only geometry/plan silence is claimed.
    if suspend_read(state, me, fulls).veto.block || foreground_elevated(me) {
        clear_windrag_gesture_scoped(state, hwnd, false);
        return Err("focus-skipped-fence");
    }
    let token = snapshot.origin.token.clone();
    if unsafe { GetForegroundWindow() } as usize as u64 == hwnd {
        // Already exact foreground: no prime, no setter, no wait.
        let log_path = state.log_path.clone();
        let tick = state.tick;
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "windrag-focus",
                "tick": tick,
                "phase": "press",
                "outcome": "focus-ok",
                "already_foreground": true,
                "prime_inserted": 0,
                "setter_accepted": false,
                "attach_ok": false,
                "eventual": false,
                "duration_ms": 0,
                "window": token.as_str(),
                "producer": "windrag",
            }),
        );
        return Ok(token);
    }
    let focus_start = Instant::now();
    let actuation = actuate_focus(state, me, fulls, observed, retained, &token);
    let duration_ms = focus_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    if actuation.outcome != "focus-ok" {
        clear_windrag_gesture_scoped(state, hwnd, false);
        return Err(actuation.outcome);
    }
    let log_path = state.log_path.clone();
    let tick = state.tick;
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "windrag-focus",
            "tick": tick,
            "phase": "press",
            "outcome": actuation.outcome,
            "already_foreground": false,
            "prime_inserted": actuation.prime_inserted,
            "setter_accepted": actuation.setter_accepted,
            "attach_ok": actuation.attach_ok,
            "eventual": actuation.eventual,
            "duration_ms": duration_ms,
            "window": token.as_str(),
            "producer": "windrag",
        }),
    );
    Ok(token)
}

/// One drained project Win+Left up edge: carry the callback-bound release
/// pointer to settle and close the hold. Requires the bound generation
/// stored at arm time, so a stray or replayed Up can never rebind or
/// settle a gesture it did not close. The shared `ended` computation below
/// settles it through `gesture_tick` on this same pump.
fn windrag_up(state: &mut TileLoop, hwnd: u64, x: i32, y: i32, esc_seq: u64) {
    if !state.windrag_bound.contains_key(&hwnd) {
        return;
    }
    if state.gesture_producer.get(&hwnd).copied() != Some("windrag") {
        return;
    }
    if !state.active.contains(&hwnd) {
        return;
    }
    state.gesture_end_cursor.insert(hwnd, (x, y));
    if let std::collections::hash_map::Entry::Vacant(e) = state.gesture_end_seq.entry(hwnd) {
        e.insert(esc_seq);
    }
    state.active.remove(&hwnd);
    state.move_kind.remove(&hwnd);
}

/// Clear every project-gesture map for one HWND (owner-side Cancel): the
/// hook already disarmed and queued this edge, so the paired Up stays
/// swallowed hook-side while the owner drops the gesture with no plan and
/// no geometry change. Native gestures on the same HWND are untouched:
/// only entries produced by a validated windrag arm are removed.
/// `hide_visible_fallback` keeps the legacy Cancel behavior (a visible
/// singleton overlay hides with the gesture even without an owned binding);
/// the press-failure path passes `false` so a concurrently armed
/// neighbour's preview is never disturbed (at press this HWND owns no
/// preview binding yet).
fn clear_windrag_gesture(state: &mut TileLoop, hwnd: u64) -> Option<String> {
    clear_windrag_gesture_scoped(state, hwnd, true)
}

/// Scoped variant of [`clear_windrag_gesture`]; see `hide_visible_fallback`.
fn clear_windrag_gesture_scoped(
    state: &mut TileLoop,
    hwnd: u64,
    hide_visible_fallback: bool,
) -> Option<String> {
    if state.gesture_producer.get(&hwnd).copied() != Some("windrag")
        && !state.windrag_bound.contains_key(&hwnd)
    {
        return None;
    }
    let token = state
        .windrag_bound
        .get(&hwnd)
        .map(|bound| bound.origin.token.clone());
    let had_preview = state.preview_bound.remove(&hwnd).is_some();
    state.preview_dead.remove(&hwnd);
    state.gesture_preview_start.remove(&hwnd);
    state.gesture_start_cursor.remove(&hwnd);
    // Owner-side cancel clears the preview with the gesture (the hook
    // already disarmed; the paired Up stays swallowed hook-side).
    if had_preview || (hide_visible_fallback && state.preview_overlay.is_visible()) {
        hide_preview(state, "cancelled");
    }
    state.active.remove(&hwnd);
    state.gesture_before.remove(&hwnd);
    state.gesture_end_cursor.remove(&hwnd);
    state.gesture_start_key.remove(&hwnd);
    state.gesture_start_tag.remove(&hwnd);
    state.gesture_esc_seq.remove(&hwnd);
    state.gesture_end_seq.remove(&hwnd);
    state.esc_latched.remove(&hwnd);
    state.gesture_producer.remove(&hwnd);
    state.windrag_start_cursor.remove(&hwnd);
    state.windrag_bound.remove(&hwnd);
    state.move_kind.remove(&hwnd);
    token
}

fn gesture_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    ended: &[u64],
) {
    let log_path = state.log_path.clone();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"tick-skip","tick":state.tick,"cause":"enum-failed"}),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    for hwnd in ended {
        // Settle producer travels with the gesture: `native` for the
        // title-bar modal loop, `windrag` for the project-driven Win+Left
        // stationary hold. Absent entries settle as native.
        let producer = state.gesture_producer.remove(hwnd).unwrap_or("native");
        // Per-gesture START snapshot and END cursor travel with the gesture:
        // remove them here so each END settles exactly once.
        let start_key = state.gesture_start_key.remove(hwnd);
        let start_tag = state.gesture_start_tag.remove(hwnd);
        let end_cursor = state.gesture_end_cursor.remove(hwnd);
        let start_cursor = state.windrag_start_cursor.remove(hwnd);
        state.windrag_bound.remove(hwnd);
        state.gesture_esc_seq.remove(hwnd);
        state.gesture_end_seq.remove(hwnd);
        // Explicit Esc cancellation first: a cancelled native move restores
        // its pre-gesture frame, so rect equality must never decide this
        // path. Snap back through one bounded reconcile with no Engine
        // mutation here at all; topology unchanged by construction.
        if state.esc_latched.remove(hwnd) {
            let token = by_hwnd.get(hwnd).map(|w| w.token.clone()).or_else(|| {
                state
                    .member_tokens
                    .iter()
                    .find(|(key, _)| key.hwnd == *hwnd)
                    .map(|(_, token)| token.clone())
            });
            let mut line = serde_json::json!({
                "event": "gesture",
                "tick": state.tick,
                "op": "gesture",
                "disposition": "observed",
                "outcome": "gesture-cancelled-esc",
                "producer": producer,
            });
            if let Some(token) = token {
                line["window"] = serde_json::Value::from(token);
            }
            log_json_at(&log_path, line);
            // Esc cancel clears the preview with the gesture (already
            // hidden at latch time; this covers latch-and-settle races).
            hide_preview(state, "esc-cancelled");
            state.preview_bound.remove(hwnd);
            state.preview_dead.remove(hwnd);
            state.gesture_preview_start.remove(hwnd);
            state.gesture_start_cursor.remove(hwnd);
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            continue;
        }
        let Some(current) = by_hwnd.get(hwnd) else {
            // A maximized/fullscreen member is retained, never eligible, so a
            // gesture ending on one misses the observation above. Refuse
            // explicitly with no Engine mutation; the next ordinary tick
            // converges. Identity-fenced on the stored member identity.
            if let Some(key) = state
                .member_tokens
                .iter()
                .find(|(k, _)| k.hwnd == *hwnd)
                .map(|(k, _)| k.clone())
                && let Some(stored) = state.member_identity.get(&key).cloned()
                && crate::workspace_owner::member_matches(
                    &key,
                    *hwnd,
                    stored.pid,
                    &stored.process_creation,
                )
                && let Some(row) = retained.iter().find(|r| r.key == key)
                && let Some(cause) = overlay_refusal_for(row)
                && let Some(token) = state.member_tokens.get(&key).cloned()
            {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "gesture",
                        "disposition": "observed",
                        "outcome": if cause == "fullscreen" {
                            "gesture-refused-fullscreen"
                        } else {
                            "gesture-refused-maximize"
                        },
                        "window": token,
                    }),
                );
            }
            continue;
        };
        let before = state
            .gesture_before
            .get(hwnd)
            .copied()
            .or_else(|| state.stable.get(hwnd).copied());
        let Some(before) = before else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "observed",
                    "outcome": "gesture-no-start",
                    "window": current.token,
                    "producer": producer,
                }),
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            continue;
        };
        // START/END reuse fence: the fresh window must still match the
        // START-time snapshot when one was captured. A same-process HWND
        // reuse between START and END keeps HWND/PID/creation but starts
        // without (or with a stale) member lifetime tag, so it fails closed
        // here exactly like every other effect gate. No snapshot falls back
        // to the stored-member check below. The tag comparison is snapshot
        // versus a fresh member-property read (never the owned-helper tag,
        // which is empty for ordinary windows).
        if let (Some(key), Some(stored)) = (start_key.as_ref(), start_tag.as_ref())
            && (!crate::workspace_owner::member_matches(
                key,
                current.hwnd,
                current.identity.pid,
                &current.identity.process_creation,
            ) || {
                let live = crate::product_hide::sys::read_member_tag(current.hwnd);
                stored != &live
            })
        {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "refused",
                    "outcome": "gesture-refused-identity",
                    "window": current.token,
                    "producer": producer,
                }),
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            continue;
        }
        // Release cursor carried from the MOVESIZEEND event callback, never
        // re-read later: an END without a captured cursor routes as no-change
        // for moves (cursor-less), while resizes classify from geometry
        // alone. The project Win+Left hold bypasses rect classification
        // entirely: its frames stay at source by design, so the tracked
        // POINTER journey (down point versus callback-bound release point)
        // decides, through the same Engine drop route below.
        let intent = if producer == "windrag" {
            let (Some(start), Some(end)) = (start_cursor, end_cursor) else {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "gesture",
                        "disposition": "observed",
                        "outcome": "gesture-no-change",
                        "window": current.token,
                        "producer": producer,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            };
            match settle_pointer_journey(start.0, start.1, end.0, end.1, false) {
                WinSettle::Cancelled | WinSettle::NoChange => {
                    // Esc reaches here only via the latch above; a zero
                    // pointer journey makes no plan and changes no geometry.
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "gesture",
                            "tick": state.tick,
                            "op": "gesture",
                            "disposition": "observed",
                            "outcome": "gesture-no-change",
                            "window": current.token,
                            "producer": producer,
                        }),
                    );
                    let fulls_owned = fulls.to_vec();
                    reconcile_tick(state, me, &fulls_owned, areas);
                    continue;
                }
                WinSettle::Drop { x, y } => GestureIntent::MoveDrop { x, y },
            }
        } else {
            let cursor = end_cursor;
            let Some(classified) = classify_gesture(&before, &current.visible, cursor) else {
                // Zero movement (or a cursor-less move): nothing to route.
                // Reconcile once so any native nudge is reasserted to the
                // Engine allocation; topology unchanged.
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "gesture",
                        "disposition": "observed",
                        "outcome": "gesture-no-change",
                        "window": current.token,
                        "producer": producer,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            };
            classified
        };
        // The gesture window's own (output, workspace) session owns this
        // settle: same per-workspace routing as directional chords.
        let member_key = state
            .member_tokens
            .iter()
            .find(|(_, token)| token.as_str() == current.token.as_str())
            .map(|(key, _)| key.clone());
        let Some(member_key) = member_key else {
            continue;
        };
        if !crate::workspace_owner::member_matches(
            &member_key,
            current.hwnd,
            current.identity.pid,
            &current.identity.process_creation,
        ) {
            continue;
        }
        // Floats never feed gesture intents to the Engine; native float
        // move/resize stays free. Sticky rides the same slotless float plus
        // an explicit gate so a pruned-domain sticky still refuses.
        if state.sticky.contains_key(&member_key) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "observed",
                    "outcome": "gesture-refused-sticky",
                    "window": current.token,
                }),
            );
            continue;
        }
        if let Some(loc) = state.workspaces.member_loc(&member_key).cloned()
            && (engine_is_float(state, &loc.output, &loc.workspace, &current.token)
                || !workspace_mode_tiled(state, &loc.output, &loc.workspace))
        {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "observed",
                    "outcome": "gesture-refused-floating",
                    "window": current.token,
                }),
            );
            continue;
        }
        let Some(loc) = state.workspaces.member_loc(&member_key).cloned() else {
            continue;
        };
        let Some((domain, domain_key)) = workspace_domain_for(
            &loc.output,
            &loc.workspace,
            areas,
            state.inner_gap,
            state.outer_gap,
        ) else {
            continue;
        };
        // Item 7 is same-output only: a release cursor outside the source
        // domain bounds (including another output) refuses with snap-back
        // and no transfer. Cross-output placement belongs to a later item;
        // the Engine would refuse the point as well, but this explicit
        // fence keeps the outcome attributable before any Engine call.
        if let GestureIntent::MoveDrop { x, y } = intent
            && !drop_point_in_domain(
                domain.bounds.x,
                domain.bounds.y,
                domain.bounds.w,
                domain.bounds.h,
                x,
                y,
            )
        {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "drag-drop",
                    "disposition": "refused",
                    "outcome": "gesture-refused-cross-output",
                    "window": current.token,
                    "producer": producer,
                }),
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            continue;
        }
        // Project-driven Win+Left focus binding (KDE mover parity): the
        // gesture owns the mover even when native focus lags (unfocused
        // Win+Left starts without focusing). The Engine binds logical focus
        // to the mover itself, but the native foreground needs the existing
        // safe focus authority (E8-prime plus attach plus one setter with an
        // exact readback; never the underlay `focused_window` value). Every
        // gate above (START identity/lifetime fence, member match,
        // sticky/float refusal, domain routing, same-output fence) passed
        // before this effect. A failed actuation refuses with snap-back and
        // no plan instead of dropping under a diverged foreground.
        if producer == "windrag" {
            let actuation = actuate_focus(state, me, fulls, &observed, &retained, &current.token);
            if actuation.outcome != "focus-ok" {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "gesture",
                        "disposition": "refused",
                        "outcome": "gesture-refused-focus",
                        "window": current.token,
                        "producer": producer,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            }
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "windrag-focus",
                    "tick": state.tick,
                    "phase": "drop",
                    "outcome": "focus-ok",
                    "window": current.token,
                    "producer": producer,
                }),
            );
        }
        state.tick += 1;
        let correlation = state.correlation();
        // Incomplete snapshot defers with retained Engine state.
        let mut hint_cx = HintCx::new();
        let Some(rows) = assemble_domain_rows(
            state,
            &loc.output,
            &loc.workspace,
            &observed,
            &retained,
            "gesture",
            correlation.as_str(),
            &mut hint_cx,
        ) else {
            continue;
        };
        let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let fp = fingerprint(
            &rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        let mover = WindowId(current.token.clone());
        // Live gap edits adopt here so the drop convergence below sees
        // matching gaps instead of refusing the gesture.
        {
            let revision = revision_for(state, &loc.output, &loc.workspace);
            let outer_gap = state.outer_gap;
            adopt_gaps_for_route(
                state,
                &domain,
                &domain_key,
                outer_gap,
                &windows,
                Some(&mover),
                revision,
                fp,
                &correlation,
            );
        }
        let event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            &correlation,
            revision_for(state, &loc.output, &loc.workspace),
            fp,
            &domain,
            &domain_key,
            state.outer_gap,
            &windows,
            Some(&mover),
        );
        // Single-domain observations run the local retained
        // propose/commit path; Core owns gesture semantics.
        // The preview track carried the exact sticky group-edge hover prior
        // across samples through the same resolver; the final drop forwards
        // that exact value (or `None` when no preview resolved, e.g. an
        // unmoved-then-dropped pointer with no samples). Preview/drop
        // agreement is by construction: same observation assembly, same
        // resolver, same carried prior.
        let drop_prior = state
            .preview_bound
            .get(hwnd)
            .and_then(|bound| bound.hover_prior.clone());
        // Closed bounded trace descriptor (`none` or `group-edge:Side`):
        // proves preview-to-drop carry equality without raw NodeIds.
        let drop_prior_desc = preview_prior_desc(&drop_prior);
        // Reply rendering must never outlive the gesture: the preview is
        // gone before the Engine drop runs, so no late sample can paint
        // over the settled layout.
        hide_preview(state, "finish");
        state.preview_bound.remove(hwnd);
        state.preview_dead.remove(hwnd);
        state.gesture_preview_start.remove(hwnd);
        state.gesture_start_cursor.remove(hwnd);
        let reply = match intent {
            GestureIntent::MoveDrop { x, y } => {
                let mut event = event;
                event.command = CoreCommand::DragDrop {
                    window: current.token.clone(),
                    x,
                    y,
                    hover_prior: drop_prior,
                    source: None,
                };
                state.engine.handle(&event)
            }
            GestureIntent::PointerResize {
                direction,
                boundary,
                direction2,
                boundary2,
            } => {
                let mut event = event;
                event.command = CoreCommand::PointerResize {
                    window: current.token.clone(),
                    direction: direction.to_owned(),
                    boundary,
                    direction2: direction2.map(str::to_owned),
                    boundary2,
                };
                state.engine.handle(&event)
            }
        };
        let op = match intent {
            GestureIntent::MoveDrop { .. } => "drag-drop",
            GestureIntent::PointerResize { .. } => "pointer-resize",
        };
        match &reply {
            CoreReply::Tiled(_) | CoreReply::Projection(_) | CoreReply::Resize(_) => {
                let tick = state.tick;
                let fulls_owned = fulls.to_vec();
                let writable = writable_tokens(state, &loc.output, &loc.workspace, &observed);
                apply_geometry(
                    state,
                    ApplyInput {
                        me,
                        fulls: &fulls_owned,
                        reply: &reply,
                        observed: &observed,
                        op,
                        tick,
                        correlation: correlation.as_str(),
                        skipped: Vec::new(),
                        writable: &writable,
                        output_token: state.workspaces.output_token(&loc.output),
                        workspace_token: state
                            .workspaces
                            .workspace_token(&loc.output, &loc.workspace),
                        revision: revision_for(state, &loc.output, &loc.workspace),
                    },
                );
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "correlation": correlation.as_str(),
                        "op": op,
                        "disposition": "applied",
                        "outcome": if op == "drag-drop" { "drag-drop-applied" } else { "pointer-resize-applied" },
                        "window": current.token,
                        "producer": producer,
                        "hover_prior": drop_prior_desc,
                    }),
                );
            }
            _ => {
                // Refusal converges through one bounded reconcile (snap-back
                // to the retained allocation), matching the KDE
                // restore-marker outcome with no ledger. Self, centre-stack,
                // and unsupported targets land here with no plan and no
                // commit; the closed Engine kind rides along for attribution.
                let kind = match &reply {
                    CoreReply::Rejected { kind, .. } => *kind,
                    CoreReply::Diverged(reason) => reason.as_str(),
                    CoreReply::SnapshotInvalid { detail, .. } => *detail,
                    _ => "unhandled-variant",
                };
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "correlation": correlation.as_str(),
                        "op": op,
                        "disposition": "refused",
                        "outcome": "drag-drop-refused",
                        "kind": kind,
                        "window": current.token,
                        "producer": producer,
                        "hover_prior": drop_prior_desc,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
            }
        }
    }
    state.gesture_before.clear();
    state.esc_latched.clear();
    state.gesture_esc_seq.clear();
    state.gesture_end_seq.clear();
    state.gesture_end_cursor.clear();
    state.gesture_start_key.clear();
    state.gesture_start_tag.clear();
    state.gesture_producer.clear();
    state.windrag_start_cursor.clear();
    state.windrag_bound.clear();
    state.preview_bound.clear();
    state.preview_dead.clear();
    state.gesture_preview_start.clear();
    state.gesture_start_cursor.clear();
    // No gesture remains open: no preview rectangle survives the settle.
    hide_preview(state, "finish");
}

/// Native foreground veto read: one fresh pass over the live foreground with
/// exact desktop identity, fresh visibility, caption, DWM frame, and DWM
/// cloak facts fed into the portable [`crate::workspace_owner::classify_foreground`] policy.
/// No titles, paths, PIDs, or content leave this function; the caller logs
/// only the returned reason plus booleans. Pure reads, never IO.
struct ForegroundRead {
    veto: crate::workspace_owner::ForegroundVeto,
    desktop: bool,
    visible: bool,
    captioned: bool,
    dwm_readable: bool,
    cloaked: bool,
}

fn foreground_read(fulls: &[Rect]) -> ForegroundRead {
    use crate::workspace_owner::{ForegroundFacts, ForegroundVetoReason, classify_foreground};
    let settled = |veto: crate::workspace_owner::ForegroundVeto,
                   desktop: bool,
                   visible: bool,
                   captioned: bool,
                   dwm_readable: bool,
                   cloaked: bool| {
        ForegroundRead {
            veto,
            desktop,
            visible,
            captioned,
            dwm_readable,
            cloaked,
        }
    };
    let allow = |reason: ForegroundVetoReason,
                 desktop: bool,
                 visible: bool,
                 captioned: bool,
                 dwm_readable: bool,
                 cloaked: bool| {
        settled(
            crate::workspace_owner::ForegroundVeto {
                block: false,
                reason,
            },
            desktop,
            visible,
            captioned,
            dwm_readable,
            cloaked,
        )
    };
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        // No foreground window: nothing covering.
        return allow(
            ForegroundVetoReason::None,
            false,
            false,
            false,
            false,
            false,
        );
    }
    if unsafe { IsWindow(foreground) } == 0 {
        // `IsWindowVisible` returns false for invalid handles, so validity is
        // checked first and fails closed.
        return settled(
            crate::workspace_owner::ForegroundVeto {
                block: true,
                reason: ForegroundVetoReason::Invalid,
            },
            false,
            false,
            false,
            false,
            false,
        );
    }
    // Exact known desktop shell handles are the desktop, not a fullscreen
    // application. Narrow API identity only: no class-name, exe, tool/popup,
    // or unmanaged exceptions.
    let shell = unsafe { GetShellWindow() };
    let desktop_handle = unsafe { GetDesktopWindow() };
    let is_desktop = (!shell.is_null() && shell == foreground)
        || (!desktop_handle.is_null() && desktop_handle == foreground);
    // Fresh unambiguous visibility read on the valid handle: invisible
    // windows cannot be covering fullscreen.
    let visible = unsafe { IsWindowVisible(foreground) } != 0;
    let style = unsafe {
        SetLastError(0);
        GetWindowLongW(foreground, GWL_STYLE)
    } as u32;
    let style_err = unsafe { GetLastError() };
    let style_ok = style != 0 || style_err == 0;
    let captioned = style & WS_CAPTION != 0;
    let mut visible_raw: RECT = unsafe { std::mem::zeroed() };
    let dwm_ok = unsafe {
        DwmGetWindowAttribute(
            foreground,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut visible_raw as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    } == 0;
    // Fresh cloak fact on the same valid handle: a cloaked foreground is
    // invisible to the compositor and never a covering fullscreen, even
    // with a monitor-spanning frame. Query failure fails closed (unreadable)
    // like the frame read, matching the admission and hold gates.
    let mut cloak_val: i32 = 0;
    let cloak_ok = unsafe {
        DwmGetWindowAttribute(
            foreground,
            DWMWA_CLOAKED,
            (&mut cloak_val as *mut i32).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    } == 0;
    if unsafe { IsWindow(foreground) } == 0 {
        return settled(
            crate::workspace_owner::ForegroundVeto {
                block: true,
                reason: ForegroundVetoReason::Invalid,
            },
            false,
            false,
            false,
            false,
            false,
        );
    }
    let readable = dwm_ok && style_ok;
    let covers_monitor = readable
        && rect_from_win(visible_raw)
            .is_some_and(|visible| is_borderless_fullscreen(true, visible, fulls));
    let veto = classify_foreground(ForegroundFacts {
        valid: true,
        is_desktop,
        visible,
        captioned,
        dwm_readable: readable,
        cloak_readable: cloak_ok,
        cloaked: cloak_val != 0,
        covers_monitor,
    });
    settled(
        veto,
        is_desktop,
        visible,
        captioned,
        readable,
        cloak_val != 0,
    )
}

/// Managed-aware suspend read: the raw foreground veto, except a real
/// fullscreen foreground verified as a managed member or a born-held
/// first-seen fullscreen rides a retained overlay with no geometry writes
/// and never suspends the workspace (`ManagedOverlay`, like a managed
/// maximize which keeps its caption and never vetoes). Unverified fullscreen
/// still suspends, and unreadable/invalid foregrounds always stay blocked:
/// a non-covering or unreadable window is never labeled a managed overlay.
/// The managed branch reuses the single `holdable_key` verification
/// (identity, session, medium integrity, safety gates, scope, hosted child,
/// proof ownership) plus membership and the live lifetime tag, so no gate
/// is duplicated. Born tracking honors the first-seen gate: a managed
/// overlay transition (slotted member or lifetime-known non-fullscreen)
/// never re-enters the hold.
fn suspend_read(state: &mut TileLoop, me: &ProcessIdentity, fulls: &[Rect]) -> ForegroundRead {
    use crate::workspace_owner::ForegroundVetoReason;
    let raw = foreground_read(fulls);
    if !raw.veto.block {
        return raw;
    }
    if raw.veto.reason != ForegroundVetoReason::Fullscreen {
        return raw;
    }
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    if foreground == 0 {
        return raw;
    }
    let managed = || ForegroundRead {
        veto: crate::workspace_owner::ForegroundVeto {
            block: false,
            reason: ForegroundVetoReason::ManagedOverlay,
        },
        desktop: raw.desktop,
        visible: raw.visible,
        captioned: raw.captioned,
        dwm_readable: raw.dwm_readable,
        cloaked: raw.cloaked,
    };
    let Some(held_key) = holdable_key(state, me, foreground) else {
        return raw;
    };
    let tag_ok = {
        let live_tag = crate::product_hide::sys::read_member_tag(foreground);
        state.member_tags.get(&held_key).is_some_and(|stored| {
            crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
        })
    };
    if state.workspaces.member_loc(&held_key).is_some()
        && state.member_tokens.contains_key(&held_key)
        && tag_ok
    {
        return managed();
    }
    let known_slot = state
        .member_tokens
        .get(&held_key)
        .is_some_and(|token| state.member_rects.contains_key(token));
    if !should_hold_born_fullscreen(
        true,
        known_slot,
        state.seen_nonfullscreen.contains(&held_key),
    ) {
        return raw;
    }
    managed()
}

/// Owned inputs for one `run_tile_loop` invocation. Bundled so the loop
/// entry keeps a narrow signature as keyboard policy joins the run.
/// `mouse_snap` arms session-only `SPI_SETWINARRANGING FALSE` prevention for
/// product runs; proof runs always pass false (no settings changes).
struct TileRun {
    seconds: Option<u64>,
    trace: bool,
    allowlist: Option<Vec<AllowEntry>>,
    proof: bool,
    /// `shortcut-proof` only: proof geometry gate stays on, but the owner
    /// installs the hook with test-only marker acceptance and drives the same
    /// keyboard dispatcher plus session-only mouse routines. Product `tile`
    /// and `tile-proof` always pass false.
    shortcut_proof: bool,
    /// `workspace-proof` only: like `shortcut-proof` plus the workspace
    /// dispatcher (select/send/follow with hide/reveal) for exactly the
    /// frozen allowlist. Never enabled on the product path.
    workspace_proof: bool,
    raw_argv: Vec<String>,
    keyboard: KeyboardConfig,
    mouse_snap: bool,
    /// First-run note for the `tile-start` log (`None` on proof paths, which
    /// never prompt): the chosen preset plus whether it persisted.
    first_run: Option<String>,
    /// Normal owner only (`None` on proof paths): the parsed options plus the
    /// pre-lease settings state, resolved into the effective options under
    /// the owner lease before any effect or hook (so a refused second owner
    /// never prompts and a mid-prompt file race never overwrites).
    first_run_pending: Option<FirstRunPending>,
    /// Live inner/outer gaps for this run (settings base, CLI overrides).
    inner_gap: i32,
    outer_gap: i32,
    /// Settings directory for live polling (`None` disables polling: proof
    /// owners never load the user's store).
    settings_dir: Option<std::path::PathBuf>,
    /// Initialized last-good live settings (`None` disables polling).
    settings_live: Option<LiveSettings>,
    /// Explicit startup CLI switches, authoritative for the whole run (empty
    /// on proof paths, which never poll the file).
    cli_overrides: crate::tiling::CliOverrides,
    /// Explicit normal-mode scope filter (empty means no filter). Proof runs
    /// always pass empty (frozen allowlist is the gate there).
    scope: Vec<String>,
    /// Explicit host-to-child scope pairs (empty means no child constraint).
    /// Proof runs always pass empty.
    scope_hosts: Vec<ScopeHostChild>,
    border: ActiveBorderOptions,
    underlay: GroupUnderlayOptions,
}

/// Apply validated live settings through the owner's pump (normal owner
/// only). Returns true when owner state changed, so the caller wakes the pump
/// and gap adoption reconciles promptly. Gaps flip immediately and adopt
/// through the retained update-gaps path in `reconcile_tick` (plain reconcile
/// would refuse the drift); border/underlay assign directly for the next
/// overlay refresh; keyboard plus rebound/suppression chords route through the
/// hook's `apply_live_config` (in-flight holds keep their down-time verdict
/// and mask, the stale queue drains, nothing dispatches under the new
/// config); mouse prevention toggles through the existing conditional
/// restore (off) and fresh-preimage disable (on) routines, flag-only while
/// suspended so the resume transition drives the effect.
///
/// Status semantics are honest by construction: `saved:{revision}` means the
/// file revision is assigned to owner state (per-lane evidence rides the
/// `settings-applied` lanes plus the `settings-keyboard` row); the mouse
/// effect rides the existing suspend/resume outcome logs (`mouse-pending`
/// while suspended defers it to resume); gap adoption rides the Engine
/// update-gaps trace. Invalid files keep last-good with a `degraded:{reason}`
/// status and are never rewritten here.
///
/// Explicit startup CLI switches stay authoritative: the file base applies
/// underneath [`crate::tiling::CliOverrides`], so an unrelated file edit
/// never drops a CLI lane.
#[allow(clippy::too_many_arguments)]
fn apply_live_settings(
    state: &mut TileLoop,
    dir: &Path,
    me: &ProcessIdentity,
    store: &LedgerStore,
    snap_want: &mut bool,
    settings: crate::settings::Settings,
    mtime: Option<std::time::SystemTime>,
) -> bool {
    let log_path = state.log_path.clone();
    let mut base = crate::settings::tile_options_from_settings(&settings);
    crate::tiling::apply_cli_overrides(&mut base, &state.cli_overrides);
    let mut lanes: Vec<&'static str> = Vec::new();
    let mut changed = false;
    if base.inner_gap != state.inner_gap || base.outer_gap != state.outer_gap {
        state.inner_gap = base.inner_gap;
        state.outer_gap = base.outer_gap;
        lanes.push("gaps");
        changed = true;
    }
    // Live default edits adopt for future workspaces only: existing session
    // overrides keep their state, exactly like startup seeding.
    if state
        .workspaces
        .set_default_tiled(settings.core.workspace.default_tiled)
    {
        lanes.push("workspace-default");
        changed = true;
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "workspace-default",
                "default_tiled": settings.core.workspace.default_tiled,
                "outcome": "adopted",
                "scope": "future-workspaces",
            }),
        );
    }
    if base.border != state.border {
        state.border = base.border;
        lanes.push("border");
        changed = true;
    }
    if base.underlay != state.underlay {
        state.underlay = base.underlay;
        lanes.push("underlay");
        changed = true;
    }
    let keyboard = KeyboardConfig {
        takeover: !base.no_keyboard_snap_takeover,
        allow_win_l: base.allow_win_l,
    };
    let remap = crate::settings::build_remap(&settings);
    let disabled = crate::settings::build_disabled(&settings);
    if keyboard != state.keyboard || remap != state.last_remap || disabled != state.last_disabled {
        state.keyboard = keyboard;
        state.last_remap = remap.clone();
        state.last_disabled = disabled.clone();
        let drained = crate::snapkey::sys::apply_live_config(keyboard, &remap, &disabled);
        lanes.push("keyboard");
        changed = true;
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "settings-keyboard",
                "takeover": keyboard.takeover,
                "allow_win_l": keyboard.allow_win_l,
                "remap": remap.len(),
                "disabled": disabled.len(),
                "drained": drained,
            }),
        );
    }
    let want_snap = !base.no_mouse_snap_prevention;
    if want_snap != *snap_want {
        // Mirror the suspend/resume fencing: while suspended the ledger
        // already holds the restored state, so only the flag flips and the
        // resume transition drives the effect.
        if want_snap {
            if !state.suspended {
                snap_resume(dir, me, store);
            }
        } else if !state.suspended {
            snap_suspend(dir, me, store);
        }
        *snap_want = want_snap;
        lanes.push(if state.suspended {
            "mouse-pending"
        } else {
            "mouse"
        });
        changed = true;
    }
    let revision = settings.revision;
    // R-MOV-03: the same-axis mode adopts here with no lane and no rebuild:
    // assigning `live.settings` below is enough, since `keyboard_tick` reads
    // the mode per move. A change to only this field returns false, so no
    // tree rebuild or resync follows; the next move simply uses the new mode.
    // R-MAX-06 rides the same assignment: `admit_slotless_maximized` binds
    // the born-maximized policy per fresh lifetime at admission, so a
    // setting-only change adopts for subsequent first admissions with no
    // rebuild, and already-bound lifetimes plus previously admitted clients
    // keep their slots and topology.
    if let Some(live) = state.settings_live.as_mut() {
        live.settings = settings;
        live.mtime = mtime;
        live.status = format!("saved:{revision}");
    }
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "settings-applied",
            "revision": revision,
            "lanes": lanes,
        }),
    );
    changed
}

/// Poll the settings file once per pump (normal owner only; proof owners
/// never poll). Changed files validate then apply (returns true when owner
/// state changed); invalid files keep the last-good values live with a
/// degraded status and are never rewritten here (no blind reset). Degraded
/// transitions log once per status.
fn poll_live_settings(
    state: &mut TileLoop,
    dir: &Path,
    me: &ProcessIdentity,
    store: &LedgerStore,
    snap_want: &mut bool,
) -> bool {
    let Some(settings_dir) = state.settings_dir.clone() else {
        return false;
    };
    let Some(last) = state.settings_live.clone() else {
        return false;
    };
    match crate::settings::poll_for_change(&settings_dir, &last) {
        crate::settings::PollOutcome::Unchanged => false,
        crate::settings::PollOutcome::Changed(settings) => {
            let mtime = std::fs::metadata(settings_dir.join(crate::settings::SETTINGS_FILE_NAME))
                .and_then(|meta| meta.modified())
                .ok();
            apply_live_settings(state, dir, me, store, snap_want, settings, mtime)
        }
        crate::settings::PollOutcome::InvalidKept(reason) => {
            let degraded = format!("degraded:{reason}");
            let changed = state
                .settings_live
                .as_ref()
                .is_some_and(|live| live.status != degraded);
            if changed {
                if let Some(live) = state.settings_live.as_mut() {
                    live.status = degraded.clone();
                }
                log_json_at(
                    &state.log_path.clone(),
                    serde_json::json!({
                        "event": "settings-degraded",
                        "status": degraded,
                    }),
                );
            }
            false
        }
    }
}

fn run_tile_loop(
    dir: &Path,
    me: &ProcessIdentity,
    store: &LedgerStore,
    run: TileRun,
) -> Result<()> {
    let TileRun {
        seconds,
        trace,
        allowlist,
        proof,
        shortcut_proof,
        workspace_proof,
        raw_argv,
        mut keyboard,
        mut mouse_snap,
        mut first_run,
        first_run_pending,
        mut inner_gap,
        mut outer_gap,
        mut settings_dir,
        mut settings_live,
        cli_overrides,
        mut scope,
        mut scope_hosts,
        mut border,
        mut underlay,
    } = run;
    // Prevention needs an active loop: proof never arms it, and the guarded
    // setup already captured the preimage plus the initial effect. The live
    // settings poll may flip this per pump (normal owner only).
    let mut snap_want = mouse_snap && (!proof || shortcut_proof || workspace_proof);
    ensure_pm_v2()?;
    let owner = OwnerId::parse(OWNER_ID).expect("static owner token is valid");
    let generation =
        GenerationId::parse(&me.process_creation).ok_or_else(|| err("error: bad generation"))?;
    let log_path = crate::lifecycle::sys::log_path_for(dir, &me.process_creation);
    let audit_path: Option<std::path::PathBuf> = if proof {
        Some(dir.join(format!("proof-audit-{}.jsonl", me.process_creation)))
    } else {
        None
    };
    // First-run preset under the owner lease (normal owner only): the lease
    // refusal already stopped a second owner before any UI, and the resolved
    // choice reapplies every explicit CLI lane before effects or hooks.
    // Proof paths carry no pending state and keep deterministic config.
    if !proof && let Some(pending) = first_run_pending {
        let resolved = resolve_first_run(dir, me, pending, &cli_overrides, &log_path)?;
        keyboard = KeyboardConfig {
            takeover: !resolved.options.no_keyboard_snap_takeover,
            allow_win_l: resolved.options.allow_win_l,
        };
        mouse_snap = !resolved.options.no_mouse_snap_prevention;
        snap_want = mouse_snap;
        first_run = resolved.first_run;
        inner_gap = resolved.options.inner_gap;
        outer_gap = resolved.options.outer_gap;
        settings_dir = resolved.settings_dir;
        settings_live = resolved.live;
        scope = resolved.options.scope_exes.clone();
        scope_hosts = resolved.options.scope_hosts.clone();
        border = resolved.options.border;
        underlay = resolved.options.underlay;
    }
    // Proof audit starts before any geometry: raw received argv (elements
    // stay separate strings so spaces survive without quoting), parsed
    // seconds/trace/mode, and allowlist digest/count. The frozen identity
    // record follows verification below. Raw argv carries flags and paths
    // only (no titles or window content) and never enters production logs.
    if let Some(audit) = audit_path.as_ref() {
        if audit.exists() {
            return Err(err("refuse: audit preexists, will not overwrite"));
        }
        let digest = allowlist
            .as_ref()
            .map(|entries| allowlist_digest(entries))
            .unwrap_or_default();
        let count = allowlist.as_ref().map(|e| e.len()).unwrap_or(0);
        let line = serde_json::json!({
            "event": "proof-start",
            "argv": raw_argv,
            "seconds": seconds,
            "trace": trace,
            "mode": if workspace_proof {
                "workspace-proof"
            } else if shortcut_proof {
                "shortcut-proof"
            } else {
                "proof"
            },
            "allowlist_digest": digest,
            "allowlist_count": count,
        });
        log_json_at(audit, line);
    }
    // Proof gate before first geometry: every frozen entry must verify as an
    // owned helper (sibling exe/class/tag plus full identity). Invisible
    // passive members verify here and stay frozen until `show`.
    if proof {
        let entries = allowlist
            .as_ref()
            .ok_or_else(|| err("refuse: allowlist missing"))?;
        if entries.is_empty() {
            return Err(err("refuse: empty allowlist"));
        }
        for (index, entry) in entries.iter().enumerate() {
            verify_proof_owned(entry.hwnd, entry, me).map_err(|reason| {
                err(format!(
                    "refuse: proof allowlist non-owned index={index} {reason}"
                ))
            })?;
        }
        if let Some(audit) = audit_path.as_ref() {
            let frozen: Vec<serde_json::Value> = entries
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "hwnd": e.hwnd,
                        "pid": e.pid,
                        "process_creation": e.process_creation,
                        "exe_path": e.exe_path,
                        "user_sid": e.user_sid,
                        "session_id": e.session_id,
                        "tag": e.tag,
                    })
                })
                .collect();
            log_json_at(
                audit,
                serde_json::json!({"event": "proof-frozen", "windows": frozen}),
            );
        }
    }
    let last_remap = settings_live
        .as_ref()
        .map(|live| crate::settings::build_remap(&live.settings))
        .unwrap_or_default();
    let last_disabled = settings_live
        .as_ref()
        .map(|live| crate::settings::build_disabled(&live.settings))
        .unwrap_or_default();
    let mut state = TileLoop {
        engine: Engine::new(),
        owner: owner.clone(),
        generation: generation.clone(),
        tokens: TokenMap::default(),
        refused: RefusedTracker::default(),
        stable: HashMap::new(),
        managed: HashSet::new(),
        active: HashSet::new(),
        gesture_before: HashMap::new(),
        gesture_end_cursor: HashMap::new(),
        gesture_start_key: HashMap::new(),
        gesture_start_tag: HashMap::new(),
        tick: 0,
        allowlist,
        scope,
        scope_hosts,
        trace,
        suspended: false,
        last_summary: None,
        log_path: log_path.clone(),
        audit_path: audit_path.clone(),
        keyboard,
        inner_gap,
        outer_gap,
        settings_live,
        settings_dir,
        last_remap,
        last_disabled,
        cli_overrides,
        snap_dropped: 0,
        cb_diag_dropped: 0,
        cb_diag_filtered: 0,
        mask_sends: 0,
        mask_send_max_us: 0,
        snap_origins: HashMap::new(),
        windrag_origins: HashMap::new(),
        gesture_producer: HashMap::new(),
        windrag_start_cursor: HashMap::new(),
        windrag_bound: HashMap::new(),
        windrag_dropped: 0,
        windrag_origin_logged: None,
        windrag_stats_logged: (0, 0, 0, 0),
        snap_advance: None,
        resize_repeat: None,
        last_enumerated: 0,
        workspaces: ManagedWorkspaces::new(),
        member_tokens: std::collections::BTreeMap::new(),
        member_rects: HashMap::new(),
        hidden_claims: std::collections::BTreeMap::new(),
        member_identity: std::collections::BTreeMap::new(),
        member_tags: std::collections::BTreeMap::new(),
        active_output: String::new(),
        workspace_proof: false,
        maximize_admission_attempted: HashSet::new(),
        born_keep_overlay: BTreeSet::new(),
        born_fullscreen: BTreeSet::new(),
        seen_nonfullscreen: BTreeSet::new(),
        last_foreground: 0,
        known_outputs: Vec::new(),
        last_hwnds: HashSet::new(),
        last_areas: Vec::new(),
        hint_logged: HashMap::new(),
        restore_wake: None,
        border,
        border_overlay: BorderOverlay::default(),
        border_last: None,
        underlay,
        underlay_overlay: UnderlayOverlay::default(),
        underlay_last: None,
        move_kind: HashMap::new(),
        gesture_start_cursor: HashMap::new(),
        preview_overlay: PreviewOverlay::default(),
        preview_last: None,
        preview_bound: HashMap::new(),
        gesture_preview_start: HashMap::new(),
        preview_dead: HashSet::new(),
        esc_latched: HashSet::new(),
        gesture_esc_seq: HashMap::new(),
        gesture_end_seq: HashMap::new(),
        underlay_chord_last: false,
        float_topmost_prev: std::collections::BTreeMap::new(),
        float_rects: HashMap::new(),
        floated: BTreeSet::new(),
        sticky: std::collections::BTreeMap::new(),
        pending_releases: crate::workspace::PendingReleases::new(),
        pending_retry_fp: 0,
    };
    state.engine.sync_binding(&owner, &generation);
    state.workspace_proof = workspace_proof;
    // Existing workspaces take the saved default at owner startup (initially
    // tiled); live default edits later affect only newly created workspaces.
    // Overrides reset on owner restart: nothing persists per workspace.
    let startup_default = state
        .settings_live
        .as_ref()
        .map(|live| live.settings.core.workspace.default_tiled)
        .unwrap_or(true);
    state.workspaces.set_default_tiled(startup_default);
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "workspace-default",
            "default_tiled": startup_default,
            "outcome": "adopted",
            "scope": "startup",
        }),
    );
    let mut areas = all_monitors()?;
    let (mut monitor, mut monitor_count) = (areas[0].clone(), areas.len());
    let mode = if workspace_proof {
        "workspace-proof"
    } else if shortcut_proof {
        "shortcut-proof"
    } else if state.allowlist.is_some() {
        "proof"
    } else {
        "normal"
    };
    // Visible takeover state: default on, explicit off, proof never hooks
    // except shortcut-proof/workspace-proof with test-only marker acceptance.
    let takeover = (state.keyboard.takeover && !proof) || shortcut_proof || workspace_proof;
    // Per-output-local workspaces start from live monitors: one domain set
    // per device key, retained Engine sessions per (output, workspace).
    sync_monitor_outputs(&mut state, &areas);
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "tile-start",
            "mode": mode,
            "trace": trace,
            "monitors": monitor_count,
            "work": [monitor.work.x, monitor.work.y, monitor.work.w, monitor.work.h],
            "full": [monitor.full.x, monitor.full.y, monitor.full.w, monitor.full.h],
            "inner": state.inner_gap,
            "outer": state.outer_gap,
            "keyboard": {"takeover": takeover, "allow_win_l": state.keyboard.allow_win_l},
            "mouse_snap_prevention": snap_want,
            "settings": state.settings_live.as_ref().map(|live| live.status.clone()),
            "first_run": first_run,
            "scope_count": state.scope.len(),
            "active_border": {
                "enabled": state.border.enabled,
                "width": state.border.style.width,
                "gap": state.border.style.gap,
                "radius": state.border.style.radius,
                "color": crate::active_border::render_color(state.border.style.color),
                "use_theme": state.border.style.use_theme,
            },
        }),
    );
    // Hook on this thread; this thread pumps messages, so callbacks run here.
    // Four narrow hooks keep the callback cheap: foreground wakes, move/size
    // transitions queue losslessly, object create/destroy/show/hide wakes,
    // and explicit location-change wakes (a range up to LOCATIONCHANGE would
    // also pull unrelated state-change noise, so it stays separate).
    let hooks = unsafe {
        [
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                std::ptr::null_mut(),
                Some(winevent_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            ),
            SetWinEventHook(
                EVENT_SYSTEM_MOVESIZESTART,
                EVENT_SYSTEM_MOVESIZEEND,
                std::ptr::null_mut(),
                Some(winevent_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            ),
            SetWinEventHook(
                EVENT_OBJECT_CREATE,
                EVENT_OBJECT_SHOW
                    .max(EVENT_OBJECT_DESTROY)
                    .max(EVENT_OBJECT_HIDE),
                std::ptr::null_mut(),
                Some(winevent_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            ),
            SetWinEventHook(
                EVENT_OBJECT_LOCATIONCHANGE,
                EVENT_OBJECT_LOCATIONCHANGE,
                std::ptr::null_mut(),
                Some(winevent_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            ),
        ]
    };
    if hooks.iter().any(|hook| hook.is_null()) {
        for hook in hooks {
            if !hook.is_null() {
                unsafe {
                    UnhookWinEvent(hook);
                }
            }
        }
        return Err(err("error: SetWinEventHook failed"));
    }
    // Owner-owned notification tray (normal owner only; proof owners stay
    // deterministic with no icon). Created after the hooks so no hook-failure
    // early return can leak it, and RAII `Drop` covers every later path.
    // Creation failure degrades to tray-less tiling, never a refused run.
    // The hidden owner window rides the loop-thread pump below, so its
    // TaskbarCreated and tray callbacks dispatch on this same thread. The
    // conflict projection uses the effective runtime keyboard policy
    // (takeover off passes everything through, so nothing warns; the Win+L
    // opt-in follows the runtime allow_win_l) over the CLI-overridden
    // settings base.
    let mut tray: Option<crate::tray_sys::TrayOwner> = if proof {
        None
    } else {
        let (conflict_empty, settings_status) = match state.settings_live.as_ref() {
            Some(live) => (
                crate::tray::unresolved_conflicts(
                    &live.settings,
                    state.keyboard.takeover,
                    state.keyboard.allow_win_l,
                )
                .is_empty(),
                live.status.clone(),
            ),
            None => (true, "saved:unknown".to_owned()),
        };
        // Session-only workspace toggle: the menu carries its rendered
        // opaque scope, the owner loop drains it once on its pump via
        // `take_toggle_request` and verifies it below. Default picks persist
        // through the settings file (normal owner).
        let toggle = Arc::new(Mutex::new(None::<crate::tray::ToggleScope>));
        match crate::tray_sys::TrayOwner::create(
            conflict_empty,
            &settings_status,
            Some(log_path.clone()),
            &toggle,
            state.settings_dir.clone(),
        ) {
            Ok(owner) => Some(owner),
            Err(error) => {
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"tray-unavailable","cause": error.to_string()}),
                );
                None
            }
        }
    };
    // Product keyboard takeover on the loop thread: the callback runs during
    // pump_wait on this same thread, so install/uninstall bracket the loop
    // with no join. Plain proof mode never installs a hook; shortcut-proof
    // installs with test-only marker acceptance. Install is best-effort with
    // bounded backoff retries below: failure degrades to keyboard-unavailable
    // (nothing consumes) while tiling continues, never a refused run.
    let mut snap_hook = None;
    let mut snap_failures: u32 = 0;
    let mut snap_retry_at: Option<Instant> = None;
    // Project Win+Left hook on the loop thread, same pump as the keyboard
    // hook. Installed once when takeover holds; normal runs also install it
    // while takeover is off so a live settings re-enable takes effect (the
    // gate below keeps everything passing through until then). Install
    // failure degrades to title-bar-only movement (Win+Left passes through
    // natively) while tiling continues, never a refused run.
    let windrag_hook = if takeover || !proof {
        match crate::win_mouse::sys::install() {
            Ok(hook) => {
                log_json_at(&log_path, serde_json::json!({"event":"windrag-available"}));
                Some(hook)
            }
            Err(message) => {
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"windrag-unavailable","cause": message}),
                );
                None
            }
        }
    } else {
        None
    };
    let deadline = seconds.map(|s| Instant::now() + Duration::from_secs(s));
    let mut slow_last = Instant::now();
    let result = (|| -> Result<()> {
        let fulls = monitor_fulls(&areas);
        // Mouse prevention is deferred until an actual active tick or resume:
        // an initial fullscreen must not disable only to immediately restore.
        // The first active tick, resume, or work-area change drives the
        // effect; teardown and crash restore keep the lease-held guarantee.
        let mut snap_primed = false;
        // Fullscreen guard applies before the initial tick as well.
        // Verified managed overlays bypass like a managed maximize.
        if suspend_read(&mut state, me, &fulls).veto.block {
            state.suspended = true;
            state.snap_advance = None;
            log_json_at(
                &log_path,
                serde_json::json!({"event":"suspend","cause":"fullscreen-foreground"}),
            );
            if snap_want {
                snap_suspend(dir, me, store);
            }
        } else {
            if !areas
                .iter()
                .any(|a| tiling_domain_bounds_with(a.work, state.outer_gap).is_some())
            {
                return Err(err("error: work area cannot carry outer gap"));
            }
            if snap_want {
                snap_resume(dir, me, store);
                snap_primed = true;
            }
            reconcile_tick(&mut state, me, &fulls, &areas);
            refresh_active_border(&mut state, me, &fulls);
            refresh_group_underlay(&mut state, me, &fulls, &areas);
        }
        loop {
            if stop_requested(dir, me)? || tray.as_ref().is_some_and(|t| t.stop_requested()) {
                return Ok(());
            }
            if deadline.is_some_and(|end| Instant::now() >= end) {
                return Ok(());
            }
            pump_wait(TICK_POLL_MS);
            let events = hook_queue()
                .lock()
                .map(|mut queue| std::mem::take(&mut *queue))
                .unwrap_or_default();
            let mut woke = false;
            for event in events {
                match event {
                    HookEvent::Wake(raw) => {
                        // Own overlay events never wake the loop: no repeated
                        // full reconcile off our own move/paint/show.
                        if state.border_overlay.hwnd() == Some(raw as u64)
                            || state.underlay_overlay.hwnd() == Some(raw as u64)
                            || state.preview_overlay.hwnd() == Some(raw as u64)
                        {
                            continue;
                        }
                        woke = true;
                    }
                    HookEvent::MoveSizeStart(raw, start_seq) => {
                        if state.border_overlay.hwnd() == Some(raw as u64)
                            || state.underlay_overlay.hwnd() == Some(raw as u64)
                            || state.preview_overlay.hwnd() == Some(raw as u64)
                        {
                            continue;
                        }
                        woke = true;
                        let hwnd = raw as u64;
                        // Shared START capture (Esc snapshot, PRE frame,
                        // identity fence); see `capture_gesture_start`. The
                        // single post-loop drain below latches edges for
                        // both held and just-ended gestures.
                        capture_gesture_start(&mut state, me, hwnd, start_seq);
                        state.active.insert(hwnd);
                        // Move-vs-resize classification, sampled once at START
                        // (the WinEvent itself does not distinguish). Hung or
                        // unreadable windows classify Unknown: never a trigger.
                        let kind = classify_move_size_start(hwnd);
                        state.move_kind.insert(hwnd, kind);
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "move-kind",
                                "tick": state.tick,
                                "kind": match kind {
                                    MoveSizeKind::Move => "move",
                                    MoveSizeKind::Resize => "resize",
                                    MoveSizeKind::Unknown => "unknown",
                                },
                            }),
                        );
                        // START-time pointer for the preview zero gate: a
                        // title click with no pointer movement never shows.
                        // A failed read fails closed to no preview (the
                        // refresh hides on `no-start`).
                        if let Some(point) = cursor_pos() {
                            state.gesture_start_cursor.insert(hwnd, point);
                        } else {
                            state.gesture_start_cursor.remove(&hwnd);
                        }
                    }
                    HookEvent::MoveSizeEnd(raw, cursor, end_seq) => {
                        if state.border_overlay.hwnd() == Some(raw as u64)
                            || state.underlay_overlay.hwnd() == Some(raw as u64)
                            || state.preview_overlay.hwnd() == Some(raw as u64)
                        {
                            continue;
                        }
                        woke = true;
                        let hwnd = raw as u64;
                        // Release cursor carried from the WinEvent callback,
                        // never re-read later. A missing capture fails closed
                        // to no-change for moves at settle time, so drop any
                        // stale entry rather than carrying it forward.
                        match cursor {
                            Some(point) => {
                                state.gesture_end_cursor.insert(hwnd, point);
                            }
                            None => {
                                state.gesture_end_cursor.remove(&hwnd);
                            }
                        }
                        // END-time Esc sequence, first END stands: the drain
                        // latches ended gestures only against this snapshot,
                        // never against a later counter read.
                        if let std::collections::hash_map::Entry::Vacant(e) =
                            state.gesture_end_seq.entry(hwnd)
                        {
                            e.insert(end_seq);
                        }
                        state.active.remove(&hwnd);
                        state.move_kind.remove(&hwnd);
                    }
                }
            }
            let now = Instant::now();
            let slow = now.duration_since(slow_last).as_millis() >= u128::from(SLOW_POLL_MS);
            // Best-effort keyboard hook with bounded backoff retries (5s
            // doubling, 60s cap): failure logs one `snap-unavailable` per
            // attempt, never per-poll noise, and the loop keeps tiling with
            // nothing consuming. Resume and work-area changes re-arm an
            // immediate retry; there is no permanent disable. The gate
            // follows the live takeover setting so a settings re-enable
            // installs the hook, and a fresh install publishes the live
            // rebound table immediately.
            let takeover_live =
                (state.keyboard.takeover && !proof) || shortcut_proof || workspace_proof;
            if takeover_live && snap_hook.is_none() && snap_retry_at.is_none_or(|at| now >= at) {
                let installed = if shortcut_proof || workspace_proof {
                    crate::snapkey::sys::install_proof(state.keyboard)
                } else {
                    crate::snapkey::sys::install(state.keyboard)
                };
                match installed {
                    Ok(hook) => {
                        snap_hook = Some(hook);
                        // Trace-only collection starts here: the hook records
                        // nothing until this setter runs, so default runs pay
                        // no timing/filter/push work in the callback.
                        crate::snapkey::sys::set_callback_diag_enabled(state.trace);
                        // Publish the live policy plus the rebound/suppression
                        // tables onto the fresh machine (a fresh install never
                        // carries held keys, so nothing drains here).
                        let remap = state.last_remap.clone();
                        let disabled = state.last_disabled.clone();
                        crate::snapkey::sys::apply_live_config(state.keyboard, &remap, &disabled);
                        snap_failures = 0;
                        snap_retry_at = None;
                        log_json_at(&log_path, serde_json::json!({"event":"snap-available"}));
                    }
                    Err(message) => {
                        snap_failures += 1;
                        let backoff = (5u64 << snap_failures.min(4)).min(60);
                        snap_retry_at = Some(now + Duration::from_secs(backoff));
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "snap-unavailable",
                                "attempt": snap_failures,
                                "retry_secs": backoff,
                                "cause": message,
                            }),
                        );
                    }
                }
            }
            // Live settings poll (normal owner only; proof owners never
            // poll): validated edits apply BEFORE the hook-queue drain, so
            // `apply_live_config` drains stale intents before this pump
            // pulls them and nothing dispatches under the old config.
            // Invalid files keep last-good. A changed apply wakes the pump
            // so gap adoption reconciles promptly.
            if poll_live_settings(&mut state, dir, me, store, &mut snap_want) {
                woke = true;
            }
            // Tray follows live settings on the same pump: badge and tooltip
            // track the validated effective bindings under the runtime
            // keyboard policy (takeover plus the runtime Win+L opt-in), and a
            // pending add retries through the state gate (no duplicate
            // icons). `sync` itself is change-gated.
            if let Some(tray) = tray.as_mut() {
                let (conflict_empty, settings_status) = match state.settings_live.as_ref() {
                    Some(live) => (
                        crate::tray::unresolved_conflicts(
                            &live.settings,
                            state.keyboard.takeover,
                            state.keyboard.allow_win_l,
                        )
                        .is_empty(),
                        live.status.clone(),
                    ),
                    None => (true, "saved:unknown".to_owned()),
                };
                tray.sync(conflict_empty, &settings_status);
                // Truthful workspace section: the opaque live scope plus its
                // tiled state and the persisted default. The next menu renders
                // exactly this; a toggle pick drains once below and flips the
                // mode on the loop thread only when the live active pair still
                // matches the rendered scope (stale/mismatched refuses with no
                // guessed fallback).
                let scope = if state.active_output.is_empty() {
                    None
                } else {
                    state
                        .workspaces
                        .active_id(&state.active_output)
                        .map(|workspace| (state.active_output.clone(), workspace))
                };
                let current = scope.as_ref().and_then(|(output, workspace)| {
                    state
                        .workspaces
                        .workspace_ids(output)
                        .iter()
                        .any(|id| id == workspace)
                        .then(|| state.workspaces.is_tiled(output, workspace))
                });
                tray.sync_workspace(scope, current, state.workspaces.default_tiled());
                if let Some(request) = tray.take_toggle_request() {
                    let live = if state.active_output.is_empty() {
                        None
                    } else {
                        state
                            .workspaces
                            .active_id(&state.active_output)
                            .map(|workspace| (state.active_output.clone(), workspace))
                    };
                    let rendered = Some((request.output.as_str(), request.workspace.as_str()));
                    let live_ref = live
                        .as_ref()
                        .map(|(output, workspace)| (output.as_str(), workspace.as_str()));
                    if crate::tray::verify_toggle_scope(rendered, live_ref) {
                        let toggle_fulls = monitor_fulls(&areas);
                        toggle_workspace_tiling(
                            &mut state,
                            me,
                            &toggle_fulls,
                            &areas,
                            &request.output,
                        );
                        woke = true;
                    } else {
                        // Stale or mismatched menu scope (output switch or a
                        // pruned workspace between render and pick): refuse
                        // with opaque tokens only, never raw scope ids.
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "workspace-mode",
                                "op": "toggle",
                                "outcome": "refused-stale-scope",
                            }),
                        );
                    }
                }
            }
            // Shortcut-handling gate for the callback: fresh downs consume
            // iff takeover holds with this gate active (the eventual Xbox
            // pause will clear it; until then publication stays takeover
            // only). Fullscreen suspension, gesture, elevated foreground,
            // and unmanaged/unadmitted/shell foreground never enter this
            // gate, so inactive tiling never pauses interception: those
            // chords still swallow at the hook and the owner rechecks fresh
            // identity/scope/elevation/fullscreen/gesture per intent before
            // any action, never manipulating protected windows. Hook-side
            // work stays cheap reads, never expensive syscalls per key.
            let snap_gate_active = state.keyboard.takeover;
            let snap_events = if snap_hook.is_some() {
                crate::snapkey::sys::publish_gate(&state.snap_origins, snap_gate_active);
                let batch = crate::snapkey::sys::drain_up_to(MAX_DISPATCH_PER_TICK);
                if !batch.is_empty() {
                    woke = true;
                }
                batch
            } else {
                Vec::new()
            };
            // Workspace digits, relative sends and history ride the same
            // hook/queue/mask authority but dispatch through the workspace
            // owner, never the directional Engine route. Partition here so
            // each batch keeps its verdict vocabulary; `shortcut-proof` keeps
            // workspace hides disabled inside `workspace_tick` via the proof
            // gate.
            let mut workspace_events = Vec::new();
            let mut send_events = Vec::new();
            let mut history_events = Vec::new();
            let mut directional_events = Vec::new();
            for event in snap_events {
                match event {
                    QueuedSnapEvent::Workspace(intent) => workspace_events.push(intent),
                    QueuedSnapEvent::WorkspaceSend(intent) => send_events.push(intent),
                    QueuedSnapEvent::WorkspaceHistory(intent) => history_events.push(intent),
                    QueuedSnapEvent::Mask(_)
                    | QueuedSnapEvent::Intent(_)
                    | QueuedSnapEvent::Resize(_)
                    | QueuedSnapEvent::Maximize(_)
                    | QueuedSnapEvent::Orientation(_)
                    | QueuedSnapEvent::Fullscreen(_)
                    | QueuedSnapEvent::Float(_)
                    | QueuedSnapEvent::Sticky(_) => {
                        directional_events.push(event);
                    }
                }
            }
            if !workspace_events.is_empty() || !send_events.is_empty() || !history_events.is_empty()
            {
                woke = true;
            }
            let snap_events = directional_events;
            // Project Win+Left edges ride the same pump and gate: the hook
            // consumes downs/ups only while takeover holds, and every bound
            // candidate revalidates against the last complete observation in
            // `windrag_down` before anything arms. Downs pause tiling through
            // the shared `active` set (frames stay at source: no mid-gesture
            // writes); ups feed the shared `ended` settle below.
            if windrag_hook.is_some() {
                crate::win_mouse::sys::publish(&state.windrag_origins, snap_gate_active);
                let stats = crate::win_mouse::sys::hook_stats();
                if stats != state.windrag_stats_logged {
                    state.windrag_stats_logged = stats;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "windrag-hook-stats",
                            "tick": state.tick,
                            "downs_seen": stats.0,
                            "downs_consumed": stats.1,
                            "ups_seen": stats.2,
                            "ups_consumed": stats.3,
                        }),
                    );
                }
                let edges = crate::win_mouse::sys::drain_up_to(WINDRAG_MAX_DISPATCH_PER_TICK);
                if !edges.is_empty() {
                    woke = true;
                }
                let dropped = crate::win_mouse::sys::queue_dropped();
                if dropped > state.windrag_dropped {
                    let lost = dropped - state.windrag_dropped;
                    state.windrag_dropped = dropped;
                    log_json_at(
                        &log_path,
                        serde_json::json!({"event":"windrag-drop","lost": lost}),
                    );
                    // A lost edge may have carried a Down whose gesture is
                    // open or an Up that would have closed one: discard every
                    // affected open project gesture rather than risk a
                    // stranded hold or a journey without its close. The hook
                    // is disarmed for them; their paired Ups stay swallowed.
                    if !state.windrag_bound.is_empty() {
                        let open: Vec<u64> = state.windrag_bound.keys().copied().collect();
                        for hwnd in open {
                            clear_windrag_gesture(&mut state, hwnd);
                            crate::win_mouse::sys::invalidate(hwnd);
                        }
                        log_json_at(
                            &log_path,
                            serde_json::json!({"event":"windrag-drop-loss","cleared": true}),
                        );
                        let fulls_owned = fulls.to_vec();
                        reconcile_tick(&mut state, me, &fulls_owned, &areas);
                    }
                }
                // Press-focus observation for drained Downs: one fresh complete
                // enumeration per pump (only when a Down waits) feeds the
                // existing focus authority inside `windrag_down`. Token
                // minting is idempotent with the later phase observations;
                // an enumeration failure leaves empty slices so Downs refuse
                // closed with no arm and no writes.
                let mut press_observed: Vec<ObservedWindow> = Vec::new();
                let mut press_retained: Vec<RetainedRow> = Vec::new();
                if edges.iter().any(|e| e.kind == WinDragKind::Down) {
                    let mut skipped: Vec<(String, String)> = Vec::new();
                    if let Some(fresh) =
                        state.observe(me, &fulls, &mut skipped, &mut press_retained)
                    {
                        press_observed = fresh;
                    }
                }
                for edge in edges {
                    match edge.kind {
                        WinDragKind::Down => match windrag_down(
                            &mut state,
                            me,
                            &edge,
                            &fulls,
                            &press_observed,
                            &press_retained,
                        ) {
                            Ok(token) => {
                                log_json_at(
                                    &log_path,
                                    serde_json::json!({
                                        "event": "windrag-down",
                                        "tick": state.tick,
                                        "window": token,
                                        "producer": "windrag",
                                    }),
                                );
                            }
                            Err(reason) => {
                                log_json_at(
                                    &log_path,
                                    serde_json::json!({
                                        "event": "windrag-down-refused",
                                        "tick": state.tick,
                                        "reason": reason,
                                        "producer": "windrag",
                                    }),
                                );
                            }
                        },
                        WinDragKind::Up => {
                            windrag_up(&mut state, edge.hwnd, edge.x, edge.y, edge.esc_seq);
                        }
                        WinDragKind::Cancel => {
                            // Hook-side disarm after a consumed Down: drop
                            // the owner gesture with no plan and no geometry
                            // change. The paired Up stays swallowed
                            // hook-side; native gestures are untouched.
                            if let Some(token) = clear_windrag_gesture(&mut state, edge.hwnd) {
                                log_json_at(
                                    &log_path,
                                    serde_json::json!({
                                        "event": "windrag-cancel",
                                        "tick": state.tick,
                                        "outcome": "gesture-no-change",
                                        "window": token,
                                        "producer": "windrag",
                                    }),
                                );
                            }
                        }
                    }
                }
            }
            // Proof-only callback diagnostics for shortcut-proof: accepted
            // marked events as the callback saw them, drained to the proof
            // audit (never production logs) so live runs can distinguish
            // actual modifier delivery from classifier state. Product `tile`
            // never records or writes these.
            if (shortcut_proof || workspace_proof) && snap_hook.is_some() {
                let diag = crate::snapkey::sys::drain_marked_diag();
                if !diag.is_empty()
                    && let Some(audit) = state.audit_path.as_ref()
                {
                    let entries: Vec<serde_json::Value> = diag
                        .iter()
                        .map(crate::snapkey::marked_diag_evidence)
                        .collect();
                    log_json_at(
                        audit,
                        serde_json::json!({
                            "event": "proof-keys",
                            "entries": entries,
                            "dropped": crate::snapkey::sys::marked_diag_dropped(),
                        }),
                    );
                }
                // Proof-only complement: non-marked modifier traffic
                // (injected-filtered plus physical, modifiers only) drained
                // to the proof audit under its own event, so the actual
                // callback modifier sequence is complete. Product `tile`
                // never records or writes these.
                let mods = crate::snapkey::sys::drain_mod_diag();
                if !mods.is_empty()
                    && let Some(audit) = state.audit_path.as_ref()
                {
                    let entries: Vec<serde_json::Value> =
                        mods.iter().map(crate::snapkey::mod_diag_evidence).collect();
                    log_json_at(
                        audit,
                        serde_json::json!({
                            "event": "proof-mods",
                            "entries": entries,
                            "dropped": crate::snapkey::sys::mod_diag_dropped(),
                        }),
                    );
                }
            }
            // Trace-only callback summary for product and proof paths: closed
            // vocabulary plus verdict/source/state/local-timing only (no
            // HWNDs, PIDs, tokens, titles). One bounded line per tick at
            // most; losses stay visible via dropped/filtered even when the
            // batch is empty. Mask `SendInput` latency rides the separate
            // `mask_sends`/`mask_send_max_us` aggregate (never inside
            // `duration_us`, never the downstream hook chain).
            if state.trace && snap_hook.is_some() {
                let diags = crate::snapkey::sys::drain_callback_diag();
                let dropped = crate::snapkey::sys::callback_diag_dropped();
                let filtered = crate::snapkey::sys::callback_diag_filtered();
                let (mask_sends, mask_send_max_us) = crate::snapkey::sys::mask_send_stats();
                if !diags.is_empty()
                    || dropped != state.cb_diag_dropped
                    || filtered != state.cb_diag_filtered
                    || mask_sends != state.mask_sends
                    || mask_send_max_us != state.mask_send_max_us
                {
                    state.cb_diag_dropped = dropped;
                    state.cb_diag_filtered = filtered;
                    state.mask_sends = mask_sends;
                    state.mask_send_max_us = mask_send_max_us;
                    let max_duration_us = diags.iter().map(|d| d.duration_us).max().unwrap_or(0);
                    let entries: Vec<serde_json::Value> = diags
                        .iter()
                        .map(crate::snapkey::callback_diag_evidence)
                        .collect();
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap-callback",
                            "entries": entries,
                            "dropped": dropped,
                            "filtered": filtered,
                            "max_duration_us": max_duration_us,
                            "mask_sends": mask_sends,
                            "mask_send_max_us": mask_send_max_us,
                        }),
                    );
                }
            }
            if slow {
                slow_last = now;
                let fresh = all_monitors()?;
                let devices: Vec<String> = areas.iter().map(|a| a.device.clone()).collect();
                let fresh_devices: Vec<String> = fresh.iter().map(|a| a.device.clone()).collect();
                let geometry_changed = fresh.len() != monitor_count
                    || fresh_devices != devices
                    || fresh.iter().any(|a| {
                        areas
                            .iter()
                            .find(|b| b.device == a.device)
                            .is_none_or(|b| b.work != a.work || b.full != a.full)
                    });
                if geometry_changed {
                    areas = fresh;
                    monitor = areas[0].clone();
                    monitor_count = areas.len();
                    // Arrival/removal relocates whole-workspace Engine
                    // sessions; bounds-only changes reproject retained
                    // sessions without touching topology.
                    sync_monitor_outputs(&mut state, &areas);
                    for output in state.workspaces.output_keys() {
                        let Some(active) = state.workspaces.active_id(&output) else {
                            continue;
                        };
                        if let Some((_, key)) = workspace_domain_for(
                            &output,
                            &active,
                            &areas,
                            state.inner_gap,
                            state.outer_gap,
                        ) && let Some(area) = areas.iter().find(|a| a.device == output)
                            && let Some(bounds) =
                                tiling_domain_bounds_with(area.work, state.outer_gap)
                        {
                            state.engine.reproject_retained(&key, bounds);
                        }
                    }
                    log_json_at(&log_path, serde_json::json!({"event":"work-area-changed"}));
                    snap_retry_at = None;
                    // A changed work area is a meaningful retry point for a
                    // degraded Snap setup; quiet when the effect holds.
                    if snap_want && !state.suspended {
                        snap_resume(dir, me, store);
                        snap_primed = true;
                    }
                    woke = true;
                }
            }
            // Exact-owner out-of-hook select wakes the loop even with no other
            // event; the poll itself consumes once and dispatches through the
            // existing resolver. Checked before the idle skip so a pending
            // request never waits for an unrelated wake.
            if dir.join(WORKSPACE_REQUEST_FILE).exists() || dir.join(RESIZE_REQUEST_FILE).exists() {
                woke = true;
            }
            // Bare-chord edge wake: GetAsyncKeyState levels are sampled here,
            // before the idle skip, so a Win+Shift press/release with no
            // WinEvent wakes the visual refresh on the 100ms pump cadence
            // instead of waiting for the 2s slow poll. Steady hold stays
            // quiet; the suspend/fullscreen path below still gates the
            // refresh, and tiling reconciliation keeps its normal pacing
            // (only transition ticks reconcile). No hook, no new framework.
            {
                let (win_held, shift_held) = chord_held_now();
                let chord = tiler_core::visual::group_underlay_chord_held(win_held, shift_held);
                if state.underlay_chord_last != chord {
                    state.underlay_chord_last = chord;
                    woke = true;
                }
            }
            // Armed restore demands a reconcile once observed; the wake
            // survives pending dispatches and gesture/suspend pauses and is
            // consumed only by a gated reconcile below, on expiry, or on loss.
            if state.restore_wake.is_some() {
                let (expired, lost, done) =
                    restore_wake_probe(&state, state.restore_wake.as_ref().expect("armed"), now);
                let (keep, demand) =
                    crate::tiling::restore_wake_step(true, expired, lost, done, false);
                if demand {
                    woke = true;
                }
                if !keep {
                    state.restore_wake = None;
                }
            }
            // An open managed gesture wakes the 100ms pump for the
            // drop-preview track: pointer moves coalesce into one Engine
            // preview sample per tick on both the native frame-following
            // and the stationary Win+Left producers. The refresh itself
            // gates resize/unknown/invalid holds; the wake only needs START
            // capture.
            if !woke
                && state.active.iter().any(|hwnd| {
                    state.managed.contains(hwnd) && state.gesture_before.contains_key(hwnd)
                })
            {
                woke = true;
            }
            if !(woke || slow) {
                continue;
            }
            let fulls = monitor_fulls(&areas);
            if suspend_read(&mut state, me, &fulls).veto.block {
                if !state.suspended {
                    state.suspended = true;
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({"event":"suspend","cause":"fullscreen-foreground"}),
                    );
                    if snap_want {
                        snap_suspend(dir, me, store);
                    }
                } else {
                    state.snap_advance = None;
                }
                if !snap_events.is_empty() {
                    keyboard_tick(
                        &mut state,
                        me,
                        &fulls,
                        &areas,
                        snap_events,
                        Some("suspended"),
                    );
                }
                if !workspace_events.is_empty() {
                    workspace_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        workspace_events,
                        Some("suspended"),
                    );
                }
                if !send_events.is_empty() {
                    workspace_relative_send_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        send_events,
                        Some("suspended"),
                    );
                }
                if !history_events.is_empty() {
                    workspace_history_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        history_events,
                        Some("suspended"),
                    );
                }
                // Out-of-hook select observes the same suspension: consumed
                // once with a suspended outcome, never applied while a
                // fullscreen foreground holds the session. The test-needed
                // resize poll carries its own suspend fence and refuses the
                // same way.
                poll_workspace_cli_request(&mut state, me, store, dir, &fulls, &areas);
                poll_resize_cli_request(&mut state, me, dir, &fulls, &areas);
                state.gesture_before.clear();
                state.active.clear();
                state.move_kind.clear();
                state.esc_latched.clear();
                state.gesture_esc_seq.clear();
                state.gesture_end_seq.clear();
                state.gesture_end_cursor.clear();
                state.gesture_start_key.clear();
                state.gesture_start_tag.clear();
                state.gesture_producer.clear();
                state.windrag_start_cursor.clear();
                state.windrag_bound.clear();
                state.preview_bound.clear();
                state.preview_dead.clear();
                state.gesture_preview_start.clear();
                state.gesture_start_cursor.clear();
                // A stale Esc edge must not leak into the next gesture.
                crate::snapkey::sys::clear_esc_edge();
                // A suspended session disarms the project hold with no
                // replay; the consumed click already passed, so no half
                // gesture can strand later.
                crate::win_mouse::sys::clear_armed();
                hide_border(&mut state, "suspended");
                hide_underlay(&mut state, "suspended");
                hide_preview(&mut state, "suspended");
                continue;
            }
            if state.suspended {
                state.suspended = false;
                log_json_at(&log_path, serde_json::json!({"event":"resume"}));
                // A fresh foreground may accept the hook now: retry soon.
                snap_retry_at = None;
                // Fresh Snap preimage when the setting drifted during
                // suspension (or setup degraded earlier); quiet when the
                // effect is still ours. This is also the deferred initial
                // effect when the run started suspended.
                if snap_want {
                    snap_resume(dir, me, store);
                    snap_primed = true;
                }
            } else if snap_want && !snap_primed {
                // Deferred initial effect: the run started active but setup
                // no longer disables. Drive once on the first active tick.
                snap_resume(dir, me, store);
                snap_primed = true;
            }
            // Border follows the live foreground on every active wake,
            // including managed gestures that pause tiling below: the refresh
            // reads one fresh frame and never waits for reconciliation.
            refresh_active_border(&mut state, me, &fulls);
            // The group fill follows on the same wake: chord/move trigger over
            // the Engine-projected source union, same freshness contract.
            refresh_group_underlay(&mut state, me, &fulls, &areas);
            // Only gestures on managed windows pause tiling; unrelated
            // windows never stall the loop.
            let ended: Vec<u64> = state
                .gesture_before
                .keys()
                .copied()
                .filter(|hwnd| state.managed.contains(hwnd) && !state.active.contains(hwnd))
                .collect();
            // Drop pre-gesture state for windows that left management.
            state
                .gesture_before
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .gesture_end_cursor
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .gesture_start_key
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .gesture_start_tag
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .gesture_esc_seq
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .gesture_end_seq
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .gesture_producer
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .windrag_start_cursor
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .windrag_bound
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .preview_bound
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .preview_dead
                .retain(|hwnd| state.managed.contains(hwnd));
            state
                .gesture_preview_start
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .gesture_start_cursor
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state.active.retain(|hwnd| state.managed.contains(hwnd));
            // Sequence-bound Esc latch, drained once per pump after event and
            // suspend handling. Still-held gestures compare the live counter
            // against START. Just-ended gestures compare the END-callback
            // snapshot against START instead: a physical Esc that lands after
            // the END but before this drain must never cancel the completed
            // drop, while a fast cancel tap arriving with the END batch still
            // latches. Only edges newer than the START snapshot latch, so
            // pre-gesture taps never cancel. Fast taps arrive via the prompt
            // callback, never via level polling. Injected Esc never sets the
            // native edge (product hook filters injected keys), so synthetic
            // title-bar proof paths keep their no-change restore without
            // claiming an explicit cancel; the project windrag edge below is
            // the one deliberate injected exception (the stationary hold has
            // no native modal loop to cancel it).
            {
                let esc_hit = crate::snapkey::sys::take_esc_edge();
                if esc_hit {
                    let now_seq = crate::snapkey::sys::esc_seq();
                    for hwnd in state.active.iter() {
                        if !state.managed.contains(hwnd) {
                            continue;
                        }
                        let start_seq = state.gesture_esc_seq.get(hwnd).copied().unwrap_or(0);
                        if esc_edge_cancels(start_seq, now_seq) {
                            state.esc_latched.insert(*hwnd);
                        }
                    }
                    for hwnd in ended.iter() {
                        if !state.managed.contains(hwnd) {
                            continue;
                        }
                        // END-bound: absent snapshots never latch (fail
                        // closed to no-cancel; settle classifies from
                        // geometry, which a real cancel restored natively).
                        let (Some(start_seq), Some(end_seq)) = (
                            state.gesture_esc_seq.get(hwnd).copied(),
                            state.gesture_end_seq.get(hwnd).copied(),
                        ) else {
                            continue;
                        };
                        if esc_edge_cancels(start_seq, end_seq) {
                            state.esc_latched.insert(*hwnd);
                        }
                    }
                }
                // Project-gesture cancel edge (physical or injected Esc):
                // latches open windrag gestures only, with the same
                // START/END sequence ordering. Never admits a command;
                // settle reports the explicit `gesture-cancelled-esc`.
                let windrag_hit = crate::snapkey::sys::take_windrag_esc_edge();
                if windrag_hit {
                    let now_seq = crate::snapkey::sys::windrag_esc_seq();
                    for hwnd in state.active.iter() {
                        if !state.managed.contains(hwnd) {
                            continue;
                        }
                        if state.gesture_producer.get(hwnd).copied() != Some("windrag") {
                            continue;
                        }
                        let start_seq = state.gesture_esc_seq.get(hwnd).copied().unwrap_or(0);
                        if esc_edge_cancels(start_seq, now_seq) {
                            state.esc_latched.insert(*hwnd);
                        }
                    }
                    for hwnd in ended.iter() {
                        if !state.managed.contains(hwnd) {
                            continue;
                        }
                        if state.gesture_producer.get(hwnd).copied() != Some("windrag") {
                            continue;
                        }
                        let (Some(start_seq), Some(end_seq)) = (
                            state.gesture_esc_seq.get(hwnd).copied(),
                            state.gesture_end_seq.get(hwnd).copied(),
                        ) else {
                            continue;
                        };
                        if esc_edge_cancels(start_seq, end_seq) {
                            state.esc_latched.insert(*hwnd);
                        }
                    }
                }
            }
            // Logical cancel hides the preview IMMEDIATELY on latch, never
            // waiting for mouse-up (the stationary gesture has no native
            // modal loop to end it; the native loop ends on its own).
            for hwnd in state.esc_latched.iter().copied().collect::<Vec<_>>() {
                if state.preview_bound.remove(&hwnd).is_some() || state.preview_overlay.is_visible()
                {
                    hide_preview(&mut state, "esc-cancelled");
                }
                state.preview_dead.remove(&hwnd);
                state.gesture_preview_start.remove(&hwnd);
                state.gesture_start_cursor.remove(&hwnd);
            }
            let paused = state.active.iter().any(|hwnd| state.managed.contains(hwnd));
            if paused {
                // A managed gesture holds the loop and invalidates any
                // own-focus chain; keyboard intents of this batch are stale
                // and drop safely.
                state.snap_advance = None;
                if !snap_events.is_empty() {
                    keyboard_tick(&mut state, me, &fulls, &areas, snap_events, Some("gesture"));
                }
                if !workspace_events.is_empty() {
                    workspace_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        workspace_events,
                        Some("gesture"),
                    );
                }
                if !send_events.is_empty() {
                    workspace_relative_send_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        send_events,
                        Some("gesture"),
                    );
                }
                if !history_events.is_empty() {
                    workspace_history_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        history_events,
                        Some("gesture"),
                    );
                }
                // Drop-preview track on the 100ms pump while held: fresh
                // observation plus the shared Engine resolver per moved
                // pointer, on both producers. No focus, no geometry writes.
                refresh_drag_preview(&mut state, me, &fulls, &areas);
                continue;
            }
            if ended.is_empty() {
                if snap_events.is_empty()
                    && workspace_events.is_empty()
                    && send_events.is_empty()
                    && history_events.is_empty()
                {
                    reconcile_tick(&mut state, me, &fulls, &areas);
                    if state.restore_wake.is_some() {
                        let (expired, lost, done) = restore_wake_probe(
                            &state,
                            state.restore_wake.as_ref().expect("armed"),
                            Instant::now(),
                        );
                        let (keep, _) =
                            crate::tiling::restore_wake_step(true, expired, lost, done, true);
                        if !keep {
                            state.restore_wake = None;
                        }
                    }
                    workspace_maintenance(&mut state, me, store, dir, &fulls, &areas);
                    poll_workspace_cli_request(&mut state, me, store, dir, &fulls, &areas);
                    poll_resize_cli_request(&mut state, me, dir, &fulls, &areas);
                } else {
                    if !snap_events.is_empty() {
                        keyboard_tick(&mut state, me, &fulls, &areas, snap_events, None);
                    }
                    if !workspace_events.is_empty() {
                        workspace_tick(
                            &mut state,
                            me,
                            store,
                            dir,
                            &fulls,
                            &areas,
                            workspace_events,
                            None,
                        );
                    }
                    if !send_events.is_empty() {
                        workspace_relative_send_tick(
                            &mut state,
                            me,
                            store,
                            dir,
                            &fulls,
                            &areas,
                            send_events,
                            None,
                        );
                    }
                    if !history_events.is_empty() {
                        workspace_history_tick(
                            &mut state,
                            me,
                            store,
                            dir,
                            &fulls,
                            &areas,
                            history_events,
                            None,
                        );
                    }
                    poll_workspace_cli_request(&mut state, me, store, dir, &fulls, &areas);
                }
            } else {
                // A just-ended managed gesture settles before keyboard
                // continuation resumes.
                state.snap_advance = None;
                if !snap_events.is_empty() {
                    keyboard_tick(&mut state, me, &fulls, &areas, snap_events, Some("gesture"));
                }
                if !workspace_events.is_empty() {
                    workspace_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        workspace_events,
                        Some("gesture"),
                    );
                }
                if !send_events.is_empty() {
                    workspace_relative_send_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        send_events,
                        Some("gesture"),
                    );
                }
                if !history_events.is_empty() {
                    workspace_history_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        history_events,
                        Some("gesture"),
                    );
                }
                gesture_tick(&mut state, me, &fulls, &areas, &ended);
                poll_workspace_cli_request(&mut state, me, store, dir, &fulls, &areas);
            }
            // Post-mutation border refresh: reconciliation may have moved the
            // target this tick, so re-read the fresh frame instead of leaving
            // the pre-reconcile geometry stale until the next wake. Own
            // overlay events are filtered above, so no feedback loop.
            refresh_active_border(&mut state, me, &fulls);
            // Same for the group fill: a moved target may have joined another
            // group, so the projected union re-resolves instead of lingering.
            refresh_group_underlay(&mut state, me, &fulls, &areas);
        }
    })();
    for hook in hooks {
        unsafe {
            UnhookWinEvent(hook);
        }
    }
    if let Some(mut snap) = snap_hook {
        // Honest release accounting: process exit releases the hook in any
        // case, but success is recorded, never fabricated.
        let ok = crate::snapkey::sys::uninstall(&mut snap);
        let mut release = serde_json::json!({"event":"snap-release","ok": ok});
        if !ok {
            release["note"] = serde_json::json!("release failed; process exit releases the hook");
        }
        log_json_at(&log_path, release);
    }
    if let Some(mut mouse) = windrag_hook {
        let ok = crate::win_mouse::sys::uninstall(&mut mouse);
        let mut release = serde_json::json!({"event":"windrag-release","ok": ok});
        if !ok {
            release["note"] = serde_json::json!("release failed; process exit releases the hook");
        }
        log_json_at(&log_path, release);
    }
    // Owner tray teardown: graceful stop removes the icon (`NIM_DELETE`
    // through RAII `Drop`); a crash ghost is pruned by the next startup or
    // independent restore through the stable GUID. The explicit end line
    // marks graceful stop only.
    if let Some(tray) = tray {
        drop(tray);
        log_json_at(&log_path, serde_json::json!({"event":"tray-end"}));
    }
    // Owned overlay teardown: explicit destroy on graceful stop (process exit
    // destroys it implicitly after a crash, so no residue either way).
    let border_snapshot = state.border_overlay.snapshot();
    state.border_overlay.destroy();
    log_json_at(
        &log_path,
        serde_json::json!({"event":"active-border-end","overlay": border_snapshot}),
    );
    let underlay_snapshot = state.underlay_overlay.snapshot();
    state.underlay_overlay.destroy();
    log_json_at(
        &log_path,
        serde_json::json!({"event":"group-underlay-end","overlay": underlay_snapshot}),
    );
    // Drop-preview teardown: explicit nonactivating destroy on graceful
    // stop (process exit destroys it implicitly after a crash, so no
    // residue either way). No ledger, no registry, no hidden windows.
    let preview_snapshot = state.preview_overlay.snapshot();
    state.preview_overlay.destroy();
    log_json_at(
        &log_path,
        serde_json::json!({"event":"drag-preview-end","overlay": preview_snapshot}),
    );
    // Graceful stop restores only project-raised topmost bands with no frame
    // change, under the same held ownership gates as dispatch. Geometry is
    // left in place; a crash leaves every frame where it is, and restart
    // resets this runtime-local preimage with the Engine.
    let mut float_topmost_restored = 0u32;
    let mut float_topmost_skipped = 0u32;
    let float_preimages: Vec<(crate::workspace::WindowKey, bool)> = state
        .float_topmost_prev
        .iter()
        .map(|(key, prior)| (key.clone(), *prior))
        .collect();
    for (key, prior) in float_preimages {
        let Some(held) = hold_band_target(&state, me, &key) else {
            float_topmost_skipped += 1;
            continue;
        };
        let current = read_topmost_now(key.hwnd);
        if !crate::tiling::float_topmost_restore_needed(prior, current) {
            continue;
        }
        let ok = set_topmost_band(key.hwnd, prior)
            && read_topmost_now(key.hwnd) == prior
            && pid_current(key.hwnd, key.pid);
        let _ = &held;
        if state.audit_path.is_some() {
            audit_json(
                &state,
                serde_json::json!({
                    "event": "proof-topmost",
                    "op": "stop",
                    "restored": ok,
                }),
            );
        }
        if ok {
            float_topmost_restored += 1;
        } else {
            float_topmost_skipped += 1;
        }
    }
    state.float_topmost_prev.clear();
    state.floated.clear();
    state.float_rects.clear();
    if float_topmost_restored > 0 || float_topmost_skipped > 0 {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "float-topmost-restore",
                "restored": float_topmost_restored,
                "skipped": float_topmost_skipped,
            }),
        );
    }
    log_json_at(
        &log_path,
        serde_json::json!({"event":"tile-end","ticks":state.tick}),
    );
    result
}

/// `tile` command: normal user tiling only (explicit `--user-start`).
/// Geometry is left in place on stop; nothing is hidden or restored.
/// Terminal windows are ordinary targets; agents never run this path.
/// Optional repeatable `--scope-exe NAME` restricts management to the named
/// executables (empty default means no filter). Product keyboard takeover
/// follows the parsed options (default on, visible off switch, Win+L
/// opt-in). Session-only mouse-Snap prevention is default on with the
/// visible `--no-mouse-snap-prevention` off switch; the exact preimage is
/// restored conditionally on stop.
pub fn cmd_tile(
    options: &TileOptions,
    live: Option<LiveSettings>,
    settings_dir: Option<std::path::PathBuf>,
    cli_overrides: crate::tiling::CliOverrides,
) -> Result<String> {
    if !options.user_start {
        return Err(err("refuse: tile requires explicit --user-start"));
    }
    // The first-run preset resolves under the owner lease inside the loop
    // below (never before it): a refused second owner never prompts, and a
    // mid-prompt file race reloads instead of overwriting. These pre-lease
    // values are the no-prompt fallback; the lease-held resolution may
    // replace every derived lane before any effect or hook.
    let trace = options.trace;
    let seconds = options.seconds;
    let keyboard = KeyboardConfig {
        takeover: !options.no_keyboard_snap_takeover,
        allow_win_l: options.allow_win_l,
    };
    let mouse_snap = !options.no_mouse_snap_prevention;
    let scope = options.scope_exes.clone();
    let scope_hosts = options.scope_hosts.clone();
    let border = options.border;
    let underlay = options.underlay;
    let inner_gap = options.inner_gap;
    let outer_gap = options.outer_gap;
    let options = options.clone();
    let pending = FirstRunPending {
        options,
        live: live.clone(),
        settings_dir: settings_dir.clone(),
    };
    crate::lifecycle::sys::run_product(trace, mouse_snap, move |dir, me, store| {
        run_tile_loop(
            dir,
            me,
            store,
            TileRun {
                seconds,
                trace,
                allowlist: None,
                proof: false,
                shortcut_proof: false,
                workspace_proof: false,
                raw_argv: Vec::new(),
                keyboard,
                mouse_snap,
                first_run: None,
                first_run_pending: Some(pending),
                inner_gap,
                outer_gap,
                settings_dir,
                settings_live: live,
                cli_overrides,
                scope,
                scope_hosts,
                border,
                underlay,
            },
        )
    })
}

/// Deferred first-run inputs for the normal `tile` owner only. The parsed
/// options plus the pre-lease settings state travel into the owned loop and
/// resolve under the lease before any effect or hook.
struct FirstRunPending {
    options: TileOptions,
    live: Option<LiveSettings>,
    settings_dir: Option<std::path::PathBuf>,
}

/// Resolved first-run state for the normal `tile` owner: the effective
/// options plus the live settings state, directory, and `tile-start` note.
#[derive(Debug, Clone)]
pub struct FirstRunResolution {
    pub options: TileOptions,
    pub live: Option<LiveSettings>,
    pub settings_dir: Option<std::path::PathBuf>,
    pub first_run: Option<String>,
}

/// Carry the CLI/run-only lanes over a settings base: explicit CLI switches
/// re-apply over the base, and seconds/trace/scope plus the user-start fence
/// (never from the file) carry over from the parsed options.
fn options_with_cli(
    base_settings: &crate::settings::Settings,
    cli_overrides: &crate::tiling::CliOverrides,
    run_options: &TileOptions,
) -> TileOptions {
    let mut base = crate::settings::tile_options_from_settings(base_settings);
    crate::tiling::apply_cli_overrides(&mut base, cli_overrides);
    base.seconds = run_options.seconds;
    base.trace = run_options.trace;
    base.user_start = run_options.user_start;
    base.scope_exes = run_options.scope_exes.clone();
    base.scope_hosts = run_options.scope_hosts.clone();
    base
}

/// Authoritative base when the prompt choice is discarded (a file appeared
/// mid-prompt, the create race was lost, or the choice failed validation):
/// the file wins when it loads, otherwise last-good defaults stay live with
/// a degraded status. Explicit CLI switches and run-only lanes re-apply over
/// either base. Nothing is written.
fn authoritative_base(
    dir: &std::path::Path,
    run_options: &TileOptions,
    cli_overrides: &crate::tiling::CliOverrides,
) -> (TileOptions, Option<LiveSettings>) {
    let mtime = std::fs::metadata(dir.join(crate::settings::SETTINGS_FILE_NAME))
        .and_then(|meta| meta.modified())
        .ok();
    match crate::settings::load_from_dir(dir) {
        crate::settings::LoadOutcome::Loaded(settings) => {
            let options = options_with_cli(&settings, cli_overrides, run_options);
            (options, Some(LiveSettings::fresh(settings, mtime)))
        }
        crate::settings::LoadOutcome::Missing => {
            let mut live = LiveSettings::fresh(crate::settings::Settings::default(), mtime);
            live.status = "missing: defaults".to_owned();
            let options = options_with_cli(&live.settings.clone(), cli_overrides, run_options);
            (options, Some(live))
        }
        crate::settings::LoadOutcome::Invalid(error) => {
            let mut live = LiveSettings::fresh(crate::settings::Settings::default(), mtime);
            live.status = format!("degraded:{error}");
            let options = options_with_cli(&live.settings.clone(), cli_overrides, run_options);
            (options, Some(live))
        }
    }
}

/// Lease-held first-run resolution for the normal `tile` owner only. The
/// caller holds the single-owner lease, so a second owner never reaches
/// here (its lease refusal lands before any UI). A stop racing startup
/// skips the modal UI and the loop exits promptly; a stop arriving while the
/// prompt is pending dismisses it and publishes nothing. After the choice,
/// the file is rechecked: a file that appeared mid-prompt discards the choice
/// and reloads the authoritative base without writing. An absent file
/// persists through the atomic create-if-absent publish: the winner's bytes
/// land complete, a lost race discards the choice and reloads instead of
/// overwriting, and a plain I/O failure keeps the choice in memory for this
/// run only with an explicit unsaved status plus a `first-run-save-failed`
/// line (never a silent success claim).
fn resolve_first_run(
    dir: &Path,
    me: &ProcessIdentity,
    pending: FirstRunPending,
    cli_overrides: &crate::tiling::CliOverrides,
    log_path: &Path,
) -> Result<FirstRunResolution> {
    let FirstRunPending {
        options,
        live,
        settings_dir,
    } = pending;
    let done = |options, live, settings_dir, first_run| FirstRunResolution {
        options,
        live,
        settings_dir,
        first_run,
    };
    let Some(sdir) = settings_dir.clone() else {
        return Ok(done(options, live, settings_dir, None));
    };
    let file_missing_before = matches!(
        crate::settings::load_from_dir(&sdir),
        crate::settings::LoadOutcome::Missing
    );
    if !crate::tray::first_run_should_prompt(file_missing_before, stop_requested(dir, me)?) {
        return Ok(done(options, live, settings_dir, None));
    }
    // Owned cancellable dialog under the lease: a graceful stop while pending
    // destroys the owned window (stop wins) and publishes nothing below.
    let outcome = crate::tray_sys::first_run_dialog(|| stop_requested(dir, me).unwrap_or(false));
    let stop_during = stop_requested(dir, me)?;
    let file_missing_after = matches!(
        crate::settings::load_from_dir(&sdir),
        crate::settings::LoadOutcome::Missing
    );
    resolve_first_run_outcome(
        &sdir,
        outcome.choice(),
        file_missing_after,
        stop_during,
        &options,
        cli_overrides,
        live,
        settings_dir,
        log_path,
    )
}

/// Lease-held first-run outcome application: the write-capable production
/// path shared by the owner and offline tests. `choice` is the dialog result
/// (`None` when dismissed), `file_missing_after` the lease-held recheck, and
/// `stop_during` the lease-held stop recheck. A stop wins over any pending
/// choice (nothing published); a mid-prompt file discards the choice without
/// overwriting; an absent file publishes the choice through the atomic
/// create-if-absent path. Every branch reports its `tile-start` note.
#[allow(clippy::too_many_arguments)]
pub fn resolve_first_run_outcome(
    sdir: &Path,
    choice: Option<crate::tray::FirstRunChoice>,
    file_missing_after: bool,
    stop_during: bool,
    run_options: &TileOptions,
    cli_overrides: &crate::tiling::CliOverrides,
    live: Option<LiveSettings>,
    settings_dir: Option<std::path::PathBuf>,
    log_path: &Path,
) -> Result<FirstRunResolution> {
    let done = |options, live, settings_dir, first_run| FirstRunResolution {
        options,
        live,
        settings_dir,
        first_run,
    };
    // The pending inputs travel by value: discarding branches rebuild the
    // authoritative base from the file, publishing branches derive from it.
    let options = run_options.clone();
    match crate::tray::first_run_post_decision(choice, file_missing_after, stop_during) {
        crate::tray::FirstRunPostDecision::CancelledStopped => {
            log_json_at(
                log_path,
                serde_json::json!({"event": "first-run-cancelled", "reason": "stopped"}),
            );
            Ok(done(
                options,
                live,
                settings_dir,
                Some("first-run:cancelled:stopped".to_owned()),
            ))
        }
        crate::tray::FirstRunPostDecision::DismissedUnsaved => Ok(done(
            options,
            live,
            settings_dir,
            Some("first-run:dismissed:unsaved".to_owned()),
        )),
        crate::tray::FirstRunPostDecision::DiscardPresent => {
            let (options, live) = authoritative_base(sdir, &options, cli_overrides);
            log_json_at(
                log_path,
                serde_json::json!({"event": "first-run-discarded", "reason": "file-appeared"}),
            );
            Ok(done(
                options,
                live,
                settings_dir,
                Some("first-run:discarded:present".to_owned()),
            ))
        }
        crate::tray::FirstRunPostDecision::Publish(choice) => {
            let chosen = crate::tray::settings_for_choice(choice);
            if let Err(reason) = crate::settings::validate_settings(&chosen) {
                let (options, live) = authoritative_base(sdir, &options, cli_overrides);
                log_json_at(
                    log_path,
                    serde_json::json!({"event": "first-run-discarded", "reason": reason.to_string()}),
                );
                return Ok(done(
                    options,
                    live,
                    settings_dir,
                    Some("first-run:discarded:invalid".to_owned()),
                ));
            }
            let bytes = serde_json::to_vec_pretty(&chosen)
                .map_err(|_| err("error: first-run serialize"))?;
            match crate::storage::publish_no_overwrite(
                sdir,
                crate::settings::SETTINGS_FILE_NAME,
                &bytes,
                "first-run",
            ) {
                Ok(()) => {
                    let mtime = std::fs::metadata(sdir.join(crate::settings::SETTINGS_FILE_NAME))
                        .and_then(|meta| meta.modified())
                        .ok();
                    let options = options_with_cli(&chosen, cli_overrides, &options);
                    let live = Some(LiveSettings::fresh(chosen, mtime));
                    Ok(done(
                        options,
                        live,
                        settings_dir,
                        Some(format!("first-run:{}:saved", choice.as_str())),
                    ))
                }
                Err(crate::storage::PublishError::Pending) => {
                    let (options, live) = authoritative_base(sdir, &options, cli_overrides);
                    log_json_at(
                        log_path,
                        serde_json::json!({"event": "first-run-discarded", "reason": "save-race"}),
                    );
                    Ok(done(
                        options,
                        live,
                        settings_dir,
                        Some("first-run:discarded:raced".to_owned()),
                    ))
                }
                Err(crate::storage::PublishError::Io(error)) => {
                    let mut fresh = LiveSettings::fresh(chosen.clone(), None);
                    fresh.status = format!("first-run:{}:unsaved", choice.as_str());
                    let options = options_with_cli(&chosen, cli_overrides, &options);
                    log_json_at(
                        log_path,
                        serde_json::json!({"event": "first-run-save-failed", "error": error.to_string()}),
                    );
                    Ok(done(
                        options,
                        Some(fresh),
                        settings_dir,
                        Some(format!("first-run:{}:unsaved", choice.as_str())),
                    ))
                }
            }
        }
    }
}

/// Load the normal-owner settings base: persisted settings supply the
/// `TileOptions` defaults that explicit CLI switches override. Proof owners
/// never call this (deterministic explicit configuration only). Returns the
/// base options plus the initialized last-good live state and directory for
/// pump polling (`None` live state disables polling).
pub fn load_normal_tile_base() -> (
    TileOptions,
    Option<LiveSettings>,
    Option<std::path::PathBuf>,
) {
    let defaults = crate::tiling::tile_options_defaults();
    let Ok(dir) = crate::native::settings_directory() else {
        return (defaults, None, None);
    };
    let mtime = std::fs::metadata(dir.join(crate::settings::SETTINGS_FILE_NAME))
        .and_then(|meta| meta.modified())
        .ok();
    match crate::settings::load_from_dir(&dir) {
        crate::settings::LoadOutcome::Loaded(settings) => {
            let options = crate::settings::tile_options_from_settings(&settings);
            (
                options,
                Some(LiveSettings::fresh(settings, mtime)),
                Some(dir),
            )
        }
        crate::settings::LoadOutcome::Missing => {
            let mut live = LiveSettings::fresh(crate::settings::Settings::default(), mtime);
            live.status = "missing: defaults".to_owned();
            (defaults, Some(live), Some(dir))
        }
        crate::settings::LoadOutcome::Invalid(error) => {
            let mut live = LiveSettings::fresh(crate::settings::Settings::default(), mtime);
            live.status = format!("degraded:{error}");
            (defaults, Some(live), Some(dir))
        }
    }
}

/// `workspace` command: exact-owner out-of-hook control for the normal `tile`
/// loop only. Queues one bounded `workspace.request` file; the owner validates
/// the full owner binding (creation/pid/exe/sid/session plus a client
/// correlation) and dispatches once through the existing `workspace_do_select`
/// / `workspace_do_send` resolvers (or the existing project-owned fullscreen
/// toggle for `--fullscreen`). No window actuation here, no synthetic input,
/// acceptance. Refuses when no normal owner runs, when the caller is not the
/// same medium path (SID/session/exe), when the ledger owner is a proof run,
/// and when a request is already pending (single-pending queue, no overwrite,
/// no replay). Stdout reports `dispatched` (queued) honestly; completion is
/// the owner's `workspace` (or `fullscreen-toggle` for `--fullscreen`) log
/// outcome, observed natively by the caller.
pub fn cmd_workspace(options: &WorkspaceOptions) -> Result<String> {
    use crate::tiling::{WORKSPACE_REQUEST_VERSION, WorkspaceRequest, render_workspace_request};
    ensure_pm_v2()?;
    let me = medium_caller()?;
    // Like `cmd_stop`, the CLI itself may run from any shell; only the
    // ledger owner liveness plus the same-path medium binding gate below.
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    let ledger_text =
        std::fs::read_to_string(dir.join(crate::storage::LEDGER_FILE_NAME)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                err("refuse: no owner running")
            } else {
                err(format!("error: ledger read: {e}"))
            }
        })?;
    let record: crate::model::RecoveryLedger =
        crate::model::parse_ledger(&ledger_text).map_err(|_| err("refuse: corrupt ledger"))?;
    // Same-path medium caller check, mirroring `cmd_stop`: SID, session, exe.
    if me.user_sid != record.owner.user_sid || me.session_id != record.owner.session_id {
        return Err(err("refuse: owner mismatch"));
    }
    let exe = crate::native::current_exe_path().map_err(|e| err(format!("error: exe {e}")))?;
    if !exe_paths_equal(&exe, &record.owner.exe_path) {
        return Err(err("refuse: owner mismatch"));
    }
    // Proof owners never serve the normal control: a proof audit marker for
    // this exact owner creation proves proof mode (normal runs write none).
    let audit = dir.join(format!(
        "proof-audit-{}.jsonl",
        record.owner.process_creation
    ));
    if audit.exists() {
        return Err(err("refuse: proof owner"));
    }
    // The ledger owner must be exactly alive (full identity, no PID reuse).
    let held = HeldProcess::open(record.owner.pid).map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    let live = held.identity().map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    if live != record.owner || !held.is_alive() {
        return Err(err("refuse: owner not running"));
    }
    let rid = held
        .integrity()
        .map_err(|e| err(format!("error: target integrity {e}")))?;
    if !is_medium_rid(rid) {
        return Err(err(format!("refuse: target integrity {rid} is not medium")));
    }
    if options.index > 9 {
        return Err(err("refuse: workspace index must be 0..=9"));
    }
    let path = dir.join(WORKSPACE_REQUEST_FILE);
    if path.exists() {
        return Err(err("refuse: workspace request pending"));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1);
    let correlation = format!("cli-{}-{:x}", me.pid, nanos);
    if tiler_core::ids::CorrelationId::parse(&correlation).is_none() {
        return Err(err("error: correlation render"));
    }
    let request = WorkspaceRequest {
        v: WORKSPACE_REQUEST_VERSION,
        creation: record.owner.process_creation.clone(),
        pid: record.owner.pid,
        exe_path: record.owner.exe_path.clone(),
        user_sid: record.owner.user_sid.clone(),
        session_id: record.owner.session_id,
        action: options.action,
        index: options.index,
        direction: options.direction,
        correlation: correlation.clone(),
    };
    let body = render_workspace_request(&request);
    if body.is_empty() {
        return Err(err("error: request render"));
    }
    // Bounded single-pending queue: the body is written and synced to a
    // same-directory unique temp first and only then published atomically
    // with no-overwrite semantics, so the owner never observes a partial
    // body. A publish race against an already-queued request refuses.
    match crate::storage::publish_no_overwrite(
        &dir,
        WORKSPACE_REQUEST_FILE,
        body.as_bytes(),
        &correlation,
    ) {
        Ok(()) => {}
        Err(crate::storage::PublishError::Pending) => {
            return Err(err("refuse: workspace request pending"));
        }
        Err(crate::storage::PublishError::Io(e)) => {
            return Err(err(format!("error: request write: {e}")));
        }
    }
    Ok(serde_json::json!({"dispatched": true, "op": options.action.as_str(), "index": options.index, "direction": options.direction.map(|d| d.as_str())}).to_string())
}

/// Test-needed exact-owner keyboard-resize control for the normal `tile`
/// loop only (tentative, pending user review; the product hook still filters
/// injected chords, so synthetic input can never drive a resize). Mirrors
/// `cmd_workspace` exactly: same-path medium caller, exact ledger-owner
/// liveness, proof-owner refusal, and a bounded single-pending request file
/// (`resize.request`). The owner consumes the request once, resolves the
/// live foreground as the resize subject through the production origin/
/// member/lifetime/scope gates, and dispatches through the real
/// `keyboard_tick` resize arm: no classifier bypass, no fence bypass. The
/// CLI carries direction/mode only, never an HWND.
pub fn cmd_resize(options: &ResizeOptions) -> Result<String> {
    use crate::tiling::{RESIZE_REQUEST_VERSION, ResizeRequest, render_resize_request};
    ensure_pm_v2()?;
    let me = medium_caller()?;
    // Like `cmd_stop`, the CLI itself may run from any shell; only the
    // ledger owner liveness plus the same-path medium binding gate below.
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    let ledger_text =
        std::fs::read_to_string(dir.join(crate::storage::LEDGER_FILE_NAME)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                err("refuse: no owner running")
            } else {
                err(format!("error: ledger read: {e}"))
            }
        })?;
    let record: crate::model::RecoveryLedger =
        crate::model::parse_ledger(&ledger_text).map_err(|_| err("refuse: corrupt ledger"))?;
    // Same-path medium caller check, mirroring `cmd_stop`: SID, session, exe.
    if me.user_sid != record.owner.user_sid || me.session_id != record.owner.session_id {
        return Err(err("refuse: owner mismatch"));
    }
    let exe = crate::native::current_exe_path().map_err(|e| err(format!("error: exe {e}")))?;
    if !exe_paths_equal(&exe, &record.owner.exe_path) {
        return Err(err("refuse: owner mismatch"));
    }
    // Proof owners never serve the normal control: a proof audit marker for
    // this exact owner creation proves proof mode (normal runs write none).
    let audit = dir.join(format!(
        "proof-audit-{}.jsonl",
        record.owner.process_creation
    ));
    if audit.exists() {
        return Err(err("refuse: proof owner"));
    }
    // The ledger owner must be exactly alive (full identity, no PID reuse).
    let held = HeldProcess::open(record.owner.pid).map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    let live = held.identity().map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    if live != record.owner || !held.is_alive() {
        return Err(err("refuse: owner not running"));
    }
    let rid = held
        .integrity()
        .map_err(|e| err(format!("error: target integrity {e}")))?;
    if !is_medium_rid(rid) {
        return Err(err(format!("refuse: target integrity {rid} is not medium")));
    }
    if !crate::tiling::resize_direction_valid(&options.direction) {
        return Err(err("refuse: resize direction must be left|right|up|down"));
    }
    if !crate::tiling::resize_mode_valid(&options.mode) {
        return Err(err("refuse: resize mode must be outwards|inwards"));
    }
    let path = dir.join(RESIZE_REQUEST_FILE);
    if path.exists() {
        return Err(err("refuse: resize request pending"));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1);
    let correlation = format!("cli-{}-{:x}", me.pid, nanos);
    if tiler_core::ids::CorrelationId::parse(&correlation).is_none() {
        return Err(err("error: correlation render"));
    }
    let request = ResizeRequest {
        v: RESIZE_REQUEST_VERSION,
        creation: record.owner.process_creation.clone(),
        pid: record.owner.pid,
        exe_path: record.owner.exe_path.clone(),
        user_sid: record.owner.user_sid.clone(),
        session_id: record.owner.session_id,
        direction: options.direction.clone(),
        mode: options.mode.clone(),
        correlation: correlation.clone(),
    };
    let body = render_resize_request(&request);
    if body.is_empty() {
        return Err(err("error: request render"));
    }
    // Bounded single-pending queue: the body is written and synced to a
    // same-directory unique temp first and only then published atomically
    // with no-overwrite semantics, so the owner never observes a partial
    // body. A publish race against an already-queued request refuses.
    match crate::storage::publish_no_overwrite(
        &dir,
        RESIZE_REQUEST_FILE,
        body.as_bytes(),
        &correlation,
    ) {
        Ok(()) => {}
        Err(crate::storage::PublishError::Pending) => {
            return Err(err("refuse: resize request pending"));
        }
        Err(crate::storage::PublishError::Io(e)) => {
            return Err(err(format!("error: request write: {e}")));
        }
    }
    Ok(serde_json::json!({"dispatched": true, "op": "resize", "direction": options.direction, "mode": options.mode, "route": "test-needed-tentative"}).to_string())
}

/// `tile-proof` command: owned-helpers-only proof loop. Requires a nonempty
/// valid `--allowlist` at parse and at native start; malformed, empty,
/// duplicate, untagged, or non-owned entries refuse before any lease or write
/// and never fall back to normal mode. The raw received argv travels with the
/// parsed options into the proof-only audit (never production logs) with a
/// parsed/raw consistency check, so flag delivery is evidenced. Every setter
/// carries a proof audit record (requested/native target plus flags/outcome);
/// production logs stay token-only. Proof installs no keyboard hook; automated
/// synthetic-input verification uses the separate `shortcut-proof` command.
pub fn cmd_tile_proof(options: &TileProofOptions, raw_argv: &[String]) -> Result<String> {
    let text = std::fs::read_to_string(&options.allowlist)
        .map_err(|e| err(format!("error: allowlist read: {e}")))?;
    let entries = parse_allowlist(&text).map_err(err)?;
    if entries.is_empty() {
        return Err(err("refuse: empty allowlist"));
    }
    crate::tiling::verify_proof_argv_consistency(raw_argv, options).map_err(err)?;
    let trace = options.trace;
    let seconds = options.seconds;
    let border = options.border;
    let underlay = options.underlay;
    let raw_argv = raw_argv.to_vec();
    let keyboard = KeyboardConfig::disabled();
    // Proof installs no keyboard hook and takes no Snap setting: the proof
    // contract is unchanged unless a future explicit opt-in lands.
    crate::lifecycle::sys::run_product(trace, false, move |dir, me, store| {
        run_tile_loop(
            dir,
            me,
            store,
            TileRun {
                seconds,
                trace,
                allowlist: Some(entries),
                proof: true,
                shortcut_proof: false,
                workspace_proof: false,
                raw_argv,
                keyboard,
                mouse_snap: false,
                first_run: None,
                first_run_pending: None,
                inner_gap: INNER_GAP,
                outer_gap: OUTER_GAP,
                settings_dir: None,
                settings_live: None,
                cli_overrides: crate::tiling::CliOverrides::default(),
                scope: Vec::new(),
                scope_hosts: Vec::new(),
                border,
                underlay,
            },
        )
    })
}

/// `shortcut-proof` command: owned-helpers-only automated shortcut proof.
/// Same frozen-allowlist geometry gate as `tile-proof` (never falls back to
/// normal), but the owner installs the hook with test-only acceptance of
/// exactly [`crate::snapkey::SHORTCUT_PROOF_MARKER`] in `dwExtraInfo` and
/// drives the same keyboard dispatcher (fresh observation per intent, Engine
/// focus/move, native readback) plus the same session-only mouse routines.
/// Unshifted Win+L stays gated off (no flag offers it): live runs must never
/// send Win+L. Raw argv travels into the proof-only audit with a consistency
/// check; production logs stay token-only.
pub fn cmd_shortcut_proof(
    options: &crate::tiling::ShortcutProofOptions,
    raw_argv: &[String],
) -> Result<String> {
    let text = std::fs::read_to_string(&options.allowlist)
        .map_err(|e| err(format!("error: allowlist read: {e}")))?;
    let entries = parse_allowlist(&text).map_err(err)?;
    if entries.is_empty() {
        return Err(err("refuse: empty allowlist"));
    }
    crate::tiling::verify_shortcut_proof_argv_consistency(raw_argv, options).map_err(err)?;
    let trace = options.trace;
    let seconds = options.seconds;
    let border = options.border;
    let underlay = options.underlay;
    let mouse_snap = !options.no_mouse_snap_prevention;
    let raw_argv = raw_argv.to_vec();
    let keyboard = KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    };
    crate::lifecycle::sys::run_product(trace, mouse_snap, move |dir, me, store| {
        run_tile_loop(
            dir,
            me,
            store,
            TileRun {
                seconds,
                trace,
                allowlist: Some(entries),
                proof: true,
                shortcut_proof: true,
                workspace_proof: false,
                raw_argv,
                keyboard,
                mouse_snap,
                first_run: None,
                first_run_pending: None,
                inner_gap: INNER_GAP,
                outer_gap: OUTER_GAP,
                settings_dir: None,
                settings_live: None,
                cli_overrides: crate::tiling::CliOverrides::default(),
                scope: Vec::new(),
                scope_hosts: Vec::new(),
                border,
                underlay,
            },
        )
    })
}

/// `workspace-proof` command: owned-helpers-only automated workspace proof.
/// Same frozen-allowlist geometry gate as `shortcut-proof` plus the workspace
/// dispatcher (select/send/follow with hide/reveal) for exactly the
/// allowlisted helpers. The hook accepts exactly
/// [`crate::snapkey::SHORTCUT_PROOF_MARKER`]; every workspace hide verifies
/// `verify_proof_owned` fresh before the write. `shortcut-proof` never hides
/// for workspace intents and `tile-proof` installs no hook.
pub fn cmd_workspace_proof(
    options: &crate::tiling::WorkspaceProofOptions,
    raw_argv: &[String],
) -> Result<String> {
    let text = std::fs::read_to_string(&options.allowlist)
        .map_err(|e| err(format!("error: allowlist read: {e}")))?;
    let entries = parse_allowlist(&text).map_err(err)?;
    if entries.is_empty() {
        return Err(err("refuse: empty allowlist"));
    }
    crate::tiling::verify_workspace_proof_argv_consistency(raw_argv, options).map_err(err)?;
    let trace = options.trace;
    let seconds = options.seconds;
    let border = options.border;
    let underlay = options.underlay;
    let mouse_snap = !options.no_mouse_snap_prevention;
    let raw_argv = raw_argv.to_vec();
    let keyboard = KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    };
    crate::lifecycle::sys::run_product(trace, mouse_snap, move |dir, me, store| {
        run_tile_loop(
            dir,
            me,
            store,
            TileRun {
                seconds,
                trace,
                allowlist: Some(entries),
                proof: true,
                shortcut_proof: false,
                workspace_proof: true,
                raw_argv,
                keyboard,
                mouse_snap,
                first_run: None,
                first_run_pending: None,
                inner_gap: INNER_GAP,
                outer_gap: OUTER_GAP,
                settings_dir: None,
                settings_live: None,
                cli_overrides: crate::tiling::CliOverrides::default(),
                scope: Vec::new(),
                scope_hosts: Vec::new(),
                border,
                underlay,
            },
        )
    })
}

/// `hide-proof` command: owned-helpers-only visibility proof over the product
/// nonce mechanism. Requires a nonempty valid `--allowlist`; every frozen
/// entry is verified as an owned helper (sibling exe/class/lifetime-tag via
/// [`verify_proof_owned`]) BEFORE the ordinary product nonce APIs run, so
/// helper-only gates are never weakened. Each verified helper is admitted with a fresh product nonce and hidden with a
/// write-before-hide ledger commit (schema v4) under the central product
/// watcher; graceful stop auto-reveals via the guarded teardown, and
/// emergency loss auto-reveals via the watcher with idempotent independent
/// restore. Installs no hook, takes no Snap setting, moves no geometry.
pub fn cmd_hide_proof(options: &HideProofOptions, raw_argv: &[String]) -> Result<String> {
    let text = std::fs::read_to_string(&options.allowlist)
        .map_err(|e| err(format!("error: allowlist read: {e}")))?;
    let entries = parse_allowlist(&text).map_err(err)?;
    if entries.is_empty() {
        return Err(err("refuse: empty allowlist"));
    }
    crate::tiling::verify_hide_proof_argv_consistency(raw_argv, options).map_err(err)?;
    let trace = options.trace;
    let seconds = options.seconds;
    let raw_argv = raw_argv.to_vec();
    // No Snap prevention, no hook, no geometry: visibility proof only.
    crate::lifecycle::sys::run_product(trace, false, move |dir, me, store| {
        run_hide_proof_loop(dir, me, store, entries, seconds, trace, raw_argv)
    })
}

fn run_hide_proof_loop(
    dir: &Path,
    me: &ProcessIdentity,
    store: &LedgerStore,
    entries: Vec<AllowEntry>,
    seconds: Option<u64>,
    trace: bool,
    raw_argv: Vec<String>,
) -> Result<()> {
    ensure_pm_v2()?;
    let log_path = log_path_for(dir, &me.process_creation);
    let audit_path = dir.join(format!("proof-audit-{}.jsonl", me.process_creation));
    if audit_path.exists() {
        return Err(err("refuse: audit preexists, will not overwrite"));
    }
    let digest = allowlist_digest(&entries);
    let count = entries.len();
    log_json_at(
        &audit_path,
        serde_json::json!({
            "event": "proof-start",
            "argv": raw_argv,
            "seconds": seconds,
            "trace": trace,
            "mode": "hide-proof",
            "allowlist_digest": digest,
            "allowlist_count": count,
        }),
    );
    // Frozen-helper gate BEFORE any product nonce API: every entry must verify
    // as an owned helper, never an ordinary window. Opaque index only in
    // product errors; raw identity stays in the proof audit below.
    for (index, entry) in entries.iter().enumerate() {
        verify_proof_owned(entry.hwnd, entry, me).map_err(|reason| {
            err(format!(
                "refuse: hide-proof allowlist non-owned index={index} {reason}"
            ))
        })?;
    }
    let frozen: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "hwnd": e.hwnd,
                "pid": e.pid,
                "process_creation": e.process_creation,
                "exe_path": e.exe_path,
                "user_sid": e.user_sid,
                "session_id": e.session_id,
                "tag": e.tag,
            })
        })
        .collect();
    log_json_at(
        &audit_path,
        serde_json::json!({"event": "proof-frozen", "windows": frozen}),
    );
    let areas = all_monitors()?;
    let monitor = areas[0].clone();
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "hide-proof-start",
            "mode": "hide-proof",
            "trace": trace,
            "monitors": areas.len(),
            "work": [monitor.work.x, monitor.work.y, monitor.work.w, monitor.work.h],
            "full": [monitor.full.x, monitor.full.y, monitor.full.w, monitor.full.h],
            "allowlist_digest": digest,
            "allowlist_count": count,
        }),
    );
    // Admit + hide each verified helper via the ordinary product APIs.
    // Write-before-hide holds per window: the ledger commit precedes the
    // hide. Opaque index only in product errors; raw identity stays in the
    // proof audit below.
    let mut hidden = 0usize;
    let mut admitted: Vec<crate::model::WindowIdentity> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let claim = crate::product_hide::sys::admit_product_target(entry.hwnd, me)
            .map_err(|e| err(format!("error: hide-proof admit index={index} {e}")))?;
        crate::product_hide::sys::hide_committed_product(store, me, &claim, dir)
            .map_err(|e| err(format!("error: hide-proof hide index={index} {e}")))?;
        admitted.push(claim);
        hidden += 1;
        log_json_at(
            &log_path,
            serde_json::json!({"event": "hide-proof-hide", "index": hidden, "count": count}),
        );
        log_json_at(
            &audit_path,
            serde_json::json!({
                "event": "hide-proof-hide",
                "hwnd": entry.hwnd,
                "pid": entry.pid,
                "process_creation": entry.process_creation,
                "tag": entry.tag,
            }),
        );
    }
    // Ledger receipt check: current version with exactly the admitted product
    // claims, verified field-by-field (kind, HWND, full process, tag), not by
    // count alone, so a substituted claim cannot pass the receipt.
    match store.committed() {
        Ok(Some(record)) => {
            let exact = record.v == crate::model::LEDGER_SCHEMA_VERSION
                && record.windows.len() == admitted.len()
                && admitted.iter().all(|claim| {
                    record.windows.iter().any(|w| {
                        w.kind == crate::model::WindowClaimKind::Product
                            && w.hwnd == claim.hwnd
                            && w.process == claim.process
                            && w.tag == claim.tag
                    })
                });
            if !exact {
                return Err(err("error: hide-proof ledger receipt mismatch"));
            }
        }
        _ => return Err(err("error: hide-proof ledger receipt missing")),
    }
    let deadline = seconds.map(|s| Instant::now() + Duration::from_secs(s));
    loop {
        if stop_requested(dir, me)? {
            return Ok(());
        }
        if deadline.is_some_and(|end| Instant::now() >= end) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// `capture` command: read-only frozen-allowlist capture of explicitly listed
/// owned-helper windows only. Each `--hwnd` target is fully validated
/// (owned-helper identity with lifetime tag, proof-mode eligibility, fresh
/// rectangles from one coherent observe pair); any ineligible or untagged
/// target refuses the whole capture. The HWND pid is rechecked immediately
/// before the receipt write. Receipts may carry raw native ids; production
/// logs never do.
pub fn cmd_capture(options: &CaptureOptions) -> Result<String> {
    ensure_pm_v2()?;
    let me = medium_caller()?;
    let areas = all_monitors()?;
    let fulls = monitor_fulls(&areas);
    let mut tokens = TokenMap::default();
    let mut entries = Vec::new();
    let mut guarded: Vec<(u64, u32)> = Vec::new();
    for hwnd_u64 in &options.hwnds {
        let hwnd = *hwnd_u64 as isize as HWND;
        let window = match observe_window(hwnd, &me, &fulls, &mut tokens) {
            Ok(window) if window.hwnd == *hwnd_u64 => window,
            _ => return Err(err("refuse: capture target ineligible")),
        };
        if window.identity.tag.is_empty() {
            return Err(err("refuse: capture target ineligible"));
        }
        // Owned-helper binding: sibling exe/class/tag. Generic windows never
        // capture even when otherwise eligible.
        let helper_exe =
            crate::test_window::sys::sibling_helper_exe().map_err(|e| err(e.to_string()))?;
        let snap = crate::test_window::sys::query_owned(
            window.hwnd,
            &helper_exe,
            &me.user_sid,
            me.session_id,
        )
        .map_err(|_| err("refuse: capture target ineligible"))?;
        if snap.tag != window.identity.tag || snap.tag.is_empty() {
            return Err(err("refuse: capture target ineligible"));
        }
        if classify(&window.facts).is_err() {
            return Err(err("refuse: capture target ineligible"));
        }
        guarded.push((window.hwnd, window.identity.pid));
        entries.push(serde_json::json!({
            "hwnd": window.hwnd,
            "pid": window.identity.pid,
            "process_creation": window.identity.process_creation,
            "exe_path": window.identity.exe_path,
            "user_sid": window.identity.user_sid,
            "session_id": window.identity.session_id,
            "tag": window.identity.tag,
            "outer": [window.outer.x, window.outer.y, window.outer.w, window.outer.h],
            "visible": [window.visible.x, window.visible.y, window.visible.w, window.visible.h],
        }));
    }
    if options.hwnds.is_empty() {
        return Err(err("refuse: capture requires at least one --hwnd"));
    }
    // Fresh per-HWND pid guard immediately before the write: a recycled
    // HWND between observation and receipt refuses the whole capture.
    for (hwnd_u64, pid) in &guarded {
        let hwnd = *hwnd_u64 as isize as HWND;
        let mut current: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut current);
        }
        if current == 0 || current != *pid {
            return Err(err("refuse: capture target identity changed"));
        }
    }
    let receipt = serde_json::json!({ "windows": entries }).to_string();
    if options.out.exists() {
        return Err(err("refuse: receipt preexists"));
    }
    std::fs::write(&options.out, &receipt)
        .map_err(|e| err(format!("error: receipt write: {e}")))?;
    Ok(serde_json::json!({"captured": entries.len()}).to_string())
}

/// `inventory` command: read-only top-level window list for selecting capture
/// targets. Reports HWND, pid, executable basename, class, and style bits;
/// never reads titles. Stdout only; nothing is written or tiled.
pub fn cmd_inventory() -> Result<String> {
    ensure_pm_v2()?;
    let Some(hwnds) = enumerate_hwnds() else {
        return Err(err("error: enumeration failed"));
    };
    let mut entries = Vec::new();
    for raw in hwnds {
        let hwnd = raw as HWND;
        if unsafe { IsWindowVisible(hwnd) } == 0 {
            continue;
        }
        let mut pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        if pid == 0 {
            continue;
        }
        let exe = HeldProcess::open(pid)
            .ok()
            .and_then(|held| held.identity().ok())
            .map(|ident| {
                ident
                    .exe_path
                    .rsplit(['\\', '/'])
                    .next()
                    .unwrap_or("unknown")
                    .to_owned()
            })
            .unwrap_or_else(|| "unknown".to_owned());
        let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
        let exstyle = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
        entries.push(serde_json::json!({
            "hwnd": raw as u64,
            "pid": pid,
            "exe": exe,
            "class": class_of(hwnd),
            "style": format!("{style:#x}"),
            "exstyle": format!("{exstyle:#x}"),
        }));
    }
    Ok(serde_json::json!({ "windows": entries }).to_string())
}

unsafe extern "system" fn child_proc(hwnd: HWND, state: LPARAM) -> i32 {
    let out = unsafe { &mut *(state as *mut Vec<isize>) };
    out.push(hwnd as isize);
    1
}

/// Enumerate direct child windows of one parent. `None` on API failure.
fn enumerate_child_windows(parent: HWND) -> Option<Vec<isize>> {
    let mut out: Vec<isize> = Vec::new();
    let ok = unsafe {
        EnumChildWindows(
            parent,
            Some(child_proc),
            &mut out as *mut Vec<isize> as LPARAM,
        )
    };
    if ok == 0 { None } else { Some(out) }
}

/// Live executable paths of a top-level window's hosted children: direct
/// children owned by a different process than the top-level window itself.
/// Unreadable children contribute nothing (never a match). Scope
/// classification only: no effects touch child windows. Empty on enumeration
/// failure, which fails closed for listed hosts at the caller's gate.
fn hosted_child_exes(parent: HWND, top_pid: u32) -> Vec<String> {
    let mut out = Vec::new();
    let Some(children) = enumerate_child_windows(parent) else {
        return out;
    };
    for raw in children {
        let hwnd = raw as HWND;
        let mut pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        if pid == 0 || pid == top_pid {
            continue;
        }
        let exe = HeldProcess::open(pid)
            .ok()
            .and_then(|held| held.identity().ok())
            .filter(|ident| ident.pid == pid)
            .map(|ident| ident.exe_path);
        if let Some(exe) = exe {
            out.push(exe);
        }
    }
    out
}

/// Fresh hosted-child gate for one top-level window: listed hosts pass only
/// while a live hosted child matches. Short-circuits before enumerating when
/// the top-level executable names no pair. Read-only; the caller selects the
/// refusal outcome.
fn hosted_gate_allows(
    top_exe: &str,
    hwnd_u64: u64,
    top_pid: u32,
    pairs: &[ScopeHostChild],
) -> bool {
    if pairs.is_empty() {
        return true;
    }
    if !pairs
        .iter()
        .any(|pair| scope_exe_basename(&pair.host) == scope_exe_basename(top_exe))
    {
        return true;
    }
    let hwnd = hwnd_u64 as isize as HWND;
    hosted_child_allows(top_exe, &hosted_child_exes(hwnd, top_pid), pairs)
}

fn exe_basename(exe_path: &str) -> String {
    exe_path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("unknown")
        .to_owned()
}

/// `children` command: read-only child-window report for explicitly listed
/// top-level targets. Each child carries HWND, pid, executable basename,
/// class, and full process identity when readable; never titles. Used to
/// bind hosted app windows (e.g. Calculator under an ApplicationFrameHost
/// root) to their owning process before any capture or mutation.
pub fn cmd_children(options: &ChildrenOptions) -> Result<String> {
    ensure_pm_v2()?;
    let mut windows = Vec::new();
    for hwnd_u64 in &options.hwnds {
        let parent = *hwnd_u64 as isize as HWND;
        let Some(raw_children) = enumerate_child_windows(parent) else {
            return Err(err("error: child enumeration failed"));
        };
        let mut children = Vec::new();
        for raw in raw_children {
            let hwnd = raw as HWND;
            let mut pid: u32 = 0;
            unsafe {
                GetWindowThreadProcessId(hwnd, &mut pid);
            }
            let identity = (pid != 0)
                .then(|| HeldProcess::open(pid).ok())
                .flatten()
                .and_then(|held| held.identity().ok())
                .filter(|ident| ident.pid == pid);
            let (exe, detail) = match identity {
                Some(ident) => (
                    exe_basename(&ident.exe_path),
                    serde_json::json!({
                        "pid": ident.pid,
                        "process_creation": ident.process_creation,
                        "exe_path": ident.exe_path,
                        "user_sid": ident.user_sid,
                        "session_id": ident.session_id,
                    }),
                ),
                None => ("unknown".to_owned(), serde_json::Value::Null),
            };
            children.push(serde_json::json!({
                "hwnd": raw as u64,
                "pid": pid,
                "exe": exe,
                "class": class_of(hwnd),
                "identity": detail,
            }));
        }
        windows.push(serde_json::json!({ "hwnd": *hwnd_u64, "children": children }));
    }
    Ok(serde_json::json!({ "windows": windows }).to_string())
}

fn rect_array(rect: &Rect) -> [i32; 4] {
    [rect.x, rect.y, rect.w, rect.h]
}

fn medium_caller() -> Result<ProcessIdentity> {
    let me = crate::native::current_identity().map_err(|e| err(format!("error: identity {e}")))?;
    let rid = crate::native::current_integrity_level()
        .map_err(|e| err(format!("error: integrity {e}")))?;
    if !is_medium_rid(rid) {
        return Err(err(format!("refuse: integrity {rid} is not medium")));
    }
    Ok(me)
}

fn read_allowlist(path: &Path) -> Result<Vec<AllowEntry>> {
    let text =
        std::fs::read_to_string(path).map_err(|e| err(format!("error: allowlist read: {e}")))?;
    parse_allowlist(&text).map_err(err)
}

/// One inspection entry without frame geometry: full frozen-matchable
/// identity, an accurate skip, and no fabricated rectangles. `outer` and
/// `visible` are null; `dpi` and `minimized` are real queried state.
fn noframe_inspect_report(
    entry: &AllowEntry,
    identity: &ObservedTarget,
    skip: SkipReason,
    minimized: bool,
    hwnd: HWND,
) -> serde_json::Value {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    serde_json::json!({
        "hwnd": entry.hwnd,
        "pid": identity.pid,
        "process_creation": identity.process_creation,
        "exe_path": identity.exe_path,
        "user_sid": identity.user_sid,
        "session_id": identity.session_id,
        "tag": identity.tag,
        "identity_match": true,
        "outer": null,
        "visible": null,
        "dpi": dpi,
        "minimized": minimized,
        "eligible": false,
        "skip": skip.as_str(),
    })
}

/// `inspect` command: read-only fresh-state report for exactly the frozen
/// allowlist. Identity and state resolve before any frame query, so a
/// legitimately hidden (passive) or minimized exact helper keeps full
/// identity match with an accurate skip and no fabricated rectangles; only a
/// genuinely unresolvable or non-matching identity reports
/// `identity-changed`. Raw native identity, fresh outer/visible rectangles
/// from one coherent observe pair when frames exist, per-window DPI, native
/// facts with proof-mode eligibility, and the current foreground HWND. No
/// titles, no writes.
pub fn cmd_inspect(options: &InspectOptions) -> Result<String> {
    use ObserveFailure::Known;
    ensure_pm_v2()?;
    let me = medium_caller()?;
    let areas = all_monitors()?;
    let fulls = monitor_fulls(&areas);
    let allow = read_allowlist(&options.allowlist)?;
    let mut tokens = TokenMap::default();
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    let mut windows = Vec::new();
    for entry in &allow {
        let hwnd = entry.hwnd as isize as HWND;
        // Stateless identity/state first, independent of geometry.
        let ident = window_identity(hwnd, entry.hwnd, &me);
        let matched = ident.as_ref().is_some_and(|i| allow_match(entry, i));
        let visible = unsafe { IsWindowVisible(hwnd) } != 0;
        let minimized = unsafe { IsIconic(hwnd) } != 0;
        match inspect_stateless_verdict(matched, visible, minimized) {
            StatelessVerdict::Report {
                identity_match,
                // No-frame reports are never eligible; the verdict type
                // guarantees this today.
                eligible: _,
                skip,
            } => {
                match (ident.as_ref(), identity_match) {
                    (Some(identity), true) => windows.push(noframe_inspect_report(
                        entry, identity, skip, minimized, hwnd,
                    )),
                    _ => windows.push(serde_json::json!({
                        "hwnd": entry.hwnd,
                        "identity_match": false,
                        "eligible": false,
                        "skip": skip.as_str(),
                    })),
                }
                continue;
            }
            StatelessVerdict::NeedsGeometry => {}
        }
        // Identity matched a visible, non-minimized window: frame queries and
        // `classify` still decide. A racy frame failure still reports the
        // known identity with its accurate reason, never `identity-changed`.
        match observe_window(hwnd, &me, &fulls, &mut tokens) {
            Ok(window) if allow_match(entry, &window.identity) => {
                let outer = window.outer;
                let dpi = unsafe { GetDpiForWindow(hwnd) };
                let (eligible, skip) = match classify(&window.facts) {
                    Ok(()) => (true, None),
                    Err(reason) => (false, Some(reason.as_str().to_owned())),
                };
                windows.push(serde_json::json!({
                    "hwnd": window.hwnd,
                    "pid": window.identity.pid,
                    "process_creation": window.identity.process_creation,
                    "exe_path": window.identity.exe_path,
                    "user_sid": window.identity.user_sid,
                    "session_id": window.identity.session_id,
                    "tag": window.identity.tag,
                    "identity_match": true,
                    "outer": rect_array(&outer),
                    "visible": rect_array(&window.visible),
                    "dpi": dpi,
                    "facts": {
                        "minimized": window.facts.minimized,
                        "maximized": window.facts.maximized,
                        "cloaked": window.facts.cloaked,
                        "elevated": window.facts.elevated,
                        "shell": window.facts.shell,
                        "tool_window": window.facts.tool_window,
                        "owned": window.facts.owned,
                        "captionless_fullscreen": window.facts.captionless_fullscreen,
                        "no_activate": window.facts.no_activate,
                        "dialog": window.facts.dialog,
                    },
                    "eligible": eligible,
                    "skip": skip,
                }));
            }
            Err(Known(known, reason)) if allow_match(entry, &known.identity) => {
                windows.push(noframe_inspect_report(
                    entry,
                    &known.identity,
                    reason,
                    reason == SkipReason::Minimized,
                    hwnd,
                ));
            }
            _ => windows.push(serde_json::json!({
                "hwnd": entry.hwnd,
                "identity_match": false,
                "eligible": false,
                "skip": SkipReason::IdentityChanged.as_str(),
            })),
        }
    }
    Ok(serde_json::json!({ "windows": windows, "foreground": foreground }).to_string())
}
/// Read-only z-order relation for `border-inspect`: the owned overlay versus
/// the current foreground window in `EnumWindows` top-to-bottom order.
/// `(above, adjacent_below)`. A background owner cannot place its surface
/// above the foreground window, so adjacent-below (directly beneath the
/// target, ring uncovered) is the achievable correct placement. `(None, None)`
/// when either endpoint is gone. No writes.
fn overlay_z_relation(overlay_u64: u64) -> (Option<bool>, Option<bool>) {
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    if overlay_u64 == 0 || foreground == 0 || overlay_u64 == foreground {
        return (None, None);
    }
    if unsafe { IsWindow(foreground as isize as HWND) } == 0
        || unsafe { IsWindow(overlay_u64 as isize as HWND) } == 0
    {
        return (None, None);
    }
    let order = {
        unsafe extern "system" fn enum_proc(hwnd: HWND, state: LPARAM) -> i32 {
            let out = unsafe { &mut *(state as *mut Vec<u64>) };
            out.push(hwnd as usize as u64);
            1
        }
        let mut out: Vec<u64> = Vec::new();
        let ok = unsafe { EnumWindows(Some(enum_proc), &mut out as *mut Vec<u64> as LPARAM) };
        if ok == 0 {
            return (None, None);
        }
        out
    };
    let mut upper_idx: Option<usize> = None;
    let mut lower_idx: Option<usize> = None;
    for (i, hwnd) in order.iter().enumerate() {
        if *hwnd == overlay_u64 && upper_idx.is_none() {
            upper_idx = Some(i);
        }
        if *hwnd == foreground && lower_idx.is_none() {
            lower_idx = Some(i);
        }
        if upper_idx.is_some() && lower_idx.is_some() {
            break;
        }
    }
    match (upper_idx, lower_idx) {
        (Some(u), Some(l)) if u < l => (Some(true), Some(false)),
        (Some(u), Some(l)) if u == l + 1 => (Some(false), Some(true)),
        (Some(_), Some(_)) => (Some(false), Some(false)),
        _ => (None, None),
    }
}
/// `border-inspect` command: read-only report of the running owner's
/// process-owned overlay window (geometry/visibility only). Binds the exact
/// ledger owner (creation/pid/exe/sid/session) and reports only that owner's
/// overlay class windows: `present` is false when the owner runs with
/// `--no-active-border` or has no eligible target. No titles, no content, no
/// screen capture; the owned-pixel checksum rides the owner's `active-border`
/// log events instead.
pub fn cmd_border_inspect() -> Result<String> {
    inspect_carrier(crate::active_border_sys::OVERLAY_CLASS)
}

/// `underlay-inspect` command: read-only report of the running owner's
/// process-owned group-underlay fill (geometry/visibility only), mirroring
/// `border-inspect`. `present` means a carrier HWND was enumerated for this
/// owner (hidden HWNDs persist: `hide` keeps the window alive for the next
/// group, so `present` stays true with `visible=false` after the first show).
/// It is false when the owner runs with `--no-group-underlay` or before the
/// first show. No titles, no content, no screen capture; the owned-pixel
/// checksum rides the owner's `group-underlay` log events instead.
pub fn cmd_underlay_inspect() -> Result<String> {
    inspect_carrier(crate::active_border_sys::UNDERLAY_CLASS)
}

/// `preview-inspect` command: read-only report of the running owner's
/// process-owned drop-preview fill (geometry/visibility only), mirroring
/// `border-inspect`/`underlay-inspect`. `present` means a carrier HWND was
/// enumerated for this owner (hidden HWNDs persist after the first show).
/// It is false before the first preview sample. No titles, no content, no
/// screen capture; the owned-pixel checksum rides the owner's `drag-preview`
/// log events instead.
pub fn cmd_preview_inspect() -> Result<String> {
    inspect_carrier(crate::active_border_sys::PREVIEW_CLASS)
}

/// Shared read-only carrier inspection for one owned overlay class.
fn inspect_carrier(class: &str) -> Result<String> {
    use windows_sys::Win32::Foundation::RECT;
    ensure_pm_v2()?;
    let me = medium_caller()?;
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    let ledger_text =
        std::fs::read_to_string(dir.join(crate::storage::LEDGER_FILE_NAME)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                err("refuse: no owner running")
            } else {
                err(format!("error: ledger read: {e}"))
            }
        })?;
    let record: crate::model::RecoveryLedger =
        crate::model::parse_ledger(&ledger_text).map_err(|_| err("refuse: corrupt ledger"))?;
    if me.user_sid != record.owner.user_sid || me.session_id != record.owner.session_id {
        return Err(err("refuse: owner mismatch"));
    }
    let exe = crate::native::current_exe_path().map_err(|e| err(format!("error: exe {e}")))?;
    if !exe_paths_equal(&exe, &record.owner.exe_path) {
        return Err(err("refuse: owner mismatch"));
    }
    let held = HeldProcess::open(record.owner.pid).map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    let live = held.identity().map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    if live != record.owner || !held.is_alive() {
        return Err(err("refuse: owner not running"));
    }
    let Some(hwnds) = enumerate_hwnds() else {
        return Err(err("error: enumeration failed"));
    };
    let mut overlays = Vec::new();
    for raw in hwnds {
        let hwnd = raw as HWND;
        if class_of(hwnd) != class {
            continue;
        }
        let mut pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        if pid == 0 || pid != record.owner.pid {
            continue;
        }
        let mut rect: RECT = unsafe { std::mem::zeroed() };
        if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
            continue;
        }
        let Some(rect) = rect_from_win(rect) else {
            continue;
        };
        let exstyle = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
        let (above_fg, adjacent_fg) = overlay_z_relation(raw as usize as u64);
        overlays.push(serde_json::json!({
            "hwnd": raw as u64,
            "visible": unsafe { IsWindowVisible(hwnd) } != 0,
            "rect": rect_array(&rect),
            "dpi": unsafe { GetDpiForWindow(hwnd) },
            "exstyle": format!("{exstyle:08x}"),
            "topmost": exstyle & WS_EX_TOPMOST != 0,
            "above_foreground": above_fg,
            "adjacent_below_foreground": adjacent_fg,
        }));
    }
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    Ok(serde_json::json!({
        "present": !overlays.is_empty(),
        "overlays": overlays,
        "foreground": foreground,
    })
    .to_string())
}

#[cfg(test)]
mod esc_sequence_tests {
    use super::esc_edge_cancels;

    #[test]
    fn end_bound_sequence_orders_cancel() {
        // Latest physical Esc AFTER the END (before the owner drain) must
        // not cancel the completed drop: the END snapshot still equals
        // START even though the live counter moved on.
        assert!(!esc_edge_cancels(5, 5));
        // Esc on/before the END cancels: the END snapshot is newer.
        assert!(esc_edge_cancels(5, 6));
        // No edge at all never cancels.
        assert!(!esc_edge_cancels(0, 0));
        assert!(!esc_edge_cancels(u64::MAX, u64::MAX));
        // Multiple edges still cancel.
        assert!(esc_edge_cancels(7, 9));
    }
}

#[cfg(test)]
mod same_axis_move_tests {
    use super::same_axis_move_for;
    use crate::settings::{LiveSettings, SAME_AXIS_MOVE_SWAP, Settings};
    use tiler_core::directional::SameAxisMove;

    #[test]
    fn proof_owners_without_live_settings_keep_group_wrap() {
        assert_eq!(same_axis_move_for(None), SameAxisMove::GroupWithNeighbor);
    }

    #[test]
    fn live_setting_selects_subsequent_move_mode() {
        let live = LiveSettings::fresh(Settings::default(), None);
        assert_eq!(
            same_axis_move_for(Some(&live)),
            SameAxisMove::GroupWithNeighbor
        );
        let mut swap = Settings::default();
        swap.core.same_axis_move = SAME_AXIS_MOVE_SWAP.to_owned();
        let live = LiveSettings::fresh(swap, None);
        assert_eq!(
            same_axis_move_for(Some(&live)),
            SameAxisMove::SwapWithNeighbor
        );
    }

    #[test]
    fn apply_same_axis_only_returns_false_adopts_and_rebuilds_nothing() {
        use crate::settings::{build_disabled, build_remap, tile_options_from_settings};
        use tiler_core::boundary::{CoreCommand, CoreReply};
        use tiler_core::directional::WindowId;
        use tiler_core::geometry::Rect;
        use tiler_core::ids::CorrelationId;

        // Scratch home for the ledger lock and the owner log line: never the
        // repo or the live product directory. No window, hook, suspend, or
        // registry effect exists on this path (every native lane below is
        // aligned away); the temp dir is removed at the end.
        let tmp = std::env::temp_dir().join(format!(
            "tiler-same-axis-live-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
        let mut state = super::rmax03_adapter_tests::test_state();
        state.log_path = tmp.join("owner.log");
        // Align every lane with the defaults so the incoming document differs
        // in exactly one field. In particular the keyboard/remap lanes must
        // match, or `apply_live_config` would touch the native hook config.
        let base = tile_options_from_settings(&Settings::default());
        state.border = base.border;
        state.underlay = base.underlay;
        state.keyboard = crate::snapkey::KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        };
        state.last_remap = build_remap(&Settings::default());
        state.last_disabled = build_disabled(&Settings::default());
        state.settings_live = Some(LiveSettings::fresh(Settings::default(), None));
        let mut snap_want = true;
        // Retained session: two tiled windows, so "no rebuild" is observable
        // instead of vacuous.
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 1600,
            h: 900,
        };
        let domain = crate::workspace_owner::workspace_domain("mon-a", "ws-1", bounds, 8);
        let seed_fp =
            crate::tiling::fingerprint(&[("w1".to_owned(), bounds), ("w2".to_owned(), bounds)]);
        let seed = crate::tiling::build_reconcile_event_for(
            &state.owner,
            &state.generation,
            &CorrelationId::parse("live-seed").expect("correlation"),
            0,
            seed_fp,
            &domain.0,
            &domain.1,
            8,
            &[
                (
                    WindowId("w1".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                ),
                (
                    WindowId("w2".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                ),
            ],
            Some(&WindowId("w2".to_owned())),
        );
        let reply = state.engine.handle(&seed);
        assert!(
            matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
            "seed must converge, got {reply:?}"
        );
        // A move command builds here exactly like `keyboard_tick` builds it.
        let moved = {
            let mut event = seed.clone();
            event.revision = state
                .engine
                .session(&domain.1)
                .map(|session| session.accepted_revision())
                .unwrap_or(0);
            event.focused_window = WindowId("w2".to_owned());
            event.command = CoreCommand::Move {
                window: "w2".to_owned(),
                direction: "left".to_owned(),
                cross_output_transfer: false,
                same_axis_move: same_axis_move_for(state.settings_live.as_ref()),
            };
            state.engine.handle(&event)
        };
        assert!(
            matches!(moved, CoreReply::MoveDirectional(_)),
            "pre-apply move must plan, got {moved:?}"
        );
        let before = state.engine.session(&domain.1).expect("session").snapshot();
        let me = crate::model::ProcessIdentity {
            pid: 4242,
            process_creation: "test-creation".to_owned(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
            exe_path: "test.exe".to_owned(),
        };
        let mut incoming = Settings::default();
        incoming.core.same_axis_move = SAME_AXIS_MOVE_SWAP.to_owned();
        incoming.revision = 7;
        let changed = super::apply_live_settings(
            &mut state,
            &tmp,
            &me,
            &store,
            &mut snap_want,
            incoming,
            None,
        );
        assert!(
            !changed,
            "same-axis-only adoption must report no owner change"
        );
        assert_eq!(
            state.engine.session(&domain.1).expect("session").snapshot(),
            before,
            "retained session untouched: no rebuild, no resync"
        );
        let live = state.settings_live.as_ref().expect("live");
        assert_eq!(live.settings.core.same_axis_move, SAME_AXIS_MOVE_SWAP);
        assert_eq!(live.status, "saved:7");
        assert_eq!(
            same_axis_move_for(state.settings_live.as_ref()),
            SameAxisMove::SwapWithNeighbor,
            "subsequent moves use the adopted setting"
        );
        assert_eq!((state.inner_gap, state.outer_gap), (8, 8));
        drop(store);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

#[cfg(test)]
mod born_maximized_tests {
    use super::{born_maximized_keep_overlay_for, rmax03_adapter_tests::test_state};
    use crate::settings::{BORN_MAXIMIZED_OVERLAY, BORN_MAXIMIZED_TILE, LiveSettings, Settings};
    use tiler_core::geometry::Rect;

    const BOUNDS: Rect = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };
    const NATIVE_MAX: Rect = Rect {
        x: -8,
        y: -8,
        w: 1936,
        h: 1056,
    };
    const RESTORED: Rect = Rect {
        x: 120,
        y: 120,
        w: 800,
        h: 600,
    };
    const SIBLING: Rect = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };

    /// Shared harness: proof-style owner state plus one workspace in the
    /// requested mode. No HWNDs, hooks, or live effects.
    fn harness(tiled: bool) -> (super::TileLoop, String, String) {
        let mut state = test_state();
        let output = "mon-1".to_owned();
        state.workspaces.ensure_output(&output);
        let active = state.workspaces.active_id(&output).expect("active");
        state.workspaces.set_tiled(&output, &active, tiled);
        assert_eq!(state.workspaces.is_tiled(&output, &active), tiled);
        (state, output, active)
    }

    fn max_key(hwnd: u64) -> crate::workspace::WindowKey {
        crate::workspace::WindowKey {
            hwnd,
            pid: 7,
            creation: "creation-live".to_owned(),
        }
    }

    fn live_with(born: &str) -> LiveSettings {
        let mut settings = Settings::default();
        settings.core.born_maximized = born.to_owned();
        LiveSettings::fresh(settings, None)
    }

    /// Stage the slotless-admission outcome for one fresh lifetime through the
    /// actual production binding helper: workspace membership without a tile
    /// slot, plus the latch exactly when production binds it. Returns the key.
    /// (`admit_slotless_maximized` itself needs live HWNDs for its ownership
    /// fences, so staging its committed outcome through the real helper is
    /// the hermetic boundary; the helper, the latch set, and the gate below
    /// are the production selection.)
    fn admit_slotless(
        state: &mut super::TileLoop,
        output: &str,
        active: &str,
        hwnd: u64,
        token: &str,
    ) -> crate::workspace::WindowKey {
        let key = max_key(hwnd);
        assert!(state.workspaces.assign(key.clone(), output, active, false));
        state.member_tokens.insert(key.clone(), token.to_owned());
        assert!(!state.member_rects.contains_key(token));
        if crate::tiling::born_keep_latch(
            state.workspaces.is_tiled(output, active),
            born_maximized_keep_overlay_for(state.settings_live.as_ref()),
        ) {
            state.born_keep_overlay.insert(key.clone());
        }
        key
    }

    /// Assemble one domain round through the production row path.
    fn assemble(
        state: &mut super::TileLoop,
        output: &str,
        active: &str,
        observed: &[super::ObservedWindow],
        retained: &[super::RetainedRow],
    ) -> Vec<crate::workspace_owner::OwnerRow> {
        let mut hint_cx = super::HintCx::new();
        super::assemble_domain_rows(
            state,
            output,
            active,
            observed,
            retained,
            "reconcile",
            "rmax-06",
            &mut hint_cx,
        )
        .expect("rows assemble")
    }

    /// Accepted Engine revision for one domain, for follow-up rounds.
    fn accepted_revision(state: &super::TileLoop, output: &str, active: &str) -> u64 {
        let (_, key) = crate::workspace_owner::workspace_domain(
            output,
            active,
            BOUNDS,
            crate::tiling::INNER_GAP,
        );
        state
            .engine
            .session(&key)
            .map(|session| session.accepted_revision())
            .unwrap_or(0)
    }

    /// Drive one production reconcile round through the real retained Engine
    /// and return the planned writes.
    fn reconcile(
        state: &mut super::TileLoop,
        output: &str,
        active: &str,
        rows: &[crate::workspace_owner::OwnerRow],
        revision: u64,
    ) -> Vec<crate::workspace_owner::PlannedWrite> {
        use tiler_core::directional::WindowId;
        use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
        let (domain, domain_key) = crate::workspace_owner::workspace_domain(
            output,
            active,
            BOUNDS,
            crate::tiling::INNER_GAP,
        );
        let windows: Vec<(
            WindowId,
            Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )> = rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("rmax06-adapter").expect("generation");
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        let event = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &CorrelationId::parse("rmax-06").expect("correlation"),
            revision,
            fp,
            &domain,
            &domain_key,
            crate::tiling::OUTER_GAP,
            &windows,
            None,
        );
        let reply = state.engine.handle(&event);
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Tiled(_)
                    | tiler_core::boundary::CoreReply::Projection(_)
            ),
            "unexpected reply: {reply:?}"
        );
        crate::workspace_owner::planned_writes(&reply).expect("tile plan")
    }

    #[test]
    fn latch_binds_tiled_first_overlay_only() {
        use crate::tiling::{born_keep_latch, born_maximized_keep_overlay};
        // Exact wire values first: only the overlay token keeps; hand-built
        // near-misses tile, and proof owners (no live settings) read default.
        assert!(born_maximized_keep_overlay(BORN_MAXIMIZED_OVERLAY));
        assert!(!born_maximized_keep_overlay(BORN_MAXIMIZED_TILE));
        for bad in [
            "",
            "tile",
            "keep-overlay",
            "TILE-ON-LAUNCH",
            "KEEP-MAXIMIZED-OVERLAY",
            "group-with-neighbor",
        ] {
            assert!(!born_maximized_keep_overlay(bad), "{bad}");
        }
        assert!(!born_maximized_keep_overlay_for(None));
        assert!(!born_keep_latch(
            true,
            born_maximized_keep_overlay_for(None)
        ));
        // Lane matrix through the actual binding helper: tiled-first under
        // the overlay value latches; floating-first (R-MAX-03) never latches
        // under either value, and tiled-first under the tile default does not.
        assert!(born_keep_latch(true, true));
        assert!(!born_keep_latch(true, false));
        assert!(!born_keep_latch(false, true));
        assert!(!born_keep_latch(false, false));
    }

    #[test]
    fn gate_honors_latch_with_legacy_gates_intact() {
        use crate::tiling::should_clear_maximize_at_admission_with_mode as gate;
        // The latched first tiled sighting keeps; the unlatched one clears.
        assert!(gate(false, true, false, false, true, false));
        assert!(!gate(false, true, false, false, true, true));
        // Every other gate is latch-independent, so R-MAX-03 retile,
        // born-fullscreen exits, and later maxima behave identically under
        // both values: fullscreen never clears, slotted members (later
        // maxima) never re-clear, attempts never retry, floating defers, and
        // non-maximized rows never clear.
        for (fullscreen, maximized, known_slot, attempted, tiled) in [
            (true, true, false, false, true),
            (false, true, true, false, true),
            (false, true, false, true, true),
            (false, true, false, false, false),
            (false, false, false, false, true),
        ] {
            for keep in [false, true] {
                assert!(
                    !gate(fullscreen, maximized, known_slot, attempted, tiled, keep),
                    "no clear for ({fullscreen},{maximized},{known_slot},{attempted},{tiled},{keep})"
                );
            }
        }
        // The legacy five-argument gate pins the unlatched lane exactly.
        for (fullscreen, maximized, known_slot, attempted, tiled) in [
            (false, true, false, false, true),
            (true, true, false, false, true),
            (false, true, true, false, true),
            (false, true, false, true, true),
            (false, true, false, false, false),
            (false, false, false, false, true),
        ] {
            assert_eq!(
                crate::tiling::should_clear_maximize_at_admission(
                    fullscreen, maximized, known_slot, attempted, tiled
                ),
                gate(fullscreen, maximized, known_slot, attempted, tiled, false),
                "legacy gate pins tile lane"
            );
        }
    }

    #[test]
    fn tile_bound_lifetime_clears_despite_overlay_flip() {
        // Tiled-first under the tile default binds no latch; flipping live to
        // overlay afterwards cannot move the earlier admission: it still clears.
        let (mut state, output, active) = harness(true);
        state.settings_live = Some(live_with(BORN_MAXIMIZED_TILE));
        let max = admit_slotless(&mut state, &output, &active, 101, "w8");
        assert!(!state.born_keep_overlay.contains(&max));
        state.settings_live = Some(live_with(BORN_MAXIMIZED_OVERLAY));
        assert!(!state.born_keep_overlay.contains(&max));
        assert!(
            crate::tiling::should_clear_maximize_at_admission_with_mode(
                false, true, false, false, true, false
            ),
            "earlier tile-bound admission still clears after the flip"
        );
    }

    fn maximized_row(
        key: &crate::workspace::WindowKey,
        token: &str,
        frame: tiler_core::geometry::Rect,
        maximized: bool,
    ) -> super::RetainedRow {
        super::RetainedRow {
            key: key.clone(),
            token: token.to_owned(),
            rect: Some(frame),
            maximized,
            fullscreen: false,
            facts: Some(crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                dialog: false,
            }),
        }
    }

    fn observed_normal(
        hwnd: u64,
        token: &str,
        frame: tiler_core::geometry::Rect,
    ) -> super::ObservedWindow {
        super::ObservedWindow {
            hwnd,
            token: token.to_owned(),
            outer: frame,
            visible: frame,
            insets: crate::tiling::FrameInsets {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            identity: crate::tiling::ObservedTarget {
                hwnd,
                pid: 7,
                process_creation: "creation-live".to_owned(),
                exe_path: "C:\\test\\app.exe".to_owned(),
                user_sid: "S-1".to_owned(),
                session_id: 1,
                tag: String::new(),
            },
            facts: crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: false,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                dialog: false,
            },
        }
    }

    #[test]
    fn overlay_born_reserves_slot_and_restore_lands_exact() {
        // Fixed-size detection is unwired on Windows (item 13 owns the gap):
        // every maximized row here rides the ordinary overlay path with no
        // fixed branch, so this delivery adds none.
        let (mut state, output, active) = harness(true);
        state.settings_live = Some(live_with(BORN_MAXIMIZED_OVERLAY));
        // Sibling already tiled; first-seen maximum admitted slotless through
        // the production binding helper (tiled-first + overlay latches).
        let sib = max_key(100);
        assert!(
            state
                .workspaces
                .assign(sib.clone(), &output, &active, false)
        );
        state.member_tokens.insert(sib.clone(), "w7".to_owned());
        state.member_rects.insert("w7".to_owned(), SIBLING);
        let max = admit_slotless(&mut state, &output, &active, 101, "w8");
        assert!(state.born_keep_overlay.contains(&max));
        // The bound lifetime, not the live value, gates its clear: flipping
        // live to tile cannot move it, and no attempt ever arms.
        state.settings_live = Some(live_with(BORN_MAXIMIZED_TILE));
        assert!(state.born_keep_overlay.contains(&max));
        assert!(
            !crate::tiling::should_clear_maximize_at_admission_with_mode(
                false, true, false, false, true, true
            )
        );
        assert!(state.maximize_admission_attempted.is_empty());
        state.settings_live = Some(live_with(BORN_MAXIMIZED_OVERLAY));

        // Production row assembly over the retained maximum plus the live
        // sibling: the slot seeds and both rows reach the real Engine.
        let observed = vec![observed_normal(100, "w7", SIBLING)];
        let retained = vec![maximized_row(&max, "w8", NATIVE_MAX, true)];
        let rows = assemble(&mut state, &output, &active, &observed, &retained);
        assert_eq!(rows.len(), 2);
        let overlaid = rows.iter().find(|r| r.token == "w8").expect("slot row");
        assert!(
            !overlaid.floating,
            "overlay keeps a tile slot, not a float exception"
        );
        assert!(
            state.member_rects.contains_key("w8"),
            "tiled overlay must seed the retained slot"
        );
        let writes = reconcile(&mut state, &output, &active, &rows, 0);
        let reserved = writes
            .iter()
            .find(|w| w.window.0 == "w8")
            .expect("reserved slot")
            .rect;
        assert_ne!(reserved, NATIVE_MAX, "slot is layout, not the max frame");
        assert!(
            writes.iter().any(|w| w.window.0 == "w7"),
            "sibling keeps its allocated share beside the overlay"
        );
        let writable = super::writable_tokens(&state, &output, &active, &observed);
        assert!(
            writable.contains("w7"),
            "sibling takes the production tiled write"
        );
        assert!(
            !writable.contains("w8"),
            "retained-only overlay takes no geometry write: no native fight"
        );

        // Repeated maximized signals settle: reassembly is stable, still no
        // attempt arms, still no writable fight.
        let slot = state.member_rects.get("w8").copied().expect("slot");
        let again = assemble(&mut state, &output, &active, &observed, &retained);
        assert_eq!(again.len(), 2);
        assert_eq!(
            state.member_rects.get("w8").copied(),
            Some(slot),
            "repeat signals must not move the reserved slot"
        );
        assert!(state.maximize_admission_attempted.is_empty());
        assert!(
            !super::writable_tokens(&state, &output, &active, &observed).contains("w8"),
            "repeat signals must not fight native state"
        );

        // Native restore refetches ordinary beside the sibling and lands
        // EXACTLY the reserved Engine geometry on the next Engine round.
        let refetched = vec![
            observed_normal(100, "w7", SIBLING),
            observed_normal(101, "w8", RESTORED),
        ];
        let rows = assemble(&mut state, &output, &active, &refetched, &[]);
        assert_eq!(rows.len(), 2);
        let revision = accepted_revision(&state, &output, &active);
        let writes = reconcile(&mut state, &output, &active, &rows, revision);
        let landed = writes
            .iter()
            .find(|w| w.window.0 == "w8")
            .expect("restored tile")
            .rect;
        assert_eq!(
            landed, reserved,
            "restore must land exactly the reserved slot"
        );
        assert_eq!(
            super::writable_tokens(&state, &output, &active, &refetched),
            std::collections::HashSet::from(["w7".to_owned(), "w8".to_owned()]),
            "restored member rejoins production writes beside its sibling"
        );
    }

    #[test]
    fn floating_first_retile_clears_under_both_values() {
        // R-MAX-03 invariant: a floating-first admission never latches, so
        // its first tiled admission keeps the existing one-shot clear under
        // BOTH setting values, including a flip while it sits slotless.
        let (mut state, output, active) = harness(false);
        state.settings_live = Some(live_with(BORN_MAXIMIZED_OVERLAY));
        let max = admit_slotless(&mut state, &output, &active, 101, "w8");
        assert!(
            !state.born_keep_overlay.contains(&max),
            "R-MAX-03 never latches"
        );
        // Floating assembly keeps it slotless (existing outcome).
        let retained = vec![maximized_row(&max, "w8", NATIVE_MAX, true)];
        let rows = assemble(&mut state, &output, &active, &[], &retained);
        assert_eq!(rows.len(), 1);
        assert!(
            !state.member_rects.contains_key("w8"),
            "floating maximum stays slotless"
        );
        // Retile: the still-slotless maximum clears under both live values.
        assert!(state.workspaces.set_tiled(&output, &active, true));
        for born in [BORN_MAXIMIZED_TILE, BORN_MAXIMIZED_OVERLAY] {
            state.settings_live = Some(live_with(born));
            assert!(
                crate::tiling::should_clear_maximize_at_admission_with_mode(
                    false,
                    true,
                    state.member_rects.contains_key("w8"),
                    false,
                    true,
                    state.born_keep_overlay.contains(&max)
                ),
                "R-MAX-03 retile must clear under {born}"
            );
        }
        assert!(state.maximize_admission_attempted.is_empty());
        assert!(state.born_keep_overlay.is_empty());
    }

    #[test]
    fn overlay_hwmd_reuse_stays_lifetime_safe() {
        // A recycled HWND mints a fresh lifetime: the stale member drops all
        // runtime state (slot included) and the new occupant starts slotless,
        // so no retained overlay leaks across windows.
        let mut state = test_state();
        let output = "mon-1".to_owned();
        state.workspaces.ensure_output(&output);
        let active = state.workspaces.active_id(&output).expect("active");
        state.workspaces.set_tiled(&output, &active, true);
        let dead = crate::workspace::WindowKey {
            hwnd: 101,
            pid: 7,
            creation: "creation-old".to_owned(),
        };
        assert!(
            state
                .workspaces
                .assign(dead.clone(), &output, &active, false)
        );
        state.member_tokens.insert(dead.clone(), "w8".to_owned());
        state.member_rects.insert(
            "w8".to_owned(),
            tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 400,
                h: 300,
            },
        );
        state.born_keep_overlay.insert(dead.clone());
        let fresh = crate::workspace::WindowKey {
            hwnd: 101,
            pid: 9,
            creation: "creation-new".to_owned(),
        };
        let stale = crate::workspace_owner::reused_hwnd_stale(
            &state.member_tokens.keys().cloned().collect::<Vec<_>>(),
            &state.hidden_claims.keys().cloned().collect(),
            &fresh,
        );
        assert_eq!(stale, vec![dead.clone()]);
        for dead in stale {
            super::drop_member_state(&mut state, &dead);
        }
        assert!(!state.member_tokens.contains_key(&dead));
        assert!(
            !state.born_keep_overlay.contains(&dead),
            "latch drops with the lifetime"
        );
        assert!(
            !state.member_rects.contains_key("w8"),
            "reused lifetime must not inherit the overlay slot"
        );
        assert!(state.workspaces.member_loc(&fresh).is_none());
    }

    #[test]
    fn born_fullscreen_exit_and_later_maxima_stay_independent() {
        use crate::tiling::should_clear_maximize_at_admission_with_mode as gate;
        // Later maxima (retained slot) never clear under either latch state.
        for keep in [false, true] {
            assert!(!gate(false, true, true, false, true, keep));
        }
        // Born-fullscreen admission stages no latch (production
        // `admit_born_fullscreen` never touches the set): after the real
        // production release drops its seed, the exit to maximized is
        // slotless and clears under both live values.
        let (mut state, output, active) = harness(true);
        state.settings_live = Some(live_with(BORN_MAXIMIZED_OVERLAY));
        // The production release below logs its once-per-lifecycle line:
        // point the owner log at system temp and remove it afterwards, so the
        // hermetic run leaves no repo residue.
        let log_path = std::env::temp_dir().join(format!(
            "tiler-born-release-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        state.log_path = log_path.clone();
        let key = max_key(101);
        assert!(
            state
                .workspaces
                .assign(key.clone(), &output, &active, false)
        );
        state.member_tokens.insert(key.clone(), "w8".to_owned());
        state.member_rects.insert("w8".to_owned(), RESTORED);
        state.born_fullscreen.insert(key.clone());
        assert!(!state.born_keep_overlay.contains(&key));
        super::track_fullscreen_holds(&mut state, &[], &[]);
        assert!(!state.born_fullscreen.contains(&key));
        assert!(
            !state.member_rects.contains_key("w8"),
            "release drops the seed"
        );
        for born in [BORN_MAXIMIZED_TILE, BORN_MAXIMIZED_OVERLAY] {
            state.settings_live = Some(live_with(born));
            assert!(
                gate(
                    false,
                    true,
                    false,
                    false,
                    true,
                    state.born_keep_overlay.contains(&key)
                ),
                "born-fullscreen exit clears under {born}"
            );
        }
        let _ = std::fs::remove_file(&log_path);
    }

    #[test]
    fn apply_born_only_returns_false_adopts_and_rebuilds_nothing() {
        use crate::settings::{build_disabled, build_remap, tile_options_from_settings};
        use tiler_core::boundary::{CoreCommand, CoreReply};
        use tiler_core::directional::WindowId;
        use tiler_core::geometry::Rect;
        use tiler_core::ids::CorrelationId;

        // Scratch home for the ledger lock and the owner log line: never the
        // repo or the live product directory. No window, hook, suspend, or
        // registry effect exists on this path (every native lane below is
        // aligned away); the temp dir is removed at the end.
        let tmp = std::env::temp_dir().join(format!(
            "tiler-born-max-live-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
        let mut state = test_state();
        state.log_path = tmp.join("owner.log");
        // Align every lane with the defaults so the incoming document differs
        // in exactly one field.
        let base = tile_options_from_settings(&Settings::default());
        state.border = base.border;
        state.underlay = base.underlay;
        state.keyboard = crate::snapkey::KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        };
        state.last_remap = build_remap(&Settings::default());
        state.last_disabled = build_disabled(&Settings::default());
        state.settings_live = Some(LiveSettings::fresh(Settings::default(), None));
        let mut snap_want = true;
        // Retained session: two tiled windows, so "no rebuild" is observable
        // instead of vacuous.
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 1600,
            h: 900,
        };
        let domain = crate::workspace_owner::workspace_domain("mon-a", "ws-1", bounds, 8);
        let seed_fp =
            crate::tiling::fingerprint(&[("w1".to_owned(), bounds), ("w2".to_owned(), bounds)]);
        let seed = crate::tiling::build_reconcile_event_for(
            &state.owner,
            &state.generation,
            &CorrelationId::parse("live-seed").expect("correlation"),
            0,
            seed_fp,
            &domain.0,
            &domain.1,
            8,
            &[
                (
                    WindowId("w1".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                ),
                (
                    WindowId("w2".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                ),
            ],
            Some(&WindowId("w2".to_owned())),
        );
        let reply = state.engine.handle(&seed);
        assert!(
            matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
            "seed must converge, got {reply:?}"
        );
        // A move command builds here exactly like `keyboard_tick` builds it.
        let moved = {
            let mut event = seed.clone();
            event.revision = state
                .engine
                .session(&domain.1)
                .map(|session| session.accepted_revision())
                .unwrap_or(0);
            event.focused_window = WindowId("w2".to_owned());
            event.command = CoreCommand::Move {
                window: "w2".to_owned(),
                direction: "left".to_owned(),
                cross_output_transfer: false,
                same_axis_move: super::same_axis_move_for(state.settings_live.as_ref()),
            };
            state.engine.handle(&event)
        };
        assert!(
            matches!(moved, CoreReply::MoveDirectional(_)),
            "pre-apply move must plan, got {moved:?}"
        );
        let before = state.engine.session(&domain.1).expect("session").snapshot();
        let me = crate::model::ProcessIdentity {
            pid: 4242,
            process_creation: "test-creation".to_owned(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
            exe_path: "test.exe".to_owned(),
        };
        let mut incoming = Settings::default();
        incoming.core.born_maximized = BORN_MAXIMIZED_OVERLAY.to_owned();
        incoming.revision = 7;
        let changed = super::apply_live_settings(
            &mut state,
            &tmp,
            &me,
            &store,
            &mut snap_want,
            incoming,
            None,
        );
        assert!(
            !changed,
            "born-maximized-only adoption must report no owner change"
        );
        assert_eq!(
            state.engine.session(&domain.1).expect("session").snapshot(),
            before,
            "retained session untouched: no rebuild, no resync"
        );
        let live = state.settings_live.as_ref().expect("live");
        assert_eq!(live.settings.core.born_maximized, BORN_MAXIMIZED_OVERLAY);
        assert_eq!(live.status, "saved:7");
        assert!(
            born_maximized_keep_overlay_for(state.settings_live.as_ref()),
            "subsequent admissions use the adopted setting"
        );
        assert!(
            state.born_keep_overlay.is_empty(),
            "setting-only Apply binds no lifetime"
        );
        assert_eq!((state.inner_gap, state.outer_gap), (8, 8));
        drop(store);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

#[cfg(test)]
mod send_preflight_tests {
    use super::SendPreflight;
    use crate::workspace_owner::SendFenceSnapshot;

    fn preflight(maximized: bool, fullscreen: bool, fallback: bool) -> SendPreflight {
        SendPreflight {
            fences: SendFenceSnapshot {
                inner_gap: 8,
                outer_gap: 8,
                source_tiled: true,
                target_tiled: false,
                overlay_maximized: maximized,
                overlay_fullscreen: fullscreen,
            },
            fallback_fullscreen: fallback,
        }
    }

    #[test]
    fn overlay_hold_covers_pre_transfer_gate_without_touching_windows() {
        // Null handle reads (false, None) with no setters and no crash. This
        // is the exact production-called gate the native pre-assign check
        // runs before `assign` (mirroring the Engine pre-assign order), so an
        // observed mover that went borderless fullscreen refuses with NO
        // membership change; the gate/assign ordering itself is straight-line
        // code in `workspace_do_send_native`, not a mock.
        let fulls = [tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        }];
        // Ordinary snapshot, unreadable frame, no fallback: holds.
        assert_eq!(
            preflight(false, false, false).overlay_hold(0, &fulls),
            Ok(())
        );
        // Unreadable frame with a fullscreen fallback refuses (the retained
        // native-boundary case): no transfer, no hide, no focus.
        assert_eq!(
            preflight(true, false, true).overlay_hold(0, &fulls),
            Err("send-refused-fullscreen")
        );
        // Ordinary snapshot with a fullscreen fallback refuses the newly
        // arrived fullscreen the same way.
        assert_eq!(
            preflight(false, false, true).overlay_hold(0, &fulls),
            Err("send-refused-fullscreen")
        );
        // Retained snapshot against a null live read defers drift with no
        // restore.
        assert_eq!(
            preflight(true, false, false).overlay_hold(0, &fulls),
            Err("deferred")
        );
        // Item 9: a fullscreen snapshot with an unreadable live flag defers
        // pre-effect instead of trusting the fallback (readable stable
        // fullscreen still carries through the portable gate, pinned there).
        assert_eq!(
            preflight(false, true, true).overlay_hold(0, &fulls),
            Err("deferred")
        );
        assert_eq!(
            preflight(false, true, false).overlay_hold(0, &fulls),
            Err("deferred")
        );
    }
}

#[cfg(test)]
mod border_shell_tests {
    use super::*;

    #[test]
    fn applet_analogues_excluded_without_blanket_dialog_tool_owned() {
        for class in [
            "MultitaskingViewFrame",
            "TaskSwitcherWnd",
            "NotifyIconOverflowWindow",
            "Shell_Flyout",
            "Progman",
            "MSCTFIME UI",
            "SystemTray_Main",
        ] {
            assert!(is_border_shell_popup(class, "explorer.exe"), "{class}");
        }
        assert!(is_border_shell_popup(
            "Windows.UI.Core.CoreWindow",
            "startmenuexperiencehost.exe"
        ));
        assert!(is_border_shell_popup(
            "XamlExplorerHostIslandWindow",
            "searchhost.exe"
        ));
        assert!(!is_border_shell_popup(
            "Windows.UI.Core.CoreWindow",
            "explorer.exe"
        ));
        assert!(!is_border_shell_popup(
            "Windows.UI.Core.CoreWindow",
            "applicationframehost.exe"
        ));
        assert!(!is_border_shell_popup("Notepad", "notepad.exe"));
        assert!(!is_border_shell_popup("#32770", "notepad.exe"));
        assert!(!is_border_shell_popup("MSPaintApp", "mspaint.exe"));
        assert!(!is_border_shell_popup("CabinetWClass", "explorer.exe"));
    }
}

#[cfg(test)]
mod preview_tests {
    use super::{
        PreviewBound, PreviewFresh, PreviewGate, preview_hide_for_refusal, preview_sample_gate,
    };

    #[test]
    fn sample_gate_needs_real_unlatched_movement() {
        // Zero movement (title click, stationary-hold start point) never
        // shows; an Esc-latched hold never shows even when moved.
        assert!(!PreviewBound::sample_allowed(10, 10, 10, 10, false));
        assert!(!PreviewBound::sample_allowed(10, 10, 12, 10, true));
        assert!(!PreviewBound::sample_allowed(10, 10, 10, 10, true));
        assert!(PreviewBound::sample_allowed(10, 10, 11, 10, false));
        assert!(PreviewBound::sample_allowed(10, 10, 10, 11, false));
    }

    fn fresh_ok() -> PreviewFresh {
        PreviewFresh {
            dead: false,
            zero: false,
            identity_ok: true,
            tag_ok: true,
            token_ok: true,
            loc_ok: true,
            revision_ok: true,
            float_hold: false,
            inside: true,
        }
    }

    fn gate_outcome(fresh: &PreviewFresh) -> &'static str {
        match preview_sample_gate(fresh) {
            PreviewGate::Proceed => "proceed",
            PreviewGate::Transient(reason) => reason,
            PreviewGate::Dead(reason) => reason,
        }
    }

    #[test]
    fn gate_proceeds_only_when_every_fence_holds() {
        assert_eq!(gate_outcome(&fresh_ok()), "proceed");
    }

    #[test]
    fn gate_invalidation_dominates_a_static_pointer() {
        // Same pointer (zero) plus identity loss still fails closed: a
        // lingering rectangle must never survive mover death or reuse on a
        // stationary pointer.
        for fresh in [
            PreviewFresh {
                zero: true,
                identity_ok: false,
                ..fresh_ok()
            },
            PreviewFresh {
                zero: true,
                tag_ok: false,
                ..fresh_ok()
            },
            PreviewFresh {
                zero: true,
                token_ok: false,
                ..fresh_ok()
            },
        ] {
            assert_eq!(gate_outcome(&fresh), "identity-changed");
        }
        // Float/sticky capture fails closed too, even unmoved.
        assert_eq!(
            gate_outcome(&PreviewFresh {
                zero: true,
                float_hold: true,
                ..fresh_ok()
            }),
            "floating"
        );
    }

    #[test]
    fn gate_drift_before_first_sample_fails_closed() {
        // Output/workspace drift and revision drift fail the whole gesture
        // from the very first sample (START binding is authoritative; the
        // refresh never rebinds to current membership).
        assert_eq!(
            gate_outcome(&PreviewFresh {
                loc_ok: false,
                ..fresh_ok()
            }),
            "drift"
        );
        assert_eq!(
            gate_outcome(&PreviewFresh {
                revision_ok: false,
                ..fresh_ok()
            }),
            "revision-drift"
        );
    }

    #[test]
    fn gate_dead_repeat_can_never_rebind() {
        // A drifted gesture stays dead on repeat samples even when every
        // fresh fact reads healthy again.
        assert_eq!(
            gate_outcome(&PreviewFresh {
                dead: true,
                ..fresh_ok()
            }),
            "dead"
        );
    }

    #[test]
    fn gate_zero_and_outside_hide_transiently() {
        // Zero movement and outside-work-area hide without killing the
        // gesture: the next moved sample may still show.
        assert_eq!(
            gate_outcome(&PreviewFresh {
                zero: true,
                ..fresh_ok()
            }),
            "zero"
        );
        assert_eq!(
            gate_outcome(&PreviewFresh {
                inside: false,
                ..fresh_ok()
            }),
            "outside"
        );
    }

    #[test]
    fn frame_gate_matches_either_lane_and_rejects_resizes() {
        use super::preview_same_size;
        // Pure move: both lanes match (no shadows).
        assert!(preview_same_size(800, 600, 800, 600, 800, 600));
        // Shadow padding shifts only the outer lane: the visible lane
        // still matches, so a move is not misread as a resize.
        assert!(preview_same_size(800, 600, 816, 616, 800, 600));
        // Outer-fallback pre-gesture rect matches the outer lane.
        assert!(preview_same_size(816, 616, 816, 616, 800, 600));
        // Genuine resizes move both lanes: width-only, height-only, both,
        // and shrinkage all fail the move gate (preview stays hidden).
        assert!(!preview_same_size(800, 600, 900, 600, 884, 600));
        assert!(!preview_same_size(800, 600, 800, 700, 800, 684));
        assert!(!preview_same_size(800, 600, 900, 700, 884, 684));
        assert!(!preview_same_size(800, 600, 700, 500, 684, 484));
        // Degenerate live frames never match a real pre-gesture rect.
        assert!(!preview_same_size(800, 600, 0, 0, 0, 0));
    }

    #[test]
    fn prior_descriptor_is_closed_without_raw_ids() {
        use tiler_core::directional::{NodeId, OutputId, WindowId, WorkspaceId};
        use tiler_core::session::{DomainKey, DragHoverPrior};
        // No prior at all, and a cleared (non-group-edge) hover, both
        // describe as `none`.
        assert_eq!(super::preview_prior_desc(&None), "none");
        let cleared = DragHoverPrior {
            domain: DomainKey {
                output: OutputId("mon-a".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
            },
            source_leaf: NodeId("leaf-w1".to_owned()),
            source_window: WindowId("w1".to_owned()),
            revision: 3,
            prior: None,
        };
        assert_eq!(super::preview_prior_desc(&Some(cleared)), "none");
        // A sticky group-edge carry describes only its side: the group
        // NodeId never reaches the log.
        let sticky = DragHoverPrior {
            domain: DomainKey {
                output: OutputId("mon-a".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
            },
            source_leaf: NodeId("leaf-w1".to_owned()),
            source_window: WindowId("w1".to_owned()),
            revision: 3,
            prior: Some(tiler_core::policy::PriorGroupEdge {
                group: NodeId("grp-secret".to_owned()),
                edge: tiler_core::contract::DragSide::Top,
            }),
        };
        let desc = super::preview_prior_desc(&Some(sticky));
        assert_eq!(desc, "group-edge:top");
        assert!(!desc.contains("grp-secret"));
    }

    #[test]
    fn refusal_kinds_map_to_no_rectangle_hides() {
        // Self, centre-stack, and outside refusals show no rectangle; every
        // other kind rides through for attribution.
        assert_eq!(preview_hide_for_refusal("unchanged"), "self");
        assert_eq!(preview_hide_for_refusal("unsupported-capability"), "centre");
        assert_eq!(preview_hide_for_refusal("cross-domain-mismatch"), "outside");
        assert_eq!(
            preview_hide_for_refusal("domain-mismatch"),
            "domain-mismatch"
        );
    }

    /// Same-output preview/drop agreement through the exact production
    /// command shapes (`DragPreview` samples with a carried `hover_prior`
    /// into a `DragDrop` with that prior): the final mover geometry equals
    /// the preview's proposed rect, previews never store, and
    /// centre/self/outside refuse with no plan. Fresh size hints ride both
    /// paths identically (same row assembly, same resolver).
    #[test]
    fn preview_samples_agree_with_final_drop() {
        use tiler_core::boundary::{CoreCommand, CoreEvent, CoreReply};
        use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
        use tiler_core::engine::Engine;
        use tiler_core::geometry::Rect;
        use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
        use tiler_core::session::OutputDomain;

        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("preview-1").expect("generation");
        let mut engine = Engine::new();
        engine.sync_binding(&owner, &generation);
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        };
        let domain = OutputDomain {
            id: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            bounds,
            gap: 8,
            adjacent: std::collections::BTreeMap::new(),
        };
        let window = |token: &str, min_w: Option<i32>| tiler_core::seed::EngineWindow {
            window: WindowId(token.to_owned()),
            output: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            rect: bounds,
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: tiler_core::size_hints::WindowSizeHints {
                min_w,
                min_h: None,
                max_w: None,
                max_h: None,
            },
        };
        let event = |command: CoreCommand,
                     correlation: &str,
                     windows: Vec<tiler_core::seed::EngineWindow>| {
            CoreEvent {
                owner: owner.clone(),
                generation: generation.clone(),
                correlation: CorrelationId::parse(correlation).expect("correlation"),
                revision: 0,
                fingerprint: 7,
                domain: domain.clone(),
                domain_key: domain.key(),
                outer_gap: 8,
                focused_window: WindowId("w1".to_owned()),
                windows,
                directional: None,
                directional_target_outer_gap: None,
                target_domain: None,
                target_windows: Vec::new(),
                command,
            }
        };
        let rows = || vec![window("w1", Some(200)), window("w2", None)];
        // Converge once to read the authoritative projected layout: the
        // edge sample below derives from Engine geometry, never a guessed
        // half-window rectangle.
        let geometry =
            match engine.handle(&event(CoreCommand::Reconcile, "corr-preview-seed", rows())) {
                CoreReply::Projection(plan) => plan
                    .geometry
                    .iter()
                    .map(|g| (g.window.0.clone(), g.rect))
                    .collect::<Vec<_>>(),
                CoreReply::Tiled(plan) => plan
                    .geometry
                    .iter()
                    .map(|g| (g.window.0.clone(), g.rect))
                    .collect::<Vec<_>>(),
                other => panic!("seed must converge, got {other:?}"),
            };
        let target = geometry
            .iter()
            .find(|(token, _)| token == "w2")
            .map(|(_, rect)| *rect)
            .expect("sibling geometry");
        let mover_rect = geometry
            .iter()
            .find(|(token, _)| token == "w1")
            .map(|(_, rect)| *rect)
            .expect("mover geometry");
        // Two pixels inside the sibling's left edge: a window-edge target
        // that changes topology (insert before), from Engine geometry.
        let edge = (target.x + 2, target.y + target.h / 2);
        let revision = engine
            .session(&domain.key())
            .map(|s| s.accepted_revision())
            .unwrap_or(0);
        let preview_cmd = |x: i32, y: i32, prior: Option<tiler_core::session::DragHoverPrior>| {
            CoreCommand::DragPreview {
                window: "w1".to_owned(),
                x,
                y,
                hover_prior: prior,
                source: None,
            }
        };
        let (proposed, prior) = match engine.handle(&event(
            preview_cmd(edge.0, edge.1, None),
            "corr-preview-1",
            rows(),
        )) {
            CoreReply::DragPreview(plan) => {
                assert_eq!(plan.base_revision, revision);
                (plan.preview.proposed_rect, plan.preview.hover_prior())
            }
            other => panic!("edge preview must resolve, got {other:?}"),
        };
        assert_ne!(proposed, mover_rect, "edge drop must re-place the mover");
        // The preview stores nothing: revision and topology untouched.
        assert_eq!(
            engine.session(&domain.key()).map(|s| s.accepted_revision()),
            Some(revision)
        );
        // A second sample carrying the exact prior resolves identically:
        // preview/drop agreement with the sticky carry in place.
        let (proposed2, prior2) = match engine.handle(&event(
            preview_cmd(edge.0, edge.1, Some(prior.clone())),
            "corr-preview-2",
            rows(),
        )) {
            CoreReply::DragPreview(plan) => {
                (plan.preview.proposed_rect, plan.preview.hover_prior())
            }
            other => panic!("carried preview must resolve, got {other:?}"),
        };
        assert_eq!(proposed2, proposed, "carried prior keeps the target");
        // Rejected points show no rectangle: centre-stack, self, outside.
        // These run before the drop mutates the layout, so the sampled
        // points still name the intended targets.
        let centre = (target.x + target.w / 2, target.y + target.h / 2);
        match engine.handle(&event(
            preview_cmd(centre.0, centre.1, None),
            "corr-preview-centre",
            rows(),
        )) {
            CoreReply::Rejected { kind, .. } => {
                assert_eq!(preview_hide_for_refusal(kind), "centre")
            }
            other => panic!("centre preview must refuse, got {other:?}"),
        }
        let itself = (
            mover_rect.x + mover_rect.w / 2,
            mover_rect.y + mover_rect.h / 2,
        );
        match engine.handle(&event(
            preview_cmd(itself.0, itself.1, None),
            "corr-preview-self",
            rows(),
        )) {
            CoreReply::Rejected { kind, .. } => assert_eq!(preview_hide_for_refusal(kind), "self"),
            other => panic!("self preview must refuse, got {other:?}"),
        }
        match engine.handle(&event(
            preview_cmd(-50, -50, None),
            "corr-preview-outside",
            rows(),
        )) {
            CoreReply::Rejected { kind, .. } => {
                assert_eq!(preview_hide_for_refusal(kind), "outside")
            }
            other => panic!("outside preview must refuse, got {other:?}"),
        }
        // The final drop forwards the carried prior and lands exactly on
        // the preview's proposed rect.
        match engine.handle(&event(
            CoreCommand::DragDrop {
                window: "w1".to_owned(),
                x: edge.0,
                y: edge.1,
                hover_prior: Some(prior2),
                source: None,
            },
            "corr-preview-drop",
            rows(),
        )) {
            CoreReply::Tiled(plan) => {
                let placed = plan
                    .geometry
                    .iter()
                    .find(|g| g.window.0 == "w1")
                    .expect("mover placement");
                assert_eq!(placed.rect, proposed, "drop must equal preview");
            }
            other => panic!("carried drop must plan, got {other:?}"),
        }
    }

    /// Sticky group-edge journey through the exact production command
    /// shapes: adopt three windows, sample a group-gap strip inside the
    /// 32px top-edge zone (GroupEdge with a sticky `Some` prior), step away
    /// past 32px but inside the 80px sticky depth (same GroupEdge only
    /// because the exact prior carried), then drop with that prior and land
    /// exactly on the preview rect. Gap geometry derives from Engine
    /// geometry, never hardcoded topology.
    #[test]
    fn preview_sticky_journey_agrees_with_drop() {
        use tiler_core::boundary::{CoreCommand, CoreEvent, CoreReply};
        use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
        use tiler_core::engine::Engine;
        use tiler_core::geometry::Rect;
        use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
        use tiler_core::session::OutputDomain;

        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("preview-sticky").expect("generation");
        let mut engine = Engine::new();
        engine.sync_binding(&owner, &generation);
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 900,
            h: 600,
        };
        let domain = OutputDomain {
            id: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            bounds,
            gap: 8,
            adjacent: std::collections::BTreeMap::new(),
        };
        let window = |token: &str| tiler_core::seed::EngineWindow {
            window: WindowId(token.to_owned()),
            output: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            rect: bounds,
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
        };
        let event = |command: CoreCommand,
                     correlation: &str,
                     windows: Vec<tiler_core::seed::EngineWindow>| {
            CoreEvent {
                owner: owner.clone(),
                generation: generation.clone(),
                correlation: CorrelationId::parse(correlation).expect("correlation"),
                revision: 0,
                fingerprint: 7,
                domain: domain.clone(),
                domain_key: domain.key(),
                outer_gap: 8,
                focused_window: WindowId("w1".to_owned()),
                windows,
                directional: None,
                directional_target_outer_gap: None,
                target_domain: None,
                target_windows: Vec::new(),
                command,
            }
        };
        let rows = || vec![window("w1"), window("w2"), window("w3")];
        let geometry =
            match engine.handle(&event(CoreCommand::Reconcile, "corr-sticky-seed", rows())) {
                CoreReply::Projection(plan) => plan
                    .geometry
                    .iter()
                    .map(|g| (g.window.0.clone(), g.rect))
                    .collect::<Vec<_>>(),
                CoreReply::Tiled(plan) => plan
                    .geometry
                    .iter()
                    .map(|g| (g.window.0.clone(), g.rect))
                    .collect::<Vec<_>>(),
                other => panic!("seed must converge, got {other:?}"),
            };
        // Inter-tile gap strip from Engine geometry: adjacent pair
        // overlapping in y with a real x gap.
        let mut rects: Vec<Rect> = geometry.iter().map(|(_, r)| *r).collect();
        rects.sort_by_key(|r| r.x);
        let mut gap = None;
        for pair in rects.windows(2) {
            let (r1, r2) = (pair[0], pair[1]);
            if r1.x + r1.w < r2.x && r1.y < r2.y + r2.h && r2.y < r1.y + r1.h {
                let top = r1.y.max(r2.y);
                let bot = (r1.y + r1.h).min(r2.y + r2.h);
                if bot - top >= 120 {
                    gap = Some(((r1.x + r1.w + r2.x) / 2, top));
                    break;
                }
            }
        }
        let (gap_x, strip_top) = gap.expect("inter-tile gap strip in 3-window layout");
        let leg1 = (gap_x, strip_top + 12);
        let leg2 = (gap_x, strip_top + 50);
        let preview_at = |x: i32,
                          y: i32,
                          prior: Option<tiler_core::session::DragHoverPrior>,
                          correlation: &str| {
            event(
                CoreCommand::DragPreview {
                    window: "w1".to_owned(),
                    x,
                    y,
                    hover_prior: prior,
                    source: None,
                },
                correlation,
                rows(),
            )
        };
        let (proposed1, prior1) =
            match engine.handle(&preview_at(leg1.0, leg1.1, None, "corr-sticky-1")) {
                CoreReply::DragPreview(plan) => {
                    assert!(
                        plan.preview.hover_prior().prior.is_some(),
                        "gap leg must set a sticky prior, got {:?}",
                        plan.preview
                    );
                    (plan.preview.proposed_rect, plan.preview.hover_prior())
                }
                other => panic!("gap leg must resolve a group edge, got {other:?}"),
            };
        // Past the 32px zone but inside the 80px sticky depth: the same
        // GroupEdge resolves only through the carried prior. Contrast:
        // without the prior the same point must NOT return the sticky
        // result (refusal or a different interior placement).
        let no_prior_sticky =
            match engine.handle(&preview_at(leg2.0, leg2.1, None, "corr-sticky-noprior")) {
                CoreReply::DragPreview(plan) => {
                    plan.preview.proposed_rect == proposed1
                        && plan.preview.hover_prior().prior.is_some()
                }
                _ => false,
            };
        assert!(
            !no_prior_sticky,
            "leg2 without prior must not reproduce the sticky target"
        );
        let (proposed2, prior2) = match engine.handle(&preview_at(
            leg2.0,
            leg2.1,
            Some(prior1.clone()),
            "corr-sticky-2",
        )) {
            CoreReply::DragPreview(plan) => {
                (plan.preview.proposed_rect, plan.preview.hover_prior())
            }
            other => panic!("sticky leg must resolve, got {other:?}"),
        };
        assert_eq!(proposed2, proposed1, "sticky leg keeps the target");
        assert_eq!(
            super::preview_prior_desc(&Some(prior2.clone())),
            super::preview_prior_desc(&Some(prior1.clone())),
            "sticky prior carries unchanged"
        );
        match engine.handle(&event(
            CoreCommand::DragDrop {
                window: "w1".to_owned(),
                x: leg2.0,
                y: leg2.1,
                hover_prior: Some(prior2),
                source: None,
            },
            "corr-sticky-drop",
            rows(),
        )) {
            CoreReply::Tiled(plan) => {
                let placed = plan
                    .geometry
                    .iter()
                    .find(|g| g.window.0 == "w1")
                    .expect("mover placement");
                assert_eq!(placed.rect, proposed1, "sticky drop must equal preview");
            }
            other => panic!("sticky drop must plan, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod rmax03_adapter_tests {
    use super::{
        HintCx, RetainedRow, TileLoop, assemble_domain_rows, workspace_mode_known, writable_tokens,
    };

    /// Shared proof-style owner harness (no HWNDs, hooks, or live effects).
    /// `pub(super)` so the R-MOV-03 live-adoption test reuses it directly.
    pub(super) fn test_state() -> TileLoop {
        use tiler_core::ids::{GenerationId, OwnerId};
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("rmax03-adapter").expect("generation");
        let mut state = TileLoop {
            engine: tiler_core::engine::Engine::new(),
            owner: owner.clone(),
            generation: generation.clone(),
            tokens: crate::tiling::TokenMap::default(),
            refused: crate::tiling::RefusedTracker::default(),
            stable: std::collections::HashMap::new(),
            managed: std::collections::HashSet::new(),
            active: std::collections::HashSet::new(),
            gesture_before: std::collections::HashMap::new(),
            gesture_end_cursor: std::collections::HashMap::new(),
            gesture_start_key: std::collections::HashMap::new(),
            gesture_start_tag: std::collections::HashMap::new(),
            tick: 0,
            allowlist: None,
            scope: Vec::new(),
            scope_hosts: Vec::new(),
            trace: false,
            suspended: false,
            last_summary: None,
            log_path: std::path::PathBuf::from("test"),
            audit_path: None,
            keyboard: crate::snapkey::KeyboardConfig::disabled(),
            inner_gap: crate::tiling::INNER_GAP,
            outer_gap: crate::tiling::OUTER_GAP,
            settings_live: None,
            settings_dir: None,
            last_remap: Vec::new(),
            last_disabled: Vec::new(),
            cli_overrides: crate::tiling::CliOverrides::default(),
            snap_dropped: 0,
            cb_diag_dropped: 0,
            cb_diag_filtered: 0,
            mask_sends: 0,
            mask_send_max_us: 0,
            snap_origins: std::collections::HashMap::new(),
            windrag_origins: std::collections::HashMap::new(),
            gesture_producer: std::collections::HashMap::new(),
            windrag_start_cursor: std::collections::HashMap::new(),
            windrag_bound: std::collections::HashMap::new(),
            windrag_dropped: 0,
            windrag_origin_logged: None,
            windrag_stats_logged: (0, 0, 0, 0),
            snap_advance: None,
            resize_repeat: None,
            last_enumerated: 0,
            workspaces: crate::workspace::ManagedWorkspaces::new(),
            member_tokens: std::collections::BTreeMap::new(),
            member_rects: std::collections::HashMap::new(),
            hidden_claims: std::collections::BTreeMap::new(),
            member_identity: std::collections::BTreeMap::new(),
            member_tags: std::collections::BTreeMap::new(),
            active_output: String::new(),
            workspace_proof: false,
            maximize_admission_attempted: std::collections::HashSet::new(),
            born_keep_overlay: std::collections::BTreeSet::new(),
            born_fullscreen: std::collections::BTreeSet::new(),
            seen_nonfullscreen: std::collections::BTreeSet::new(),
            last_foreground: 0,
            known_outputs: Vec::new(),
            last_hwnds: std::collections::HashSet::new(),
            last_areas: Vec::new(),
            hint_logged: std::collections::HashMap::new(),
            restore_wake: None,
            border: crate::active_border::ActiveBorderOptions::default(),
            border_overlay: crate::active_border_sys::BorderOverlay::default(),
            border_last: None,
            underlay: crate::group_underlay::GroupUnderlayOptions::default(),
            underlay_overlay: crate::active_border_sys::UnderlayOverlay::default(),
            underlay_last: None,
            move_kind: std::collections::HashMap::new(),
            gesture_start_cursor: std::collections::HashMap::new(),
            preview_overlay: crate::active_border_sys::PreviewOverlay::default(),
            preview_last: None,
            preview_bound: std::collections::HashMap::new(),
            gesture_preview_start: std::collections::HashMap::new(),
            preview_dead: std::collections::HashSet::new(),
            esc_latched: std::collections::HashSet::new(),
            gesture_esc_seq: std::collections::HashMap::new(),
            gesture_end_seq: std::collections::HashMap::new(),
            underlay_chord_last: false,
            float_topmost_prev: std::collections::BTreeMap::new(),
            float_rects: std::collections::HashMap::new(),
            floated: std::collections::BTreeSet::new(),
            sticky: std::collections::BTreeMap::new(),
            pending_releases: crate::workspace::PendingReleases::new(),
            pending_retry_fp: 0,
        };
        state.engine.sync_binding(&owner, &generation);
        state
    }

    #[test]
    fn floating_retained_max_stays_slotless_then_tiled_seeds_and_plans() {
        use tiler_core::boundary::CoreReply;
        use tiler_core::directional::WindowId;
        use tiler_core::geometry::Rect;
        use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};

        let mut state = test_state();
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        };
        let native_max = Rect {
            x: -8,
            y: -8,
            w: 1936,
            h: 1056,
        };
        let restored = Rect {
            x: 120,
            y: 120,
            w: 800,
            h: 600,
        };
        let output = "mon-1".to_owned();
        state.workspaces.ensure_output(&output);
        let active = state.workspaces.active_id(&output).expect("active");
        assert!(state.workspaces.set_tiled(&output, &active, false));
        assert_eq!(workspace_mode_known(&state, &output, &active), Some(false));

        // Slotless admission outcome without native effects: production
        // `admit_slotless_maximized` membership plus stable token, deliberately
        // no `member_rects` seed, no Engine exception, no hold.
        let key = crate::workspace::WindowKey {
            hwnd: 101,
            pid: 7,
            creation: "creation-max".to_owned(),
        };
        assert!(
            state
                .workspaces
                .assign(key.clone(), &output, &active, false)
        );
        state.member_tokens.insert(key.clone(), "w8".to_owned());
        assert!(!state.member_rects.contains_key("w8"));

        // Floating release assembly over the retained maximum: the actual
        // production row path, not the pure `should_seed_member_slot`
        // predicate. Mutation oracle: removing the `should_seed` guard (always
        // seeding) inserts `w8` here and fails the assertion below.
        let retained = vec![RetainedRow {
            key: key.clone(),
            token: "w8".to_owned(),
            rect: Some(native_max),
            maximized: true,
            fullscreen: false,
            facts: Some(crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: true,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                dialog: false,
            }),
        }];
        let observed: Vec<super::ObservedWindow> = Vec::new();
        let mut hint_cx = HintCx::new();
        let rows = assemble_domain_rows(
            &mut state,
            &output,
            &active,
            &observed,
            &retained,
            "release",
            "rmax-03",
            &mut hint_cx,
        )
        .expect("floating rows assemble");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].token, "w8");
        assert!(
            !state.member_rects.contains_key("w8"),
            "floating retained maximum must stay slotless so the tiled admission clear still fires"
        );
        let (_, domain_key) = crate::workspace_owner::workspace_domain(
            &output,
            &active,
            bounds,
            crate::tiling::INNER_GAP,
        );
        assert!(state.engine.session(&domain_key).is_none());
        assert!(writable_tokens(&state, &output, &active, &observed).is_empty());

        // Toggle tiled, then refetch the restored frame into tiled assembly:
        // member-path seed plus an actual fresh Engine tile with an eligible
        // production writable token.
        assert!(state.workspaces.set_tiled(&output, &active, true));
        assert_eq!(workspace_mode_known(&state, &output, &active), Some(true));
        let refetched = vec![super::ObservedWindow {
            hwnd: 101,
            token: "w8".to_owned(),
            outer: restored,
            visible: restored,
            insets: crate::tiling::FrameInsets {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            identity: crate::tiling::ObservedTarget {
                hwnd: 101,
                pid: 7,
                process_creation: "creation-max".to_owned(),
                exe_path: "C:\\test\\app.exe".to_owned(),
                user_sid: "S-1".to_owned(),
                session_id: 1,
                tag: String::new(),
            },
            facts: crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: false,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                dialog: false,
            },
        }];
        let mut hint_cx = HintCx::new();
        let rows = assemble_domain_rows(
            &mut state,
            &output,
            &active,
            &refetched,
            &[],
            "reconcile",
            "rmax-03",
            &mut hint_cx,
        )
        .expect("tiled rows assemble");
        assert_eq!(rows.len(), 1);
        assert!(
            state.member_rects.contains_key("w8"),
            "tiled refetch must seed the tile slot"
        );
        let (domain, domain_key) = crate::workspace_owner::workspace_domain(
            &output,
            &active,
            bounds,
            crate::tiling::INNER_GAP,
        );
        let windows: Vec<(
            WindowId,
            Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )> = rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("rmax03-adapter").expect("generation");
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        let event = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &CorrelationId::parse("rmax-03").expect("correlation"),
            0,
            fp,
            &domain,
            &domain_key,
            crate::tiling::OUTER_GAP,
            &windows,
            None,
        );
        let reply = state.engine.handle(&event);
        let writes = crate::workspace_owner::planned_writes(&reply).expect("fresh tile plan");
        let tile = writes
            .iter()
            .find(|w| w.window.0 == "w8")
            .expect("tile contains the restored token");
        assert_ne!(tile.rect, native_max);
        assert_ne!(tile.rect, restored);
        assert!(
            matches!(reply, CoreReply::Tiled(_) | CoreReply::Projection(_)),
            "unexpected reply: {reply:?}"
        );
        assert_eq!(
            writable_tokens(&state, &output, &active, &refetched),
            std::collections::HashSet::from(["w8".to_owned()]),
            "restored member takes the production tiled write"
        );
    }

    #[test]
    fn select_departure_remembers_live_fullscreen_game_for_return() {
        // REQ-WS-09: the chord-time game origin still holds live foreground
        // at dispatch, so the source remembers it before hiding. A slotless
        // born-fullscreen game carries membership (`admit_born_fullscreen`)
        // and rides the retained overlay row, so the return `focus_target`
        // restores it while `writable_tokens` still excludes it. Table
        // negatives pin exact identity, source membership, and liveness.
        // Hermetic: fabricated keys/tokens/rows only.
        use std::collections::HashSet;
        let output = "mon-9".to_owned();
        let game_key = crate::workspace::WindowKey {
            hwnd: 501,
            pid: 7,
            creation: "creation-game".to_owned(),
        };
        let steam_key = crate::workspace::WindowKey {
            hwnd: 502,
            pid: 8,
            creation: "creation-steam".to_owned(),
        };
        let other_key = crate::workspace::WindowKey {
            hwnd: 503,
            pid: 9,
            creation: "creation-other".to_owned(),
        };
        let game_token = "tok-game".to_owned();
        let steam_token = "tok-steam".to_owned();
        let other_token = "tok-other".to_owned();
        let game_origin = crate::snapkey::SnapOrigin {
            hwnd: 501,
            token: game_token.clone(),
            pid: 7,
            creation: "creation-game".to_owned(),
        };
        let rect = tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        };
        let mk_observed =
            |hwnd: u64, token: String, pid: u32, creation: &str| super::ObservedWindow {
                hwnd,
                token,
                outer: rect,
                visible: rect,
                insets: crate::tiling::FrameInsets {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
                identity: crate::tiling::ObservedTarget {
                    hwnd,
                    pid,
                    process_creation: creation.to_owned(),
                    exe_path: "C:\\test\\app.exe".to_owned(),
                    user_sid: "S-1-5-test".to_owned(),
                    session_id: 1,
                    tag: String::new(),
                },
                facts: crate::tiling::WindowFacts {
                    visible: true,
                    minimized: false,
                    maximized: false,
                    cloaked: false,
                    elevated: false,
                    shell: false,
                    tool_window: false,
                    owned: false,
                    captionless_fullscreen: false,
                    no_activate: false,
                    dialog: false,
                },
            };
        let mk_game_row = || RetainedRow {
            key: game_key.clone(),
            token: game_token.clone(),
            rect: None,
            maximized: false,
            fullscreen: true,
            facts: None,
        };
        // Stale sibling memory on ws1 plus a bystander on ws2.
        let setup = || {
            let mut state = test_state();
            state.workspaces.ensure_output(&output);
            let ws1 = state.workspaces.active_id(&output).expect("active");
            let ws2 = state.workspaces.resolve_send(&output, 2).expect("ws2");
            assert!(
                state
                    .workspaces
                    .assign(game_key.clone(), &output, &ws1, false)
            );
            assert!(
                state
                    .workspaces
                    .assign(steam_key.clone(), &output, &ws1, false)
            );
            assert!(
                state
                    .workspaces
                    .assign(other_key.clone(), &output, &ws2, false)
            );
            state
                .member_tokens
                .insert(game_key.clone(), game_token.clone());
            state
                .member_tokens
                .insert(steam_key.clone(), steam_token.clone());
            state
                .member_tokens
                .insert(other_key.clone(), other_token.clone());
            state.workspaces.note_foreground(&steam_key);
            (state, ws1)
        };
        // Production return-target assembly: source members plus the
        // retained-inclusive fresh token union (focus only).
        let return_target = |state: &TileLoop,
                             ws1: &str,
                             observed: &[super::ObservedWindow],
                             retained: &[RetainedRow]| {
            let members = state.workspaces.workspace_members(&output, ws1);
            let mut fresh: HashSet<String> = observed.iter().map(|w| w.token.clone()).collect();
            for row in retained {
                if row.maximized || row.fullscreen {
                    fresh.insert(row.token.clone());
                }
            }
            let eligible =
                state
                    .workspaces
                    .eligible_focus_set(&members, &state.member_tokens, &fresh);
            state.workspaces.focus_target(&output, ws1, &eligible)
        };
        // Live game: remembered, restored, and still geometry-exempt.
        let (mut state, ws1) = setup();
        let observed = [mk_observed(502, steam_token.clone(), 8, "creation-steam")];
        let retained = [mk_game_row()];
        assert_eq!(
            super::remember_select_departure(
                &mut state,
                &game_origin,
                &output,
                &ws1,
                501,
                &observed,
                &retained
            ),
            Some(game_token.clone())
        );
        assert_eq!(
            return_target(&state, &ws1, &observed, &retained),
            Some(game_key.clone()),
            "departure memory restores the live fullscreen game"
        );
        let writable = writable_tokens(&state, &output, &ws1, &observed);
        assert!(writable.contains(&steam_token));
        assert!(
            !writable.contains(&game_token),
            "fullscreen game takes no geometry writes"
        );
        // Changed foreground remembers nothing.
        let (mut state, ws1) = setup();
        let observed = [mk_observed(502, steam_token.clone(), 8, "creation-steam")];
        let retained = [mk_game_row()];
        assert_eq!(
            super::remember_select_departure(
                &mut state,
                &game_origin,
                &output,
                &ws1,
                502,
                &observed,
                &retained
            ),
            None
        );
        assert_eq!(
            return_target(&state, &ws1, &observed, &retained),
            Some(steam_key.clone())
        );
        // Wrong token: no member maps, nothing remembered.
        let (mut state, ws1) = setup();
        let observed = [mk_observed(502, steam_token.clone(), 8, "creation-steam")];
        let retained = [mk_game_row()];
        let evil = crate::snapkey::SnapOrigin {
            hwnd: 501,
            token: "tok-evil".to_owned(),
            pid: 7,
            creation: "creation-game".to_owned(),
        };
        assert_eq!(
            super::remember_select_departure(
                &mut state, &evil, &output, &ws1, 501, &observed, &retained
            ),
            None
        );
        assert_eq!(
            return_target(&state, &ws1, &observed, &retained),
            Some(steam_key.clone())
        );
        // PID mismatch: a recycled identity never matches.
        let (mut state, ws1) = setup();
        let observed = [mk_observed(502, steam_token.clone(), 8, "creation-steam")];
        let retained = [mk_game_row()];
        let recycled = crate::snapkey::SnapOrigin {
            hwnd: 501,
            token: game_token.clone(),
            pid: 999,
            creation: "creation-game".to_owned(),
        };
        assert_eq!(
            super::remember_select_departure(
                &mut state, &recycled, &output, &ws1, 501, &observed, &retained
            ),
            None
        );
        assert_eq!(
            return_target(&state, &ws1, &observed, &retained),
            Some(steam_key.clone())
        );
        // Other-workspace origin: source membership required.
        let (mut state, ws1) = setup();
        let observed = [
            mk_observed(502, steam_token.clone(), 8, "creation-steam"),
            mk_observed(503, other_token.clone(), 9, "creation-other"),
        ];
        let retained = [mk_game_row()];
        let elsewhere = crate::snapkey::SnapOrigin {
            hwnd: 503,
            token: other_token.clone(),
            pid: 9,
            creation: "creation-other".to_owned(),
        };
        assert_eq!(
            super::remember_select_departure(
                &mut state, &elsewhere, &output, &ws1, 503, &observed, &retained
            ),
            None
        );
        assert_eq!(
            return_target(&state, &ws1, &observed, &retained),
            Some(steam_key.clone())
        );
        // Vanished: game live nowhere (absent observed, no retained row).
        let (mut state, ws1) = setup();
        let observed = [mk_observed(502, steam_token.clone(), 8, "creation-steam")];
        let retained: [RetainedRow; 0] = [];
        assert_eq!(
            super::remember_select_departure(
                &mut state,
                &game_origin,
                &output,
                &ws1,
                501,
                &observed,
                &retained
            ),
            None
        );
        assert_eq!(
            return_target(&state, &ws1, &observed, &retained),
            Some(steam_key.clone())
        );
    }
}

#[cfg(test)]
mod restart_adoption_tests {
    // Production-preamble adoption through a real `TileLoop` against an
    // owned message-only window: real `SetProp` markers, real `GetProp`
    // reads, real Engine commits. Message-only windows are invisible,
    // owned by the test process, and destroyed at test end; the log path
    // points at NUL so no file is created. No other window is touched.
    use super::adopt_restart_markers;
    use super::rmax03_adapter_tests::test_state;
    use crate::model::ProcessIdentity;
    use crate::workspace::WindowKey;
    use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
    use tiler_core::geometry::Rect;
    use tiler_core::session::DomainKey;

    const ADOPT_TEST_CLASS: &str = "OmniTilerRestartAdoptTest";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    unsafe extern "system" fn adopt_wndproc(
        hwnd: windows_sys::Win32::Foundation::HWND,
        msg: u32,
        wp: windows_sys::Win32::Foundation::WPARAM,
        lp: windows_sys::Win32::Foundation::LPARAM,
    ) -> windows_sys::Win32::Foundation::LRESULT {
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wp, lp) }
    }

    struct AdoptWindow {
        hwnd: u64,
    }

    impl AdoptWindow {
        fn create() -> AdoptWindow {
            use windows_sys::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, GetLastError};
            use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                CreateWindowExW, HWND_MESSAGE, RegisterClassW, WNDCLASSW,
            };
            let class_w = wide(ADOPT_TEST_CLASS);
            let title_w = wide("restart-adopt-test");
            let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
            let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
            cls.lpfnWndProc = Some(adopt_wndproc);
            cls.hInstance = hinst;
            cls.lpszClassName = class_w.as_ptr();
            let atom = unsafe { RegisterClassW(&cls) };
            assert!(
                atom != 0 || unsafe { GetLastError() } == ERROR_CLASS_ALREADY_EXISTS,
                "test class registers"
            );
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class_w.as_ptr(),
                    title_w.as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    std::ptr::null_mut(),
                    hinst,
                    std::ptr::null_mut(),
                )
            };
            assert!(!hwnd.is_null(), "message-only test window creates");
            AdoptWindow {
                hwnd: hwnd as usize as u64,
            }
        }
    }

    impl Drop for AdoptWindow {
        fn drop(&mut self) {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;
            unsafe {
                DestroyWindow(self.hwnd as usize as HWND);
            }
        }
    }

    fn fake_me() -> ProcessIdentity {
        ProcessIdentity {
            pid: std::process::id(),
            process_creation: "create-restart-adopt".to_owned(),
            user_sid: "S-1-5-21-restart".to_owned(),
            session_id: 1,
            exe_path: "C:\\test\\app.exe".to_owned(),
        }
    }

    fn bounds() -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        }
    }

    fn frame() -> Rect {
        Rect {
            x: 10,
            y: 10,
            w: 400,
            h: 300,
        }
    }

    fn observed(hwnd: u64, token: &str) -> super::ObservedWindow {
        super::ObservedWindow {
            hwnd,
            token: token.to_owned(),
            outer: bounds(),
            visible: frame(),
            insets: crate::tiling::FrameInsets {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            identity: crate::tiling::ObservedTarget {
                hwnd,
                pid: std::process::id(),
                process_creation: "create-restart-adopt".to_owned(),
                exe_path: "C:\\test\\app.exe".to_owned(),
                user_sid: "S-1-5-21-restart".to_owned(),
                session_id: 1,
                tag: String::new(),
            },
            facts: crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: false,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                dialog: false,
            },
        }
    }

    /// Seed admitted membership for one owned window and return its key,
    /// token, output, and active workspace. Mirrors the admission preamble
    /// tables (tokens, rects, identity, tags, workspace) without geometry.
    fn seed_member(
        state: &mut super::TileLoop,
        hwnd: u64,
        tag: &str,
    ) -> (WindowKey, String, String, String) {
        let me = fake_me();
        let key = WindowKey {
            hwnd,
            pid: me.pid,
            creation: me.process_creation.clone(),
        };
        let token = state.tokens.token_for(hwnd, &key.creation);
        let output = "mon-restart".to_owned();
        state.workspaces.ensure_output(&output);
        let active = state.workspaces.active_id(&output).expect("active");
        assert!(
            state
                .workspaces
                .assign(key.clone(), &output, &active, false)
        );
        state.member_tokens.insert(key.clone(), token.clone());
        state.member_rects.insert(token.clone(), frame());
        state.member_identity.insert(key.clone(), me);
        state.member_tags.insert(key.clone(), tag.to_owned());
        (key, token, output, active)
    }

    fn areas_for(output: &str) -> Vec<super::MonitorArea> {
        vec![super::MonitorArea {
            device: output.to_owned(),
            work: bounds(),
            full: bounds(),
        }]
    }

    fn is_float(state: &super::TileLoop, output: &str, ws: &str, token: &str) -> bool {
        let key = DomainKey {
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(ws.to_owned()),
        };
        state
            .engine
            .session(&key)
            .is_some_and(|s| s.is_exception(&WindowId(token.to_owned())))
    }

    #[test]
    fn sticky_markers_readopt_through_real_tileloop_twice() {
        // Both pre-sticky origins re-adopt as sticky through the production
        // preamble with real markers, and a second fresh owner re-adopts
        // again off the kept marker. No post-restart un-stick outcome set.
        for prior in [false, true] {
            let win = AdoptWindow::create();
            let pid = std::process::id();
            let tag = crate::product_hide::sys::install_member_tag(win.hwnd, pid)
                .expect("member tag installs on the owned window");
            crate::product_hide::sys::install_sticky_marker(win.hwnd, prior)
                .expect("sticky marker installs on the owned window");
            for restart in 1..=2 {
                let mut state = test_state();
                state.log_path = std::path::PathBuf::from("NUL");
                let (key, token, output, active) = seed_member(&mut state, win.hwnd, &tag);
                let observed = vec![observed(win.hwnd, &token)];
                adopt_restart_markers(&mut state, &fake_me(), &observed, &[], &areas_for(&output));
                assert_eq!(
                    state.sticky.get(&key),
                    Some(&prior),
                    "prior {prior} restart {restart}: sticky origin re-adopted"
                );
                assert!(
                    is_float(&state, &output, &active, &token),
                    "prior {prior} restart {restart}: Engine carries the float"
                );
                assert!(
                    crate::product_hide::sys::peek_sticky_raw(win.hwnd)
                        == crate::workspace_owner::MarkerSlot::Value(
                            crate::tiling::sticky_marker_value(prior)
                        ),
                    "prior {prior} restart {restart}: durable marker kept"
                );
            }
        }
    }

    #[test]
    fn float_marker_readopts_through_real_tileloop() {
        // An intentional-float marker re-adopts as an ordinary float with
        // the marker kept and no sticky entry.
        let win = AdoptWindow::create();
        let pid = std::process::id();
        let tag = crate::product_hide::sys::install_member_tag(win.hwnd, pid)
            .expect("member tag installs on the owned window");
        crate::product_hide::sys::install_float_intent_marker(win.hwnd)
            .expect("float marker installs on the owned window");
        let mut state = test_state();
        state.log_path = std::path::PathBuf::from("NUL");
        let (key, token, output, active) = seed_member(&mut state, win.hwnd, &tag);
        let observed = vec![observed(win.hwnd, &token)];
        adopt_restart_markers(&mut state, &fake_me(), &observed, &[], &areas_for(&output));
        assert!(
            state.floated.contains(&key),
            "float intent re-adopted into the runtime set"
        );
        assert!(
            !state.sticky.contains_key(&key),
            "ordinary float takes no sticky entry"
        );
        assert!(
            is_float(&state, &output, &active, &token),
            "Engine carries the re-adopted float"
        );
        assert!(
            crate::product_hide::sys::read_float_intent_marker(win.hwnd),
            "durable float marker kept for later restarts"
        );
    }

    #[test]
    fn mixed_markers_readopt_through_real_tileloop_in_any_order() {
        // Live item 8 shape: two sticky markers (both pre-sticky origins),
        // one ordinary float marker, and one unmarked tile in a single
        // domain. The production preamble (sticky pass then float pass) must
        // hydrate all three before any tiling, in either observed order.
        // Log goes to a temp file so a candidate rejection shows its true
        // correlated cause instead of a silent rollback.
        for reverse in [false, true] {
            let a = AdoptWindow::create();
            let b = AdoptWindow::create();
            let c = AdoptWindow::create();
            let d = AdoptWindow::create();
            let pid = std::process::id();
            let install = |win: &AdoptWindow| {
                crate::product_hide::sys::install_member_tag(win.hwnd, pid)
                    .expect("member tag installs on the owned window")
            };
            let (tag_a, tag_b, tag_c, tag_d) = (install(&a), install(&b), install(&c), install(&d));
            crate::product_hide::sys::install_sticky_marker(a.hwnd, false)
                .expect("sticky marker installs");
            crate::product_hide::sys::install_sticky_marker(b.hwnd, true)
                .expect("sticky marker installs");
            crate::product_hide::sys::install_float_intent_marker(c.hwnd)
                .expect("float marker installs");
            let mut state = test_state();
            let log_path = std::env::temp_dir().join(format!(
                "tiler-windows-mixed-adopt-{}-{}.log",
                std::process::id(),
                reverse as u8
            ));
            let _ = std::fs::remove_file(&log_path);
            state.log_path = log_path.clone();
            let (key_a, token_a, output, active) = seed_member(&mut state, a.hwnd, &tag_a);
            let (key_b, token_b, _, _) = seed_member(&mut state, b.hwnd, &tag_b);
            let (key_c, token_c, _, _) = seed_member(&mut state, c.hwnd, &tag_c);
            let (key_d, token_d, _, _) = seed_member(&mut state, d.hwnd, &tag_d);
            let mut ordered = vec![
                observed(a.hwnd, &token_a),
                observed(b.hwnd, &token_b),
                observed(c.hwnd, &token_c),
                observed(d.hwnd, &token_d),
            ];
            if reverse {
                ordered.reverse();
            }
            adopt_restart_markers(&mut state, &fake_me(), &ordered, &[], &areas_for(&output));
            let log = std::fs::read_to_string(&log_path).unwrap_or_default();
            let _ = std::fs::remove_file(&log_path);
            assert_eq!(
                state.sticky.get(&key_a),
                Some(&false),
                "reverse {reverse}: ex-tiled sticky re-adopts; log:\n{log}"
            );
            assert_eq!(
                state.sticky.get(&key_b),
                Some(&true),
                "reverse {reverse}: ex-float sticky re-adopts; log:\n{log}"
            );
            assert!(
                state.floated.contains(&key_c),
                "reverse {reverse}: ordinary float re-adopts; log:\n{log}"
            );
            assert!(
                !state.sticky.contains_key(&key_d) && !state.floated.contains(&key_d),
                "reverse {reverse}: unmarked tile stays tiled; log:\n{log}"
            );
            for (token, what) in [
                (&token_a, "sticky-a"),
                (&token_b, "sticky-b"),
                (&token_c, "float-c"),
            ] {
                assert!(
                    is_float(&state, &output, &active, token),
                    "reverse {reverse}: Engine carries {what}; log:\n{log}"
                );
            }
            assert!(
                !is_float(&state, &output, &active, &token_d),
                "reverse {reverse}: Engine keeps the tile tiled; log:\n{log}"
            );
        }
    }

    #[test]
    fn marker_install_remove_roundtrip_on_owned_window() {
        // Actual persistence native ops: install reads back, settled removal
        // verifies absence, and a wrong sticky value refuses without
        // touching the live marker.
        let win = AdoptWindow::create();
        assert!(
            !crate::product_hide::sys::read_float_intent_marker(win.hwnd),
            "fresh window carries no float intent"
        );
        crate::product_hide::sys::install_float_intent_marker(win.hwnd)
            .expect("float marker installs on the owned window");
        assert!(crate::product_hide::sys::read_float_intent_marker(win.hwnd));
        crate::product_hide::sys::remove_float_intent_marker(win.hwnd)
            .expect("settled float removal verifies");
        assert!(
            !crate::product_hide::sys::read_float_intent_marker(win.hwnd),
            "settled removal clears the float marker"
        );
        crate::product_hide::sys::install_sticky_marker(win.hwnd, true)
            .expect("sticky marker installs on the owned window");
        assert!(
            !crate::product_hide::sys::remove_sticky_marker(win.hwnd, false).expect("readable"),
            "wrong pre-sticky value refuses without touching"
        );
        assert_eq!(
            crate::product_hide::sys::read_sticky_marker(win.hwnd),
            Some(true),
            "refused removal keeps the live marker"
        );
        assert!(
            crate::product_hide::sys::remove_sticky_marker(win.hwnd, true).expect("readable"),
            "settled removal with the live value verifies"
        );
        assert_eq!(
            crate::product_hide::sys::read_sticky_marker(win.hwnd),
            None,
            "settled removal clears the sticky marker"
        );
    }

    #[test]
    fn absent_markers_read_silent_never_unreadable() {
        // Never-installed marker names read silent-absent on a real owned
        // window: `GetPropW` documents NULL for a missing string with no
        // last-error promise, and absent reads report nonzero errors here,
        // so NULL must never classify as unreadable (else every unmarked
        // window logs marker-unreadable every tick).
        let win = AdoptWindow::create();
        for peek in [
            crate::product_hide::sys::peek_sticky_raw(win.hwnd),
            crate::product_hide::sys::peek_float_intent_raw(win.hwnd),
            crate::product_hide::sys::peek_tile_override_raw(win.hwnd),
        ] {
            assert_eq!(peek, crate::workspace_owner::MarkerSlot::Absent);
        }
    }

    #[test]
    fn corrupt_marker_adopts_nothing_without_side_effects() {
        // A foreign value on our property name classifies corrupt through
        // the production preamble: diagnosed, no intent, no state change.
        use windows_sys::Win32::UI::WindowsAndMessaging::SetPropW;
        fn wide(s: &str) -> Vec<u16> {
            s.encode_utf16().chain([0]).collect()
        }
        let win = AdoptWindow::create();
        let pid = std::process::id();
        let tag = crate::product_hide::sys::install_member_tag(win.hwnd, pid)
            .expect("member tag installs on the owned window");
        let name = wide(crate::model::FLOAT_INTENT_PROP);
        let ok = unsafe {
            SetPropW(
                win.hwnd as usize as windows_sys::Win32::Foundation::HWND,
                name.as_ptr(),
                99usize as _,
            )
        };
        assert_ne!(ok, 0, "foreign value plants on the owned window");
        let mut state = test_state();
        state.log_path = std::path::PathBuf::from("NUL");
        let (key, token, output, _active) = seed_member(&mut state, win.hwnd, &tag);
        let observed = vec![observed(win.hwnd, &token)];
        adopt_restart_markers(&mut state, &fake_me(), &observed, &[], &areas_for(&output));
        assert!(!state.sticky.contains_key(&key), "corrupt never sticks");
        assert!(!state.floated.contains(&key), "corrupt never floats");
    }

    #[test]
    fn absent_markers_adopt_nothing_without_side_effects() {
        // A fabricated HWND exercises only failing reads: no adoption, no
        // state change, no writes, no panic.
        let mut state = test_state();
        state.log_path = std::path::PathBuf::from("NUL");
        let hwnd = 0x00BEEF42u64;
        let (key, token, output, _active) = seed_member(&mut state, hwnd, "9f2c41aa07bd33e0");
        let observed = vec![observed(hwnd, &token)];
        adopt_restart_markers(&mut state, &fake_me(), &observed, &[], &areas_for(&output));
        assert!(
            !state.sticky.contains_key(&key),
            "no sticky without a marker"
        );
        assert!(!state.floated.contains(&key), "no float without a marker");
        assert!(
            !state.float_rects.contains_key(&token),
            "no float frame without adoption"
        );
    }
}

#[cfg(test)]
mod orientation_route_tests {
    use super::rmax03_adapter_tests::test_state;
    use super::{
        MonitorArea, dispatch_orientation_intent, keyboard_tick, orientation_overlay_refusal,
        orientation_plan, resolve_chord_target, workspace_domain_for, writable_tokens,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    /// Null/disposable-handle harness: every HWND below is fake (101/102 are
    /// never live windows), `me` is fabricated so every pre-write identity
    /// gate fails closed on real windows, and logs redirect to a temp dir
    /// that is removed afterwards. Only OS reads plus in-memory state run
    /// here; no hooks, no setters on foreign windows, no live apps.
    const FAKE_HWND: u64 = 101;
    const FAKE_HWND_2: u64 = 102;

    fn log_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tiler-ori-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp log dir");
        dir
    }

    fn fake_me() -> crate::model::ProcessIdentity {
        crate::model::ProcessIdentity {
            pid: 4242,
            process_creation: "test-creation".to_owned(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
            exe_path: "test.exe".to_owned(),
        }
    }

    fn fake_origin() -> crate::snapkey::SnapOrigin {
        crate::snapkey::SnapOrigin {
            hwnd: FAKE_HWND,
            token: "w1".to_owned(),
            pid: 7,
            creation: "creation-ori".to_owned(),
        }
    }

    fn fake_key() -> crate::workspace::WindowKey {
        crate::workspace::WindowKey {
            hwnd: FAKE_HWND,
            pid: 7,
            creation: "creation-ori".to_owned(),
        }
    }

    fn ori_intent(
        origin: Option<crate::snapkey::SnapOrigin>,
    ) -> crate::snapkey::QueuedOrientationIntent {
        crate::snapkey::QueuedOrientationIntent {
            edge: crate::snapkey::SnapEdge::Down,
            origin,
            consumed: true,
            announce: true,
            tick: std::time::Instant::now(),
        }
    }

    fn test_areas() -> Vec<MonitorArea> {
        let rect = tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        };
        vec![MonitorArea {
            device: "mon-9".to_owned(),
            work: rect,
            full: rect,
        }]
    }

    /// Seed a tiled w1+w2 pair on the fabricated output through direct Engine
    /// handles (no native calls) and return the Engine domain key the
    /// production route re-derives from `areas`.
    fn seed_pair(state: &mut super::TileLoop) -> tiler_core::session::DomainKey {
        use tiler_core::directional::WindowId;
        let areas = test_areas();
        let (domain, key) =
            workspace_domain_for("mon-9", "ws-9", &areas, state.inner_gap, state.outer_gap)
                .expect("fabricated domain");
        let none = tiler_core::size_hints::WindowSizeHints::none();
        let bounds = tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        };
        for (correlation, tokens) in [
            ("ori-rt-seed-1", vec!["w1"]),
            ("ori-rt-seed-2", vec!["w1", "w2"]),
        ] {
            let rows: Vec<(
                WindowId,
                tiler_core::geometry::Rect,
                tiler_core::size_hints::WindowSizeHints,
                bool,
            )> = tokens
                .iter()
                .map(|token| (WindowId((*token).to_owned()), bounds, none, false))
                .collect();
            let correlation_id =
                tiler_core::ids::CorrelationId::parse(correlation).expect("correlation");
            let fp = crate::tiling::fingerprint(
                &rows
                    .iter()
                    .map(|(token, rect, _, _)| (token.0.clone(), *rect))
                    .collect::<Vec<_>>(),
            );
            let revision = state
                .engine
                .session(&key)
                .map(|session| session.accepted_revision())
                .unwrap_or(0);
            let event = crate::tiling::build_reconcile_event_for_floating(
                &state.owner,
                &state.generation,
                &correlation_id,
                revision,
                fp,
                &domain,
                &key,
                state.outer_gap,
                &rows,
                Some(&WindowId("w1".to_owned())),
            );
            let reply = state.engine.handle(&event);
            assert!(
                matches!(
                    reply,
                    tiler_core::boundary::CoreReply::Projection(_)
                        | tiler_core::boundary::CoreReply::Tiled(_)
                ),
                "seed {correlation} converges, got {reply:?}"
            );
        }
        key
    }

    fn engine_snapshot(
        state: &super::TileLoop,
        key: &tiler_core::session::DomainKey,
    ) -> Option<tiler_core::session::SessionSnapshot> {
        state.engine.session(key).map(|session| session.snapshot())
    }

    fn log_text(dir: &std::path::Path) -> String {
        std::fs::read_to_string(dir.join("owner.log")).unwrap_or_default()
    }

    fn assert_outcome(dir: &std::path::Path, outcome: &str) {
        let text = log_text(dir);
        assert!(
            text.contains(&format!("\"outcome\":\"{outcome}\"")),
            "expected outcome {outcome} in log, got: {text}"
        );
    }

    #[test]
    fn orientation_missing_origin_settles_without_mutation() {
        // Missing chord origin through the real dispatch route: the `None`
        // branch runs before the suspension/elevation gate and before any
        // host read, so it settles `origin-vanished` deterministically on
        // every runner (including elevated ones) with zero Engine, focus, or
        // view mutation. The log outcome pins the fence.
        let dir = log_dir("ori-missing");
        let mut state = test_state();
        state.log_path = dir.join("owner.log");
        let key = seed_pair(&mut state);
        state.workspaces.ensure_output("mon-9");
        let before = engine_snapshot(&state, &key);
        assert!(before.is_some(), "seeded session exists");
        let active_before = state.workspaces.active_id("mon-9");
        let me = fake_me();
        let fulls: Vec<tiler_core::geometry::Rect> = Vec::new();
        let areas = test_areas();
        dispatch_orientation_intent(&mut state, &me, &fulls, &areas, ori_intent(None));
        assert_outcome(&dir, "origin-vanished");
        assert_eq!(
            engine_snapshot(&state, &key),
            before,
            "missing origin must not mutate the Engine"
        );
        assert_eq!(
            state.workspaces.active_id("mon-9"),
            active_before,
            "missing origin must not switch the view"
        );
        assert!(
            state.snap_advance.is_none(),
            "missing origin carries no focus advance"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn orientation_unknown_origin_refuses_without_mutation() {
        // Fabricated `Some` origin unknown to the owner tables, checked at
        // the production resolver (`resolve_chord_target`, called by every
        // toggle route): the missing-table check refuses `origin-vanished`
        // before member, lifetime, scope, or any live-eligibility state, so
        // it is deterministic on every runner. Engine, view, and advance
        // stay unchanged.
        let mut state = test_state();
        let key = seed_pair(&mut state);
        let output = "mon-9".to_owned();
        state.workspaces.ensure_output(&output);
        let active = state.workspaces.active_id(&output).expect("active");
        let me = fake_me();
        let areas = test_areas();
        let before = engine_snapshot(&state, &key);
        assert!(before.is_some(), "seeded session exists");
        match resolve_chord_target(&mut state, &me, &areas, &fake_origin()) {
            Err(reject) => assert_eq!(reject.outcome, "origin-vanished"),
            Ok(_) => panic!("unknown origin must refuse"),
        }
        assert!(
            state.snap_advance.is_none(),
            "unknown origin carries no focus advance"
        );
        assert_eq!(
            engine_snapshot(&state, &key),
            before,
            "unknown origin must not mutate the Engine"
        );
        assert_eq!(
            state.workspaces.active_id(&output),
            Some(active),
            "unknown origin must not switch the view"
        );
    }

    #[test]
    fn orientation_resolver_fences_settle_without_mutation() {
        // The production origin resolver (`resolve_chord_target`, called by
        // every toggle route) with fabricated table state and no publication
        // step: a live-but-unfocused origin settles `foreground-changed` and
        // clears any advance, while an owner-advanced continuation to a
        // lifetime-stale member settles `identity-changed` and drops the
        // stale membership. Neither mutates Engine state or the view. The
        // foreground HWND is only ever READ to mirror the continuation.
        let mut state = test_state();
        let key = seed_pair(&mut state);
        let output = "mon-9".to_owned();
        state.workspaces.ensure_output(&output);
        let active = state.workspaces.active_id(&output).expect("active");
        let member = fake_key();
        assert!(
            state
                .workspaces
                .assign(member.clone(), &output, &active, false)
        );
        state.member_tokens.insert(member.clone(), "w1".to_owned());
        state
            .member_tags
            .insert(member.clone(), "tag-ori-1".to_owned());
        let me = fake_me();
        let areas = test_areas();
        let before = engine_snapshot(&state, &key);
        assert!(before.is_some(), "seeded session exists");

        // Unfocused origin: live in the table, foreground elsewhere.
        state.snap_advance = Some(fake_origin());
        state.snap_origins.insert(FAKE_HWND, fake_origin());
        match resolve_chord_target(&mut state, &me, &areas, &fake_origin()) {
            Err(reject) => assert_eq!(reject.outcome, "foreground-changed"),
            Ok(_) => panic!("unfocused origin must refuse"),
        }
        assert!(
            state.snap_advance.is_none(),
            "external focus clears the advance"
        );

        // Lifetime-stale member behind an owner-advanced continuation: mirror
        // the live foreground into the advance so only the lifetime fence
        // refuses.
        let foreground = unsafe { GetForegroundWindow() } as usize as u64;
        let advance = crate::snapkey::SnapOrigin {
            hwnd: foreground,
            token: "w1".to_owned(),
            pid: 7,
            creation: "creation-ori".to_owned(),
        };
        state.snap_origins.insert(foreground, advance.clone());
        state.snap_advance = Some(advance);
        match resolve_chord_target(&mut state, &me, &areas, &fake_origin()) {
            Err(reject) => assert_eq!(reject.outcome, "identity-changed"),
            Ok(_) => panic!("lifetime-stale member must refuse"),
        }
        assert!(
            !state.member_tokens.contains_key(&member),
            "lifetime mismatch drops the stale membership"
        );
        assert_eq!(
            engine_snapshot(&state, &key),
            before,
            "resolver fences must not mutate the Engine"
        );
        assert_eq!(
            state.workspaces.active_id(&output),
            Some(active),
            "resolver fences must not switch the view"
        );
    }

    #[test]
    fn orientation_suspended_queue_drops_without_mutation() {
        // The pump-level suspension route (`blocked`): a consumed orientation
        // intent drops with zero Engine, focus, or view mutation and no tick
        // advance. Fully hermetic: no Win32 reads at all.
        let dir = log_dir("ori-suspended");
        let mut state = test_state();
        state.log_path = dir.join("owner.log");
        let key = seed_pair(&mut state);
        state.workspaces.ensure_output("mon-9");
        let before = engine_snapshot(&state, &key);
        assert!(before.is_some(), "seeded session exists");
        let active_before = state.workspaces.active_id("mon-9");
        let me = fake_me();
        let fulls: Vec<tiler_core::geometry::Rect> = Vec::new();
        let areas = test_areas();
        keyboard_tick(
            &mut state,
            &me,
            &fulls,
            &areas,
            vec![crate::snapkey::QueuedSnapEvent::Orientation(ori_intent(
                Some(fake_origin()),
            ))],
            Some("test-suspend"),
        );
        assert_eq!(
            engine_snapshot(&state, &key),
            before,
            "suspended queue must not mutate the Engine"
        );
        assert_eq!(
            state.workspaces.active_id("mon-9"),
            active_before,
            "suspended queue must not switch the view"
        );
        assert!(state.snap_advance.is_none());
        assert_eq!(state.tick, 0, "suspended queue advances no tick");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn orientation_plan_accepts_only_matching_tiled() {
        // The extracted production seam against real Engine replies: a
        // ToggleOrientation plan accepts; a ToggleFloat plan and a refusal
        // do not.
        use tiler_core::boundary::{CoreCommand, CoreReply};
        use tiler_core::directional::WindowId;
        let mut state = test_state();
        let _key = seed_pair(&mut state);
        let none = tiler_core::size_hints::WindowSizeHints::none();
        let bounds = tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        };
        let rows = [
            (WindowId("w1".to_owned()), bounds, none, false),
            (WindowId("w2".to_owned()), bounds, none, false),
        ];
        let event_for =
            |state: &super::TileLoop, correlation: &str, focused: &str, command: CoreCommand| {
                let areas = test_areas();
                let (domain, key) =
                    workspace_domain_for("mon-9", "ws-9", &areas, state.inner_gap, state.outer_gap)
                        .expect("fabricated domain");
                let correlation_id =
                    tiler_core::ids::CorrelationId::parse(correlation).expect("correlation");
                let fp = crate::tiling::fingerprint(
                    &rows
                        .iter()
                        .map(|(token, rect, _, _)| (token.0.clone(), *rect))
                        .collect::<Vec<_>>(),
                );
                let revision = state
                    .engine
                    .session(&key)
                    .map(|session| session.accepted_revision())
                    .unwrap_or(0);
                let mut event = crate::tiling::build_reconcile_event_for_floating(
                    &state.owner,
                    &state.generation,
                    &correlation_id,
                    revision,
                    fp,
                    &domain,
                    &key,
                    state.outer_gap,
                    &rows,
                    Some(&WindowId(focused.to_owned())),
                );
                event.command = command;
                event
            };
        let reply = state.engine.handle(&event_for(
            &state,
            "ori-seam-toggle",
            "w1",
            CoreCommand::ToggleOrientation {
                window: "w1".to_owned(),
            },
        ));
        let plan = orientation_plan(&reply).expect("matching Tiled accepts");
        assert_eq!(
            plan.kind,
            tiler_core::boundary::TiledKind::ToggleOrientation
        );
        assert_eq!(plan.geometry.len(), 2);
        // Writes extract from the accepted plan: both tiles reproject.
        let writes = crate::workspace_owner::planned_writes(&reply).expect("plan extracts");
        assert_eq!(writes.len(), 2);
        let reply = state.engine.handle(&event_for(
            &state,
            "ori-seam-float",
            "w1",
            CoreCommand::ToggleFloat {
                window: "w1".to_owned(),
                float_rect: None,
            },
        ));
        assert!(
            matches!(reply, CoreReply::Tiled(_)),
            "float commits, got {reply:?}"
        );
        assert!(
            orientation_plan(&reply).is_none(),
            "wrong Tiled kind refuses"
        );
        let reply = state.engine.handle(&event_for(
            &state,
            "ori-seam-unknown",
            "w1",
            CoreCommand::ToggleOrientation {
                window: "w9".to_owned(),
            },
        ));
        assert!(
            matches!(reply, CoreReply::Rejected { .. }),
            "unknown window refuses, got {reply:?}"
        );
        assert!(orientation_plan(&reply).is_none(), "refusal refuses");
    }

    #[test]
    fn orientation_guard_seams_refuse_overlays_and_scope_writes() {
        // Production-called guard seams with fabricated native state (fake
        // HWND reads only): focused overlays refuse with orientation
        // vocabulary, retained-only siblings take no writes, and the
        // suspension/elevation gate vocabulary holds.
        let mut state = test_state();
        let member = fake_key();
        let member2 = crate::workspace::WindowKey {
            hwnd: FAKE_HWND_2,
            pid: 8,
            creation: "creation-ori-2".to_owned(),
        };
        let observed = |hwnd: u64, token: &str, fullscreen: bool| super::ObservedWindow {
            hwnd,
            token: token.to_owned(),
            outer: tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
            visible: tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
            insets: crate::tiling::FrameInsets {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            identity: crate::tiling::ObservedTarget {
                hwnd,
                pid: 7,
                process_creation: "creation-ori".to_owned(),
                exe_path: "C:\\test\\app.exe".to_owned(),
                user_sid: "S-1-5-test".to_owned(),
                session_id: 1,
                tag: String::new(),
            },
            facts: crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: false,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: fullscreen,
                no_activate: false,
                dialog: false,
            },
        };
        // Clean focus proceeds; live fullscreen refuses; retained-only
        // fullscreen refuses through the retained row.
        assert_eq!(
            orientation_overlay_refusal(&[observed(FAKE_HWND, "w1", false)], &[], &member),
            None
        );
        assert_eq!(
            orientation_overlay_refusal(&[observed(FAKE_HWND, "w1", true)], &[], &member),
            Some("orientation-refused-fullscreen")
        );
        let retained = vec![super::RetainedRow {
            key: member.clone(),
            token: "w1".to_owned(),
            rect: None,
            maximized: false,
            fullscreen: true,
            facts: None,
        }];
        assert_eq!(
            orientation_overlay_refusal(&[], &retained, &member),
            Some("orientation-refused-fullscreen")
        );
        // Write scoping: w2 assigned but never freshly observed takes no
        // writes while w1 does.
        let output = "mon-9".to_owned();
        state.workspaces.ensure_output(&output);
        let active = state.workspaces.active_id(&output).expect("active");
        assert!(
            state
                .workspaces
                .assign(member.clone(), &output, &active, false)
        );
        assert!(
            state
                .workspaces
                .assign(member2.clone(), &output, &active, false)
        );
        state.member_tokens.insert(member.clone(), "w1".to_owned());
        state.member_tokens.insert(member2.clone(), "w2".to_owned());
        let observed = vec![observed(FAKE_HWND, "w1", false)];
        assert_eq!(
            writable_tokens(&state, &output, &active, &observed),
            std::collections::HashSet::from(["w1".to_owned()]),
            "retained-only sibling takes no writes"
        );
        // Suspension/elevation gate vocabulary: blocked or elevated settles,
        // quiet proceeds.
        assert!(crate::tiling::toggle_gate_outcome(true, false).is_some());
        assert!(crate::tiling::toggle_gate_outcome(false, true).is_some());
        assert!(crate::tiling::toggle_gate_outcome(false, false).is_none());
    }
}

#[cfg(test)]
mod windrag_press_focus_tests {
    // Deterministic gaps (need a live desktop, verified later): the positive
    // press actuation itself, the suspend/elevation precheck, and the
    // already-foreground skip all read/actuate the real foreground.
    use super::{clear_windrag_gesture, clear_windrag_gesture_scoped, windrag_down, windrag_up};
    use crate::model::ProcessIdentity;
    use crate::win_mouse::{WinDragEdge, WinDragKind, WinDragSnapshot};

    const HWND_A: u64 = 0xA001;
    const HWND_B: u64 = 0xB002;
    const DEAD_HWND: u64 = 0x00BEEF42;

    fn fake_me() -> ProcessIdentity {
        ProcessIdentity {
            pid: 4242,
            process_creation: "create-me".to_owned(),
            user_sid: "S-1-5-21-me".to_owned(),
            session_id: 1,
            exe_path: "C:\\bin\\tiler-windows.exe".to_owned(),
        }
    }

    fn origin(hwnd: u64, token: &str) -> crate::snapkey::SnapOrigin {
        crate::snapkey::SnapOrigin {
            hwnd,
            token: token.to_owned(),
            pid: 100,
            creation: "create-1".to_owned(),
        }
    }

    fn member_key(hwnd: u64) -> crate::workspace::WindowKey {
        crate::workspace::WindowKey {
            hwnd,
            pid: 100,
            creation: "create-1".to_owned(),
        }
    }

    fn down_edge(hwnd: u64, token: &str, tag: Option<&str>) -> WinDragEdge {
        WinDragEdge {
            hwnd,
            kind: WinDragKind::Down,
            x: 10,
            y: 10,
            esc_seq: 1,
            snapshot: Some(WinDragSnapshot {
                origin: origin(hwnd, token),
                tag: tag.map(str::to_owned),
            }),
        }
    }

    fn publish_tiled(state: &mut super::TileLoop, hwnd: u64, token: &str, tag: Option<&str>) {
        state.managed.insert(hwnd);
        state.windrag_origins.insert(
            hwnd,
            crate::win_mouse::sys::WinDragPublished {
                origin: origin(hwnd, token),
                tag: tag.map(str::to_owned),
            },
        );
        state
            .member_tokens
            .insert(member_key(hwnd), token.to_owned());
        state.stable.insert(
            hwnd,
            tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 400,
                h: 300,
            },
        );
    }

    fn assert_no_arm(state: &super::TileLoop, hwnd: u64) {
        assert!(!state.active.contains(&hwnd), "no active capture");
        assert!(!state.gesture_producer.contains_key(&hwnd), "no producer");
        assert!(!state.windrag_bound.contains_key(&hwnd), "no bound");
        assert!(
            !state.windrag_start_cursor.contains_key(&hwnd),
            "no start cursor"
        );
        assert!(!state.move_kind.contains_key(&hwnd), "no move arm");
    }

    fn scratch_log(state: &mut super::TileLoop, tag: &str) {
        // Preview logging must never touch the repo tree.
        state.log_path = std::env::temp_dir().join(format!(
            "tiler-windrag-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
    }

    fn arm_pair(state: &mut super::TileLoop) {
        // Two fully armed windrag gestures with owned preview bindings.
        let rect = tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 300,
        };
        for (hwnd, token) in [(HWND_A, "w1"), (HWND_B, "w2")] {
            publish_tiled(state, hwnd, token, Some("tag-1"));
            state.active.insert(hwnd);
            state.gesture_before.insert(hwnd, rect);
            state.gesture_esc_seq.insert(hwnd, 1);
            state.gesture_start_key.insert(hwnd, member_key(hwnd));
            state
                .gesture_start_tag
                .insert(hwnd, Some("tag-1".to_owned()));
            state.gesture_preview_start.insert(
                hwnd,
                super::PreviewStartBinding {
                    token: token.to_owned(),
                    output: "mon-9".to_owned(),
                    workspace: "ws-9".to_owned(),
                    revision: 0,
                },
            );
            state.windrag_start_cursor.insert(hwnd, (10, 10));
            state.windrag_bound.insert(
                hwnd,
                WinDragSnapshot {
                    origin: origin(hwnd, token),
                    tag: Some("tag-1".to_owned()),
                },
            );
            state
                .move_kind
                .insert(hwnd, crate::group_underlay::MoveSizeKind::Move);
            state.gesture_producer.insert(hwnd, "windrag");
            state
                .preview_bound
                .insert(hwnd, super::PreviewBound { hover_prior: None });
        }
        state.member_rects.insert("w1".to_owned(), rect);
    }

    fn assert_no_maps(state: &super::TileLoop, hwnd: u64) {
        for map_has in [
            state.active.contains(&hwnd),
            state.gesture_producer.contains_key(&hwnd),
            state.windrag_bound.contains_key(&hwnd),
            state.windrag_start_cursor.contains_key(&hwnd),
            state.move_kind.contains_key(&hwnd),
            state.gesture_before.contains_key(&hwnd),
            state.gesture_end_cursor.contains_key(&hwnd),
            state.gesture_start_key.contains_key(&hwnd),
            state.gesture_start_tag.contains_key(&hwnd),
            state.gesture_esc_seq.contains_key(&hwnd),
            state.gesture_end_seq.contains_key(&hwnd),
            state.esc_latched.contains(&hwnd),
            state.preview_bound.contains_key(&hwnd),
            state.gesture_preview_start.contains_key(&hwnd),
        ] {
            assert!(!map_has, "every arm/preview map cleared");
        }
    }

    #[test]
    fn down_refuses_before_any_probe_with_no_arm() {
        // Every pre-probe fence refuses the production `windrag_down` with
        // its exact reason and arms nothing (no active capture, producer,
        // bound, cursor, or move arm). No native call runs on the pre-probe
        // paths; the closing identity case performs one bounded
        // thread/pid read of a dead HWND. (Tag/creation/token drift past
        // the probe is pinned portably by `validate_windrag_down`.)
        let me = fake_me();
        // No snapshot at all.
        let mut state = super::rmax03_adapter_tests::test_state();
        let edge = WinDragEdge {
            hwnd: HWND_A,
            kind: WinDragKind::Down,
            x: 10,
            y: 10,
            esc_seq: 1,
            snapshot: None,
        };
        assert_eq!(
            windrag_down(&mut state, &me, &edge, &[], &[], &[]),
            Err("no-snapshot")
        );
        assert_no_arm(&state, HWND_A);
        // Known snapshot but unmanaged subject.
        let mut state = super::rmax03_adapter_tests::test_state();
        assert_eq!(
            windrag_down(
                &mut state,
                &me,
                &down_edge(HWND_A, "w1", Some("tag-1")),
                &[],
                &[],
                &[]
            ),
            Err("unmanaged")
        );
        assert_no_arm(&state, HWND_A);
        // Managed but never published (floating/sticky/unknown pass through
        // hook-side; the owner still refuses).
        let mut state = super::rmax03_adapter_tests::test_state();
        state.managed.insert(HWND_A);
        assert_eq!(
            windrag_down(
                &mut state,
                &me,
                &down_edge(HWND_A, "w1", Some("tag-1")),
                &[],
                &[],
                &[]
            ),
            Err("unknown-subject")
        );
        assert_no_arm(&state, HWND_A);
        // Busy: an open gesture, a held PRE frame, or a stored bound each
        // refuses without rebinding.
        for seed in ["active", "before", "bound"] {
            let mut state = super::rmax03_adapter_tests::test_state();
            publish_tiled(&mut state, HWND_A, "w1", Some("tag-1"));
            match seed {
                "active" => {
                    state.active.insert(HWND_A);
                }
                "before" => {
                    state.gesture_before.insert(
                        HWND_A,
                        tiler_core::geometry::Rect {
                            x: 0,
                            y: 0,
                            w: 400,
                            h: 300,
                        },
                    );
                }
                _ => {
                    state.windrag_bound.insert(
                        HWND_A,
                        WinDragSnapshot {
                            origin: origin(HWND_A, "w1"),
                            tag: Some("tag-1".to_owned()),
                        },
                    );
                }
            }
            assert_eq!(
                windrag_down(
                    &mut state,
                    &me,
                    &down_edge(HWND_A, "w1", Some("tag-1")),
                    &[],
                    &[],
                    &[]
                ),
                Err("busy"),
                "seed {seed} refuses"
            );
            assert!(
                !state.gesture_producer.contains_key(&HWND_A),
                "seed {seed} never arms a producer"
            );
        }
        // Live-probe gate through the production path: a published member
        // whose HWND no longer resolves refuses `identity-changed` with no
        // arm. One bounded thread/pid read of a dead HWND: no setter, no
        // plan, no geometry.
        let mut state = super::rmax03_adapter_tests::test_state();
        publish_tiled(&mut state, DEAD_HWND, "w1", Some("tag-1"));
        assert_eq!(
            windrag_down(
                &mut state,
                &me,
                &down_edge(DEAD_HWND, "w1", Some("tag-1")),
                &[],
                &[],
                &[]
            ),
            Err("identity-changed")
        );
        assert_no_arm(&state, DEAD_HWND);
    }

    #[test]
    fn press_failure_clear_keeps_neighbour_preview_and_maps() {
        // B has only just armed at press, before acquiring a preview binding.
        // Its failed focus must not invoke hide_preview on A's preview.
        let mut state = super::rmax03_adapter_tests::test_state();
        scratch_log(&mut state, "press-own");
        arm_pair(&mut state);
        state.preview_bound.remove(&HWND_B);
        state.preview_last = Some("neighbour-preview".to_owned());
        assert_eq!(
            clear_windrag_gesture_scoped(&mut state, HWND_B, false),
            Some("w2".to_owned())
        );
        assert_no_maps(&state, HWND_B);
        assert!(state.active.contains(&HWND_A));
        assert!(state.windrag_bound.contains_key(&HWND_A));
        assert!(state.gesture_producer.contains_key(&HWND_A));
        assert!(state.preview_bound.contains_key(&HWND_A));
        assert!(state.gesture_preview_start.contains_key(&HWND_A));
        assert_eq!(state.preview_last.as_deref(), Some("neighbour-preview"));
        // B's journey is dead: a later Up settles nothing.
        windrag_up(&mut state, HWND_B, 50, 60, 2);
        assert!(!state.gesture_end_cursor.contains_key(&HWND_B));
    }

    #[test]
    fn cancel_clears_arm_preview_and_leaves_source_stationary() {
        // Owner-side Cancel (hook disarmed, paired Up swallowed hook-side):
        // the legacy clear drops every gesture map for the HWND with no
        // plan and no geometry change. Stable source and member rects are
        // identical after, and a later Up settles nothing.
        let mut state = super::rmax03_adapter_tests::test_state();
        scratch_log(&mut state, "cancel");
        arm_pair(&mut state);
        let stable_before = state.stable.clone();
        let rects_before = state.member_rects.clone();
        assert_eq!(
            clear_windrag_gesture(&mut state, HWND_A),
            Some("w1".to_owned())
        );
        assert_no_maps(&state, HWND_A);
        assert_eq!(state.stable, stable_before, "source stays stationary");
        assert_eq!(state.member_rects, rects_before, "no geometry writes");
        windrag_up(&mut state, HWND_A, 50, 60, 2);
        assert!(!state.gesture_end_cursor.contains_key(&HWND_A));
        assert_eq!(clear_windrag_gesture(&mut state, 0xDEAD), None);
    }

    #[test]
    fn same_output_fence_keeps_cross_output_release() {
        // Same-output gate through the production predicate: inside the
        // source domain may proceed to the Engine drop; taskbar/outside or
        // cross-output points refuse with no transfer.
        assert!(crate::tiling::drop_point_in_domain(
            0, 0, 1920, 1040, 100, 100
        ));
        assert!(crate::tiling::drop_point_in_domain(
            0, 0, 1920, 1040, 1919, 1039
        ));
        assert!(!crate::tiling::drop_point_in_domain(
            0, 0, 1920, 1040, 100, 1100
        ));
        assert!(!crate::tiling::drop_point_in_domain(
            0, 0, 1920, 1040, 2000, 100
        ));
        assert!(!crate::tiling::drop_point_in_domain(
            0, 0, 1920, 1040, -10, 100
        ));
    }

    #[test]
    fn shared_drop_contract_still_lands_on_preview() {
        // Drop contract press-focus leaves unchanged: preview a window-edge
        // target, carry the exact hover prior into the drop, and the mover
        // lands exactly on the preview rect (R-DRAG-07 split reads from
        // this agreement). Real Engine drop through the production command
        // shapes with `source: None`.
        use tiler_core::boundary::{CoreCommand, CoreEvent, CoreReply};
        use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
        use tiler_core::engine::Engine;
        use tiler_core::geometry::Rect;
        use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
        use tiler_core::session::OutputDomain;

        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("windrag-press").expect("generation");
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        };
        let domain = OutputDomain {
            id: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            bounds,
            gap: 8,
            adjacent: std::collections::BTreeMap::new(),
        };
        let window = |token: &str| tiler_core::seed::EngineWindow {
            window: WindowId(token.to_owned()),
            output: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            rect: bounds,
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
        };
        let event = |command: CoreCommand,
                     correlation: &str,
                     windows: Vec<tiler_core::seed::EngineWindow>| {
            CoreEvent {
                owner: owner.clone(),
                generation: generation.clone(),
                correlation: CorrelationId::parse(correlation).expect("correlation"),
                revision: 0,
                fingerprint: 7,
                domain: domain.clone(),
                domain_key: domain.key(),
                outer_gap: 8,
                focused_window: WindowId("w1".to_owned()),
                windows,
                directional: None,
                directional_target_outer_gap: None,
                target_domain: None,
                target_windows: Vec::new(),
                command,
            }
        };
        let rows = || vec![window("w1"), window("w2")];
        let mut engine = Engine::new();
        let geometry =
            match engine.handle(&event(CoreCommand::Reconcile, "corr-press-seed", rows())) {
                CoreReply::Projection(plan) => plan
                    .geometry
                    .iter()
                    .map(|g| (g.window.0.clone(), g.rect))
                    .collect::<Vec<_>>(),
                CoreReply::Tiled(plan) => plan
                    .geometry
                    .iter()
                    .map(|g| (g.window.0.clone(), g.rect))
                    .collect::<Vec<_>>(),
                other => panic!("seed must converge, got {other:?}"),
            };
        let target = geometry
            .iter()
            .find(|(token, _)| token == "w2")
            .map(|(_, rect)| *rect)
            .expect("sibling geometry");
        let at = (target.x + 2, target.y + target.h / 2);
        let (proposed, prior) = match engine.handle(&event(
            CoreCommand::DragPreview {
                window: "w1".to_owned(),
                x: at.0,
                y: at.1,
                hover_prior: None,
                source: None,
            },
            "corr-press-preview",
            rows(),
        )) {
            CoreReply::DragPreview(plan) => {
                (plan.preview.proposed_rect, plan.preview.hover_prior())
            }
            other => panic!("edge preview must resolve, got {other:?}"),
        };
        match engine.handle(&event(
            CoreCommand::DragDrop {
                window: "w1".to_owned(),
                x: at.0,
                y: at.1,
                hover_prior: Some(prior),
                source: None,
            },
            "corr-press-drop",
            rows(),
        )) {
            CoreReply::Tiled(plan) => {
                let placed = plan
                    .geometry
                    .iter()
                    .find(|g| g.window.0 == "w1")
                    .expect("mover placement");
                assert_eq!(placed.rect, proposed, "drop must equal preview");
            }
            other => panic!("carried drop must plan, got {other:?}"),
        }
    }
}
