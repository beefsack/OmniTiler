//! Native tray owner window, icon, menu, and first-run dialog.
//!
//! Sole Win32 owner for the tray surface: every `Shell_NotifyIconW` call,
//! icon-handle lifetime, popup-menu Win32, window class, `extern` procedure,
//! and owned dialog loop lives here. Safe policy (menu model, tooltip text,
//! persistence, spawn, logging) stays outside as pure helpers or
//! snapshot-based callouts inside `catch_unwind`.
//!
//! Borrow discipline: the registry proves provenance only. Unique borrowing
//! is enforced at runtime by `RefCell`: procedure and owner entries resolve
//! the validated cell and use `try_borrow`/`try_borrow_mut`. A failed borrow
//! means reentrant nesting while a guard is live, and the nested path fails
//! closed instead of aliasing. Guards are held across the `Shell_NotifyIconW`
//! sequences they serve (Explorer never dispatches this procedure, so no
//! nesting arises there); the modal menu pump and every safe callout run
//! with no guard held, from owned snapshots cloned beforehand. The registry
//! lock guards bookkeeping only and is never held across modality or
//! callouts.
//!
//! Threading: the window is confined to the creating (owner) thread; the
//! procedure runs there. Sinks and the stop predicate run on that thread,
//! must never call back into tray window functions, and must never hold
//! owner locks across a pump.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::core::GUID;

use crate::tray::{
    FIRST_RUN_TITLE, TASKBAR_CREATED_MESSAGE, TRAY_CALLBACK_MESSAGE, TRAY_ICON_SIZE,
    TRAY_WINDOW_CLASS, ToggleScope, TrayIconState, WorkspaceMenu, first_run_body,
    menu_items_with_pager, status_line,
};
use crate::tray_pager::SelectScope;

type DynError = Box<dyn std::error::Error>;

fn err(msg: impl Into<String>) -> DynError {
    Box::new(std::io::Error::other(msg.into()))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

const TRAY_ICON_ID: u32 = 1;

const TRAY_GUID: GUID = GUID {
    data1: 0x8F3A2B1C,
    data2: 0x4D5E,
    data3: 0x4F6A,
    data4: [0x9B, 0x8C, 0x7D, 0x6E, 0x5F, 0x40, 0x91, 0xA2],
};

/// Best-effort removal of a crash-ghost icon. Call only with no live owner.
#[must_use]
pub fn remove_stale_icon() -> bool {
    use windows_sys::Win32::UI::Shell::{NIF_GUID, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW};
    let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.uFlags = NIF_GUID;
    data.guidItem = TRAY_GUID;
    unsafe { Shell_NotifyIconW(NIM_DELETE, &data) != 0 }
}

struct Inner {
    hwnd: HWND,
    icon_normal: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
    icon_warning: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
    state: TrayIconState,
    showing_warning: bool,
    conflict_empty: bool,
    status: String,
    taskbar_created: u32,
    stop: Arc<AtomicBool>,
    stop_poll: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    menu_tracking: bool,
    log_path: Option<std::path::PathBuf>,
    workspace_current: Option<bool>,
    workspace_scope: Option<(String, String)>,
    workspace_default: bool,
    toggle: Arc<Mutex<Option<ToggleScope>>>,
    select: Arc<Mutex<Option<SelectScope>>>,
    pager_output: Option<String>,
    pager_ids: Vec<String>,
    pager_active: Option<String>,
    ordinal: Option<u32>,
    settings_dir: Option<std::path::PathBuf>,
}

impl Inner {
    fn current_icon(&self) -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
        if self.showing_warning {
            self.icon_warning
        } else {
            self.icon_normal
        }
    }

    fn notify(&mut self, message: u32) -> bool {
        use windows_sys::Win32::UI::Shell::{
            NIF_GUID, NIF_ICON, NIF_MESSAGE, NIF_TIP, NOTIFYICONDATAW, Shell_NotifyIconW,
        };
        let tip_text = crate::tray_sys::tooltip_text(!self.conflict_empty, &self.status);
        let mut tip = [0u16; 128];
        for (slot, unit) in tip_text.encode_utf16().take(127).enumerate() {
            tip[slot] = unit;
        }
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = TRAY_ICON_ID;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_GUID;
        data.guidItem = TRAY_GUID;
        data.uCallbackMessage = TRAY_CALLBACK_MESSAGE;
        data.hIcon = self.current_icon();
        data.szTip = tip;
        unsafe { Shell_NotifyIconW(message, &data) != 0 }
    }

    fn recover_taskbar(&mut self) {
        use crate::tray::TaskbarRecovery;
        use windows_sys::Win32::UI::Shell::{NIM_ADD, NIM_DELETE, NIM_MODIFY};
        let modify_ok = self.notify(NIM_MODIFY);
        let add_ok = if modify_ok {
            false
        } else {
            let _ = self.notify(NIM_DELETE);
            self.notify(NIM_ADD)
        };
        let outcome = match crate::tray::taskbar_recovery(modify_ok, add_ok) {
            TaskbarRecovery::Revalidated => {
                self.state.on_added();
                "revalidated"
            }
            TaskbarRecovery::ReAdded => {
                self.state.on_added();
                "re-added"
            }
            TaskbarRecovery::Pending => {
                self.state.on_removed();
                "pending"
            }
        };
        let path = self.log_path.clone();
        let value = serde_json::json!({"event": "tray-taskbar", "outcome": outcome});
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::tray_sys::append_tray_log(path, value);
        }));
    }

    fn add(&mut self) {
        use windows_sys::Win32::UI::Shell::NIM_ADD;
        if !self.state.needs_add() {
            return;
        }
        if self.notify(NIM_ADD) {
            self.state.on_added();
        }
    }

    fn sync_icon(&mut self) {
        use windows_sys::Win32::UI::Shell::NIM_MODIFY;
        if !self.state.is_added() {
            return;
        }
        let _ = self.notify(NIM_MODIFY);
    }

    fn remove(&mut self) {
        use windows_sys::Win32::UI::Shell::NIM_DELETE;
        if !self.state.is_added() {
            return;
        }
        let _ = self.notify(NIM_DELETE);
        self.state.on_removed();
    }
}

fn icon_from_bgra(
    pixels: &[u8],
) -> Result<windows_sys::Win32::UI::WindowsAndMessaging::HICON, DynError> {
    use windows_sys::Win32::Graphics::Gdi::CreateBitmap;
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO};
    assert_eq!(pixels.len(), TRAY_ICON_SIZE * TRAY_ICON_SIZE * 4);
    let color = unsafe { CreateBitmap(32, 32, 1, 32, pixels.as_ptr().cast()) };
    if color.is_null() {
        return Err(err("error: tray color bitmap failed"));
    }
    let mask = unsafe { CreateBitmap(32, 32, 1, 1, std::ptr::null()) };
    if mask.is_null() {
        unsafe {
            windows_sys::Win32::Graphics::Gdi::DeleteObject(color);
        }
        return Err(err("error: tray mask bitmap failed"));
    }
    let info = ICONINFO {
        fIcon: 1,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: color,
    };
    let icon = unsafe { CreateIconIndirect(&info) };
    unsafe {
        windows_sys::Win32::Graphics::Gdi::DeleteObject(color);
        windows_sys::Win32::Graphics::Gdi::DeleteObject(mask);
    }
    if icon.is_null() {
        return Err(err("error: tray CreateIcon failed"));
    }
    Ok(icon)
}

fn slots() -> &'static Mutex<HashMap<u64, usize>> {
    static SLOTS: OnceLock<Mutex<HashMap<u64, usize>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn hwnd_key(hwnd: HWND) -> u64 {
    hwnd as usize as u64
}

fn register_window(hwnd: HWND, state: *mut RefCell<Inner>) {
    if let Ok(mut guard) = slots().lock() {
        guard.insert(hwnd_key(hwnd), state as usize);
    }
}

fn unregister_window(hwnd: HWND) {
    if let Ok(mut guard) = slots().lock() {
        guard.remove(&hwnd_key(hwnd));
    }
}

fn cell_of(hwnd: HWND) -> Option<*mut RefCell<Inner>> {
    let user = unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
            hwnd,
            windows_sys::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
        )
    } as usize;
    if user == 0 {
        return None;
    }
    let registered = slots()
        .lock()
        .ok()
        .and_then(|g| g.get(&hwnd_key(hwnd)).copied())?;
    if registered != user {
        return None;
    }
    Some(registered as *mut RefCell<Inner>)
}

/// Checked shared access. `None` when unregistered or a guard is live
/// (nested reentrancy fails closed, never aliases).
fn with_inner<R>(hwnd: HWND, f: impl FnOnce(&Inner) -> R) -> Option<R> {
    let ptr = cell_of(hwnd)?;
    let guard = unsafe { &*ptr }.try_borrow().ok()?;
    Some(f(&guard))
}

/// Checked exclusive access scoped to `f`. `None` when unregistered or a
/// guard is live. Guards may span the `Shell_NotifyIconW` sequences (which
/// never dispatch this procedure); pumps and safe callouts use snapshots
/// cloned beforehand with no guard held.
fn with_inner_mut<R>(hwnd: HWND, f: impl FnOnce(&mut Inner) -> R) -> Option<R> {
    let ptr = cell_of(hwnd)?;
    let mut guard = unsafe { &*ptr }.try_borrow_mut().ok()?;
    Some(f(&mut guard))
}

const TRAY_MENU_STOP_TIMER_ID: usize = 0x54524159;
const TRAY_MENU_STOP_POLL_MS: u32 = 50;

#[derive(Clone)]
struct MenuSnapshot {
    conflict_empty: bool,
    status: String,
    workspace_current: Option<bool>,
    workspace_default: bool,
    pager_output: Option<String>,
    pager_ids: Vec<String>,
    pager_active: Option<String>,
    stop: Arc<AtomicBool>,
    stop_poll: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    toggle: Arc<Mutex<Option<ToggleScope>>>,
    select: Arc<Mutex<Option<SelectScope>>>,
    settings_dir: Option<std::path::PathBuf>,
    log_path: Option<std::path::PathBuf>,
    workspace_scope: Option<(String, String)>,
    workspace_current_raw: Option<bool>,
}

fn snapshot_of(inner: &Inner) -> MenuSnapshot {
    MenuSnapshot {
        conflict_empty: inner.conflict_empty,
        status: inner.status.clone(),
        workspace_current: inner.workspace_current,
        workspace_default: inner.workspace_default,
        pager_output: inner.pager_output.clone(),
        pager_ids: inner.pager_ids.clone(),
        pager_active: inner.pager_active.clone(),
        stop: Arc::clone(&inner.stop),
        stop_poll: inner.stop_poll.clone(),
        toggle: Arc::clone(&inner.toggle),
        select: Arc::clone(&inner.select),
        settings_dir: inner.settings_dir.clone(),
        log_path: inner.log_path.clone(),
        workspace_scope: inner.workspace_scope.clone(),
        workspace_current_raw: inner.workspace_current,
    }
}

fn show_menu(hwnd: HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CheckMenuRadioItem, CreatePopupMenu, DestroyMenu, GetCursorPos, KillTimer, MF_BYCOMMAND,
        MF_CHECKED, MF_DISABLED, MF_GRAYED, MF_SEPARATOR, MF_STRING, PostMessageW,
        SetForegroundWindow, SetTimer, TPM_RETURNCMD, TrackPopupMenu, WM_NULL,
    };
    let tracking_enter = with_inner(hwnd, |inner| inner.menu_tracking);
    let Some(currently_tracking) = tracking_enter else {
        return;
    };
    if !crate::tray::menu_track_enter(currently_tracking) {
        return;
    }
    let Some(snap) = with_inner(hwnd, snapshot_of) else {
        return;
    };
    let cached_workspace = WorkspaceMenu::new(snap.workspace_current, snap.workspace_default);
    let items = menu_items_with_pager(
        !snap.conflict_empty,
        &status_line(&snap.status),
        cached_workspace,
        &snap.pager_ids,
        snap.pager_active.as_deref(),
    );
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }
    for item in &items {
        let crate::tray::MenuItem::Command {
            id,
            label,
            enabled,
            visible,
            checked,
        } = item
        else {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::AppendMenuW(
                    menu,
                    MF_SEPARATOR,
                    0,
                    std::ptr::null(),
                );
            }
            continue;
        };
        if !visible {
            continue;
        }
        let pager_row = *id >= crate::tray_pager::MENU_ID_WORKSPACE_FIRST
            && (((*id - crate::tray_pager::MENU_ID_WORKSPACE_FIRST) as usize)
                < snap.pager_ids.len());
        let mut flags = MF_STRING;
        if *checked && !pager_row {
            flags |= MF_CHECKED;
        }
        if !enabled {
            flags |= MF_DISABLED | MF_GRAYED;
        }
        let text = wide(label);
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::AppendMenuW(
                menu,
                flags,
                *id as usize,
                text.as_ptr(),
            );
        }
    }
    let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point) } == 0 {
        unsafe {
            DestroyMenu(menu);
        }
        return;
    }
    if !snap.pager_ids.is_empty()
        && let Some(active) = snap
            .pager_active
            .as_deref()
            .and_then(|active| snap.pager_ids.iter().position(|id| id == active))
    {
        let first = crate::tray_pager::MENU_ID_WORKSPACE_FIRST;
        unsafe {
            CheckMenuRadioItem(
                menu,
                first,
                first + snap.pager_ids.len() as u32 - 1,
                first + active as u32,
                MF_BYCOMMAND,
            );
        }
    }
    with_inner_mut(hwnd, |inner| {
        inner.menu_tracking = true;
    });
    let timer_installed = snap.stop_poll.is_none()
        || unsafe { SetTimer(hwnd, TRAY_MENU_STOP_TIMER_ID, TRAY_MENU_STOP_POLL_MS, None) } != 0;
    if !crate::tray::menu_track_proceed(snap.stop_poll.is_some(), timer_installed) {
        with_inner_mut(hwnd, |inner| {
            inner.menu_tracking = false;
        });
        unsafe {
            DestroyMenu(menu);
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::tray_sys::append_tray_log(
                snap.log_path.clone(),
                serde_json::json!({"event": "tray-menu", "outcome": "timer-unavailable"}),
            );
        }));
        return;
    }
    unsafe {
        SetForegroundWindow(hwnd);
    }
    let picked = unsafe {
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD,
            point.x,
            point.y,
            0,
            hwnd,
            std::ptr::null(),
        )
    } as u32;
    with_inner_mut(hwnd, |inner| {
        inner.menu_tracking = false;
        if inner.stop_poll.is_some() {
            unsafe {
                KillTimer(hwnd, TRAY_MENU_STOP_TIMER_ID);
            }
        }
        unsafe {
            PostMessageW(hwnd, WM_NULL, 0, 0);
            DestroyMenu(menu);
        }
    });
    let stop_during = snap.stop_poll.as_ref().is_some_and(|poll| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| poll())).unwrap_or(false)
    }) || snap.stop.load(Ordering::SeqCst);
    let toggle_available = snap.workspace_current_raw.is_some() && snap.workspace_scope.is_some();
    let decision = crate::tray::resolve_menu_pick_with_pager(
        picked,
        stop_during,
        toggle_available,
        snap.pager_ids.len(),
    );
    match decision {
        crate::tray::MenuPickOutcome::SpawnSettings => {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::tray_sys::spawn_settings_detached();
            }));
        }
        crate::tray::MenuPickOutcome::RequestStop => {
            snap.stop.store(true, Ordering::SeqCst);
        }
        crate::tray::MenuPickOutcome::ToggleWorkspace => {
            if let Some((output, workspace)) = snap.workspace_scope
                && let Ok(mut guard) = snap.toggle.lock()
            {
                *guard = Some(ToggleScope::new(&output, &workspace));
            }
        }
        crate::tray::MenuPickOutcome::PersistDefault { default_tiled } => {
            let dir = snap.settings_dir.clone();
            let log = snap.log_path.clone();
            let (outcome, new_default) =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::tray_sys::persist_default_snapshot(dir, default_tiled)
                }))
                .unwrap_or(("panic", default_tiled));
            with_inner_mut(hwnd, |inner| {
                if outcome == "written" {
                    inner.workspace_default = new_default;
                }
                let path = log.clone();
                let value = serde_json::json!({
                    "event": "tray-default",
                    "default_tiled": default_tiled,
                    "outcome": outcome,
                });
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::tray_sys::append_tray_log(path, value);
                }));
            });
        }
        crate::tray::MenuPickOutcome::SelectWorkspace { index } => {
            if let (Some(output), Some(workspace)) = (snap.pager_output, snap.pager_ids.get(index))
                && let Ok(mut guard) = snap.select.lock()
            {
                *guard = Some(SelectScope::new(&output, workspace, index as u32 + 1));
            }
        }
        crate::tray::MenuPickOutcome::Noop => {}
    }
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CREATESTRUCTW, DefWindowProcW, EndMenu, GWLP_USERDATA, SetWindowLongPtrW, WM_CREATE,
        WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_LBUTTONUP, WM_RBUTTONUP, WM_SETTINGCHANGE,
        WM_TIMER,
    };
    if msg == WM_CREATE {
        let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        return 0;
    }
    let taskbar_hit = with_inner(hwnd, |inner| inner.taskbar_created);
    if let Some(taskbar_created) = taskbar_hit {
        if msg == taskbar_created {
            // Durable-owner broadcast: the pager overlays may not exist to
            // receive it, so forward through the shared invalidation flag
            // (atomic only, no logging or native work) before the icon
            // recovery below. The host consumes it once and refreshes the
            // pid-scoped hook scope on the same turn.
            super::pager::note_taskbar_rediscover();
            with_inner_mut(hwnd, |inner| {
                inner.recover_taskbar();
            });
            return 0;
        }
        if msg == WM_TIMER && wparam == TRAY_MENU_STOP_TIMER_ID {
            let snapshot = with_inner(hwnd, |inner| {
                (
                    inner.stop.load(Ordering::SeqCst),
                    inner.stop_poll.clone(),
                    inner.menu_tracking,
                )
            });
            if let Some((stop_flag, poll, tracking)) = snapshot {
                let poll_hit = poll.as_ref().is_some_and(|p| {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p())).unwrap_or(false)
                });
                if crate::tray::menu_cancel_due(stop_flag || poll_hit, tracking) {
                    unsafe {
                        EndMenu();
                    }
                }
            }
            return 0;
        }
        if msg == TRAY_CALLBACK_MESSAGE {
            let mouse = (lparam & 0xffff) as u32;
            if wparam as u32 == TRAY_ICON_ID && (mouse == WM_LBUTTONUP || mouse == WM_RBUTTONUP) {
                // The menu pump runs safe policy callouts: a panic must
                // never unwind across the `extern` edge (fail closed).
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    show_menu(hwnd);
                }));
            }
            return 0;
        }
    }
    if msg == WM_DESTROY {
        return 0;
    }
    if msg == WM_DISPLAYCHANGE || msg == WM_SETTINGCHANGE || msg == WM_DPICHANGED {
        crate::tiling_sys::mark_monitor_dirty();
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Live owner tray: hidden window plus exactly one notification icon.
///
/// The frame owns `Box<RefCell<Inner>>`; procedure and owner entries use
/// checked borrows so reentrant nesting fails closed instead of aliasing.
pub struct TrayWindow {
    hwnd: HWND,
    stop: Arc<AtomicBool>,
    toggle: Arc<Mutex<Option<ToggleScope>>>,
    select: Arc<Mutex<Option<SelectScope>>>,
    state: Box<RefCell<Inner>>,
    _confined: std::marker::PhantomData<Rc<()>>,
}

impl TrayWindow {
    pub fn create(
        conflict_empty: bool,
        settings_status: &str,
        log_path: Option<std::path::PathBuf>,
        toggle: &Arc<Mutex<Option<ToggleScope>>>,
        select: &Arc<Mutex<Option<SelectScope>>>,
        settings_dir: Option<std::path::PathBuf>,
        stop_poll: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    ) -> Result<Self, DynError> {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CS_HREDRAW, CS_VREDRAW, CreateWindowExW, RegisterClassW, RegisterWindowMessageW,
            WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
        };
        let class_w = wide(TRAY_WINDOW_CLASS);
        let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
        let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
        cls.style = CS_HREDRAW | CS_VREDRAW;
        cls.lpfnWndProc = Some(tray_wnd_proc);
        cls.hInstance = hinst;
        cls.lpszClassName = class_w.as_ptr();
        let atom = unsafe { RegisterClassW(&cls) };
        if atom == 0 {
            let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            if code != 1410 {
                return Err(err("error: tray RegisterClass failed"));
            }
        }
        let taskbar_created =
            unsafe { RegisterWindowMessageW(wide(TASKBAR_CREATED_MESSAGE).as_ptr()) };
        if taskbar_created == 0 {
            return Err(err("error: tray TaskbarCreated register failed"));
        }
        let icon_normal = icon_from_bgra(&crate::tray::tray_icon_bgra(false))?;
        let icon_warning = match icon_from_bgra(&crate::tray::tray_icon_bgra(true)) {
            Ok(icon) => icon,
            Err(error) => {
                unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon(icon_normal);
                }
                return Err(error);
            }
        };
        let stop = Arc::new(AtomicBool::new(false));
        let mut state = Box::new(RefCell::new(Inner {
            hwnd: std::ptr::null_mut(),
            icon_normal,
            icon_warning,
            state: TrayIconState::new(),
            showing_warning: !conflict_empty,
            conflict_empty,
            status: settings_status.to_owned(),
            taskbar_created,
            stop: Arc::clone(&stop),
            stop_poll,
            menu_tracking: false,
            log_path,
            workspace_current: None,
            workspace_scope: None,
            workspace_default: true,
            toggle: Arc::clone(toggle),
            select: Arc::clone(select),
            pager_output: None,
            pager_ids: Vec::new(),
            pager_active: None,
            ordinal: None,
            settings_dir,
        }));
        let raw: *mut RefCell<Inner> = &mut *state as *mut RefCell<Inner>;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class_w.as_ptr(),
                std::ptr::null(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinst,
                raw.cast(),
            )
        };
        if hwnd.is_null() {
            if let Ok(inner) = state.try_borrow() {
                let (normal, warning) = (inner.icon_normal, inner.icon_warning);
                unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon(normal);
                    windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon(warning);
                }
            }
            return Err(err("error: tray CreateWindow failed"));
        }
        if state
            .try_borrow_mut()
            .ok()
            .map(|mut inner| {
                inner.hwnd = hwnd;
                inner.add();
            })
            .is_none()
        {
            // Creation raced a live guard (unreachable on the owner thread
            // without a pump); tear down without an icon add.
            unregister_window(hwnd);
            return Err(err("error: tray CreateWindow failed"));
        }
        register_window(hwnd, raw);
        Ok(Self {
            hwnd,
            stop,
            toggle: Arc::clone(toggle),
            select: Arc::clone(select),
            state,
            _confined: std::marker::PhantomData,
        })
    }

    pub fn sync(&self, conflict_empty: bool, settings_status: &str, ordinal: Option<u32>) {
        let Ok(mut inner) = self.state.try_borrow_mut() else {
            return;
        };
        let warning = !conflict_empty;
        let presentation_changed = warning != inner.showing_warning
            || ordinal != inner.ordinal
            || settings_status != inner.status;
        inner.conflict_empty = conflict_empty;
        inner.status = settings_status.to_owned();
        if warning != inner.showing_warning || ordinal != inner.ordinal {
            let normal = icon_from_bgra(&crate::tray::tray_icon_bgra_with_number(false, ordinal));
            let warned = icon_from_bgra(&crate::tray::tray_icon_bgra_with_number(true, ordinal));
            match (normal, warned) {
                (Ok(normal), Ok(warned)) => {
                    unsafe {
                        windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon(inner.icon_normal);
                        windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon(
                            inner.icon_warning,
                        );
                    }
                    inner.icon_normal = normal;
                    inner.icon_warning = warned;
                    inner.showing_warning = warning;
                    inner.ordinal = ordinal;
                }
                (Ok(icon), Err(_)) | (Err(_), Ok(icon)) => unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon(icon);
                },
                (Err(_), Err(_)) => {}
            }
        }
        if inner.state.needs_add() {
            inner.add();
        } else if presentation_changed {
            inner.sync_icon();
        }
    }

    #[must_use]
    pub fn stop_requested(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    #[must_use]
    pub fn hwnd_value(&self) -> u64 {
        self.hwnd as usize as u64
    }

    pub fn take_toggle_request(&self) -> Option<ToggleScope> {
        self.toggle
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
    }

    pub fn has_toggle_request(&self) -> bool {
        self.toggle
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .is_some()
    }

    pub fn has_select_request(&self) -> bool {
        self.select
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .is_some()
    }

    pub fn take_select_request(&self) -> Option<SelectScope> {
        self.select
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
    }

    pub fn sync_pager(&self, output: Option<String>, ids: Vec<String>, active: Option<String>) {
        if let Ok(mut inner) = self.state.try_borrow_mut() {
            inner.pager_output = output;
            inner.pager_ids = ids;
            inner.pager_active = active;
        }
    }

    pub fn sync_workspace(
        &self,
        scope: Option<(String, String)>,
        current_tiled: Option<bool>,
        default_tiled: bool,
    ) {
        if let Ok(mut inner) = self.state.try_borrow_mut() {
            inner.workspace_scope = scope;
            inner.workspace_current = current_tiled;
            inner.workspace_default = default_tiled;
        }
    }
}

impl Drop for TrayWindow {
    fn drop(&mut self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DestroyIcon, DestroyWindow, GWLP_USERDATA, SetWindowLongPtrW,
        };
        unregister_window(self.hwnd);
        // Snapshot icons under a checked guard, then release before the
        // synchronous destroy (whose `WM_DESTROY` path fails closed on the
        // live guard instead of aliasing).
        let icons = self.state.try_borrow_mut().ok().map(|mut inner| {
            inner.remove();
            (inner.icon_normal, inner.icon_warning)
        });
        unsafe {
            if let Some((normal, warning)) = icons {
                DestroyIcon(normal);
                DestroyIcon(warning);
            }
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            DestroyWindow(self.hwnd);
        }
    }
}

/// First-run prompt outcome on the normal owner path only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstRunOutcome {
    Authentic,
    Compatible,
    Dismissed,
}

impl FirstRunOutcome {
    #[must_use]
    pub const fn choice(self) -> Option<crate::tray::FirstRunChoice> {
        match self {
            Self::Authentic => Some(crate::tray::FirstRunChoice::Authentic),
            Self::Compatible => Some(crate::tray::FirstRunChoice::Compatible),
            Self::Dismissed => None,
        }
    }
}

pub const FIRST_RUN_WINDOW_CLASS: &str = "OmniTilerFirstRun";

const FIRST_RUN_AUTHENTIC: u32 = 1;
const FIRST_RUN_COMPATIBLE: u32 = 2;
const FIRST_RUN_STOP_POLL_MS: u32 = 50;

struct FirstRunBox {
    font: windows_sys::Win32::Graphics::Gdi::HFONT,
    outcome: Arc<Mutex<FirstRunOutcome>>,
}

fn first_run_slots() -> &'static Mutex<HashMap<u64, usize>> {
    static SLOTS: OnceLock<Mutex<HashMap<u64, usize>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn first_run_register(hwnd: HWND, state: *mut RefCell<FirstRunBox>) {
    if let Ok(mut guard) = first_run_slots().lock() {
        guard.insert(hwnd_key(hwnd), state as usize);
    }
}

fn first_run_unregister(hwnd: HWND) {
    if let Ok(mut guard) = first_run_slots().lock() {
        guard.remove(&hwnd_key(hwnd));
    }
}

fn first_run_validated(hwnd: HWND) -> Option<*mut RefCell<FirstRunBox>> {
    let user = unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
            hwnd,
            windows_sys::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
        )
    } as usize;
    if user == 0 {
        return None;
    }
    let registered = first_run_slots()
        .lock()
        .ok()
        .and_then(|g| g.get(&hwnd_key(hwnd)).copied())?;
    if registered != user {
        return None;
    }
    Some(registered as *mut RefCell<FirstRunBox>)
}

unsafe extern "system" fn first_run_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CREATESTRUCTW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, SetWindowLongPtrW, WM_CLOSE,
        WM_COMMAND, WM_CREATE, WM_DESTROY,
    };
    match msg {
        WM_CREATE => {
            let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xffff) as u32;
            let notify = ((wparam >> 16) & 0xffff) as u32;
            if notify == 0 && (id == FIRST_RUN_AUTHENTIC || id == FIRST_RUN_COMPATIBLE) {
                if let Some(ptr) = first_run_validated(hwnd) {
                    // Shared guard only to clone the outcome channel; the
                    // mutex set below runs with no guard held.
                    let channel = unsafe { &*ptr }
                        .try_borrow()
                        .ok()
                        .map(|boxed| Arc::clone(&boxed.outcome));
                    let outcome = if id == FIRST_RUN_AUTHENTIC {
                        FirstRunOutcome::Authentic
                    } else {
                        FirstRunOutcome::Compatible
                    };
                    if let Some(channel) = channel {
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            *channel.lock().unwrap_or_else(|poison| poison.into_inner()) = outcome;
                        }));
                    }
                }
                unsafe {
                    DestroyWindow(hwnd);
                }
            }
            0
        }
        WM_CLOSE => {
            unsafe {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => 0,
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn first_run_font(dpi: u32) -> windows_sys::Win32::Graphics::Gdi::HFONT {
    use windows_sys::Win32::Graphics::Gdi::{CreateFontW, DEFAULT_GUI_FONT, GetStockObject};
    let height = -((9 * dpi as i32 + 36) / 72);
    let face = wide("Segoe UI");
    let font = unsafe { CreateFontW(height, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, face.as_ptr()) };
    if font.is_null() {
        unsafe { GetStockObject(DEFAULT_GUI_FONT) }
    } else {
        font
    }
}

pub struct FirstRunWindow {
    hwnd: HWND,
    state: Box<RefCell<FirstRunBox>>,
    _confined: std::marker::PhantomData<Rc<()>>,
}

impl Drop for FirstRunWindow {
    fn drop(&mut self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DestroyWindow, GWLP_USERDATA, SetWindowLongPtrW,
        };
        first_run_unregister(self.hwnd);
        let font = self
            .state
            .try_borrow()
            .map(|boxed| boxed.font)
            .unwrap_or(std::ptr::null_mut());
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            if windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(self.hwnd) != 0 {
                DestroyWindow(self.hwnd);
            }
            if !font.is_null() {
                windows_sys::Win32::Graphics::Gdi::DeleteObject(font);
            }
        }
    }
}

pub fn first_run_dialog(stop: impl Fn() -> bool) -> FirstRunOutcome {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BS_DEFPUSHBUTTON, BS_PUSHBUTTON, CreateWindowExW, DestroyWindow, DispatchMessageW,
        ES_AUTOVSCROLL, ES_MULTILINE, ES_READONLY, GetSystemMetrics, IDC_ARROW, IsDialogMessageW,
        IsWindow, LoadCursorW, MSG, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW,
        QS_ALLINPUT, RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNORMAL, SetForegroundWindow,
        ShowWindow, TranslateMessage, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN,
        WS_EX_DLGMODALFRAME, WS_EX_TOPMOST, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
    };
    let outcome = Arc::new(Mutex::new(FirstRunOutcome::Dismissed));
    let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class_w = wide(FIRST_RUN_WINDOW_CLASS);
    let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
    cls.lpfnWndProc = Some(first_run_wnd_proc);
    cls.hInstance = hinst;
    cls.hCursor = unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) };
    cls.hbrBackground = unsafe {
        windows_sys::Win32::Graphics::Gdi::GetSysColorBrush(
            windows_sys::Win32::Graphics::Gdi::COLOR_BTNFACE,
        )
    };
    cls.lpszClassName = class_w.as_ptr();
    let atom = unsafe { RegisterClassW(&cls) };
    if atom == 0 {
        let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        if code != 1410 {
            return FirstRunOutcome::Dismissed;
        }
    }
    let dpi = unsafe { GetDpiForSystem() };
    let dpi = if dpi == 0 { 96 } else { dpi };
    let px = |value: i32| ((i64::from(value) * i64::from(dpi) + 48) / 96) as i32;
    let (client_w, client_h) = (px(500), px(268));
    let style = WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
    let mut rect = windows_sys::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: client_h,
    };
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::AdjustWindowRect(&mut rect, style, 0);
    }
    let win_w = rect.right - rect.left;
    let win_h = rect.bottom - rect.top;
    let (screen_w, screen_h) =
        unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    let (pos_x, pos_y) = (
        ((screen_w - win_w) / 2).max(0),
        ((screen_h - win_h) / 2).max(0),
    );
    let font = first_run_font(dpi);
    let mut owned = Box::new(RefCell::new(FirstRunBox {
        font,
        outcome: Arc::clone(&outcome),
    }));
    let raw: *mut RefCell<FirstRunBox> = &mut *owned;
    let title_w = wide(FIRST_RUN_TITLE);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME | WS_EX_TOPMOST,
            class_w.as_ptr(),
            title_w.as_ptr(),
            style,
            pos_x,
            pos_y,
            win_w,
            win_h,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            raw.cast(),
        )
    };
    if hwnd.is_null() {
        if let Ok(boxed) = owned.try_borrow() {
            unsafe {
                windows_sys::Win32::Graphics::Gdi::DeleteObject(boxed.font);
            }
        }
        return FirstRunOutcome::Dismissed;
    }
    first_run_register(hwnd, raw);
    let window = FirstRunWindow {
        hwnd,
        state: owned,
        _confined: std::marker::PhantomData,
    };
    let make_child = |class: &str,
                      text: &str,
                      id: u32,
                      button_style: u32,
                      x: i32,
                      y: i32,
                      w: i32,
                      h: i32| unsafe {
        CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | button_style,
            px(x),
            px(y),
            px(w),
            px(h),
            hwnd,
            id as isize as windows_sys::Win32::UI::WindowsAndMessaging::HMENU,
            hinst,
            std::ptr::null(),
        )
    };
    let text_style = ES_MULTILINE as u32
        | ES_READONLY as u32
        | ES_AUTOVSCROLL as u32
        | WS_VSCROLL
        | WS_BORDER
        | WS_TABSTOP;
    let body = first_run_body();
    let text_ok = {
        let class_w = wide("EDIT");
        let body_w = wide(&body);
        unsafe {
            CreateWindowExW(
                0,
                class_w.as_ptr(),
                body_w.as_ptr(),
                WS_CHILD | WS_VISIBLE | text_style,
                px(10),
                px(10),
                px(480),
                px(196),
                hwnd,
                std::ptr::null_mut(),
                hinst,
                std::ptr::null(),
            )
        }
    };
    let authentic_ok = make_child(
        "BUTTON",
        "Authentic",
        FIRST_RUN_AUTHENTIC,
        BS_DEFPUSHBUTTON as u32 | WS_TABSTOP,
        230,
        218,
        124,
        30,
    );
    let compatible_ok = make_child(
        "BUTTON",
        "Compatible",
        FIRST_RUN_COMPATIBLE,
        BS_PUSHBUTTON as u32 | WS_TABSTOP,
        366,
        218,
        124,
        30,
    );
    if text_ok.is_null() || authentic_ok.is_null() || compatible_ok.is_null() {
        unsafe {
            DestroyWindow(hwnd);
        }
    } else {
        let set_font = |child: windows_sys::Win32::Foundation::HWND| unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW(
                child,
                windows_sys::Win32::UI::WindowsAndMessaging::WM_SETFONT,
                font as usize,
                1,
            );
        };
        set_font(text_ok);
        set_font(authentic_ok);
        set_font(compatible_ok);
        unsafe {
            ShowWindow(hwnd, SW_SHOWNORMAL);
            SetForegroundWindow(hwnd);
        }
    }
    loop {
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        let mut quit = false;
        while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            if msg.message == windows_sys::Win32::UI::WindowsAndMessaging::WM_QUIT {
                quit = true;
                break;
            }
            if unsafe { IsWindow(hwnd) } == 0 {
                continue;
            }
            if msg.message == windows_sys::Win32::UI::WindowsAndMessaging::WM_KEYDOWN
                && msg.wParam == 0x1B
            {
                unsafe {
                    DestroyWindow(hwnd);
                }
                continue;
            }
            let eaten = unsafe { IsDialogMessageW(hwnd, std::ptr::addr_of!(msg)) };
            if eaten == 0 {
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        if quit || unsafe { IsWindow(hwnd) } == 0 {
            break;
        }
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(&stop)).unwrap_or(false) {
            unsafe {
                DestroyWindow(hwnd);
            }
            continue;
        }
        unsafe {
            MsgWaitForMultipleObjectsEx(
                0,
                std::ptr::null(),
                FIRST_RUN_STOP_POLL_MS,
                QS_ALLINPUT,
                0,
            );
        }
    }
    if unsafe { IsWindow(hwnd) } != 0 {
        unsafe {
            DestroyWindow(hwnd);
        }
    }
    {
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        while unsafe {
            PeekMessageW(
                &mut msg,
                std::ptr::null_mut(),
                windows_sys::Win32::UI::WindowsAndMessaging::WM_QUIT,
                windows_sys::Win32::UI::WindowsAndMessaging::WM_QUIT,
                PM_REMOVE,
            )
        } != 0
        {}
    }
    drop(window);
    *outcome.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_build_headless() {
        for warning in [false, true] {
            let icon = icon_from_bgra(&crate::tray::tray_icon_bgra(warning))
                .expect("icon builds headless");
            assert!(!icon.is_null());
            unsafe {
                assert_ne!(
                    windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon(icon),
                    0
                );
            }
        }
    }
}
