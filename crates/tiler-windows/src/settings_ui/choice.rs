#![forbid(unsafe_code)]
//! Two/three-choice option controls for the settings window.
//!
//! Radio selection/staging helpers plus the native tooltip text buffers
//! naming the reference WM per choice. Functional labels stay in the dialog;
//! WM references live only in the tooltips. Tooltip attach lives in
//! [`crate::ffi::settings_window`]; no Win32 lives here.

use crate::settings::Settings;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

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

/// Best-effort admission D2 selector state from saved/draft settings: the
/// index into [`crate::settings::anchor_fallback_options`] (0
/// largest-splittable default, 1 closest-relative, 2 no-search). Unknown
/// values (possible only in hand-built drafts, never from disk) show the
/// default.
pub(super) fn anchor_fallback_selected(settings: &Settings) -> usize {
    match settings.core.anchor_fallback.as_str() {
        s if s == crate::settings::ANCHOR_FALLBACK_CLOSEST => 1,
        s if s == crate::settings::ANCHOR_FALLBACK_NO_SEARCH => 2,
        _ => 0,
    }
}

/// Best-effort admission D2 draft staging from the selector index: writes
/// the exact wire token for the selected choice, so Apply persists a
/// validated value. Out-of-range indices stage the default, never an
/// unknown token.
pub(super) fn apply_anchor_fallback_selection(draft: &mut Settings, index: usize) {
    draft.core.anchor_fallback = match index {
        1 => crate::settings::ANCHOR_FALLBACK_CLOSEST.to_owned(),
        2 => crate::settings::ANCHOR_FALLBACK_NO_SEARCH.to_owned(),
        _ => crate::settings::ANCHOR_FALLBACK_LARGEST.to_owned(),
    };
}

/// Best-effort admission D2 native tooltip text buffers from the shared
/// options table, wide-encoded for the tooltip control. Radio labels stay
/// functional-only per the naming rule.
pub(super) fn anchor_fallback_tip_texts() -> [Vec<u16>; 3] {
    let options = crate::settings::anchor_fallback_options();
    [wide(options[0].2), wide(options[1].2), wide(options[2].2)]
}

/// Best-effort admission D3 radio selection from saved/draft settings: true
/// selects the allow-overlap radio, false the float default. Unknown values
/// (possible only in hand-built drafts, never from disk) show the default.
pub(super) fn no_space_overlap_selected(settings: &Settings) -> bool {
    settings.core.no_space_result == crate::settings::NO_SPACE_OVERLAP
}

/// Best-effort admission D3 draft staging from the overlap radio state:
/// writes the exact wire token for the selected choice, so Apply persists
/// a validated value.
pub(super) fn apply_no_space_selection(draft: &mut Settings, overlap: bool) {
    draft.core.no_space_result = if overlap {
        crate::settings::NO_SPACE_OVERLAP.to_owned()
    } else {
        crate::settings::NO_SPACE_FLOAT.to_owned()
    };
}

/// Best-effort admission D3 native tooltip text buffers from the shared
/// options table, wide-encoded for the tooltip control. Radio labels stay
/// functional-only per the naming rule.
pub(super) fn no_space_tip_texts() -> [Vec<u16>; 2] {
    let options = crate::settings::no_space_options();
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

/// All thirteen choice tooltip buffers in control order: the two
/// R-MOV-03 buffers, the two R-MAX-06 buffers, the two R-SPC-04 D1
/// fixed-size buffers, the two R-WS-12e G-37 migration-refill buffers,
/// then the three best-effort admission D2 buffers and the two D3 buffers.
pub(super) fn choice_tip_texts() -> [Vec<u16>; 13] {
    let axis = same_axis_tip_texts();
    let born = born_maximized_tip_texts();
    let fixed = fixed_size_tip_texts();
    let refill = migration_refill_tip_texts();
    let anchor = anchor_fallback_tip_texts();
    let no_space = no_space_tip_texts();
    let [a0, a1] = axis;
    let [b0, b1] = born;
    let [c0, c1] = fixed;
    let [d0, d1] = refill;
    let [e0, e1, e2] = anchor;
    let [f0, f1] = no_space;
    [a0, a1, b0, b1, c0, c1, d0, d1, e0, e1, e2, f0, f1]
}
