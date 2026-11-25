//! Unit tests for the settings window helpers (choice staging, tooltip text,
//! dialog fit). All portable logic; no window is created.
#![forbid(unsafe_code)]

use super::choice::{
    apply_born_maximized_selection, apply_fixed_size_selection, apply_fresh_placement_selection,
    apply_migration_refill_selection, apply_same_axis_selection, apply_switch_target_selection,
    apply_workspace_mode_selection, born_maximized_tip_texts, born_overlay_selected,
    choice_tip_texts, fixed_size_either_selected, fixed_size_tip_texts, fresh_placement_selected,
    fresh_placement_tip_texts, migration_refill_mru_selected, migration_refill_tip_texts,
    same_axis_swap_selected, same_axis_tip_texts, switch_focused_selected, switch_target_tip_texts,
    workspace_mode_selected,
};
use super::ctl::wide;
use crate::settings::{
    BORN_MAXIMIZED_OVERLAY, BORN_MAXIMIZED_TILE, FIXED_SIZE_BOTH_AXES, FIXED_SIZE_EITHER_AXIS,
    FRESH_PLACEMENT_CURSOR_CLOSEST, FRESH_PLACEMENT_CURSOR_MRU, FRESH_PLACEMENT_FOCUSED_MRU,
    MIGRATION_REFILL_LAST_REMAINING, MIGRATION_REFILL_MRU, SAME_AXIS_MOVE_GROUP,
    SAME_AXIS_MOVE_SWAP, SWITCH_TARGET_CURSOR, SWITCH_TARGET_FOCUSED, Settings,
    WORKSPACE_MODE_GLOBAL_UNIQUE, WORKSPACE_MODE_PER_OUTPUT_LOCAL, WORKSPACE_MODE_SHARED,
};

#[test]
fn same_axis_radio_round_trips_both_tokens() {
    let mut draft = Settings::default();
    assert!(!same_axis_swap_selected(&draft));
    apply_same_axis_selection(&mut draft, true);
    assert_eq!(draft.core.same_axis_move, SAME_AXIS_MOVE_SWAP);
    assert!(same_axis_swap_selected(&draft));
    apply_same_axis_selection(&mut draft, false);
    assert_eq!(draft.core.same_axis_move, SAME_AXIS_MOVE_GROUP);
    assert!(!same_axis_swap_selected(&draft));
}

#[test]
fn same_axis_radio_shows_group_for_unknown_hand_built_value() {
    let mut draft = Settings::default();
    draft.core.same_axis_move = "retired-alias".to_owned();
    assert!(!same_axis_swap_selected(&draft));
}

#[test]
fn same_axis_selection_validates_after_apply() {
    for swap in [false, true] {
        let mut draft = Settings::default();
        apply_same_axis_selection(&mut draft, swap);
        crate::settings::validate_settings(&draft).expect("radio stages a valid token");
    }
}

#[test]
fn same_axis_tip_texts_come_from_shared_options() {
    let options = crate::settings::same_axis_move_options();
    let tips = same_axis_tip_texts();
    assert_eq!(tips.len(), 2);
    for (buffer, option) in tips.iter().zip(options.iter()) {
        assert_eq!(*buffer, wide(option.2));
    }
    // Exact WM references, functional labels kept out of the tips.
    assert_eq!(tips[0], wide("COSMIC"));
    assert_eq!(tips[1], wide("i3, sway"));
}

#[test]
fn born_maximized_radio_round_trips_both_tokens() {
    let mut draft = Settings::default();
    assert!(!born_overlay_selected(&draft));
    apply_born_maximized_selection(&mut draft, true);
    assert_eq!(draft.core.born_maximized, BORN_MAXIMIZED_OVERLAY);
    assert!(born_overlay_selected(&draft));
    apply_born_maximized_selection(&mut draft, false);
    assert_eq!(draft.core.born_maximized, BORN_MAXIMIZED_TILE);
    assert!(!born_overlay_selected(&draft));
}

#[test]
fn born_maximized_radio_shows_tile_for_unknown_hand_built_value() {
    let mut draft = Settings::default();
    draft.core.born_maximized = "retired-alias".to_owned();
    assert!(!born_overlay_selected(&draft));
}

#[test]
fn born_maximized_selection_validates_after_apply() {
    for overlay in [false, true] {
        let mut draft = Settings::default();
        apply_born_maximized_selection(&mut draft, overlay);
        crate::settings::validate_settings(&draft).expect("radio stages a valid token");
    }
}

#[test]
fn born_maximized_tip_texts_come_from_shared_options() {
    let options = crate::settings::born_maximized_options();
    let tips = born_maximized_tip_texts();
    assert_eq!(tips.len(), 2);
    for (buffer, option) in tips.iter().zip(options.iter()) {
        assert_eq!(*buffer, wide(option.2));
    }
    // Exact WM references, functional labels kept out of the tips.
    assert_eq!(tips[0], wide("bspwm, i3, xmonad, sway, qtile"));
    assert_eq!(tips[1], wide("COSMIC, Hyprland"));
}

#[test]
fn choice_tip_buffers_cover_all_radio_pairs() {
    // Thirteen buffers in control order: same-axis pair, born-maximized
    // pair, fixed-size pair, migration-refill pair, stage 2c switch-target
    // pair, and stage 2c fresh-placement triple, so each tooltip tool reads
    // its own choice text.
    let tips = choice_tip_texts();
    assert_eq!(tips.len(), 13);
    assert_eq!(tips[0], wide("COSMIC"));
    assert_eq!(tips[1], wide("i3, sway"));
    assert_eq!(tips[0], same_axis_tip_texts()[0]);
    assert_eq!(tips[1], same_axis_tip_texts()[1]);
    assert_eq!(tips[2], born_maximized_tip_texts()[0]);
    assert_eq!(tips[3], born_maximized_tip_texts()[1]);
    assert_eq!(tips[4], fixed_size_tip_texts()[0]);
    assert_eq!(tips[5], fixed_size_tip_texts()[1]);
    assert_eq!(tips[4], wide("COSMIC"));
    assert_eq!(tips[5], wide("Hyprland (Wayland), sway"));
    assert_eq!(tips[6], migration_refill_tip_texts()[0]);
    assert_eq!(tips[7], migration_refill_tip_texts()[1]);
    assert_eq!(tips[6], wide("COSMIC"));
    assert_eq!(tips[7], wide("bspwm, i3, awesome"));
    assert_eq!(tips[8], switch_target_tip_texts()[0]);
    assert_eq!(tips[9], switch_target_tip_texts()[1]);
    assert_eq!(tips[8], wide("COSMIC"));
    assert_eq!(tips[9], wide("i3, sway"));
    assert_eq!(tips[10], fresh_placement_tip_texts()[0]);
    assert_eq!(tips[11], fresh_placement_tip_texts()[1]);
    assert_eq!(tips[12], fresh_placement_tip_texts()[2]);
    assert_eq!(tips[10], wide("COSMIC"));
    assert_eq!(tips[11], wide("Hyprland"));
    assert_eq!(tips[12], wide("i3, sway"));
}

#[test]
fn fixed_size_radio_round_trips_both_tokens() {
    let mut draft = Settings::default();
    assert!(!fixed_size_either_selected(&draft));
    apply_fixed_size_selection(&mut draft, true);
    assert_eq!(draft.core.fixed_size_predicate, FIXED_SIZE_EITHER_AXIS);
    assert!(fixed_size_either_selected(&draft));
    apply_fixed_size_selection(&mut draft, false);
    assert_eq!(draft.core.fixed_size_predicate, FIXED_SIZE_BOTH_AXES);
    assert!(!fixed_size_either_selected(&draft));
}

#[test]
fn fixed_size_radio_shows_both_axes_for_unknown_hand_built_value() {
    let mut draft = Settings::default();
    draft.core.fixed_size_predicate = "retired-alias".to_owned();
    assert!(!fixed_size_either_selected(&draft));
}

#[test]
fn fixed_size_selection_validates_after_apply() {
    for either in [false, true] {
        let mut draft = Settings::default();
        apply_fixed_size_selection(&mut draft, either);
        crate::settings::validate_settings(&draft).expect("radio stages a valid token");
    }
}

#[test]
fn fixed_size_tip_texts_come_from_shared_options() {
    let options = crate::settings::fixed_size_predicate_options();
    let tips = fixed_size_tip_texts();
    assert_eq!(tips.len(), 2);
    for (buffer, option) in tips.iter().zip(options.iter()) {
        assert_eq!(*buffer, wide(option.2));
    }
    // Exact WM references, functional labels kept out of the tips.
    assert_eq!(tips[0], wide("COSMIC"));
    assert_eq!(tips[1], wide("Hyprland (Wayland), sway"));
}

#[test]
fn migration_refill_radio_round_trips_both_tokens() {
    let mut draft = Settings::default();
    assert!(!migration_refill_mru_selected(&draft));
    apply_migration_refill_selection(&mut draft, true);
    assert_eq!(draft.core.migration_source_refill, MIGRATION_REFILL_MRU);
    assert!(migration_refill_mru_selected(&draft));
    apply_migration_refill_selection(&mut draft, false);
    assert_eq!(
        draft.core.migration_source_refill,
        MIGRATION_REFILL_LAST_REMAINING
    );
    assert!(!migration_refill_mru_selected(&draft));
}

#[test]
fn migration_refill_radio_shows_last_remaining_for_unknown_hand_built_value() {
    let mut draft = Settings::default();
    draft.core.migration_source_refill = "retired-alias".to_owned();
    assert!(!migration_refill_mru_selected(&draft));
}

#[test]
fn migration_refill_selection_validates_after_apply() {
    for mru in [false, true] {
        let mut draft = Settings::default();
        apply_migration_refill_selection(&mut draft, mru);
        crate::settings::validate_settings(&draft).expect("radio stages a valid token");
    }
}

#[test]
fn migration_refill_tip_texts_come_from_shared_options() {
    let options = crate::settings::migration_source_refill_options();
    let tips = migration_refill_tip_texts();
    assert_eq!(tips.len(), 2);
    for (buffer, option) in tips.iter().zip(options.iter()) {
        assert_eq!(*buffer, wide(option.2));
    }
    // Exact WM references, functional labels kept out of the tips.
    assert_eq!(tips[0], wide("COSMIC"));
    assert_eq!(tips[1], wide("bspwm, i3, awesome"));
}

#[test]
fn migration_refill_options_carry_functional_labels() {
    assert_eq!(
        crate::settings::migration_source_refill_options(),
        [
            (
                MIGRATION_REFILL_LAST_REMAINING,
                "Last remaining workspace",
                "COSMIC"
            ),
            (
                MIGRATION_REFILL_MRU,
                "Most recently used workspace",
                "bspwm, i3, awesome"
            ),
        ]
    );
}

#[test]
fn workspace_mode_selector_round_trips_all_three_tokens() {
    let mut draft = Settings::default();
    assert_eq!(workspace_mode_selected(&draft), 0);
    apply_workspace_mode_selection(&mut draft, 1);
    assert_eq!(draft.core.workspace.mode, WORKSPACE_MODE_GLOBAL_UNIQUE);
    assert_eq!(workspace_mode_selected(&draft), 1);
    apply_workspace_mode_selection(&mut draft, 2);
    assert_eq!(draft.core.workspace.mode, WORKSPACE_MODE_SHARED);
    assert_eq!(workspace_mode_selected(&draft), 2);
    apply_workspace_mode_selection(&mut draft, 0);
    assert_eq!(draft.core.workspace.mode, WORKSPACE_MODE_PER_OUTPUT_LOCAL);
    assert_eq!(workspace_mode_selected(&draft), 0);
}

#[test]
fn workspace_mode_selector_shows_local_for_unknown_hand_built_value() {
    let mut draft = Settings::default();
    draft.core.workspace.mode = "retired-alias".to_owned();
    assert_eq!(workspace_mode_selected(&draft), 0);
}

#[test]
fn workspace_mode_out_of_range_index_stages_the_default() {
    let mut draft = Settings::default();
    apply_workspace_mode_selection(&mut draft, 99);
    assert_eq!(draft.core.workspace.mode, WORKSPACE_MODE_PER_OUTPUT_LOCAL);
    assert!(crate::settings::validate_settings(&draft).is_ok());
}

#[test]
fn workspace_mode_selection_validates_after_apply() {
    for index in [0, 1, 2] {
        let mut draft = Settings::default();
        apply_workspace_mode_selection(&mut draft, index);
        crate::settings::validate_settings(&draft).expect("selector stages a valid token");
    }
}

#[test]
fn workspace_mode_options_carry_functional_labels() {
    assert_eq!(
        crate::settings::workspace_mode_options(),
        [
            (
                WORKSPACE_MODE_PER_OUTPUT_LOCAL,
                "Per-output local",
                "Each output keeps its own set"
            ),
            (
                WORKSPACE_MODE_GLOBAL_UNIQUE,
                "Global unique",
                "Distinct sets with cross-output swap"
            ),
            (
                WORKSPACE_MODE_SHARED,
                "Workspaces span displays",
                "All displays show and switch the same workspace together"
            ),
        ]
    );
}

#[test]
fn window_style_is_a_normal_resizable_application_frame() {
    // Settings is an ordinary resizable app window (contract decision), so
    // the frame carries the sizing border plus minimize/maximize boxes and
    // no modal dialog extended style. Any fixed min/max track override
    // would defeat resizing and is gone from the window procedure.
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        WS_CAPTION, WS_CLIPCHILDREN, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU, WS_THICKFRAME,
    };
    let style = super::window_style();
    for bit in [
        WS_CAPTION,
        WS_SYSMENU,
        WS_THICKFRAME,
        WS_MINIMIZEBOX,
        WS_MAXIMIZEBOX,
        WS_CLIPCHILDREN,
    ] {
        assert_ne!(style & bit, 0, "resizable frame missing {bit:#x}");
    }
    assert_eq!(super::window_ex_style(), 0, "no modal dialog frame");
}

#[test]
fn settings_class_stays_an_ordinary_workspace_member() {
    // Resizability changes the frame, never the identity rules: the shell
    // class is the classifier's own-settings class, and an own-executable
    // window of that class is an ordinary slotless member, not a dialog.
    assert_eq!(
        super::SETTINGS_WINDOW_CLASS,
        crate::tiling::OWN_SETTINGS_WINDOW_CLASS
    );
    assert!(
        !crate::tiling::workspace_dialog_refuses(
            super::SETTINGS_WINDOW_CLASS,
            false,
            "C:\\apps\\tiler-windows.exe",
            "C:\\apps\\tiler-windows.exe",
        ),
        "own Settings must stay an ordinary member, never a dialog"
    );
}

/// Every design rect the shell creates: automated controls, groupboxes,
/// and label statics. The relayout model must cover all three families.
fn design_rects() -> Vec<(i32, i32, i32, i32)> {
    use super::tabs;
    let mut rects: Vec<(i32, i32, i32, i32)> = tabs::ALL_CONTROL_IDS
        .iter()
        .filter_map(|id| tabs::control_rect(*id))
        .collect();
    rects.extend(tabs::GROUPS.iter().map(|(_, x, y, w, h)| (*x, *y, *w, *h)));
    rects.extend(
        tabs::LABEL_RECTS
            .iter()
            .map(|(_, x, y, w, h)| (*x, *y, *w, *h)),
    );
    rects
}

#[test]
fn relayout_is_identity_at_the_design_client() {
    use super::tabs;
    for rect in design_rects() {
        assert_eq!(
            super::relayout_rect(rect, tabs::CLIENT_W, tabs::CLIENT_H),
            rect,
            "design layout must not move at its own client size"
        );
    }
}

#[test]
fn relayout_scales_every_rect_inside_a_larger_client() {
    use super::tabs;
    let (client_w, client_h) = (tabs::CLIENT_W * 2, tabs::CLIENT_H * 2);
    let mut grew = false;
    for rect in design_rects() {
        let (x, y, w, h) = super::relayout_rect(rect, client_w, client_h);
        assert!(x >= 0 && y >= 0, "origin outside client for {rect:?}");
        assert!(w >= 1 && h >= 1, "non-positive extent for {rect:?}");
        assert!(x + w <= client_w, "right edge outside client for {rect:?}");
        assert!(y + h <= client_h, "bottom edge outside client for {rect:?}");
        if w > rect.2 && h > rect.3 {
            grew = true;
        }
    }
    assert!(grew, "the whole layout tracks a larger client");
}

#[test]
fn relayout_keeps_proportional_margins_when_growing() {
    use super::{ID_TABS, tabs};
    let client_w = tabs::CLIENT_W * 3 / 2;
    let client_h = tabs::CLIENT_H * 3 / 2;
    let (x, y, w, h) = super::relayout_rect(
        tabs::control_rect(ID_TABS).expect("strip rect"),
        client_w,
        client_h,
    );
    assert_eq!((x, y, w, h), (15, 12, 1356, 780));
    assert_eq!(x, client_w - (x + w), "equal left/right margins");
}

#[test]
fn relayout_keeps_tiny_clients_bounded_and_positive() {
    use super::tabs;
    for (client_w, client_h) in [
        (2, 2),
        (120, 90),
        (200, 200),
        (tabs::CLIENT_W / 2, tabs::CLIENT_H / 2),
    ] {
        for rect in design_rects() {
            let (x, y, w, h) = super::relayout_rect(rect, client_w, client_h);
            assert!(
                x >= 0 && y >= 0,
                "origin for {rect:?} at {client_w}x{client_h}"
            );
            assert!(
                w >= 1 && h >= 1,
                "extent for {rect:?} at {client_w}x{client_h}"
            );
            assert!(
                x + w <= client_w,
                "right for {rect:?} at {client_w}x{client_h}"
            );
            assert!(
                y + h <= client_h,
                "bottom for {rect:?} at {client_w}x{client_h}"
            );
        }
    }
}

#[test]
fn relayout_keeps_page_content_above_the_footer() {
    // Proportional mapping is monotone: page content (controls, groups,
    // labels) must stay above the chrome footer at every client size, as
    // it does in the design rects (528 page bottom vs 532 footer top).
    use super::{ID_TABS, tabs};
    for (client_w, client_h) in [
        (tabs::CLIENT_W, tabs::CLIENT_H),
        (tabs::CLIENT_W * 3 / 2, tabs::CLIENT_H * 3 / 2),
        (tabs::CLIENT_W / 2, tabs::CLIENT_H / 2),
        (400, 300),
    ] {
        let mut page_bottom = 0;
        let mut footer_top = i32::MAX;
        for id in tabs::ALL_CONTROL_IDS {
            let Some(rect) = tabs::control_rect(*id) else {
                continue;
            };
            let (_, y, _, h) = super::relayout_rect(rect, client_w, client_h);
            match tabs::control_tab(*id) {
                None => {
                    if *id != ID_TABS {
                        footer_top = footer_top.min(y);
                    }
                }
                Some(_) => page_bottom = page_bottom.max(y + h),
            }
        }
        for (_, x, y, w, h) in tabs::GROUPS {
            let (_, y, _, h) = super::relayout_rect((*x, *y, *w, *h), client_w, client_h);
            page_bottom = page_bottom.max(y + h);
        }
        for (_, x, y, w, h) in tabs::LABEL_RECTS {
            let (_, y, _, h) = super::relayout_rect((*x, *y, *w, *h), client_w, client_h);
            page_bottom = page_bottom.max(y + h);
        }
        assert!(
            page_bottom <= footer_top,
            "page bottom {page_bottom} overlaps footer top {footer_top} at {client_w}x{client_h}"
        );
    }
}
#[test]
fn switch_target_radio_round_trips_both_tokens() {
    let mut draft = Settings::default();
    assert!(!switch_focused_selected(&draft));
    apply_switch_target_selection(&mut draft, true);
    assert_eq!(draft.core.switch_target, SWITCH_TARGET_FOCUSED);
    assert!(switch_focused_selected(&draft));
    apply_switch_target_selection(&mut draft, false);
    assert_eq!(draft.core.switch_target, SWITCH_TARGET_CURSOR);
    assert!(!switch_focused_selected(&draft));
}

#[test]
fn switch_target_radio_shows_cursor_for_unknown_hand_built_value() {
    let mut draft = Settings::default();
    draft.core.switch_target = "retired-alias".to_owned();
    assert!(!switch_focused_selected(&draft));
}

#[test]
fn switch_target_selection_validates_after_apply() {
    for focused in [false, true] {
        let mut draft = Settings::default();
        apply_switch_target_selection(&mut draft, focused);
        crate::settings::validate_settings(&draft).expect("radio stages a valid token");
    }
}

#[test]
fn switch_target_tip_texts_come_from_shared_options() {
    let options = crate::settings::switch_target_options();
    let tips = switch_target_tip_texts();
    assert_eq!(tips.len(), 2);
    for (buffer, option) in tips.iter().zip(options.iter()) {
        assert_eq!(*buffer, wide(option.2));
    }
    // Exact WM references, functional labels kept out of the tips: cursor
    // output is the COSMIC default, focused output the i3/sway option.
    assert_eq!(tips[0], wide("COSMIC"));
    assert_eq!(tips[1], wide("i3, sway"));
}

#[test]
fn fresh_placement_selector_round_trips_all_three_tokens() {
    let mut draft = Settings::default();
    assert_eq!(fresh_placement_selected(&draft), 0);
    apply_fresh_placement_selection(&mut draft, 1);
    assert_eq!(draft.core.fresh_placement, FRESH_PLACEMENT_CURSOR_CLOSEST);
    assert_eq!(fresh_placement_selected(&draft), 1);
    apply_fresh_placement_selection(&mut draft, 2);
    assert_eq!(draft.core.fresh_placement, FRESH_PLACEMENT_FOCUSED_MRU);
    assert_eq!(fresh_placement_selected(&draft), 2);
    apply_fresh_placement_selection(&mut draft, 0);
    assert_eq!(draft.core.fresh_placement, FRESH_PLACEMENT_CURSOR_MRU);
    assert_eq!(fresh_placement_selected(&draft), 0);
}

#[test]
fn fresh_placement_selector_shows_cursor_mru_for_unknown_hand_built_value() {
    let mut draft = Settings::default();
    draft.core.fresh_placement = "retired-alias".to_owned();
    assert_eq!(fresh_placement_selected(&draft), 0);
}

#[test]
fn fresh_placement_out_of_range_index_stages_the_default() {
    let mut draft = Settings::default();
    apply_fresh_placement_selection(&mut draft, 99);
    assert_eq!(draft.core.fresh_placement, FRESH_PLACEMENT_CURSOR_MRU);
}

#[test]
fn fresh_placement_selection_validates_after_apply() {
    for index in [0, 1, 2] {
        let mut draft = Settings::default();
        apply_fresh_placement_selection(&mut draft, index);
        crate::settings::validate_settings(&draft).expect("radio stages a valid token");
    }
}

#[test]
fn fresh_placement_tip_texts_come_from_shared_options() {
    let options = crate::settings::fresh_placement_options();
    let tips = fresh_placement_tip_texts();
    assert_eq!(tips.len(), 3);
    for (buffer, option) in tips.iter().zip(options.iter()) {
        assert_eq!(*buffer, wide(option.2));
    }
    // Placement names: COSMIC (default cursor/MRU), Hyprland
    // (cursor/closest), i3/sway (focused/MRU). No all-settings preset.
    assert_eq!(tips[0], wide("COSMIC"));
    assert_eq!(tips[1], wide("Hyprland"));
    assert_eq!(tips[2], wide("i3, sway"));
}
