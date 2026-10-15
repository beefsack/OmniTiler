//! Native Windows notification tray (`cfg(windows)` only).
//!
//! Mechanism: one hidden owner-pump top-level window (class
//! [`crate::tray::TRAY_WINDOW_CLASS`]) owns exactly one official
//! `Shell_NotifyIconW` icon. The window procedure receives the tray callback
//! ([`crate::tray::TRAY_CALLBACK_MESSAGE`]) and the registered
//! `TaskbarCreated` broadcast: Explorer restarts recover MODIFY-first (a
//! surviving icon answers `NIM_MODIFY`, so no duplicate add is ever issued)
//! with a tolerant remove plus re-add only when the icon is truly gone, and
//! exactly one bounded `tray-taskbar` line carries the outcome.
//! Left-click and right-click both open the menu (KDE parity: the icon opens
//! its menu, never Settings directly).
//!
//! Menu commands: the top conflict row and Settings spawn the existing
//! `tiler-windows settings` UI as a detached child (the per-user singleton
//! mutex brings an open window forward instead); Stop sets an atomic the
//! owner loop polls next to its own stop request, so Stop follows the normal
//! owner cleanup (ledger reveal, Snap restore, watcher stop). Proof owners
//! never create this window.
//!
//! Crash recovery holds no persistent state: graceful stop issues
//! `NIM_DELETE`, while a dead owner leaves only an Explorer-side ghost that
//! the next startup or independent restore removes with a cross-process
//! `NIM_DELETE` against the stable icon GUID below (no live owner then, so
//! no live icon can be disturbed). The single-owner ledger lease guarantees
//! at most one live owner (hence at most one live icon).
//!
//! Stable automation surface: window class `OmniTilerTray`, menu ids
//! 1001 (conflict), 1003 (Settings), 1004 (Stop), first-run prompt title
//! `OmniTiler - First Run`.
//!
//! GUID cross-process semantics (Microsoft Learn, `NOTIFYICONDATAW` plus the
//! `NotificationIcon` classic sample): once an icon is added with `NIF_GUID`
//! plus `guidItem`, every later call addressing that icon must carry the same
//! `NIF_GUID`/`guidItem`, and `NIM_DELETE` needs only those two fields — a
//! zero `hWnd`/`uID` from a later same-user process removes the stale
//! registration (the sample's `DeleteNotificationIcon` does exactly this).

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::core::GUID;

use crate::tray::{
    FIRST_RUN_TITLE, TASKBAR_CREATED_MESSAGE, TRAY_CALLBACK_MESSAGE, TRAY_ICON_SIZE,
    TRAY_TOOLTIP_TITLE, TRAY_WINDOW_CLASS, ToggleScope, TrayIconState, WorkspaceMenu,
    first_run_body, menu_items_with_pager, status_line,
};
use crate::tray_pager::SelectScope;

type DynError = Box<dyn std::error::Error>;

fn err(msg: impl Into<String>) -> DynError {
    Box::new(std::io::Error::other(msg.into()))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// Our single notification-icon id within the owner window.
const TRAY_ICON_ID: u32 = 1;

/// Stable project icon identifier. One fixed GUID, no ledger, no registry:
/// every add/modify/delete carries `NIF_GUID` plus this value, so a later
/// same-user process can delete a crash ghost with [`remove_stale_icon`]
/// while no live owner holds the lease.
const TRAY_GUID: GUID = GUID {
    data1: 0x8F3A2B1C,
    data2: 0x4D5E,
    data3: 0x4F6A,
    data4: [0x9B, 0x8C, 0x7D, 0x6E, 0x5F, 0x40, 0x91, 0xA2],
};

/// Best-effort removal of a crash-ghost icon. Call only with no live owner
/// (under the owner lease): the GUID addresses exactly this project's icon,
/// so a live owner's icon cannot exist then. Returns true when Explorer
/// accepted the delete. Same-user trust governs this call like every other
/// ledger-gated recovery step.
#[must_use]
pub fn remove_stale_icon() -> bool {
    use windows_sys::Win32::UI::Shell::{NIF_GUID, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW};
    let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.uFlags = NIF_GUID;
    data.guidItem = TRAY_GUID;
    unsafe { Shell_NotifyIconW(NIM_DELETE, &data) != 0 }
}

/// Tooltip text: identity plus the short settings status, with a conflict
/// suffix when the warning badge is up. Bounded for `szTip`.
#[must_use]
pub fn tooltip_text(conflict: bool, settings_status: &str) -> String {
    let short: String = settings_status.chars().take(48).collect();
    let mut tip = format!("{TRAY_TOOLTIP_TITLE} - Enabled ({short})");
    if conflict {
        tip.push_str(" - check settings");
    }
    tip.chars().take(120).collect()
}

/// Build one `HICON` from 32x32 BGRA bytes (opaque mask: every pixel shows).
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
    // All-zero 1bpp mask: every pixel opaque (alpha rides the color bits).
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
    /// Graceful-stop poll observed by the menu-thread stop timer while modal
    /// tracking blocks the owner pump (`None` keeps legacy tracking). Set once
    /// at creation from the owner loop's exact-owner check.
    stop_poll: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    /// True while one modal track owns this thread: reentrant opens refuse
    /// and the stop timer only cancels then. Same-thread only.
    menu_tracking: bool,
    log_path: Option<std::path::PathBuf>,
    /// Cached current-workspace tiled state (`None` without a readable
    /// scope: the checkbox disables). Refreshed by `sync_workspace`.
    workspace_current: Option<bool>,
    /// Opaque `(output, workspace)` scope the checkbox rendered with. A
    /// toggle pick carries exactly this scope to the owner loop, which
    /// refuses it when the live active pair no longer matches.
    workspace_scope: Option<(String, String)>,
    /// Cached persisted new-workspace default. Refreshed by
    /// `sync_workspace` and by successful default picks below.
    workspace_default: bool,
    /// Scoped workspace-toggle request (rendered output+workspace); the
    /// owner loop drains it once and refuses stale/mismatched scopes.
    toggle: Arc<Mutex<Option<ToggleScope>>>,
    /// Scoped pager-select request (rendered output+workspace+ordinal); the
    /// owner loop drains it once via `take_select_request` and selects that
    /// exact workspace only when the rendered output is still focused and
    /// the workspace is still live there.
    select: Arc<Mutex<Option<SelectScope>>>,
    /// Cached focused-output pager rows for the next menu open: the output
    /// key, its workspace ids in order, and the active id. Refreshed by
    /// `sync_pager`; no syscalls here.
    pager_output: Option<String>,
    pager_ids: Vec<String>,
    pager_active: Option<String>,
    /// Focused-output active 1-based ordinal rendered into the icon glyph
    /// (`None` keeps the plain glyph). Refreshed by `sync`.
    ordinal: Option<u32>,
    /// Settings directory for persisting the default choice (normal owner
    /// only; `None` leaves the default rows as no-ops with a log line).
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
        let mut tip = [0u16; 128];
        for (slot, unit) in tooltip_text(!self.conflict_empty, &self.status)
            .encode_utf16()
            .take(127)
            .enumerate()
        {
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

    /// `TaskbarCreated` recovery: a surviving icon answers `NIM_MODIFY` (a
    /// duplicate `NIM_ADD` would fail and retry forever), otherwise a
    /// tolerant remove plus re-add. Exactly one bounded `tray-taskbar` line
    /// carries the outcome; a still-pending add retries on the next sync.
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
        self.log_line(serde_json::json!({"event": "tray-taskbar", "outcome": outcome}));
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

fn inner_of(hwnd: HWND) -> *mut Inner {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GWLP_USERDATA, GetWindowLongPtrW};
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Inner }
}

/// Stop-cancel window timer id for menu tracking (per-window: no other timer
/// exists on the owner). Distinctive value for Spy++ readability.
const TRAY_MENU_STOP_TIMER_ID: usize = 0x54524159;
/// Stop-poll cadence inside menu tracking (matches the owner tick and the
/// first-run stop poll).
const TRAY_MENU_STOP_POLL_MS: u32 = 50;

fn show_menu(hwnd: HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CheckMenuRadioItem, CreatePopupMenu, DestroyMenu, GetCursorPos, KillTimer, MF_BYCOMMAND,
        MF_CHECKED, MF_DISABLED, MF_GRAYED, MF_SEPARATOR, MF_STRING, PostMessageW,
        SetForegroundWindow, SetTimer, TPM_RETURNCMD, TrackPopupMenu, WM_NULL,
    };
    let raw = inner_of(hwnd);
    if raw.is_null() {
        return;
    }
    // Reentrant opens refuse while one track owns this thread (a second tray
    // callback dispatched by the modal track never starts its own menu).
    if !crate::tray::menu_track_enter(unsafe { (&*raw).menu_tracking }) {
        return;
    }
    let cached = unsafe { &*raw };
    let cached_conflicts_empty = cached.conflict_empty;
    let cached_status = cached.status.clone();
    let cached_workspace = WorkspaceMenu::new(cached.workspace_current, cached.workspace_default);
    let cached_pager_output = cached.pager_output.clone();
    let cached_pager_ids = cached.pager_ids.clone();
    let cached_pager_active = cached.pager_active.clone();
    let stop_poll = cached.stop_poll.clone();
    // Rebuild the visible menu from cached state: no syscalls here, so a
    // stale poll can never project a wrong conflict row or workspace check.
    let items = menu_items_with_pager(
        !cached_conflicts_empty,
        &status_line(&cached_status),
        cached_workspace,
        &cached_pager_ids,
        cached_pager_active.as_deref(),
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
        // The disabled status row renders greyed; every other visible row is
        // a live command. The workspace checkbox and default choices render
        // their truthful checked state; pager rows skip the check here and
        // get the radio bullet below (`AppendMenuW` draws checks only).
        let pager_row = *id >= crate::tray_pager::MENU_ID_WORKSPACE_FIRST
            && (((*id - crate::tray_pager::MENU_ID_WORKSPACE_FIRST) as usize)
                < cached_pager_ids.len());
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
    // Radio bullet for the active pager row over its id range, by command:
    // every other check above is untouched. Skipped when the active id is
    // unknown (rows stay unchecked, like the model).
    if !cached_pager_ids.is_empty()
        && let Some(active) = cached_pager_active
            .as_deref()
            .and_then(|active| cached_pager_ids.iter().position(|id| id == active))
    {
        let first = crate::tray_pager::MENU_ID_WORKSPACE_FIRST;
        unsafe {
            CheckMenuRadioItem(
                menu,
                first,
                first + cached_pager_ids.len() as u32 - 1,
                first + active as u32,
                MF_BYCOMMAND,
            );
        }
    }
    unsafe {
        (&mut *raw).menu_tracking = true;
    }
    // The modal track below blocks the owner pump, so a window timer observes
    // the graceful-stop poll on this same menu thread and `EndMenu` cancels
    // the track. Without a poll (`None`) tracking behaves as before. A failed
    // install fails closed: the owned menu is destroyed without tracking, so
    // graceful stop cannot starve on an untracked blocking menu.
    let timer_installed = !stop_poll.is_some()
        || unsafe { SetTimer(hwnd, TRAY_MENU_STOP_TIMER_ID, TRAY_MENU_STOP_POLL_MS, None) } != 0;
    if !crate::tray::menu_track_proceed(stop_poll.is_some(), timer_installed) {
        unsafe {
            (&mut *raw).menu_tracking = false;
            DestroyMenu(menu);
        }
        unsafe {
            (&*raw).log_line(
                serde_json::json!({"event": "tray-menu", "outcome": "timer-unavailable"}),
            );
        }
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
    // Dismissal contract for `SetForegroundWindow` menus: the posted null
    // lets the menu close when the user clicks away instead of lingering. The
    // guard clears before teardown so a queued stale timer cannot cancel
    // anything; the timer dies and the menu is destroyed exactly once.
    unsafe {
        (&mut *raw).menu_tracking = false;
        if stop_poll.is_some() {
            KillTimer(hwnd, TRAY_MENU_STOP_TIMER_ID);
        }
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
    }
    // Stop precedence (first-run post-decision analogue): a stop observed
    // while open discards every racing pick except Stop itself, so teardown
    // spawns nothing, flips nothing and persists nothing.
    let stop_during = stop_poll.as_ref().is_some_and(|poll| poll())
        || unsafe { (&*raw).stop.load(Ordering::SeqCst) };
    let toggle_available =
        unsafe { (&*raw).workspace_current.is_some() && (&*raw).workspace_scope.is_some() };
    match crate::tray::resolve_menu_pick_with_pager(
        picked,
        stop_during,
        toggle_available,
        cached_pager_ids.len(),
    ) {
        crate::tray::MenuPickOutcome::SpawnSettings => {
            spawn_settings_detached();
        }
        crate::tray::MenuPickOutcome::RequestStop => {
            let inner = unsafe { &*raw };
            inner.stop.store(true, Ordering::SeqCst);
        }
        crate::tray::MenuPickOutcome::ToggleWorkspace => {
            // Session-only toggle: carry the rendered opaque scope to the owner
            // loop, which drains it once on its pump and flips the mode there
            // only when the live active pair still matches (never here, so a
            // stale menu can never project or flip a wrong check).
            let inner = unsafe { &*raw };
            if let Some((output, workspace)) = inner.workspace_scope.clone() {
                *inner
                    .toggle
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner()) =
                    Some(ToggleScope::new(&output, &workspace));
            }
        }
        crate::tray::MenuPickOutcome::PersistDefault { default_tiled } => {
            let inner = unsafe { &mut *raw };
            persist_default_choice(inner, default_tiled);
        }
        crate::tray::MenuPickOutcome::SelectWorkspace { index } => {
            // Exact-workspace select: carry the rendered output, workspace
            // id and ordinal to the owner loop, which drains it once on its
            // pump and selects it only when the rendered output is still
            // focused and the workspace is still live there (never here, so
            // a stale menu can never select a wrong workspace).
            if let (Some(output), Some(workspace)) =
                (cached_pager_output, cached_pager_ids.get(index))
            {
                let inner = unsafe { &*raw };
                *inner
                    .select
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner()) =
                    Some(SelectScope::new(&output, workspace, index as u32 + 1));
            }
        }
        crate::tray::MenuPickOutcome::Noop => {}
    }
}

/// Persist one new-workspace default choice (`true` for Tiled): load,
/// modify only `workspace.default_tiled`, and atomically save. Only the
/// default persists; per-workspace session overrides never touch the file.
/// An invalid on-disk file refuses with a log line and no write.
fn persist_default_choice(inner: &mut Inner, default_tiled: bool) {
    let outcome = (|| -> &'static str {
        let Some(dir) = inner.settings_dir.clone() else {
            return "no-settings-dir";
        };
        let mut settings = match crate::settings::load_from_dir(&dir) {
            crate::settings::LoadOutcome::Loaded(settings) => settings,
            crate::settings::LoadOutcome::Missing => crate::settings::Settings::default(),
            crate::settings::LoadOutcome::Invalid(_) => return "refused-invalid",
        };
        if settings.core.workspace.default_tiled == default_tiled {
            return "unchanged";
        }
        settings.core.workspace.default_tiled = default_tiled;
        match crate::settings::save_to_dir(&dir, &mut settings) {
            Ok(()) => {
                inner.workspace_default = default_tiled;
                "written"
            }
            Err(_) => "write-failed",
        }
    })();
    inner.log_line(serde_json::json!({
        "event": "tray-default",
        "default_tiled": default_tiled,
        "outcome": outcome,
    }));
}

/// Open the existing native Settings window as a detached child: the
/// per-user singleton mutex brings an open window forward instead of
/// racing it, and the owner pump never blocks.
fn spawn_settings_detached() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let _ = std::process::Command::new(exe).arg("settings").spawn();
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CREATESTRUCTW, DefWindowProcW, EndMenu, GWLP_USERDATA, SetWindowLongPtrW, WM_CREATE,
        WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WM_TIMER,
    };
    if msg == WM_CREATE {
        let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        return 0;
    }
    // TaskbarCreated arrives as the registered message id, never WM_CREATE.
    let raw = inner_of(hwnd);
    if !raw.is_null() {
        let inner = unsafe { &mut *raw };
        if msg == inner.taskbar_created {
            inner.recover_taskbar();
            return 0;
        }
        if msg == WM_TIMER && wparam == TRAY_MENU_STOP_TIMER_ID {
            // Menu-thread stop cancel on the exact owned window, never a
            // title search. The live tracking guard (not a literal) decides,
            // so with no active track this is a no-op by construction.
            let stop_now = inner.stop.load(Ordering::SeqCst)
                || inner.stop_poll.as_ref().is_some_and(|poll| poll());
            if crate::tray::menu_cancel_due(stop_now, inner.menu_tracking) {
                unsafe {
                    EndMenu();
                }
            }
            return 0;
        }
        if msg == TRAY_CALLBACK_MESSAGE {
            let mouse = (lparam & 0xffff) as u32;
            if wparam as u32 == TRAY_ICON_ID && (mouse == WM_LBUTTONUP || mouse == WM_RBUTTONUP) {
                show_menu(hwnd);
            }
            return 0;
        }
    }
    if msg == WM_DESTROY {
        // Owned by `TrayOwner::drop` (detach-then-destroy there): never
        // free here, so exactly one owner exists either way.
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Live owner tray: hidden window plus exactly one notification icon.
pub struct TrayOwner {
    hwnd: HWND,
    stop: Arc<AtomicBool>,
    toggle: Arc<Mutex<Option<ToggleScope>>>,
    select: Arc<Mutex<Option<SelectScope>>>,
}

impl TrayOwner {
    /// Create the hidden owner window, both glyph variants, and the icon.
    /// `conflict_empty` and `settings_status` seed the first presentation;
    /// `sync` refreshes them later. Icon-add failure is not fatal: the next
    /// `sync` retries through the state gate. `log_path` receives the single
    /// bounded `tray-taskbar` line per Explorer restart, if any, plus the
    /// bounded `tray-default` lines for default picks. `toggle` carries the
    /// scoped workspace-toggle request to the owner loop (drained once via
    /// `take_toggle_request`); `select` carries the scoped pager-select
    /// request (drained once via `take_select_request`); `settings_dir`
    /// persists default picks
    /// (normal owner only). `stop_poll` is the graceful-stop predicate the
    /// menu-thread stop timer observes while modal tracking blocks the owner
    /// pump (`None` keeps legacy tracking).
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
        let inner = Box::new(Inner {
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
        });
        let raw = Box::into_raw(inner);
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
            unsafe {
                drop(Box::from_raw(raw));
            }
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon(icon_normal);
                windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon(icon_warning);
            }
            return Err(err("error: tray CreateWindow failed"));
        }
        unsafe {
            (*raw).hwnd = hwnd;
            (*raw).add();
        }
        Ok(Self {
            hwnd,
            stop,
            toggle: Arc::clone(toggle),
            select: Arc::clone(select),
        })
    }

    /// Refresh the badge/number/tooltip from live owner state and retry a
    /// pending add through the state gate. Change-gated: steady state issues
    /// no syscalls, so the 100 ms pump costs nothing when nothing changed.
    /// `ordinal` is the focused-output active 1-based workspace number
    /// (`None` keeps the plain glyph); a changed number rebuilds both glyph
    /// variants with the warning badge preserved. A failed rebuild keeps the
    /// old pair fail-closed and retries on the next change.
    pub fn sync(&mut self, conflict_empty: bool, settings_status: &str, ordinal: Option<u32>) {
        let raw = inner_of(self.hwnd);
        if raw.is_null() {
            return;
        }
        let inner = unsafe { &mut *raw };
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
            // Tooltip/status/number refresh rides one modify; icon swaps above.
            inner.sync_icon();
        }
    }

    /// True after the user picks Stop from the menu.
    #[must_use]
    pub fn stop_requested(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    /// Raw owner window value for exact owned-UI foreground comparison: the
    /// per-pump focus computation treats this foreground as our own
    /// transient menu, never as a genuine output focus change.
    #[must_use]
    pub fn hwnd_value(&self) -> u64 {
        self.hwnd as usize as u64
    }

    /// Drain one pending scoped workspace-toggle request (set by the menu,
    /// consumed once by the owner loop). Returns `None` when no pick waits.
    pub fn take_toggle_request(&self) -> Option<ToggleScope> {
        self.toggle
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
    }

    /// True when a pager-select pick waits (non-draining peek so the pump
    /// can wake for it before the dispatch site drains it once).
    pub fn has_select_request(&self) -> bool {
        self.select
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .is_some()
    }

    /// Drain one pending scoped pager-select request (set by the menu,
    /// consumed once by the owner loop). Returns `None` when no pick waits.
    pub fn take_select_request(&self) -> Option<SelectScope> {
        self.select
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
    }

    /// Refresh the cached pager section from live owner state: the focused
    /// output key (`None` when unknown), its workspace ids in order, and the
    /// active id. The next menu open renders exactly this; no syscalls here.
    /// An empty id list omits the section.
    pub fn sync_pager(&mut self, output: Option<String>, ids: Vec<String>, active: Option<String>) {
        let raw = inner_of(self.hwnd);
        if raw.is_null() {
            return;
        }
        let inner = unsafe { &mut *raw };
        inner.pager_output = output;
        inner.pager_ids = ids;
        inner.pager_active = active;
    }

    /// Refresh the cached workspace section from live owner state: the opaque
    /// scope the checkbox rendered with, its tiled state, and the persisted
    /// default. The next menu open renders it; no syscalls here. `None`
    /// scope disables only the checkbox while the default choices stay
    /// usable.
    pub fn sync_workspace(
        &mut self,
        scope: Option<(String, String)>,
        current_tiled: Option<bool>,
        default_tiled: bool,
    ) {
        let raw = inner_of(self.hwnd);
        if raw.is_null() {
            return;
        }
        let inner = unsafe { &mut *raw };
        inner.workspace_scope = scope;
        inner.workspace_current = current_tiled;
        inner.workspace_default = default_tiled;
    }
}

impl Drop for TrayOwner {
    /// Plain RAII teardown on every path (graceful stop and early errors
    /// alike): `NIM_DELETE` when added, both glyphs destroyed, window
    /// destroyed, owner box freed exactly once.
    fn drop(&mut self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DestroyIcon, DestroyWindow, GWLP_USERDATA, SetWindowLongPtrW,
        };
        let raw = inner_of(self.hwnd);
        if raw.is_null() {
            return;
        }
        unsafe {
            let inner = &mut *raw;
            inner.remove();
            DestroyIcon(inner.icon_normal);
            DestroyIcon(inner.icon_warning);
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            DestroyWindow(self.hwnd);
            drop(Box::from_raw(raw));
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

/// Owned first-run dialog window class (project-specific, like the tray
/// class). The dialog is an exact owned `HWND`: creation, dismissal, and
/// teardown all address that handle, never a title search.
pub const FIRST_RUN_WINDOW_CLASS: &str = "OmniTilerFirstRun";

/// Dialog button: stage the authentic catalog defaults.
const FIRST_RUN_AUTHENTIC: u32 = 1;
/// Dialog button: disable every OS-conflicting chord.
const FIRST_RUN_COMPATIBLE: u32 = 2;
/// Stop-poll cadence inside the dialog loop (matches the owner tick).
const FIRST_RUN_STOP_POLL_MS: u32 = 50;

/// Owned dialog state. Boxed before creation and reclaimed in `WM_DESTROY`
/// (same shape as the settings window); the outcome rides an `Arc` the caller
/// keeps, so the loop reads the exact choice after teardown with no post-free
/// access. Same-thread only: the dialog runs on the calling owner thread.
struct FirstRunBox {
    font: windows_sys::Win32::Graphics::Gdi::HFONT,
    outcome: Arc<Mutex<FirstRunOutcome>>,
}

unsafe extern "system" fn first_run_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CREATESTRUCTW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetWindowLongPtrW,
        PostQuitMessage, SetWindowLongPtrW, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY,
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
            // Push-button clicks only (`BN_CLICKED`): focus/paint
            // notifications must never stage a preset.
            let id = (wparam & 0xffff) as u32;
            let notify = ((wparam >> 16) & 0xffff) as u32;
            if notify == 0 && (id == FIRST_RUN_AUTHENTIC || id == FIRST_RUN_COMPATIBLE) {
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                if ptr != 0 {
                    let boxed = unsafe { &*(ptr as *mut FirstRunBox) };
                    let outcome = if id == FIRST_RUN_AUTHENTIC {
                        FirstRunOutcome::Authentic
                    } else {
                        FirstRunOutcome::Compatible
                    };
                    *boxed
                        .outcome
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner()) = outcome;
                }
                // Own window, own thread: `DestroyWindow` runs `WM_DESTROY`
                // synchronously, reclaiming the box below.
                unsafe {
                    DestroyWindow(hwnd);
                }
            }
            0
        }
        WM_CLOSE => {
            // Dismissed: the outcome stays `Dismissed`, no write follows.
            unsafe {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if ptr != 0 {
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    let boxed = Box::from_raw(ptr as *mut FirstRunBox);
                    windows_sys::Win32::Graphics::Gdi::DeleteObject(boxed.font);
                    drop(boxed);
                }
            }
            unsafe {
                PostQuitMessage(0);
            }
            0
        }
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

/// Cancellable owned first-run dialog on the calling (owner) thread: one
/// exact owned top-level window with directly labeled Authentic/Compatible
/// buttons, no `MessageBoxW` (whose documented dismissal needs a Cancel
/// button this prompt must not offer) and no title search. The loop pumps the
/// thread queue and polls `stop` every 50 ms; a stop destroys the owned window
/// and the loop only returns after the actual `WM_QUIT`, so no UI can outlive
/// the call. Close/Escape dismisses with no write. Creation failure also
/// reports `Dismissed` (fail-closed, nothing published). The caller still
/// rechecks stop before publishing so a stop racing the choice wins.
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
    // Client layout at 96-DPI units: wrapped readonly text plus two buttons.
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
    let boxed = Box::new(FirstRunBox {
        font,
        outcome: Arc::clone(&outcome),
    });
    let raw = Box::into_raw(boxed);
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
        // No window exists, so `WM_CREATE` never ran and `WM_DESTROY` never
        // will: reclaim the box here and release the created font with it.
        unsafe {
            let boxed = Box::from_raw(raw);
            windows_sys::Win32::Graphics::Gdi::DeleteObject(boxed.font);
            drop(boxed);
        }
        return FirstRunOutcome::Dismissed;
    }
    // Wrapped readonly text (same control shape as the settings info box) and
    // the two directly labeled choice buttons. Any control failure tears down
    // the owned window (reclaiming the box synchronously) and dismisses.
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
    // `wide` temporaries must outlive the call only; each child copies what it
    // needs at creation, so per-call vectors are sufficient.
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
        // Owned window torn down before any show; the pump loop below
        // observes the posted quit at once and exits through the single
        // drain, so no separate return path is needed here.
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
    // Owner-thread modal loop: pump the queue, then wait (own queue only) so
    // a stop is observed within 50 ms. A stop destroys the owned window and
    // the loop keeps pumping until the actual `WM_QUIT`; the only exits are
    // the observed quit and the destroyed-window backstop, so no UI outlives
    // this call and no timeout is ever needed.
    loop {
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        let mut quit = false;
        while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            if msg.message == windows_sys::Win32::UI::WindowsAndMessaging::WM_QUIT {
                quit = true;
                break;
            }
            if unsafe { IsWindow(hwnd) } == 0 {
                // Owned window already gone (a stop/Escape destroys, then keeps
                // pumping to the quit): drop the message, never route a dead
                // handle to dialog handling or dispatch.
                continue;
            }
            // Escape dismisses through the close path below.
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
        if stop() {
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
    // Every loop exit destroys the exact owned window first: a pre-existing
    // `WM_QUIT` can break the loop above while it still lives. The destroy
    // runs `WM_DESTROY` synchronously and posts its own quit, which the drain
    // below removes, so later thread-queue waits never observe stale state.
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
    *outcome.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray::{MENU_ID_CONFLICT, MENU_ID_SETTINGS, MENU_ID_STATUS, menu_items};

    #[test]
    fn tooltip_carries_status_and_conflict_suffix() {
        let plain = tooltip_text(false, "saved:3");
        assert!(plain.contains(TRAY_TOOLTIP_TITLE));
        assert!(plain.contains("saved:3"));
        assert!(!plain.contains("check settings"));
        let warned = tooltip_text(true, "saved:3");
        assert!(warned.contains("check settings"));
        let long = "x".repeat(400);
        assert!(tooltip_text(true, &long).len() <= 128);
    }

    #[test]
    fn icons_build_and_destroy_headless() {
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

    #[test]
    fn menu_model_projects_cached_conflict_visibility() {
        // Sys menu builder consumes the portable model: visible conflict
        // row exactly when the cache is non-empty.
        let workspace = crate::tray::WorkspaceMenu::new(Some(true), true);
        let shown = menu_items(true, &status_line("saved:2"), workspace);
        let hidden = menu_items(false, &status_line("saved:2"), workspace);
        let visible_of = |items: &Vec<crate::tray::MenuItem>| {
            items.iter().find_map(|item| match item {
                crate::tray::MenuItem::Command { id, visible, .. } if *id == MENU_ID_CONFLICT => {
                    Some(*visible)
                }
                _ => None,
            })
        };
        assert_eq!(visible_of(&shown), Some(true));
        assert_eq!(visible_of(&hidden), Some(false));
        assert!(shown.iter().any(|item| matches!(
            item,
            crate::tray::MenuItem::Command {
                id: MENU_ID_STATUS,
                ..
            }
        )));
        assert!(shown.iter().any(|item| matches!(
            item,
            crate::tray::MenuItem::Command {
                id: MENU_ID_SETTINGS,
                ..
            }
        )));
    }
}
