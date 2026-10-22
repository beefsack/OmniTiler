#![forbid(unsafe_code)]
//! Native Win32 settings window (`cfg(windows)` only).
//!
//! Mechanism: one overlapped settings window with four native tab pages
//! (`SysTabControl32`) plus plain Win32 child controls (`STATIC`, `EDIT`,
//! `BUTTON`, `LISTBOX`) created through the FFI route. No registry/policy
//! writes, no tray.
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
//!
//! All Win32 lives in [`crate::ffi`]; this module holds layout orchestration,
//! draft routing, and pure helpers only.

use std::path::PathBuf;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::HFONT;

use crate::settings::{BindingState, Settings, binding_catalog};

use self::choice::choice_tip_texts;
use self::ctl::{Ctl, create_ctl, edit_limit, get_text, set_text};
use self::draft::{
    apply_rebind_text, do_apply, do_preset, do_revert, load_base, refresh_all, stage_state,
};
use self::info::{refresh_badge, refresh_info, refresh_list};

pub(crate) mod choice;
pub(crate) mod ctl;
pub(crate) mod dialogs;
pub(crate) mod draft;
pub(crate) mod info;
pub(crate) mod tabs;
#[cfg(test)]
mod tests;

pub(crate) type DynError = Box<dyn std::error::Error>;

pub(crate) fn err(msg: impl Into<String>) -> DynError {
    Box::new(std::io::Error::other(msg.into()))
}

/// Native window class for the settings window.
pub const SETTINGS_WINDOW_CLASS: &str = "OmniTilerSettings";
/// Window title for the settings window.
pub const SETTINGS_WINDOW_TITLE: &str = "OmniTiler Settings";

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
pub const ID_TASKBAR_PAGER: u32 = 144;
pub const ID_ANCHOR_LARGEST: u32 = 152;
pub const ID_ANCHOR_CLOSEST: u32 = 153;
pub const ID_ANCHOR_NO_SEARCH: u32 = 154;
pub const ID_NO_SPACE_FLOAT: u32 = 155;
pub const ID_NO_SPACE_OVERLAP: u32 = 156;
pub const ID_GAMING_OPEN_XBOX: u32 = 145;
pub const ID_GAMING_OPEN_GAMEBAR: u32 = 146;
pub const ID_GAMING_OPEN_APP: u32 = 147;
pub const ID_GAMING_RECHECK: u32 = 148;
pub const ID_TABS: u32 = 150;
pub const ID_TAB_BADGE: u32 = 151;
/// Stage 2c cursor-first targeting radios: workspace-switch target pair.
pub const ID_SWITCH_CURSOR: u32 = 157;
pub const ID_SWITCH_FOCUSED: u32 = 158;
/// Stage 2c fresh-window placement radios.
pub const ID_PLACE_CURSOR_MRU: u32 = 159;
pub const ID_PLACE_CURSOR_CLOSEST: u32 = 160;
pub const ID_PLACE_FOCUSED_MRU: u32 = 161;

pub(crate) const GAMING_TIMER_ID: usize = 1;
pub(crate) const GAMING_TIMER_MS: u32 = 1000;

pub(crate) const ESCAPE_VK: usize = 0x1B;

/// Draft plus saved state for one open settings window.
pub(crate) struct App {
    pub(crate) font: HFONT,
    pub(crate) controls: Vec<(u32, HWND)>,
    pub(crate) dir: PathBuf,
    pub(crate) base: Settings,
    pub(crate) base_present: bool,
    pub(crate) work: Settings,
    pub(crate) selected: Option<usize>,
    pub(crate) invalid: Option<String>,
    pub(crate) tip_text: [Vec<u16>; 18],
    pub(crate) gaming: crate::gaming::GamingMonitor,
    pub(crate) gaming_generation: u64,
    pub(crate) current_tab: usize,
    pub(crate) dirty: bool,
    pub(crate) groups: Vec<(usize, HWND)>,
    pub(crate) statics: Vec<(usize, HWND)>,
    pub(crate) window: HWND,
}

impl App {
    pub(crate) fn ctl(&self, id: u32) -> HWND {
        self.controls
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, hwnd)| *hwnd)
            .unwrap_or(std::ptr::null_mut())
    }
}

pub(crate) fn status(app: &App, text: &str) {
    set_text(app.ctl(ID_STATUS), text);
}

fn catalog_index_of(id: &str) -> Option<usize> {
    binding_catalog().iter().position(|def| def.id == id)
}

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

pub(crate) fn show_tab_for(app: &mut App, index: usize) {
    show_tab(app, index);
}

pub(crate) fn refresh_badge_for(app: &App) {
    refresh_badge(app);
}

pub(crate) fn handle_timer(app: &mut App) {
    drop(app.gaming.drain_logs());
    app.gaming.maybe_refresh(Some(app.work.revision));
    let generation = app.gaming.snapshot().generation;
    if generation != app.gaming_generation {
        app.gaming_generation = generation;
        refresh_list(app);
        refresh_badge(app);
    }
}

pub(crate) fn handle_command(app: &mut App, id: u32, notify: u32) -> bool {
    on_command(app, id, notify)
}

pub(crate) fn show_tab(app: &mut App, index: usize) {
    use crate::ffi::settings_controls::{enable, get_focus, set_focus, set_tab_sel, show_window};
    let index = index % tabs::TAB_COUNT;
    app.current_tab = index;
    let strip = app.ctl(ID_TABS);
    if !strip.is_null() {
        set_tab_sel(strip, index);
    }
    let focused = get_focus();
    let mut focus_hidden = false;
    for (id, hwnd) in app.controls.clone() {
        let visible = match tabs::control_tab(id) {
            None => true,
            Some(tab) => tab == index,
        };
        show_window(hwnd, visible);
        crate::ffi::settings_controls::enable(hwnd, visible);
        if hwnd == focused && tabs::needs_focus_move(id, index) {
            focus_hidden = true;
        }
    }
    for (tab, grouped) in app.groups.clone() {
        show_window(grouped, tab == index);
    }
    for (tab, labelled) in app.statics.clone() {
        let visible = tab == index;
        show_window(labelled, visible);
        enable(labelled, visible);
        if labelled == focused && !visible {
            focus_hidden = true;
        }
    }
    if focus_hidden && !strip.is_null() {
        set_focus(strip);
    }
    enable(app.ctl(ID_APPLY), app.invalid.is_none());
    refresh_list(app);
    for (id, hwnd) in app.controls.clone() {
        if matches!(tabs::control_tab(id), Some(tab) if tab != index) {
            enable(hwnd, false);
        }
    }
    refresh_badge(app);
}

fn on_command(app: &mut App, id: u32, notify: u32) -> bool {
    use crate::ffi::settings_controls::{list_cur_sel, send_list_sel_changed};
    const EN_CHANGE: u32 = 768;
    const BN_CLICKED: u32 = 0;
    const LBN_SELCHANGE: u32 = 1;
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
            let picked = list_cur_sel(app.ctl(ID_BINDING_LIST));
            if picked >= 0 {
                let pending = get_text(app.ctl(ID_BIND_CHORD));
                if !pending.trim().is_empty() {
                    if let Err(reason) = apply_rebind_text(app, &pending) {
                        if let Some(old) = app.selected {
                            send_list_sel_changed(app.ctl(ID_BINDING_LIST), old);
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
        | ID_ANCHOR_LARGEST
        | ID_ANCHOR_CLOSEST
        | ID_ANCHOR_NO_SEARCH
        | ID_NO_SPACE_FLOAT
        | ID_NO_SPACE_OVERLAP
        | ID_SWITCH_CURSOR
        | ID_SWITCH_FOCUSED
        | ID_PLACE_CURSOR_MRU
        | ID_PLACE_CURSOR_CLOSEST
        | ID_PLACE_FOCUSED_MRU
        | ID_WORKSPACE_MODE_LOCAL
        | ID_WORKSPACE_MODE_GLOBAL
        | ID_WORKSPACE_MODE_SHARED => {
            app.dirty = true;
        }
        _ => {}
    }
    match id {
        ID_APPLY => do_apply(app),
        ID_REVERT => do_revert(app),
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

pub(crate) fn fixed_minmax_tracks(outer_w: i32, outer_h: i32) -> Option<(i32, i32)> {
    if (1..=tiler_core::bounds::GEOMETRY_BOUND).contains(&outer_w)
        && (1..=tiler_core::bounds::GEOMETRY_BOUND).contains(&outer_h)
    {
        Some((outer_w, outer_h))
    } else {
        None
    }
}

pub(crate) fn fixed_tracks_ready(user_data: isize, lparam: isize) -> bool {
    user_data != 0 && lparam != 0
}

/// Open the modal settings window and run its message loop. Returns when the
/// window closes; Close never applies.
pub fn cmd_settings() -> Result<String, DynError> {
    use windows_sys::Win32::System::SystemServices::SS_LEFT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BS_DEFPUSHBUTTON, BS_GROUPBOX, BS_PUSHBUTTON,
        ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE, ES_READONLY, LBS_NOINTEGRALHEIGHT,
        LBS_NOTIFY, WS_BORDER, WS_CAPTION, WS_CLIPCHILDREN, WS_EX_DLGMODALFRAME, WS_GROUP,
        WS_HSCROLL, WS_MINIMIZEBOX, WS_SYSMENU, WS_TABSTOP, WS_VSCROLL,
    };
    crate::tiling_sys::ensure_pm_v2()?;
    let sid = crate::native::current_identity()
        .map_err(|error| err(format!("error: settings identity: {error}")))?
        .user_sid;
    let safe_sid: String = sid
        .chars()
        .map(|c| if c == '\\' || c == '/' { '_' } else { c })
        .collect();
    let mutex_name = format!("Local\\OmniTilerSettings-{safe_sid}");
    let singleton =
        match crate::ffi::settings_window::acquire_singleton(SETTINGS_WINDOW_CLASS, &mutex_name)
            .map_err(err)?
        {
            Some(guard) => guard,
            None => return Ok("settings already open".to_owned()),
        };
    let dir = crate::native::settings_directory().map_err(|e| err(format!("error: {e}")))?;
    let (base, present, invalid) = load_base(&dir);
    let dpi = crate::ffi::dpi::system_dpi();
    let font = crate::ffi::settings_controls::message_font(dpi);

    let gaming =
        crate::gaming::GamingMonitor::new(std::sync::Arc::new(crate::tray_sys::gaming::probe_all));
    gaming.sync_revision(Some(base.revision));
    gaming.request_refresh(crate::gaming::RefreshReason::Startup);
    crate::ffi::settings_window::ensure_tab_classes().map_err(err)?;
    let app = Box::new(std::cell::RefCell::new(App {
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
    }));
    let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
    let client_w = ctl::px(tabs::CLIENT_W, dpi);
    let client_h = ctl::px(tabs::CLIENT_H, dpi);
    let window = crate::ffi::settings_window::SettingsWindow::create(
        SETTINGS_WINDOW_CLASS,
        SETTINGS_WINDOW_TITLE,
        style,
        WS_EX_DLGMODALFRAME,
        tabs::ORIGIN_X,
        tabs::ORIGIN_Y,
        client_w,
        client_h,
        app,
        Some(singleton),
    )
    .map_err(|(_, message)| err(message))?;
    let hinst = crate::ffi::settings_window::SettingsWindow::hinstance();
    let hwnd = window.hwnd();
    let tab = WS_TABSTOP;
    let edits = ES_AUTOHSCROLL as u32;
    let push = BS_PUSHBUTTON as u32;
    let check = BS_AUTOCHECKBOX as u32;
    let radio = BS_AUTORADIOBUTTON as u32;
    let radio_first = radio | WS_GROUP;
    let group = BS_GROUPBOX as u32;
    let defpush = BS_DEFPUSHBUTTON as u32;
    let label = SS_LEFT;
    let list_style =
        LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32 | WS_VSCROLL | WS_HSCROLL | WS_BORDER;
    let edit_style = WS_BORDER | edits;
    let info_style = ES_MULTILINE as u32
        | ES_AUTOVSCROLL as u32
        | ES_READONLY as u32
        | WS_VSCROLL
        | WS_BORDER
        | tab;
    let style_for = |id: u32, class: &str| -> u32 {
        if class == "SysTabControl32" {
            return tab;
        }
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
            | ID_ANCHOR_CLOSEST
            | ID_ANCHOR_NO_SEARCH
            | ID_NO_SPACE_OVERLAP
            | ID_SWITCH_FOCUSED
            | ID_PLACE_CURSOR_CLOSEST
            | ID_PLACE_FOCUSED_MRU
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
    let mut owned: Vec<(u32, HWND)> = Vec::new();
    let mut grouped: Vec<(usize, HWND)> = Vec::new();
    let mut labelled: Vec<(usize, HWND)> = Vec::new();
    let font_copy = window
        .with_app(|app| app.font)
        .unwrap_or(std::ptr::null_mut());
    for step in tabs::creation_sequence() {
        let result: Result<(), DynError> = (|| {
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
                    create_ctl(hwnd, hinst, &spec, dpi, font_copy, &mut owned).map(|_| ())?;
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
                    let mut sink = Vec::new();
                    match create_ctl(hwnd, hinst, &spec, dpi, font_copy, &mut sink) {
                        Ok(handle) => {
                            grouped.push((tabs::group_tab(key).expect("group tab"), handle))
                        }
                        Err(error) => return Err(error),
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
                    let mut sink = Vec::new();
                    match create_ctl(hwnd, hinst, &spec, dpi, font_copy, &mut sink) {
                        Ok(handle) => labelled.push((page, handle)),
                        Err(error) => return Err(error),
                    }
                }
                tabs::CreateStep::Control(id) => {
                    if id == ID_TABS {
                        return Ok(());
                    }
                    let Some((x, y, w, h)) = tabs::control_rect(id) else {
                        return Ok(());
                    };
                    let Some(class) = tabs::control_class(id) else {
                        return Ok(());
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
                    let handle = create_ctl(hwnd, hinst, &spec, dpi, font_copy, &mut owned)?;
                    if spec.class == "EDIT"
                        && let Some(limit) = edit_limit(spec.id)
                    {
                        crate::ffi::settings_controls::set_edit_limit(handle, limit);
                    }
                }
            }
            Ok(())
        })();
        result?;
    }
    window
        .with_app_mut(|app| {
            app.controls = owned;
            app.groups = grouped;
            app.statics = labelled;
            app.window = hwnd;
        })
        .unwrap_or(());
    debug_assert!(tabs::layout_valid());
    debug_assert!(tabs::client_fits_1080p(dpi));
    debug_assert!(tabs::origin_fits_1080p(dpi));
    window.apply_zorder(&tabs::zorder_plan()).map_err(err)?;
    window
        .with_app_mut(|app| {
            refresh_all(app);
        })
        .unwrap_or(());
    {
        let titles = tabs::tab_titles();
        window.insert_tab_items(&titles).map_err(err)?;
    }
    window.attach_tips();
    window
        .with_app_mut(|app| {
            show_tab(app, 0);
            let invalid = app.invalid.clone();
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
        })
        .unwrap_or(());
    window.show();
    window.start_gaming_timer();
    window.run_loop().map_err(err)?;
    Ok("settings closed".to_owned())
}
