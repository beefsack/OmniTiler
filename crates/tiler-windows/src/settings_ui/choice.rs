//! Two/three-choice option controls for the settings window.
//!
//! Radio selection/staging helpers plus the native tooltip buffers naming the
//! reference WM per choice. Functional labels stay in the dialog; WM
//! references live only in the tooltips.

use windows_sys::Win32::Foundation::HWND;

use crate::settings::Settings;

use super::ctl::wide;
use super::{
    App, ID_BORN_MAX_OVERLAY, ID_BORN_MAX_TILE, ID_FIXED_SIZE_BOTH, ID_FIXED_SIZE_EITHER,
    ID_MIGRATION_REFILL_LAST, ID_MIGRATION_REFILL_MRU, ID_SAME_AXIS_GROUP, ID_SAME_AXIS_SWAP,
};

/// R-MOV-03 radio selection from saved/draft settings: true selects the
/// flat-swap radio, false the group default. Unknown values (possible only
/// in hand-built drafts, never from disk) show the group default.
pub(super) fn same_axis_swap_selected(settings: &Settings) -> bool {
    settings.core.same_axis_move == crate::settings::SAME_AXIS_MOVE_SWAP
}

/// R-MOV-03 draft staging from the swap radio state: writes the exact wire
/// token for the selected choice, so Apply persists a validated value.
pub(super) fn apply_same_axis_selection(draft: &mut Settings, swap: bool) {
    draft.core.same_axis_move = if swap {
        crate::settings::SAME_AXIS_MOVE_SWAP.to_owned()
    } else {
        crate::settings::SAME_AXIS_MOVE_GROUP.to_owned()
    };
}

/// R-MOV-03 native tooltip text buffers from the shared options table: the
/// WM reference per choice (`COSMIC` / `i3, sway`), wide-encoded for the
/// tooltip control. Radio labels stay functional-only per the naming rule.
pub(super) fn same_axis_tip_texts() -> [Vec<u16>; 2] {
    let options = crate::settings::same_axis_move_options();
    [wide(options[0].2), wide(options[1].2)]
}

/// R-MAX-06 radio selection from saved/draft settings: true selects the
/// keep-overlay radio, false the tile-on-launch default. Unknown values
/// (possible only in hand-built drafts, never from disk) show the default.
pub(super) fn born_overlay_selected(settings: &Settings) -> bool {
    settings.core.born_maximized_keep_overlay()
}

/// R-MAX-06 draft staging from the overlay radio state: writes the exact wire
/// token for the selected choice, so Apply persists a validated value.
pub(super) fn apply_born_maximized_selection(draft: &mut Settings, overlay: bool) {
    draft.core.born_maximized = if overlay {
        crate::settings::BORN_MAXIMIZED_OVERLAY.to_owned()
    } else {
        crate::settings::BORN_MAXIMIZED_TILE.to_owned()
    };
}

/// R-MAX-06 native tooltip text buffers from the shared options table: the
/// WM reference per choice (`bspwm, i3, xmonad, sway, qtile` /
/// `COSMIC, Hyprland`), wide-encoded for the tooltip control. Radio labels
/// stay functional-only per the naming rule.
pub(super) fn born_maximized_tip_texts() -> [Vec<u16>; 2] {
    let options = crate::settings::born_maximized_options();
    [wide(options[0].2), wide(options[1].2)]
}

/// R-SPC-04 D1 radio selection from saved/draft settings: true selects the
/// either-axis radio, false the both-axes default. Unknown values (possible
/// only in hand-built drafts, never from disk) show the default.
pub(super) fn fixed_size_either_selected(settings: &Settings) -> bool {
    settings.core.fixed_size_predicate == crate::settings::FIXED_SIZE_EITHER_AXIS
}

/// R-SPC-04 D1 draft staging from the either-axis radio state: writes the
/// exact wire token for the selected choice, so Apply persists a validated
/// value.
pub(super) fn apply_fixed_size_selection(draft: &mut Settings, either: bool) {
    draft.core.fixed_size_predicate = if either {
        crate::settings::FIXED_SIZE_EITHER_AXIS.to_owned()
    } else {
        crate::settings::FIXED_SIZE_BOTH_AXES.to_owned()
    };
}

/// R-SPC-04 D1 native tooltip text buffers from the shared options table:
/// the WM reference per choice (`COSMIC` / `Hyprland (Wayland), sway`),
/// wide-encoded for the tooltip control. Radio labels stay functional-only
/// per the naming rule.
pub(super) fn fixed_size_tip_texts() -> [Vec<u16>; 2] {
    let options = crate::settings::fixed_size_predicate_options();
    [wide(options[0].2), wide(options[1].2)]
}

/// R-WS-12e G-37 radio selection from saved/draft settings: true selects the
/// most-recently-used radio, false the last-remaining default. Unknown values
/// (possible only in hand-built drafts, never from disk) show the default.
pub(super) fn migration_refill_mru_selected(settings: &Settings) -> bool {
    settings.core.migration_source_refill == crate::settings::MIGRATION_REFILL_MRU
}

/// R-WS-12e G-37 draft staging from the most-recently-used radio state:
/// writes the exact wire token for the selected choice, so Apply persists a
/// validated value.
pub(super) fn apply_migration_refill_selection(draft: &mut Settings, mru: bool) {
    draft.core.migration_source_refill = if mru {
        crate::settings::MIGRATION_REFILL_MRU.to_owned()
    } else {
        crate::settings::MIGRATION_REFILL_LAST_REMAINING.to_owned()
    };
}

/// R-WS-12e G-37 native tooltip text buffers from the shared options table:
/// the WM reference per choice (`COSMIC` / `bspwm, i3, awesome`),
/// wide-encoded for the tooltip control. Radio labels stay functional-only
/// per the naming rule.
pub(super) fn migration_refill_tip_texts() -> [Vec<u16>; 2] {
    let options = crate::settings::migration_source_refill_options();
    [wide(options[0].2), wide(options[1].2)]
}

/// Item 12 selector state from saved/draft settings: the index into
/// [`crate::settings::workspace_mode_options`] (0 per-output-local default,
/// 1 global-unique, 2 shared). Unknown values (possible only in hand-built
/// drafts, never from disk) show the default.
pub(super) fn workspace_mode_selected(settings: &Settings) -> usize {
    match settings.core.workspace.mode.as_str() {
        s if s == crate::settings::WORKSPACE_MODE_GLOBAL_UNIQUE => 1,
        s if s == crate::settings::WORKSPACE_MODE_SHARED => 2,
        _ => 0,
    }
}

/// Item 12 draft staging from the selector index: writes the exact wire token
/// for the selected choice, so Apply persists a validated value. Out-of-range
/// indices stage the default, never an unknown token.
pub(super) fn apply_workspace_mode_selection(draft: &mut Settings, index: usize) {
    draft.core.workspace.mode = match index {
        1 => crate::settings::WORKSPACE_MODE_GLOBAL_UNIQUE.to_owned(),
        2 => crate::settings::WORKSPACE_MODE_SHARED.to_owned(),
        _ => crate::settings::WORKSPACE_MODE_PER_OUTPUT_LOCAL.to_owned(),
    };
}

/// All eight two-choice radio tooltip buffers in control order: the two
/// R-MOV-03 buffers, the two R-MAX-06 buffers, the two R-SPC-04 D1
/// fixed-size buffers, then the two R-WS-12e G-37 migration-refill buffers.
pub(super) fn choice_tip_texts() -> [Vec<u16>; 8] {
    let axis = same_axis_tip_texts();
    let born = born_maximized_tip_texts();
    let fixed = fixed_size_tip_texts();
    let refill = migration_refill_tip_texts();
    let [a0, a1] = axis;
    let [b0, b1] = born;
    let [c0, c1] = fixed;
    let [d0, d1] = refill;
    [a0, a1, b0, b1, c0, c1, d0, d1]
}

/// Attach native tooltips naming the reference WM to the two R-MOV-03
/// radios, the two R-MAX-06 born-maximized radios, the two R-SPC-04 D1
/// fixed-size radios and the two R-WS-12e G-37 migration-refill radios. One
/// `tooltips_class32` child (`TTS_ALWAYSTIP`) with `TTF_SUBCLASS` tools, so
/// hover tracking needs no manual relay. Text pointers ride the `App`-owned
/// `tip_text` buffers (see the field docs for the lifetime). Best-effort: a
/// failed tooltip leaves the functional dialog untouched.
pub(super) fn attach_choice_tips(
    app: &App,
    hwnd: HWND,
    hinst: windows_sys::Win32::Foundation::HINSTANCE,
) {
    use windows_sys::Win32::UI::Controls::{
        ICC_BAR_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx, TTF_IDISHWND, TTF_SUBCLASS,
        TTM_ADDTOOLW, TTS_ALWAYSTIP, TTTOOLINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, SendMessageW, WS_POPUP};
    let icc = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_BAR_CLASSES,
    };
    unsafe {
        InitCommonControlsEx(&icc);
    }
    let class_w = wide("tooltips_class32");
    let tip = unsafe {
        CreateWindowExW(
            0,
            class_w.as_ptr(),
            std::ptr::null(),
            WS_POPUP | TTS_ALWAYSTIP,
            0,
            0,
            0,
            0,
            hwnd,
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        )
    };
    if tip.is_null() {
        return;
    }
    for (id, text) in [
        (ID_SAME_AXIS_GROUP, &app.tip_text[0]),
        (ID_SAME_AXIS_SWAP, &app.tip_text[1]),
        (ID_BORN_MAX_TILE, &app.tip_text[2]),
        (ID_BORN_MAX_OVERLAY, &app.tip_text[3]),
        (ID_FIXED_SIZE_BOTH, &app.tip_text[4]),
        (ID_FIXED_SIZE_EITHER, &app.tip_text[5]),
        (ID_MIGRATION_REFILL_LAST, &app.tip_text[6]),
        (ID_MIGRATION_REFILL_MRU, &app.tip_text[7]),
    ] {
        let control = app.ctl(id);
        if control.is_null() {
            continue;
        }
        let info = TTTOOLINFOW {
            cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
            uFlags: TTF_IDISHWND | TTF_SUBCLASS,
            hwnd,
            uId: control as usize,
            rect: unsafe { std::mem::zeroed() },
            hinst: std::ptr::null_mut(),
            lpszText: text.as_ptr() as *mut u16,
            lParam: 0,
            lpReserved: std::ptr::null_mut(),
        };
        unsafe {
            SendMessageW(tip, TTM_ADDTOOLW, 0, &info as *const TTTOOLINFOW as isize);
        }
    }
}
