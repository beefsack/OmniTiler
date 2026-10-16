//! Native Win32 settings window (`cfg(windows)` only).
//!
//! Mechanism: one overlapped settings window with plain Win32 child controls
//! (`STATIC`, `EDIT`, `BUTTON`, `LISTBOX`) created through the existing
//! `windows-sys` route. No new dependency, no registry/policy writes, no tray.
//! The window edits a draft [`crate::settings::Settings`]; Apply validates the
//! whole draft (including the selected binding editor) and saves atomically,
//! Revert reloads the last saved values without writing, and Close never
//! applies. The running owner adopts saved revisions live on its own pump
//! (`settings-applied`); this window never claims the save as applied.
//!
//! Stable automation surface: window class [`SETTINGS_WINDOW_CLASS`] with the
//! control ids below, launched via `tiler-windows settings` (plus the
//! `settings` just recipe through the Explorer broker) and closed with the
//! Close button, `Alt+F4`, `Escape`, or `WM_CLOSE`.
//!
//! Concurrency is stale-content refusal plus one UI instance, not a file
//! lock: Apply re-reads the file and refuses (reloading the newer values)
//! when another writer created or changed the document since this window
//! loaded (full-document compare, with a missing file distinguished from a
//! saved revision-1 file). A per-user named mutex (`Local\` plus the caller
//! SID) keeps a second Settings window from racing the first: the second
//! call brings the existing window forward and returns. Edits from other
//! tools stay best-effort freshness. An invalid on-disk file shows its error
//! and disables Apply; nothing is ever silently reset.

use std::path::PathBuf;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::HFONT;

use crate::settings::{BindingState, Settings, binding_catalog};

use self::choice::{attach_choice_tips, choice_tip_texts};
use self::ctl::{
    Ctl, abort_create, create_ctl, edit_limit, get_text, message_font, px, set_text, wide,
};
use self::draft::{
    apply_rebind_text, do_apply, do_preset, do_revert, load_base, refresh_all, stage_state,
};
use self::info::{refresh_info, refresh_list};

mod choice;
mod ctl;
mod draft;
mod info;
#[cfg(test)]
mod tests;

pub(super) type DynError = Box<dyn std::error::Error>;

pub(super) fn err(msg: impl Into<String>) -> DynError {
    Box::new(std::io::Error::other(msg.into()))
}

/// Native window class for the settings window.
pub const SETTINGS_WINDOW_CLASS: &str = "OmniTilerSettings";
/// Window title for the settings window.
pub const SETTINGS_WINDOW_TITLE: &str = "OmniTiler Settings";

// Stable control ids for later `SendMessage` automation.
pub const ID_INNER_GAP: u32 = 101;
pub const ID_OUTER_GAP: u32 = 102;
pub const ID_BORDER_ENABLED: u32 = 103;
pub const ID_BORDER_THEME: u32 = 104;
pub const ID_BORDER_COLOR: u32 = 105;
pub const ID_BORDER_WIDTH: u32 = 106;
pub const ID_BORDER_GAP: u32 = 107;
pub const ID_BORDER_RADIUS: u32 = 108;
pub const ID_UNDERLAY_ENABLED: u32 = 109;
pub const ID_UNDERLAY_COLOR: u32 = 110;
pub const ID_UNDERLAY_EXTENSION: u32 = 111;
pub const ID_KEYBOARD_TAKEOVER: u32 = 112;
pub const ID_MOUSE_SNAP: u32 = 113;
pub const ID_SNAP_MEANING: u32 = 114;
pub const ID_ALLOW_WIN_L: u32 = 115;
pub const ID_LOCK_WARNING: u32 = 116;
pub const ID_PRESET_AUTHENTIC: u32 = 117;
pub const ID_PRESET_COMPATIBLE: u32 = 118;
pub const ID_PRESET_EXPLAIN: u32 = 119;
pub const ID_BINDING_LIST: u32 = 120;
pub const ID_BINDING_INFO: u32 = 121;
pub const ID_BIND_KEEP: u32 = 122;
pub const ID_BIND_DISABLE: u32 = 123;
pub const ID_BIND_CHORD: u32 = 124;
pub const ID_BIND_REBIND: u32 = 125;
pub const ID_REBIND_NOTE: u32 = 126;
pub const ID_APPLY: u32 = 127;
pub const ID_REVERT: u32 = 128;
pub const ID_CLOSE: u32 = 129;
pub const ID_STATUS: u32 = 130;
pub const ID_DEFAULT_TILED: u32 = 131;
pub const ID_DEFAULT_FLOATING: u32 = 132;
pub const ID_SAME_AXIS_GROUP: u32 = 133;
pub const ID_SAME_AXIS_SWAP: u32 = 134;
pub const ID_BORN_MAX_TILE: u32 = 135;
pub const ID_BORN_MAX_OVERLAY: u32 = 136;
pub const ID_FIXED_SIZE_BOTH: u32 = 137;
pub const ID_FIXED_SIZE_EITHER: u32 = 138;
pub const ID_MIGRATION_REFILL_LAST: u32 = 139;
pub const ID_MIGRATION_REFILL_MRU: u32 = 140;
pub const ID_WORKSPACE_MODE_LOCAL: u32 = 141;
pub const ID_WORKSPACE_MODE_GLOBAL: u32 = 142;
pub const ID_WORKSPACE_MODE_SHARED: u32 = 143;
/// Taskbar pager opt-out checkbox (default on).
pub const ID_TASKBAR_PAGER: u32 = 144;
/// Xbox mode Settings page opener (per selected Xbox conflict only).
pub const ID_GAMING_OPEN_XBOX: u32 = 145;
/// Game Bar settings page opener (per selected Game Bar conflict only).
pub const ID_GAMING_OPEN_GAMEBAR: u32 = 146;
/// Game Bar app-settings page opener (preferred user-owned removal route).
pub const ID_GAMING_OPEN_APP: u32 = 147;
/// Explicit gaming recheck (async worker, never blocks the UI).
pub const ID_GAMING_RECHECK: u32 = 148;

/// Poll timer for the Settings-process gaming worker (1s; the worker itself
/// stays attempt-gated at its 5-minute interval, so steady-state ticks spawn
/// nothing).
const GAMING_TIMER_ID: usize = 1;
const GAMING_TIMER_MS: u32 = 1000;

const ESCAPE_VK: usize = 0x1B;

/// Draft plus saved state for one open settings window.
pub(super) struct App {
    font: HFONT,
    controls: Vec<(u32, HWND)>,
    dir: PathBuf,
    /// Last-good saved values (what Revert restores).
    base: Settings,
    /// True when `base` came from an existing file; false when the file was
    /// absent at load (defaults). Distinguishes a missing file from a saved
    /// revision-1 file so two fresh windows cannot silently overwrite each
    /// other.
    base_present: bool,
    /// Editable draft; presets and the binding editor mutate the bindings,
    /// Apply additionally reads every core control into the core section.
    work: Settings,
    /// Selection into the catalog order backing the listbox.
    selected: Option<usize>,
    /// Set while the on-disk file is invalid: Apply stays disabled.
    invalid: Option<String>,
    /// R-MOV-03 native tooltip text buffers (wide, NUL-terminated), one per
    /// radio, built from the shared options table, followed by the two
    /// R-MAX-06 born-maximized buffers, the two R-SPC-04 D1 fixed-size
    /// buffers and the two R-WS-12e G-37 migration-refill buffers in the
    /// same order. Owned by the boxed
    /// `App` so the pointers handed to the tooltip control stay valid for the
    /// whole window lifetime: the tooltip is a child window, destroyed with
    /// its parent before `WM_DESTROY` reclaims this box.
    tip_text: [Vec<u16>; 8],
    /// Settings-process gaming worker (own async monitor: requested at
    /// startup, refreshed on Recheck/timer, never waited on by the UI).
    gaming: crate::gaming::GamingMonitor,
    /// Last worker generation rendered into the binding info; the timer only
    /// rewrites the info when a pass completes.
    gaming_generation: u64,
}

impl App {
    fn ctl(&self, id: u32) -> HWND {
        self.controls
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, hwnd)| *hwnd)
            .unwrap_or(std::ptr::null_mut())
    }
}

pub(super) fn status(app: &App, text: &str) {
    set_text(app.ctl(ID_STATUS), text);
}

fn catalog_index_of(id: &str) -> Option<usize> {
    binding_catalog().iter().position(|def| def.id == id)
}

/// Open one allowlisted gaming Settings page without claiming remediation.
/// Success reports the navigation only; failure points at the manual path in
/// the binding info above and leaves the conflict unresolved.
fn open_gaming_uri(app: &App, uri: &str, what: &str) {
    if crate::tray_sys::gaming::open_navigation_uri(uri) {
        status(
            app,
            &format!(
                "Opened {what} settings (no change applied; Recheck after your change; navigation never marks this resolved)."
            ),
        );
    } else {
        status(
            app,
            &format!(
                "Could not open {what} settings; use the manual path in the binding info above. This stays unresolved."
            ),
        );
    }
}

fn on_command(app: &mut App, id: u32, notify: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BN_CLICKED, LB_GETCURSEL, LB_SETCURSEL, LBN_SELCHANGE, SendMessageW,
    };
    if id == ID_BINDING_LIST {
        if notify == LBN_SELCHANGE {
            let picked = unsafe { SendMessageW(app.ctl(ID_BINDING_LIST), LB_GETCURSEL, 0, 0) };
            if picked >= 0 {
                // A typed-but-unset chord belongs to the old row: flush it
                // there first. On refusal the edit is preserved, the old row
                // is re-selected, and the switch does not happen silently.
                let pending = get_text(app.ctl(ID_BIND_CHORD));
                if !pending.trim().is_empty() {
                    if let Err(reason) = apply_rebind_text(app, &pending) {
                        if let Some(old) = app.selected {
                            unsafe {
                                SendMessageW(app.ctl(ID_BINDING_LIST), LB_SETCURSEL, old, 0);
                            }
                        }
                        status(
                            app,
                            &format!(
                                "Error: {reason}. Press Set rebind (or clear the edit) before switching rows."
                            ),
                        );
                        return;
                    }
                    set_text(app.ctl(ID_BIND_CHORD), "");
                }
                app.selected = Some(picked as usize);
                refresh_info(app);
            }
        }
        return;
    }
    if notify != BN_CLICKED {
        return;
    }
    match id {
        ID_APPLY => do_apply(app),
        ID_REVERT => do_revert(app),
        // ID_CLOSE never reaches here: `wnd_proc` destroys the window without
        // materializing the `App` borrow (see below), so the synchronous
        // `WM_DESTROY` free below never nests inside a live `&mut App`.
        ID_PRESET_AUTHENTIC => do_preset(app, crate::settings::Preset::Authentic),
        ID_PRESET_COMPATIBLE => do_preset(app, crate::settings::Preset::Compatible),
        ID_BIND_KEEP => {
            if let Err(reason) = stage_state(app, BindingState::Keep) {
                status(app, &format!("Error: {reason}."));
            }
        }
        ID_BIND_DISABLE => {
            if let Err(reason) = stage_state(app, BindingState::Disabled) {
                status(app, &format!("Error: {reason}."));
            }
        }
        ID_BIND_REBIND => {
            let text = get_text(app.ctl(ID_BIND_CHORD));
            if let Err(reason) = apply_rebind_text(app, &text) {
                status(app, &format!("Error: {reason}."));
            } else {
                set_text(app.ctl(ID_BIND_CHORD), "");
                status(
                    app,
                    "Rebind staged (unsaved). Apply to save, Revert to discard.",
                );
            }
        }
        ID_GAMING_OPEN_XBOX => {
            open_gaming_uri(app, crate::gaming::XBOX_SETTINGS_URI, "Xbox mode");
        }
        ID_GAMING_OPEN_GAMEBAR => {
            open_gaming_uri(app, crate::gaming::GAMEBAR_SETTINGS_URI, "Game Bar");
        }
        ID_GAMING_OPEN_APP => {
            let uri = crate::gaming::gamebar_app_settings_uri();
            open_gaming_uri(app, &uri, "Game Bar app");
        }
        ID_GAMING_RECHECK => {
            // Fire-and-forget: the worker pass runs off the UI thread and the
            // timer rewrites the info when its generation lands.
            app.gaming.recheck();
            status(
                app,
                "Recheck requested; evidence refreshes when the probe completes. Navigation never marks a conflict resolved.",
            );
        }
        _ => {}
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CREATESTRUCTW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetWindowLongPtrW,
        PostQuitMessage, SetWindowLongPtrW, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_TIMER,
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
            if id == ID_CLOSE {
                // Close without borrowing `App`: `DestroyWindow` runs
                // `WM_DESTROY` synchronously, which reclaims the box. Routing
                // through `on_command` would nest that free inside a live
                // `&mut App`; destroying here keeps exactly one owner.
                unsafe {
                    DestroyWindow(hwnd);
                }
                return 0;
            }
            let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if ptr != 0 {
                let app = unsafe { &mut *(ptr as *mut App) };
                let notify = ((wparam >> 16) & 0xffff) as u32;
                on_command(app, id, notify);
            }
            0
        }
        WM_CLOSE => {
            unsafe {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_TIMER => {
            // Settings-process poll: never blocks; rewrites the binding info
            // only when a worker pass lands, and keeps the infrequent
            // interval arm honest about external (Settings-made) changes.
            // Completed probe lines are dropped here, not logged: this
            // separate Settings process owns no log writer (the running
            // owner's pump is the single writer and logs its own identical
            // passes), and writing to the owner log would break that
            // single-writer integrity. The queue stays bounded regardless.
            let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if ptr != 0 {
                let app = unsafe { &mut *(ptr as *mut App) };
                drop(app.gaming.drain_logs());
                app.gaming.maybe_refresh(Some(app.work.revision));
                let generation = app.gaming.snapshot().generation;
                if generation != app.gaming_generation {
                    app.gaming_generation = generation;
                    refresh_list(app);
                }
            }
            0
        }
        WM_DESTROY => {
            use windows_sys::Win32::UI::WindowsAndMessaging::KillTimer;
            unsafe {
                KillTimer(hwnd, GAMING_TIMER_ID);
            }
            let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if ptr != 0 {
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    let app = Box::from_raw(ptr as *mut App);
                    if !app.font.is_null() {
                        windows_sys::Win32::Graphics::Gdi::DeleteObject(app.font);
                    }
                    drop(app);
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

/// One held per-user settings mutex for the life of the open window.
struct SingletonGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

impl Drop for SingletonGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}

/// Bring an already-open settings window forward (restore then foreground).
fn focus_existing_settings() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow,
    };
    let class_w = wide(SETTINGS_WINDOW_CLASS);
    let hwnd = unsafe { FindWindowW(class_w.as_ptr(), std::ptr::null()) };
    if hwnd.is_null() {
        return;
    }
    unsafe {
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
    }
}

/// Acquire the per-user settings singleton (`Local\` plus the caller SID).
/// `Ok(None)` means another Settings window already holds it: the existing
/// window was brought forward and the caller must return without writing.
fn acquire_settings_singleton() -> Result<Option<SingletonGuard>, DynError> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let sid = crate::native::current_identity()
        .map_err(|error| err(format!("error: settings identity: {error}")))?
        .user_sid;
    let safe: String = sid
        .chars()
        .map(|c| if c == '\\' || c == '/' { '_' } else { c })
        .collect();
    let name = format!("Local\\OmniTilerSettings-{safe}");
    let name_w = wide(&name);
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name_w.as_ptr()) };
    if handle.is_null() {
        return Err(err("error: settings single-instance lock failed"));
    }
    // 183 = ERROR_ALREADY_EXISTS: we do not own the mutex, so close our
    // handle, forward the existing window, and report the clean refusal.
    if unsafe { GetLastError() } == 183 {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(handle);
        }
        focus_existing_settings();
        return Ok(None);
    }
    Ok(Some(SingletonGuard { handle }))
}

/// Open the modal settings window and run its message loop. Returns when the
/// window closes; Close never applies.
pub fn cmd_settings() -> Result<String, DynError> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::SystemServices::SS_LEFT;
    use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRect, BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BS_DEFPUSHBUTTON, BS_GROUPBOX,
        BS_PUSHBUTTON, CreateWindowExW, DispatchMessageW, ES_AUTOHSCROLL, ES_AUTOVSCROLL,
        ES_MULTILINE, ES_READONLY, GetMessageW, IDC_ARROW, IsDialogMessageW, LBS_NOINTEGRALHEIGHT,
        LBS_NOTIFY, LoadCursorW, RegisterClassW, SW_SHOWNORMAL, ShowWindow, TranslateMessage,
        WM_KEYDOWN, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CLIPCHILDREN, WS_EX_DLGMODALFRAME,
        WS_GROUP, WS_HSCROLL, WS_MINIMIZEBOX, WS_SYSMENU, WS_TABSTOP, WS_VSCROLL,
    };
    crate::tiling_sys::ensure_pm_v2()?;
    let _singleton = match acquire_settings_singleton()? {
        Some(guard) => guard,
        None => return Ok("settings already open".to_owned()),
    };
    let dir = crate::native::settings_directory().map_err(|e| err(format!("error: {e}")))?;
    let (base, present, invalid) = load_base(&dir);
    let dpi = unsafe { GetDpiForSystem() };
    let dpi = if dpi == 0 { 96 } else { dpi };
    let font = message_font(dpi);

    let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class_w = wide(SETTINGS_WINDOW_CLASS);
    let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
    cls.lpfnWndProc = Some(wnd_proc);
    cls.hInstance = hinst;
    cls.hCursor = unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) };
    // System dialog-face brush: a zero brush leaves the background dark and
    // unpainted. The brush is owned by the system, never deleted here.
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
            return Err(err("error: settings RegisterClass failed"));
        }
    }

    let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
    let client_w = px(924, dpi);
    let client_h = px(1088, dpi);
    let mut rect = windows_sys::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: client_h,
    };
    unsafe {
        AdjustWindowRect(&mut rect, style, 0);
    }
    let title_w = wide(SETTINGS_WINDOW_TITLE);
    // Settings-process gaming worker: read-only current-user probes on a
    // detached thread, requested once here; Recheck and the 1s poll timer
    // below refresh it without ever blocking this UI.
    let gaming =
        crate::gaming::GamingMonitor::new(std::sync::Arc::new(crate::tray_sys::gaming::probe_all));
    gaming.sync_revision(Some(base.revision));
    gaming.request_refresh(crate::gaming::RefreshReason::Startup);
    // Boxed before creation; reclaimed in WM_DESTROY. Pointer travels via
    // CREATESTRUCT lpCreateParams into GWLP_USERDATA.
    let app = Box::new(App {
        font,
        controls: Vec::new(),
        dir,
        base: base.clone(),
        base_present: present,
        work: base,
        selected: Some(catalog_index_of("focus-left").unwrap_or(0)),
        invalid: invalid.clone(),
        tip_text: choice_tip_texts(),
        gaming,
        gaming_generation: 0,
    });
    let raw = Box::into_raw(app);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            class_w.as_ptr(),
            title_w.as_ptr(),
            style,
            120,
            80,
            rect.right - rect.left,
            rect.bottom - rect.top,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            raw.cast(),
        )
    };
    if hwnd.is_null() {
        // No window exists, so WM_CREATE never ran and WM_DESTROY never will:
        // the manual drop is the single owner (no WM_NCDESTROY reclaim here).
        unsafe {
            drop(Box::from_raw(raw));
        }
        return Err(err("error: settings CreateWindow failed"));
    }
    let tab = WS_TABSTOP;
    let edits = ES_AUTOHSCROLL as u32;
    let push = BS_PUSHBUTTON as u32;
    let check = BS_AUTOCHECKBOX as u32;
    let radio = BS_AUTORADIOBUTTON as u32;
    // First radio of each auto pair starts its own WS_GROUP: without this
    // every auto radio in the dialog shares one exclusion group, so picking
    // a same-axis choice would clear the workspace default (and vice versa).
    let radio_first = radio | WS_GROUP;
    let group = BS_GROUPBOX as u32;
    let defpush = BS_DEFPUSHBUTTON as u32;
    let label = SS_LEFT;
    let list_style =
        LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32 | WS_VSCROLL | WS_HSCROLL | WS_BORDER;
    let edit_style = WS_BORDER | edits;
    // Selected-row readout: a readonly multiline edit with its own scrollbar
    // so the full OS-conflict text stays readable instead of clipping in an
    // 84px static.
    let info_style = ES_MULTILINE as u32
        | ES_AUTOVSCROLL as u32
        | ES_READONLY as u32
        | WS_VSCROLL
        | WS_BORDER
        | tab;
    let controls = [
        Ctl { id: 0, class: "BUTTON", text: "Gaps, tiles (0..64)".to_owned(), x: 10, y: 8, w: 272, h: 64, style: group },
        Ctl { id: 0, class: "STATIC", text: "Inner:".to_owned(), x: 20, y: 32, w: 52, h: 20, style: label },
        Ctl { id: ID_INNER_GAP, class: "EDIT", text: String::new(), x: 76, y: 30, w: 56, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "STATIC", text: "Outer:".to_owned(), x: 142, y: 32, w: 52, h: 20, style: label },
        Ctl { id: ID_OUTER_GAP, class: "EDIT", text: String::new(), x: 198, y: 30, w: 56, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "BUTTON", text: "Active border".to_owned(), x: 292, y: 8, w: 320, h: 200, style: group },
        Ctl { id: ID_BORDER_ENABLED, class: "BUTTON", text: "Enabled".to_owned(), x: 302, y: 28, w: 200, h: 20, style: check | tab },
        Ctl { id: ID_BORDER_THEME, class: "BUTTON", text: "Use system accent (theme)".to_owned(), x: 302, y: 50, w: 290, h: 20, style: check | tab },
        Ctl { id: 0, class: "STATIC", text: "Colour #rrggbb:".to_owned(), x: 302, y: 74, w: 120, h: 20, style: label },
        Ctl { id: ID_BORDER_COLOR, class: "EDIT", text: String::new(), x: 428, y: 72, w: 120, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "STATIC", text: "Width 0..32:".to_owned(), x: 302, y: 98, w: 120, h: 20, style: label },
        Ctl { id: ID_BORDER_WIDTH, class: "EDIT", text: String::new(), x: 428, y: 96, w: 120, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "STATIC", text: "Gap 0..64:".to_owned(), x: 302, y: 122, w: 120, h: 20, style: label },
        Ctl { id: ID_BORDER_GAP, class: "EDIT", text: String::new(), x: 428, y: 120, w: 120, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "STATIC", text: "Radius 0..64:".to_owned(), x: 302, y: 146, w: 120, h: 20, style: label },
        Ctl { id: ID_BORDER_RADIUS, class: "EDIT", text: String::new(), x: 428, y: 144, w: 120, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "STATIC", text: "Configured colour wins when theme is off or no accent.".to_owned(), x: 302, y: 170, w: 300, h: 30, style: label },
        Ctl { id: 0, class: "BUTTON", text: "Group underlay".to_owned(), x: 622, y: 8, w: 292, h: 124, style: group },
        Ctl { id: ID_UNDERLAY_ENABLED, class: "BUTTON", text: "Enabled".to_owned(), x: 632, y: 28, w: 200, h: 20, style: check | tab },
        Ctl { id: 0, class: "STATIC", text: "Colour #aarrggbb:".to_owned(), x: 632, y: 52, w: 140, h: 20, style: label },
        Ctl { id: ID_UNDERLAY_COLOR, class: "EDIT", text: String::new(), x: 776, y: 50, w: 120, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "STATIC", text: "Extension -1..32:".to_owned(), x: 632, y: 76, w: 140, h: 20, style: label },
        Ctl { id: ID_UNDERLAY_EXTENSION, class: "EDIT", text: String::new(), x: 776, y: 74, w: 120, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "STATIC", text: "-1 follows the border width.".to_owned(), x: 632, y: 100, w: 272, h: 22, style: label },
        Ctl { id: 0, class: "BUTTON", text: "Keyboard and mouse takeover".to_owned(), x: 10, y: 212, w: 602, h: 168, style: group },
        Ctl { id: ID_KEYBOARD_TAKEOVER, class: "BUTTON", text: "Keyboard takeover (Win shortcut interception)".to_owned(), x: 20, y: 234, w: 560, h: 20, style: check | tab },
        Ctl { id: ID_MOUSE_SNAP, class: "BUTTON", text: "Mouse Snap prevention (session only, no registry)".to_owned(), x: 20, y: 256, w: 560, h: 20, style: check | tab },
        Ctl { id: ID_SNAP_MEANING, class: "STATIC", text: "Off restores native keyboard chords / captured mouse-Snap value. Keyboard takeover follows individual bindings. Snap Layouts coverage is unverified.".to_owned(), x: 20, y: 278, w: 572, h: 44, style: label },
        Ctl { id: ID_ALLOW_WIN_L, class: "BUTTON", text: "Allow unshifted Win+L (explicit opt-in)".to_owned(), x: 20, y: 324, w: 560, h: 20, style: check | tab },
        Ctl { id: ID_LOCK_WARNING, class: "STATIC", text: "Warning: the hook cannot reliably suppress the OS lock chord.".to_owned(), x: 20, y: 346, w: 572, h: 24, style: label },
        Ctl { id: 0, class: "BUTTON", text: "Presets".to_owned(), x: 622, y: 140, w: 292, h: 240, style: group },
        Ctl { id: ID_PRESET_AUTHENTIC, class: "BUTTON", text: "Authentic".to_owned(), x: 632, y: 162, w: 132, h: 28, style: push | tab },
        Ctl { id: ID_PRESET_COMPATIBLE, class: "BUTTON", text: "Compatible".to_owned(), x: 772, y: 162, w: 132, h: 28, style: push | tab },
        Ctl { id: ID_PRESET_EXPLAIN, class: "STATIC", text: "Authentic restores KDE defaults for every binding (Win+L opt-in preserved). Compatible disables known OS-conflicting chords only (including Win+Ctrl+Left/Right) and rebinds float to Win+Ctrl+G and fullscreen to Win+Ctrl+F11; every other chord is kept, including ones with unknown Windows ownership (letter moves, sticky, Win+Ctrl+Tab/letters/Up/Down, relative-send follows, keyboard resize Alt chords). Kept does not mean conflict-free. Stay rows and workspace-migration rows stay unbound under both presets. No other replacement defaults are invented; manual rebind stays available.".to_owned(), x: 632, y: 196, w: 272, h: 174, style: label },
        Ctl { id: 0, class: "BUTTON", text: "New workspaces".to_owned(), x: 10, y: 388, w: 904, h: 100, style: group },
        Ctl { id: ID_DEFAULT_TILED, class: "BUTTON", text: "Tiled".to_owned(), x: 20, y: 410, w: 140, h: 24, style: radio_first | tab },
        Ctl { id: ID_DEFAULT_FLOATING, class: "BUTTON", text: "Floating".to_owned(), x: 170, y: 410, w: 140, h: 24, style: radio | tab },
        Ctl { id: 0, class: "STATIC", text: "New workspaces start tiled or floating. Applies to workspaces created after Apply; existing workspaces keep their session tiling. Only the default is saved.".to_owned(), x: 320, y: 408, w: 584, h: 30, style: label },
        Ctl { id: ID_WORKSPACE_MODE_LOCAL, class: "BUTTON", text: "Per-output local".to_owned(), x: 20, y: 442, w: 170, h: 24, style: radio_first | tab },
        Ctl { id: ID_WORKSPACE_MODE_GLOBAL, class: "BUTTON", text: "Global unique".to_owned(), x: 200, y: 442, w: 160, h: 24, style: radio | tab },
        Ctl { id: ID_WORKSPACE_MODE_SHARED, class: "BUTTON", text: "Shared".to_owned(), x: 370, y: 442, w: 100, h: 24, style: radio | tab },
        Ctl { id: 0, class: "STATIC", text: "Scope (restart required): saved immediately; the running owner keeps its startup scope until restart.".to_owned(), x: 480, y: 440, w: 424, h: 30, style: label },
        Ctl { id: ID_TASKBAR_PAGER, class: "BUTTON", text: "Taskbar pager (per-output workspace strip, default on)".to_owned(), x: 20, y: 468, w: 560, h: 20, style: check | tab },
        Ctl { id: 0, class: "BUTTON", text: "Same-axis move".to_owned(), x: 10, y: 496, w: 904, h: 56, style: group },
        Ctl { id: ID_SAME_AXIS_GROUP, class: "BUTTON", text: "Group with neighbor".to_owned(), x: 20, y: 518, w: 200, h: 24, style: radio_first | tab },
        Ctl { id: ID_SAME_AXIS_SWAP, class: "BUTTON", text: "Swap with neighbor".to_owned(), x: 230, y: 518, w: 200, h: 24, style: radio | tab },
        Ctl { id: 0, class: "STATIC", text: "Same-axis moves apply to subsequent moves only; existing trees keep their layout until the next move.".to_owned(), x: 440, y: 516, w: 464, h: 30, style: label },
        Ctl { id: 0, class: "BUTTON", text: "Born maximized".to_owned(), x: 10, y: 560, w: 904, h: 56, style: group },
        Ctl { id: ID_BORN_MAX_TILE, class: "BUTTON", text: "Tile on launch".to_owned(), x: 20, y: 582, w: 200, h: 24, style: radio_first | tab },
        Ctl { id: ID_BORN_MAX_OVERLAY, class: "BUTTON", text: "Keep maximized overlay".to_owned(), x: 230, y: 582, w: 200, h: 24, style: radio | tab },
        Ctl { id: 0, class: "STATIC", text: "Born-maximized admission applies to subsequent admissions only; admitted windows keep their state.".to_owned(), x: 440, y: 580, w: 464, h: 30, style: label },
        Ctl { id: 0, class: "BUTTON", text: "Fixed-size admission".to_owned(), x: 10, y: 624, w: 904, h: 56, style: group },
        Ctl { id: ID_FIXED_SIZE_BOTH, class: "BUTTON", text: "Width and height both fixed".to_owned(), x: 20, y: 646, w: 200, h: 24, style: radio_first | tab },
        Ctl { id: ID_FIXED_SIZE_EITHER, class: "BUTTON", text: "Width or height fixed".to_owned(), x: 230, y: 646, w: 200, h: 24, style: radio | tab },
        Ctl { id: 0, class: "STATIC", text: "Fixed-size admission applies to subsequent admissions only; admitted windows keep their state.".to_owned(), x: 440, y: 644, w: 464, h: 30, style: label },
        Ctl { id: 0, class: "BUTTON", text: "Source workspace after migration".to_owned(), x: 10, y: 688, w: 904, h: 56, style: group },
        Ctl { id: ID_MIGRATION_REFILL_LAST, class: "BUTTON", text: "Last remaining workspace".to_owned(), x: 20, y: 710, w: 210, h: 24, style: radio_first | tab },
        Ctl { id: ID_MIGRATION_REFILL_MRU, class: "BUTTON", text: "Most recently used workspace".to_owned(), x: 240, y: 710, w: 220, h: 24, style: radio | tab },
        Ctl { id: 0, class: "STATIC", text: "Source refill applies to subsequent migrations only; migrated workspaces keep their state.".to_owned(), x: 470, y: 708, w: 434, h: 30, style: label },
        Ctl { id: 0, class: "BUTTON", text: "Shortcuts (103 rows)".to_owned(), x: 10, y: 752, w: 904, h: 268, style: group },
        Ctl { id: ID_BINDING_LIST, class: "LISTBOX", text: String::new(), x: 20, y: 774, w: 540, h: 192, style: list_style | tab },
        Ctl { id: ID_BINDING_INFO, class: "EDIT", text: String::new(), x: 570, y: 774, w: 324, h: 84, style: info_style },
        Ctl { id: ID_BIND_KEEP, class: "BUTTON", text: "Keep".to_owned(), x: 570, y: 868, w: 100, h: 26, style: push | tab },
        Ctl { id: ID_BIND_DISABLE, class: "BUTTON", text: "Disable".to_owned(), x: 676, y: 868, w: 100, h: 26, style: push | tab },
        Ctl { id: ID_BIND_CHORD, class: "EDIT", text: String::new(), x: 570, y: 898, w: 150, h: 24, style: edit_style | tab },
        Ctl { id: ID_BIND_REBIND, class: "BUTTON", text: "Set rebind".to_owned(), x: 726, y: 896, w: 120, h: 26, style: push | tab },
        Ctl { id: ID_REBIND_NOTE, class: "STATIC", text: "Rebind: Win[+Shift][+Alt][+Ctrl]+Key, keeping this action's modifier arm (focus/select unshifted, float/fullscreen Win or Win+Ctrl, move/send shifted, numbered stay Win+Shift or Win+Ctrl+Shift, previous/relative Win+Ctrl, relative send Win+Ctrl+Shift, output follow Win+Ctrl+Alt, output stay Win+Ctrl+Alt or Win+Ctrl+Alt+Shift, workspace migration Win+Ctrl+Alt+Shift, resize out Win+Alt, resize in Win+Shift+Alt). Win+L can never be a target. Win+G / Win+F11 cannot fully contain the OS Xbox/Game Bar handlers; Win+Ctrl+Left/Right virtual-desktop, Win+Ctrl+Shift arrow, Win+Ctrl+Alt, and Win+Alt chord ownership are unverified.".to_owned(), x: 570, y: 924, w: 324, h: 60, style: label },
        Ctl { id: ID_GAMING_OPEN_XBOX, class: "BUTTON", text: "Open Xbox mode settings".to_owned(), x: 20, y: 986, w: 205, h: 26, style: push | tab },
        Ctl { id: ID_GAMING_OPEN_GAMEBAR, class: "BUTTON", text: "Open Game Bar settings".to_owned(), x: 235, y: 986, w: 205, h: 26, style: push | tab },
        Ctl { id: ID_GAMING_OPEN_APP, class: "BUTTON", text: "Open Game Bar app settings".to_owned(), x: 450, y: 986, w: 215, h: 26, style: push | tab },
        Ctl { id: ID_GAMING_RECHECK, class: "BUTTON", text: "Recheck gaming status".to_owned(), x: 675, y: 986, w: 219, h: 26, style: push | tab },
        Ctl { id: ID_STATUS, class: "STATIC", text: "Status: ready.".to_owned(), x: 20, y: 1028, w: 540, h: 60, style: label },
        Ctl { id: ID_APPLY, class: "BUTTON", text: "Apply".to_owned(), x: 580, y: 1028, w: 100, h: 30, style: defpush | tab },
        Ctl { id: ID_REVERT, class: "BUTTON", text: "Revert".to_owned(), x: 690, y: 1028, w: 100, h: 30, style: push | tab },
        Ctl { id: ID_CLOSE, class: "BUTTON", text: "Close".to_owned(), x: 800, y: 1028, w: 100, h: 30, style: push | tab },
    ];
    // Group/label/statics carry id 0 and skip automation lookup. Groupboxes
    // are pinned behind every sibling (see below); their handles ride here.
    let mut owned: Vec<(u32, HWND)> = Vec::new();
    let mut groups: Vec<HWND> = Vec::new();
    for ctl in &controls {
        let handle = if ctl.id == 0 {
            let mut sink: Vec<(u32, HWND)> = Vec::new();
            match create_ctl(hwnd, hinst, ctl, dpi, font, &mut sink) {
                Ok(handle) => {
                    if ctl.class == "BUTTON" {
                        groups.push(handle);
                    }
                    handle
                }
                Err(error) => {
                    abort_create(hwnd, raw);
                    return Err(error);
                }
            }
        } else {
            match create_ctl(hwnd, hinst, ctl, dpi, font, &mut owned) {
                Ok(handle) => handle,
                Err(error) => {
                    abort_create(hwnd, raw);
                    return Err(error);
                }
            }
        };
        if ctl.class == "EDIT"
            && let Some(limit) = edit_limit(ctl.id)
        {
            use windows_sys::Win32::UI::Controls::EM_SETLIMITTEXT;
            use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
            unsafe {
                SendMessageW(handle, EM_SETLIMITTEXT, limit, 0);
            }
        }
    }
    let app = unsafe { &mut *raw };
    app.controls = owned;
    // Pin every groupbox behind all siblings: creation order alone left the
    // first-created groupboxes topmost, and each opaque groupbox then covered
    // every control inside it (blank frames in both screen and `WM_PRINT`
    // captures, machine-proven live). `SWP_NOACTIVATE` keeps focus untouched;
    // geometry flags keep every rect. A failure aborts like any other
    // control-creation failure.
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
        };
        for grouped in &groups {
            let moved = unsafe {
                SetWindowPos(
                    *grouped,
                    HWND_BOTTOM,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            };
            if moved == 0 {
                abort_create(hwnd, raw);
                return Err(err("error: settings groupbox order failed"));
            }
        }
    }
    refresh_all(app);
    // Native tooltips for the R-MOV-03 and R-MAX-06 radios; best-effort (see
    // the helper).
    attach_choice_tips(app, hwnd, hinst);
    if let Some(reason) = invalid {
        status(
            app,
            &format!(
                "Error: settings file is invalid ({reason}). Showing defaults; Apply is disabled, nothing was written."
            ),
        );
    } else {
        status(
            app,
            "Status: editing saved settings. Apply validates and saves; Revert discards edits; Close never applies.",
        );
    }

    unsafe {
        ShowWindow(hwnd, SW_SHOWNORMAL);
    }
    // Best-effort poll for worker-pass completion (Recheck and selection
    // changes work without it; it only auto-rewrites the info on landing).
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::SetTimer;
        unsafe {
            SetTimer(hwnd, GAMING_TIMER_ID, GAMING_TIMER_MS, None);
        }
    }
    let mut msg: windows_sys::Win32::UI::WindowsAndMessaging::MSG = unsafe { std::mem::zeroed() };
    loop {
        let turn = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if turn == 0 {
            break;
        }
        if turn == -1 {
            return Err(err("error: settings message loop failed"));
        }
        if msg.message == WM_KEYDOWN && msg.wParam == ESCAPE_VK {
            use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;
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
    // WM_DESTROY reclaimed the box and ended the loop; the window is gone
    // and Close never applied. Save counts rode the status line while open.
    Ok("settings closed".to_owned())
}
