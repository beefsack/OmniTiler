//! Native Win32 settings window (`cfg(windows)` only).
//!
//! Mechanism: one overlapped settings window with four native tab pages
//! (`SysTabControl32`) plus plain Win32 child controls (`STATIC`, `EDIT`,
//! `BUTTON`, `LISTBOX`) created through the existing `windows-sys` route. No new dependency, no registry/policy writes, no tray.
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
mod dialogs;
mod draft;
mod info;
mod tabs;
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
/// Native tab strip (`SysTabControl32`): four catalog tabs, always visible.
pub const ID_TABS: u32 = 150;
/// Global badge outside the tab pages: invalid file, unsaved draft, and the
/// selected-row gaming conflict stay visible on every tab. Never claims
/// resolution or runtime adoption; the tab's own controls do the work.
pub const ID_TAB_BADGE: u32 = 151;

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
    /// Active tab index into `tabs::tab_titles` (draft lives in the HWND
    /// text of every page, so switching tabs never collects or discards).
    current_tab: usize,
    /// True once a user edit lands in any HWND (EN_CHANGE on an edit,
    /// BN_CLICKED on a check/radio). Programmatic pushes never set it
    /// (their notifications are suppressed in `ctl::set_text`, and
    /// `BM_SETCHECK` notifies nothing). Cleared by `refresh_all`
    /// (reloads and successful saves); retained on validation failure.
    /// The badge treats this, `work != base`, and a pending chord as
    /// requested-but-unsaved, never "saved".
    dirty: bool,
    /// Groupbox handles with their owning tab (creation order parallel to
    /// `tabs::GROUPS`): hidden with their page on tab switches.
    groups: Vec<(usize, HWND)>,
    /// Label-static handles with their owning tab (`tabs::LABEL_RECTS`
    /// order): hidden with their page on tab switches.
    statics: Vec<(usize, HWND)>,
    /// Top-level Settings window handle (owner for owned modal dialogs).
    window: HWND,
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

/// Register the common-control tab classes before any `SysTabControl32`
/// child exists. `insert_tab_items` also calls this, but that runs after
/// the strip's `CreateWindowExW`: on a cold process the class may not be
/// registered yet and strip creation itself fails. Called once before the
/// main window exists; idempotent on repeat calls.
fn ensure_tab_classes() -> Result<(), DynError> {
    use windows_sys::Win32::UI::Controls::{
        ICC_TAB_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx,
    };
    let icc = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_TAB_CLASSES,
    };
    if unsafe { InitCommonControlsEx(&icc) } == 0 {
        return Err(err("error: settings tab classes failed"));
    }
    Ok(())
}

/// Insert the four catalog tab titles into the strip. Missing tabs would
/// leave catalog fields unreachable, so any failure aborts the window
/// through the caller's `abort_create` path.
fn insert_tab_items(app: &App) -> Result<(), DynError> {
    use windows_sys::Win32::UI::Controls::{TCIF_TEXT, TCITEMW, TCM_INSERTITEMW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
    let strip = app.ctl(ID_TABS);
    if strip.is_null() {
        return Err(err("error: settings tab strip missing"));
    }
    // Idempotent re-registration: the classes were ensured before any
    // strip existed; this keeps item insertion self-sufficient.
    ensure_tab_classes()?;
    let titles = tabs::tab_titles();
    for (index, title) in titles.iter().enumerate() {
        let text_w = ctl::wide(title);
        let item = TCITEMW {
            mask: TCIF_TEXT,
            dwState: 0,
            dwStateMask: 0,
            pszText: text_w.as_ptr() as *mut u16,
            cchTextMax: 0,
            iImage: 0,
            lParam: 0,
        };
        let placed = unsafe {
            SendMessageW(
                strip,
                TCM_INSERTITEMW,
                index,
                &item as *const TCITEMW as isize,
            )
        };
        if placed < 0 {
            return Err(err("error: settings tab item failed"));
        }
    }
    if titles.len() != tabs::TAB_COUNT {
        return Err(err("error: settings tab count drifted"));
    }
    Ok(())
}

/// Show one tab page: the tab strip and footer stay visible and enabled;
/// every other page's controls hide and disable so `IsDialogMessageW`
/// skips them. HWND text is untouched, so cross-tab draft edits and the
/// pending chord survive. Focus parked on a hiding control moves to the
/// tab strip; focus elsewhere (e.g. the strip after a mouse click) stays.
pub(super) fn show_tab(app: &mut App, index: usize) {
    use windows_sys::Win32::UI::Controls::TCM_SETCURSEL;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOW, SendMessageW, ShowWindow};
    let index = index % tabs::TAB_COUNT;
    app.current_tab = index;
    let strip = app.ctl(ID_TABS);
    if !strip.is_null() {
        unsafe {
            SendMessageW(strip, TCM_SETCURSEL, index, 0);
        }
    }
    let focused = unsafe { GetFocus() };
    let mut focus_hidden = false;
    for (id, hwnd) in app.controls.clone() {
        let visible = match tabs::control_tab(id) {
            None => true,
            Some(tab) => tab == index,
        };
        unsafe {
            ShowWindow(hwnd, if visible { SW_SHOW } else { SW_HIDE });
        }
        // Hidden pages disable so `IsDialogMessageW` skips them; shown
        // pages re-enable, then the gates below restore semantic disables.
        ctl::enable(hwnd, visible);
        if hwnd == focused && tabs::needs_focus_move(id, index) {
            focus_hidden = true;
        }
    }
    for (tab, grouped) in app.groups.clone() {
        unsafe {
            ShowWindow(grouped, if tab == index { SW_SHOW } else { SW_HIDE });
        }
    }
    for (tab, labelled) in app.statics.clone() {
        let visible = tab == index;
        unsafe {
            ShowWindow(labelled, if visible { SW_SHOW } else { SW_HIDE });
        }
        // Note edits carry a tab stop: hidden ones disable like hidden
        // page controls so keyboard traversal never parks off-tab.
        ctl::enable(labelled, visible);
        if labelled == focused && !visible {
            focus_hidden = true;
        }
    }
    if focus_hidden && !strip.is_null() {
        unsafe {
            SetFocus(strip);
        }
    }
    ctl::enable(app.ctl(ID_APPLY), app.invalid.is_none());
    info::refresh_list(app);
    // `refresh_list` restores the gaming buttons' own enable gates, which
    // re-enables hidden ones: re-disable every hidden page control so a
    // tab switch never enables an off-tab button.
    for (id, hwnd) in app.controls.clone() {
        if matches!(tabs::control_tab(id), Some(tab) if tab != index) {
            ctl::enable(hwnd, false);
        }
    }
    info::refresh_badge(app);
}

fn on_command(app: &mut App, id: u32, notify: u32) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BN_CLICKED, LB_GETCURSEL, LB_SETCURSEL, LBN_SELCHANGE, SendMessageW,
    };
    // Edit notifications: 768 is EN_CHANGE (winuser). Every `set_text`
    // call site is programmatic and suppressed in `ctl`, and `BM_SETCHECK`
    // notifies nothing, so a live EN_CHANGE is always a user keystroke:
    // the HWND no longer matches the last push and the badge must say so.
    // Edits stage at Apply; nothing else happens here.
    const EN_CHANGE: u32 = 768;
    if notify == EN_CHANGE {
        match id {
            ID_INNER_GAP
            | ID_OUTER_GAP
            | ID_BORDER_COLOR
            | ID_BORDER_WIDTH
            | ID_BORDER_GAP
            | ID_BORDER_RADIUS
            | ID_UNDERLAY_COLOR
            | ID_UNDERLAY_EXTENSION
            | ID_BIND_CHORD => {
                app.dirty = true;
            }
            _ => {}
        }
        return true;
    }
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
                        return true;
                    }
                    set_text(app.ctl(ID_BIND_CHORD), "");
                }
                app.selected = Some(picked as usize);
                refresh_info(app);
            }
        }
        return true;
    }
    if notify != BN_CLICKED {
        return true;
    }
    // Check/radio clicks edit the HWND draft immediately (auto controls
    // flip before this notification); Apply stages them into the draft.
    match id {
        ID_BORDER_ENABLED
        | ID_BORDER_THEME
        | ID_UNDERLAY_ENABLED
        | ID_KEYBOARD_TAKEOVER
        | ID_ALLOW_WIN_L
        | ID_MOUSE_SNAP
        | ID_TASKBAR_PAGER
        | ID_DEFAULT_TILED
        | ID_DEFAULT_FLOATING
        | ID_SAME_AXIS_GROUP
        | ID_SAME_AXIS_SWAP
        | ID_BORN_MAX_TILE
        | ID_BORN_MAX_OVERLAY
        | ID_FIXED_SIZE_BOTH
        | ID_FIXED_SIZE_EITHER
        | ID_MIGRATION_REFILL_LAST
        | ID_MIGRATION_REFILL_MRU
        | ID_WORKSPACE_MODE_LOCAL
        | ID_WORKSPACE_MODE_GLOBAL
        | ID_WORKSPACE_MODE_SHARED => {
            app.dirty = true;
        }
        _ => {}
    }
    // Returns false only when the owner died inside an Apply/Revert modal
    // (the `App` box was freed with it): the `WM_COMMAND` caller must skip
    // its post-command badge refresh and touch nothing.
    match id {
        ID_APPLY => do_apply(app),
        ID_REVERT => do_revert(app),
        // ID_CLOSE never reaches here: `wnd_proc` destroys the window without
        // materializing the `App` borrow (see below), so the synchronous
        // `WM_DESTROY` free below never nests inside a live `&mut App`.
        ID_PRESET_AUTHENTIC => {
            do_preset(app, crate::settings::Preset::Authentic);
            true
        }
        ID_PRESET_COMPATIBLE => {
            do_preset(app, crate::settings::Preset::Compatible);
            true
        }
        ID_BIND_KEEP => {
            if let Err(reason) = stage_state(app, BindingState::Keep) {
                status(app, &format!("Error: {reason}."));
            }
            true
        }
        ID_BIND_DISABLE => {
            if let Err(reason) = stage_state(app, BindingState::Disabled) {
                status(app, &format!("Error: {reason}."));
            }
            true
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
            true
        }
        ID_GAMING_OPEN_XBOX => {
            open_gaming_uri(app, crate::gaming::XBOX_SETTINGS_URI, "Xbox mode");
            true
        }
        ID_GAMING_OPEN_GAMEBAR => {
            open_gaming_uri(app, crate::gaming::GAMEBAR_SETTINGS_URI, "Game Bar");
            true
        }
        ID_GAMING_OPEN_APP => {
            let uri = crate::gaming::gamebar_app_settings_uri();
            open_gaming_uri(app, &uri, "Game Bar app");
            true
        }
        ID_GAMING_RECHECK => {
            // Fire-and-forget: the worker pass runs off the UI thread and the
            // timer rewrites the info when its generation lands.
            app.gaming.recheck();
            status(
                app,
                "Recheck requested; evidence refreshes when the probe completes. Navigation never marks a conflict resolved.",
            );
            true
        }
        _ => true,
    }
}

/// Fixed-size `WM_GETMINMAXINFO` tracks from the live outer size: equal
/// min/max pins the current size on both axes, which the shared fixed-size
/// classifier reads as fixed under both predicates once converted with the
/// measured frame insets. `None` when the outer size is not a usable fixed
/// answer (non-positive or beyond the shared carried-geometry bound); the
/// production handler falls back to `DefWindowProcW` then.
pub(super) fn fixed_minmax_tracks(outer_w: i32, outer_h: i32) -> Option<(i32, i32)> {
    if (1..=tiler_core::bounds::GEOMETRY_BOUND).contains(&outer_w)
        && (1..=tiler_core::bounds::GEOMETRY_BOUND).contains(&outer_h)
    {
        Some((outer_w, outer_h))
    } else {
        None
    }
}

/// Readiness for the explicit fixed-size `WM_GETMINMAXINFO` answer: the
/// info pointer must be non-null and the window must have completed
/// `WM_CREATE` initialization (nonzero user data, checked read-only and
/// never dereferenced here). Anything earlier falls through to
/// `DefWindowProcW`, so initial creation sizing is never constrained by
/// this window's own advertisement.
fn fixed_tracks_ready(user_data: isize, lparam: isize) -> bool {
    user_data != 0 && lparam != 0
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CREATESTRUCTW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetWindowLongPtrW,
        GetWindowRect, IsWindow, MINMAXINFO, PostQuitMessage, SetWindowLongPtrW, WM_CLOSE,
        WM_COMMAND, WM_CREATE, WM_DESTROY, WM_GETMINMAXINFO, WM_NOTIFY, WM_TIMER,
    };
    match msg {
        WM_CREATE => {
            let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            0
        }
        WM_COMMAND | WM_NOTIFY | WM_TIMER if dialogs::dialog_open() => {
            // An owned modal (Apply preview, discard confirm, result) runs
            // a nested pump while the outer `on_command` borrow is still
            // live: ignore every App-touching message so no second `&mut
            // App` can materialize and alias it. `WM_CLOSE` is NOT ignored:
            // its arm below defers teardown without touching `App`, and no
            // `DestroyWindow` path remains while a modal holds, so the box
            // cannot be freed mid-modal either. Together the pre-modal
            // borrow stays the unique live reference until the drain past
            // `on_command` honors any deferred close.
            0
        }
        WM_GETMINMAXINFO => {
            // Explicit fixed-size advertisement from the live outer size:
            // this window is fixed-size by construction (no sizing
            // border), so its min/max tracks pin the current size and the
            // shared classifier observes equal bounds through the existing
            // hint contract. No App state is dereferenced: this can
            // arrive before WM_CREATE stores it, so the answer is gated
            // on initialization readiness (nonzero user data, read-only)
            // plus a null info-pointer check; anything earlier, an
            // unreadable rect, or an invalid outer falls through to
            // DefWindowProc.
            let user_data = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if !fixed_tracks_ready(user_data, lparam) {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
            let mut outer: windows_sys::Win32::Foundation::RECT = unsafe { std::mem::zeroed() };
            let read = unsafe { GetWindowRect(hwnd, &mut outer) } != 0;
            let tracks = if read {
                let w: Option<i32> = (i64::from(outer.right) - i64::from(outer.left))
                    .try_into()
                    .ok();
                let h: Option<i32> = (i64::from(outer.bottom) - i64::from(outer.top))
                    .try_into()
                    .ok();
                w.and_then(|w| h.and_then(|h| fixed_minmax_tracks(w, h)))
            } else {
                None
            };
            match tracks {
                Some((w, h)) => {
                    let info = unsafe { &mut *(lparam as *mut MINMAXINFO) };
                    info.ptMinTrackSize.x = w;
                    info.ptMinTrackSize.y = h;
                    info.ptMaxTrackSize.x = w;
                    info.ptMaxTrackSize.y = h;
                    0
                }
                None => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
            }
        }
        WM_COMMAND => {
            // Programmatic pushes (`ctl::set_text`) suppress their own
            // synchronous EN_CHANGE here, so a live notification is always
            // a user edit. Suppressed commands never touch `App`.
            if ctl::notifications_suppressed() {
                return 0;
            }
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
                let alive = on_command(app, id, notify);
                if alive {
                    // Every command may move draft state (HWND dirty flag,
                    // staged bindings, pending chord): re-badge afterwards.
                    info::refresh_badge(app);
                }
                // The `&mut App` borrow ends here (last use above). Only now
                // is teardown safe: honor a `WM_CLOSE` deferred by a modal.
                // `WM_DESTROY` reclaims the box synchronously; the flag
                // drains exactly once so a later command never inherits it.
                if dialogs::take_close_deferred() && unsafe { IsWindow(hwnd) } != 0 {
                    unsafe {
                        DestroyWindow(hwnd);
                    }
                }
            }
            0
        }
        WM_CLOSE => {
            if dialogs::dialog_open() {
                // Owner close during an owned modal (taskbar, Alt+F4,
                // programmatic): never destroy while the outer `&mut App`
                // borrow is live across the nested pump. Record without
                // touching `App`; the `WM_COMMAND` drain above honors it
                // the moment that borrow ends, so the close is deferred by
                // one command, never dropped and never run while borrowed.
                dialogs::defer_close();
                return 0;
            }
            unsafe {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_NOTIFY => {
            // Tab strip selection: mouse clicks and arrow keys land here
            // (`TCN_SELCHANGE`). Keyboard Ctrl+Tab is handled in the
            // message loop below; both funnel into `show_tab`, which never
            // touches HWND draft text.
            use windows_sys::Win32::UI::Controls::{NMHDR, TCM_GETCURSEL, TCN_SELCHANGE};
            use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
            let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if ptr != 0 && lparam != 0 {
                let hdr = unsafe { &*(lparam as *const NMHDR) };
                if hdr.idFrom as u32 == ID_TABS && hdr.code == TCN_SELCHANGE {
                    let app = unsafe { &mut *(ptr as *mut App) };
                    let picked = unsafe { SendMessageW(hdr.hwndFrom, TCM_GETCURSEL, 0, 0) };
                    if picked >= 0 {
                        show_tab(app, picked as usize);
                    }
                }
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
                    info::refresh_badge(app);
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
    let client_w = px(tabs::CLIENT_W, dpi);
    let client_h = px(tabs::CLIENT_H, dpi);
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
    // Tab classes before any tab strip exists: strip creation below fails
    // on a cold process without this (no window yet, so no teardown).
    ensure_tab_classes()?;
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
        current_tab: 0,
        dirty: false,
        groups: Vec::new(),
        statics: Vec::new(),
        window: std::ptr::null_mut(),
    });
    let raw = Box::into_raw(app);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            class_w.as_ptr(),
            title_w.as_ptr(),
            style,
            tabs::ORIGIN_X,
            tabs::ORIGIN_Y,
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
    // 84px static. Long native notes and workspace notes share it.
    let info_style = ES_MULTILINE as u32
        | ES_AUTOVSCROLL as u32
        | ES_READONLY as u32
        | WS_VSCROLL
        | WS_BORDER
        | tab;
    // Catalog-driven controls: geometry and text come from `tabs`
    // (sections/fields rendered from the shared catalog plus the Windows
    // adapter, never the old single-page form). Native styling stays here.
    let style_for = |id: u32, class: &str| -> u32 {
        if class == "SysTabControl32" {
            return tab;
        }
        // Radio exclusion groups live in `tabs::is_radio_first` (single
        // source, covered by its own test): exactly one WS_GROUP starter
        // per field, so sibling reordering below can never merge fields.
        if tabs::is_radio_first(id) {
            return radio_first | tab;
        }
        match id {
            ID_APPLY => return defpush | tab,
            ID_PRESET_AUTHENTIC
            | ID_PRESET_COMPATIBLE
            | ID_BIND_KEEP
            | ID_BIND_DISABLE
            | ID_BIND_REBIND
            | ID_GAMING_OPEN_XBOX
            | ID_GAMING_OPEN_GAMEBAR
            | ID_GAMING_OPEN_APP
            | ID_GAMING_RECHECK
            | ID_REVERT
            | ID_CLOSE => return push | tab,
            ID_DEFAULT_FLOATING
            | ID_SAME_AXIS_SWAP
            | ID_BORN_MAX_OVERLAY
            | ID_FIXED_SIZE_EITHER
            | ID_MIGRATION_REFILL_MRU
            | ID_WORKSPACE_MODE_GLOBAL
            | ID_WORKSPACE_MODE_SHARED => {
                return radio | tab;
            }
            ID_BORDER_ENABLED | ID_BORDER_THEME | ID_UNDERLAY_ENABLED | ID_KEYBOARD_TAKEOVER
            | ID_ALLOW_WIN_L | ID_MOUSE_SNAP | ID_TASKBAR_PAGER => return check | tab,
            ID_BINDING_LIST => return list_style | tab,
            ID_BINDING_INFO | ID_PRESET_EXPLAIN | ID_REBIND_NOTE => return info_style,
            ID_TAB_BADGE | ID_STATUS | ID_SNAP_MEANING | ID_LOCK_WARNING => return label,
            _ => {}
        }
        if class == "EDIT" {
            return edit_style | tab;
        }
        label
    };
    // Creation follows `tabs::creation_sequence` so every buddy label
    // lands immediately before its editor (accessible-name association).
    let mut owned: Vec<(u32, HWND)> = Vec::new();
    let mut grouped: Vec<(usize, HWND)> = Vec::new();
    let mut labelled: Vec<(usize, HWND)> = Vec::new();
    let mut create_owned = |ctl: &Ctl| -> Result<HWND, DynError> {
        create_ctl(hwnd, hinst, ctl, dpi, font, &mut owned)
    };
    let create_unowned = |ctl: &Ctl| -> Result<HWND, DynError> {
        let mut sink: Vec<(u32, HWND)> = Vec::new();
        create_ctl(hwnd, hinst, ctl, dpi, font, &mut sink)
    };
    for step in tabs::creation_sequence() {
        match step {
            tabs::CreateStep::TabStrip => {
                let (x, y, w, h) = tabs::control_rect(ID_TABS).expect("strip rect");
                let spec = Ctl {
                    id: ID_TABS,
                    class: "SysTabControl32",
                    text: tabs::control_text(ID_TABS),
                    x,
                    y,
                    w,
                    h,
                    style: tab,
                };
                if create_owned(&spec).is_err() {
                    abort_create(hwnd, raw);
                    return Err(err("error: settings tab strip failed"));
                }
            }
            tabs::CreateStep::Group(group_index) => {
                let (key, x, y, w, h) = tabs::GROUPS[group_index];
                let spec = Ctl {
                    id: 0,
                    class: "BUTTON",
                    text: tabs::group_title(key),
                    x,
                    y,
                    w,
                    h,
                    style: group,
                };
                match create_unowned(&spec) {
                    Ok(handle) => grouped.push((tabs::group_tab(key).expect("group tab"), handle)),
                    Err(error) => {
                        abort_create(hwnd, raw);
                        return Err(error);
                    }
                }
            }
            tabs::CreateStep::Label(label_index) => {
                let (page, x, y, w, h) = tabs::LABEL_RECTS[label_index];
                let class = tabs::label_class(label_index);
                let spec = Ctl {
                    id: 0,
                    class,
                    text: tabs::label_text(label_index),
                    x,
                    y,
                    w,
                    h,
                    style: if class == "EDIT" { info_style } else { label },
                };
                match create_unowned(&spec) {
                    Ok(handle) => labelled.push((page, handle)),
                    Err(error) => {
                        abort_create(hwnd, raw);
                        return Err(error);
                    }
                }
            }
            tabs::CreateStep::Control(id) => {
                if id == ID_TABS {
                    continue;
                }
                let Some((x, y, w, h)) = tabs::control_rect(id) else {
                    continue;
                };
                let Some(class) = tabs::control_class(id) else {
                    continue;
                };
                let spec = Ctl {
                    id,
                    class,
                    text: tabs::control_text(id),
                    x,
                    y,
                    w,
                    h,
                    style: style_for(id, class),
                };
                let handle = match create_owned(&spec) {
                    Ok(handle) => handle,
                    Err(error) => {
                        abort_create(hwnd, raw);
                        return Err(error);
                    }
                };
                if spec.class == "EDIT"
                    && let Some(limit) = edit_limit(spec.id)
                {
                    use windows_sys::Win32::UI::Controls::EM_SETLIMITTEXT;
                    use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
                    unsafe {
                        SendMessageW(handle, EM_SETLIMITTEXT, limit, 0);
                    }
                }
            }
        }
    }
    let app = unsafe { &mut *raw };
    app.controls = owned;
    app.groups = grouped;
    app.statics = labelled;
    app.window = hwnd;
    // Offline layout invariant over the built specs (rects, tabs, text
    // fit): zero release cost, fails debug builds before first paint.
    debug_assert!(tabs::layout_valid());
    debug_assert!(tabs::client_fits_1080p(dpi));
    debug_assert!(tabs::origin_fits_1080p(dpi));
    // Sibling order, bottom to top: tab strip, groupboxes in creation
    // order, then page controls and footer on top. The strip must sit
    // bottommost: it shares the parent with every page control and its
    // rect covers the display area, so a strip above the pages would take
    // their clicks and paint over the group frames. Each plan target goes
    // to `HWND_BOTTOM` in `tabs::zorder_plan` order (groups reversed,
    // strip last): absolute placements only, never an `hWndInsertAfter`
    // anchor, which would put the moved window BELOW its anchor and leave
    // the strip on top. Groupboxes end just above the strip so the opaque
    // frames never cover their own controls (historical pre-tab window:
    // blank frames in both screen and `WM_PRINT` captures, not a claim
    // about this tabbed layout, which has no live proof yet); page
    // controls keep the creation-sequence tab-stop flow because only
    // non-tab-stop groupboxes move, and every radio field keeps its own
    // `WS_GROUP` starter (see `tabs::is_radio_first`), so clustering the
    // frames cannot merge two exclusion groups. `SWP_NOACTIVATE` keeps
    // focus untouched; geometry flags keep every rect. A failure aborts
    // like any other control-creation failure.
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
        };
        let order_flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
        let strip = app.ctl(ID_TABS);
        if strip.is_null() {
            abort_create(hwnd, raw);
            return Err(err("error: settings tab strip missing"));
        }
        // `app.groups` parallels `tabs::GROUPS` in creation order (one push
        // per `CreateStep::Group`, and group tabs never decrease over
        // `GROUPS`, asserted by test), so plan indices address it directly.
        let created = app.groups.clone();
        for op in tabs::zorder_plan() {
            let target = match op {
                tabs::ZOrderOp::StripToBottom => strip,
                tabs::ZOrderOp::GroupToBottom(index) => created
                    .get(index)
                    .map(|(_, handle)| *handle)
                    .unwrap_or(std::ptr::null_mut()),
            };
            if target.is_null() {
                abort_create(hwnd, raw);
                return Err(err("error: settings groupbox order failed"));
            }
            let placed = unsafe { SetWindowPos(target, HWND_BOTTOM, 0, 0, 0, 0, order_flags) };
            if placed == 0 {
                abort_create(hwnd, raw);
                return Err(err("error: settings groupbox order failed"));
            }
        }
    }
    refresh_all(app);
    // Tab strip items from the portable catalog order. Missing tabs would
    // leave catalog fields unreachable, so this aborts like any other
    // control-creation failure.
    if let Err(error) = insert_tab_items(app) {
        abort_create(hwnd, raw);
        return Err(error);
    }
    // Native tooltips for the R-MOV-03 and R-MAX-06 radios; best-effort (see
    // the helper).
    attach_choice_tips(app, hwnd, hinst);
    // First paint shows tab zero; HWND draft text is untouched.
    show_tab(app, 0);
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
        // Ctrl+Tab / Ctrl+Shift+Tab cycles the four catalog tabs without
        // touching HWND draft text. Handled before `IsDialogMessageW` so
        // the chord never reaches a focused edit.
        if msg.message == WM_KEYDOWN && msg.wParam == 0x09 {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
            use windows_sys::Win32::UI::WindowsAndMessaging::GWLP_USERDATA;
            use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW;
            let ctrl = unsafe { GetKeyState(0x11) } < 0;
            if ctrl {
                let shift = unsafe { GetKeyState(0x10) } < 0;
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                if ptr != 0 {
                    let app = unsafe { &mut *(ptr as *mut App) };
                    show_tab(app, tabs::next_tab(app.current_tab, !shift));
                }
                continue;
            }
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
