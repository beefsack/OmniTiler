//! Unit tests for the settings window helpers (choice staging, tooltip text,
//! dialog fit). All portable logic; no window is created.
#![forbid(unsafe_code)]

use super::choice::{
    anchor_fallback_selected, anchor_fallback_tip_texts, apply_anchor_fallback_selection,
    apply_born_maximized_selection, apply_fixed_size_selection, apply_fresh_placement_selection,
    apply_migration_refill_selection, apply_no_space_selection, apply_same_axis_selection,
    apply_switch_target_selection, apply_workspace_mode_selection, born_maximized_tip_texts,
    born_overlay_selected, choice_tip_texts, fixed_size_either_selected, fixed_size_tip_texts,
    fresh_placement_selected, fresh_placement_tip_texts, migration_refill_mru_selected,
    migration_refill_tip_texts, no_space_overlap_selected, no_space_tip_texts,
    same_axis_swap_selected, same_axis_tip_texts, switch_focused_selected, switch_target_tip_texts,
    workspace_mode_selected,
};
use super::ctl::wide;
use crate::settings::{
    ANCHOR_FALLBACK_CLOSEST, ANCHOR_FALLBACK_LARGEST, ANCHOR_FALLBACK_NO_SEARCH,
    BORN_MAXIMIZED_OVERLAY, BORN_MAXIMIZED_TILE, FIXED_SIZE_BOTH_AXES, FIXED_SIZE_EITHER_AXIS,
    FRESH_PLACEMENT_CURSOR_CLOSEST, FRESH_PLACEMENT_CURSOR_MRU, FRESH_PLACEMENT_FOCUSED_MRU,
    MIGRATION_REFILL_LAST_REMAINING, MIGRATION_REFILL_MRU, NO_SPACE_FLOAT, NO_SPACE_OVERLAP,
    SAME_AXIS_MOVE_GROUP, SAME_AXIS_MOVE_SWAP, SWITCH_TARGET_CURSOR, SWITCH_TARGET_FOCUSED,
    Settings, WORKSPACE_MODE_GLOBAL_UNIQUE, WORKSPACE_MODE_PER_OUTPUT_LOCAL, WORKSPACE_MODE_SHARED,
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
    // Eighteen buffers in control order: same-axis pair, born-maximized
    // pair, fixed-size pair, migration-refill pair, the admission D2
    // triple, then the admission D3 pair, the stage 2c switch-target pair,
    // and the stage 2c fresh-placement triple, so each tooltip tool reads
    // its own choice text.
    let tips = choice_tip_texts();
    assert_eq!(tips.len(), 18);
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
    assert_eq!(tips[8], anchor_fallback_tip_texts()[0]);
    assert_eq!(tips[9], anchor_fallback_tip_texts()[1]);
    assert_eq!(tips[10], anchor_fallback_tip_texts()[2]);
    assert_eq!(tips[11], no_space_tip_texts()[0]);
    assert_eq!(tips[12], no_space_tip_texts()[1]);
    assert_eq!(tips[13], switch_target_tip_texts()[0]);
    assert_eq!(tips[14], switch_target_tip_texts()[1]);
    assert_eq!(tips[13], wide("COSMIC"));
    assert_eq!(tips[14], wide("i3, sway"));
    assert_eq!(tips[15], fresh_placement_tip_texts()[0]);
    assert_eq!(tips[16], fresh_placement_tip_texts()[1]);
    assert_eq!(tips[17], fresh_placement_tip_texts()[2]);
    assert_eq!(tips[15], wide("COSMIC"));
    assert_eq!(tips[16], wide("Hyprland"));
    assert_eq!(tips[17], wide("i3, sway"));
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
fn anchor_fallback_selector_round_trips_all_three_tokens() {
    let mut draft = Settings::default();
    assert_eq!(anchor_fallback_selected(&draft), 0);
    apply_anchor_fallback_selection(&mut draft, 1);
    assert_eq!(draft.core.anchor_fallback, ANCHOR_FALLBACK_CLOSEST);
    assert_eq!(anchor_fallback_selected(&draft), 1);
    apply_anchor_fallback_selection(&mut draft, 2);
    assert_eq!(draft.core.anchor_fallback, ANCHOR_FALLBACK_NO_SEARCH);
    assert_eq!(anchor_fallback_selected(&draft), 2);
    apply_anchor_fallback_selection(&mut draft, 0);
    assert_eq!(draft.core.anchor_fallback, ANCHOR_FALLBACK_LARGEST);
    assert_eq!(anchor_fallback_selected(&draft), 0);
}

#[test]
fn anchor_fallback_selector_shows_largest_for_unknown_hand_built_value() {
    let mut draft = Settings::default();
    draft.core.anchor_fallback = "retired-alias".to_owned();
    assert_eq!(anchor_fallback_selected(&draft), 0);
}

#[test]
fn anchor_fallback_out_of_range_index_stages_the_default() {
    let mut draft = Settings::default();
    apply_anchor_fallback_selection(&mut draft, 99);
    assert_eq!(draft.core.anchor_fallback, ANCHOR_FALLBACK_LARGEST);
    assert!(crate::settings::validate_settings(&draft).is_ok());
}

#[test]
fn anchor_fallback_selection_validates_after_apply() {
    for index in [0, 1, 2] {
        let mut draft = Settings::default();
        apply_anchor_fallback_selection(&mut draft, index);
        crate::settings::validate_settings(&draft).expect("selector stages a valid token");
    }
}

#[test]
fn anchor_fallback_tip_texts_come_from_shared_options() {
    let options = crate::settings::anchor_fallback_options();
    let tips = anchor_fallback_tip_texts();
    assert_eq!(tips.len(), 3);
    for (buffer, option) in tips.iter().zip(options.iter()) {
        assert_eq!(*buffer, wide(option.2));
    }
    assert_eq!(tips[0], wide("Most remaining area first"));
    assert_eq!(tips[1], wide("Nearest branch first"));
    assert_eq!(tips[2], wide("Go directly to the no-space setting"));
}

#[test]
fn no_space_radio_round_trips_both_tokens() {
    let mut draft = Settings::default();
    assert!(!no_space_overlap_selected(&draft));
    apply_no_space_selection(&mut draft, true);
    assert_eq!(draft.core.no_space_result, NO_SPACE_OVERLAP);
    assert!(no_space_overlap_selected(&draft));
    apply_no_space_selection(&mut draft, false);
    assert_eq!(draft.core.no_space_result, NO_SPACE_FLOAT);
    assert!(!no_space_overlap_selected(&draft));
}

#[test]
fn no_space_radio_shows_float_for_unknown_hand_built_value() {
    let mut draft = Settings::default();
    draft.core.no_space_result = "retired-alias".to_owned();
    assert!(!no_space_overlap_selected(&draft));
}

#[test]
fn no_space_selection_validates_after_apply() {
    for overlap in [false, true] {
        let mut draft = Settings::default();
        apply_no_space_selection(&mut draft, overlap);
        crate::settings::validate_settings(&draft).expect("radio stages a valid token");
    }
}

#[test]
fn no_space_tip_texts_come_from_shared_options() {
    let options = crate::settings::no_space_options();
    let tips = no_space_tip_texts();
    assert_eq!(tips.len(), 2);
    for (buffer, option) in tips.iter().zip(options.iter()) {
        assert_eq!(*buffer, wide(option.2));
    }
    assert_eq!(tips[0], wide("Keep tiling intact on a separate layer"));
    assert_eq!(
        tips[1],
        wide("Keep tiled membership with overflowing geometry")
    );
}

#[test]
fn admission_options_carry_functional_labels() {
    assert_eq!(
        crate::settings::anchor_fallback_options(),
        [
            (
                ANCHOR_FALLBACK_LARGEST,
                "Largest splittable window",
                "Most remaining area first"
            ),
            (
                ANCHOR_FALLBACK_CLOSEST,
                "Closest splittable relative",
                "Nearest branch first"
            ),
            (
                ANCHOR_FALLBACK_NO_SEARCH,
                "No search",
                "Go directly to the no-space setting"
            ),
        ]
    );
    assert_eq!(
        crate::settings::no_space_options(),
        [
            (
                NO_SPACE_FLOAT,
                "Float",
                "Keep tiling intact on a separate layer"
            ),
            (
                NO_SPACE_OVERLAP,
                "Allow overlap",
                "Keep tiled membership with overflowing geometry"
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
fn fixed_tracks_ready_needs_init_and_pointer() {
    // The production `WM_GETMINMAXINFO` readiness gate: advertise only
    // with a non-null info pointer once `WM_CREATE` stored user data.
    // Zero user data (pre-creation query) or a null pointer falls back
    // to `DefWindowProcW` in the handler.
    assert!(super::fixed_tracks_ready(1, 0x1000));
    assert!(super::fixed_tracks_ready(-1, 0x1000));
    assert!(!super::fixed_tracks_ready(0, 0x1000));
    assert!(!super::fixed_tracks_ready(1, 0));
    assert!(!super::fixed_tracks_ready(0, 0));
}

#[test]
fn fixed_minmax_tracks_pins_live_outer_on_both_axes() {
    // The production `WM_GETMINMAXINFO` answer shape: a usable live outer
    // size advertises equal min/max tracks, which the shared classifier
    // reads as fixed under both predicates once converted with the
    // measured frame insets (same subtraction both sides preserves
    // equality while the remainder stays usable).
    use tiler_core::size_hints::{FixedSizePredicate, is_fixed_size_with};
    let insets = crate::tiling::FrameInsets {
        left: 8,
        top: 31,
        right: 8,
        bottom: 8,
    };
    for (outer_w, outer_h) in [(940, 1120), (1848, 2224), (200, 100)] {
        let (w, h) = super::fixed_minmax_tracks(outer_w, outer_h).expect("usable outer pins");
        assert_eq!((w, h), (outer_w, outer_h));
        let hints = crate::tiling::hints_from_outer(Some((w, h)), Some((w, h)), insets);
        assert!(
            is_fixed_size_with(hints, FixedSizePredicate::BothAxes),
            "fixed under both-axes at {outer_w}x{outer_h}"
        );
        assert!(
            is_fixed_size_with(hints, FixedSizePredicate::EitherAxis),
            "fixed under either-axis at {outer_w}x{outer_h}"
        );
    }
}

#[test]
fn fixed_minmax_tracks_falls_back_on_unusable_outer() {
    // Non-positive or out-of-bound outers carry no fixed claim: the
    // production handler falls back to `DefWindowProcW` for these.
    use tiler_core::bounds::GEOMETRY_BOUND;
    for (outer_w, outer_h) in [
        (0, 0),
        (0, 600),
        (800, 0),
        (-32000, -32000),
        (-8, 600),
        (GEOMETRY_BOUND + 1, 600),
        (800, GEOMETRY_BOUND + 1),
        (i32::MAX, i32::MAX),
    ] {
        assert_eq!(
            super::fixed_minmax_tracks(outer_w, outer_h),
            None,
            "no fixed claim at {outer_w}x{outer_h}"
        );
    }
    // Boundary itself still pins.
    assert_eq!(
        super::fixed_minmax_tracks(GEOMETRY_BOUND, GEOMETRY_BOUND),
        Some((GEOMETRY_BOUND, GEOMETRY_BOUND))
    );
    // A fallback-shaped conversion (unknown max side) never classifies,
    // under either predicate.
    use tiler_core::size_hints::{FixedSizePredicate, is_fixed_size_with};
    let insets = crate::tiling::FrameInsets {
        left: 8,
        top: 31,
        right: 8,
        bottom: 8,
    };
    let hints = crate::tiling::hints_from_outer(Some((800, 600)), None, insets);
    assert!(!is_fixed_size_with(hints, FixedSizePredicate::BothAxes));
    assert!(!is_fixed_size_with(hints, FixedSizePredicate::EitherAxis));
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
