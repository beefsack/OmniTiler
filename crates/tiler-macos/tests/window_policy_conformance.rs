#![forbid(unsafe_code)]
//! Two-sided conformance: actual `tiler_windows::tiling` predicates against
//! `tiler_core::window_policy`.
//!
//! Portable (runs on Linux/macOS/Windows CI without live windows): the
//! Windows crate's portable policy compiles everywhere, so this test calls
//! the real Windows functions — never a transcription — alongside the core
//! API over identical inputs, plus the shared JSON fixture. The private
//! `cfg(windows)` `tiling_sys::admission_clear_eligible` is never invoked;
//! its boolean formula is proven through the public `classify` (overlay-free
//! facts) and `classify_focus`, and that qualification is explicit wherever
//! the evidence is cited. Boolean eligibility only: Windows `classify` orders
//! `no_activate` after the overlays, which the core does not model.

use tiler_core::born_maximized::BornMaximized;
use tiler_core::size_hints::{FixedSizePredicate, WindowSizeHints};
use tiler_core::window_policy as core;
use tiler_windows::tiling as win;
use tiler_windows::tiling::SkipReason;

/// Map normalized core facts to overlay-free Windows facts for the safety
/// formula comparison.
fn overlay_free_facts(facts: &core::SafetyFacts) -> win::WindowFacts {
    win::WindowFacts {
        visible: facts.visible,
        minimized: facts.minimized,
        maximized: false,
        cloaked: facts.cloaked,
        elevated: facts.privileged,
        shell: facts.shell,
        tool_window: facts.tool,
        owned: facts.owned_dialog,
        captionless_fullscreen: false,
        no_activate: facts.no_activate,
        dialog: facts.dialog,
    }
}

#[test]
fn overlay_refusal_matches_actual_windows_on_all_combos() {
    for fullscreen in [false, true] {
        for maximized in [false, true] {
            assert_eq!(
                core::overlay_refusal(fullscreen, maximized),
                win::overlay_refusal(fullscreen, maximized),
                "fs={fullscreen} max={maximized}"
            );
        }
    }
}

#[test]
fn toggle_refusals_match_actual_windows_on_all_combos() {
    for fullscreen in [false, true] {
        for maximized in [false, true] {
            assert_eq!(
                core::float_toggle_refusal(fullscreen, maximized),
                win::float_toggle_refusal(fullscreen, maximized),
                "float fs={fullscreen} max={maximized}"
            );
            assert_eq!(
                core::sticky_toggle_refusal(fullscreen, maximized),
                win::sticky_toggle_refusal(fullscreen, maximized),
                "sticky fs={fullscreen} max={maximized}"
            );
            assert_eq!(
                core::orientation_toggle_refusal(fullscreen, maximized),
                win::orientation_toggle_refusal(fullscreen, maximized),
                "orientation fs={fullscreen} max={maximized}"
            );
        }
    }
}

#[test]
fn b9_lane_matches_actual_windows_on_all_combos() {
    for fullscreen in [false, true] {
        for maximized in [false, true] {
            for is_float in [false, true] {
                for is_sticky_origin in [false, true] {
                    assert_eq!(
                        core::unfloat_maximize_clear_eligible(
                            fullscreen,
                            maximized,
                            is_float,
                            is_sticky_origin
                        ),
                        win::unfloat_maximize_clear_eligible(
                            fullscreen,
                            maximized,
                            is_float,
                            is_sticky_origin
                        ),
                        "fs={fullscreen} max={maximized} float={is_float} sticky={is_sticky_origin}"
                    );
                }
            }
        }
    }
}

#[test]
fn sticky_directional_and_focus_fence_match_actual_windows() {
    for is_sticky in [false, true] {
        for is_move in [false, true] {
            assert_eq!(
                core::sticky_directional_refusal(is_sticky, is_move),
                win::sticky_directional_refusal(is_sticky, is_move),
                "sticky={is_sticky} move={is_move}"
            );
        }
    }
    for fullscreen in [false, true] {
        for maximized in [false, true] {
            assert_eq!(
                core::maximized_focus_fence(maximized, fullscreen),
                win::maximized_focus_fence(maximized, fullscreen),
                "max={maximized} fs={fullscreen}"
            );
        }
    }
}

#[test]
fn born_hold_matches_actual_windows_on_all_combos() {
    for fullscreen in [false, true] {
        for known_slot in [false, true] {
            for seen_nonfullscreen in [false, true] {
                assert_eq!(
                    core::should_hold_born_fullscreen(fullscreen, known_slot, seen_nonfullscreen),
                    win::should_hold_born_fullscreen(fullscreen, known_slot, seen_nonfullscreen),
                    "fs={fullscreen} slot={known_slot} seen={seen_nonfullscreen}"
                );
            }
        }
    }
}

#[test]
fn admission_clear_matches_actual_windows_on_all_combos() {
    for fullscreen in [false, true] {
        for maximized in [false, true] {
            for known_slot in [false, true] {
                for attempted in [false, true] {
                    for tiled in [false, true] {
                        assert_eq!(
                            core::should_clear_maximize_at_admission(
                                fullscreen, maximized, known_slot, attempted, tiled
                            ),
                            win::should_clear_maximize_at_admission(
                                fullscreen, maximized, known_slot, attempted, tiled
                            ),
                            "base fs={fullscreen} max={maximized} slot={known_slot} attempted={attempted} tiled={tiled}"
                        );
                        for keep_overlay in [false, true] {
                            assert_eq!(
                                core::should_clear_maximize_at_admission_with_mode(
                                    fullscreen,
                                    maximized,
                                    known_slot,
                                    attempted,
                                    tiled,
                                    keep_overlay
                                ),
                                win::should_clear_maximize_at_admission_with_mode(
                                    fullscreen,
                                    maximized,
                                    known_slot,
                                    attempted,
                                    tiled,
                                    keep_overlay
                                ),
                                "mode fs={fullscreen} max={maximized} slot={known_slot} attempted={attempted} tiled={tiled} keep={keep_overlay}"
                            );
                        }
                        // Core born-mode binding equals the Windows keep flag.
                        assert_eq!(
                            core::should_clear_maximize_at_admission_with_born_mode(
                                fullscreen,
                                maximized,
                                known_slot,
                                attempted,
                                tiled,
                                BornMaximized::TileOnLaunch
                            ),
                            win::should_clear_maximize_at_admission_with_mode(
                                fullscreen, maximized, known_slot, attempted, tiled, false
                            )
                        );
                        assert_eq!(
                            core::should_clear_maximize_at_admission_with_born_mode(
                                fullscreen,
                                maximized,
                                known_slot,
                                attempted,
                                tiled,
                                BornMaximized::KeepMaximizedOverlay
                            ),
                            win::should_clear_maximize_at_admission_with_mode(
                                fullscreen, maximized, known_slot, attempted, tiled, true
                            )
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn slot_and_latch_gates_match_actual_windows() {
    for maximized in [false, true] {
        for fullscreen in [false, true] {
            for is_member in [false, true] {
                for has_slot in [false, true] {
                    assert_eq!(
                        core::should_admit_slotless_maximized(
                            maximized, fullscreen, is_member, has_slot
                        ),
                        win::should_admit_slotless_maximized(
                            maximized, fullscreen, is_member, has_slot
                        ),
                        "max={maximized} fs={fullscreen} member={is_member} slot={has_slot}"
                    );
                }
            }
        }
    }
    for domain_tiled in [false, true] {
        for maximized in [false, true] {
            for has_slot in [false, true] {
                assert_eq!(
                    core::should_seed_member_slot(domain_tiled, maximized, has_slot),
                    win::should_seed_member_slot(domain_tiled, maximized, has_slot),
                    "tiled={domain_tiled} max={maximized} slot={has_slot}"
                );
            }
        }
    }
    for tiled_first in [false, true] {
        for keep_setting in [false, true] {
            assert_eq!(
                core::born_keep_latch(tiled_first, keep_setting),
                win::born_keep_latch(tiled_first, keep_setting),
                "first={tiled_first} keep={keep_setting}"
            );
        }
    }
}

#[test]
fn safety_formula_matches_actual_classify_on_all_512_combos() {
    // The private cfg(windows) admission-clear function is never invoked:
    // this proves its boolean formula through the public classify
    // (overlay-free) and classify_focus, which share it.
    for bits in 0..512u32 {
        let facts = core::SafetyFacts {
            visible: bits & 1 != 0,
            minimized: bits & 2 != 0,
            cloaked: bits & 4 != 0,
            privileged: bits & 8 != 0,
            shell: bits & 16 != 0,
            tool: bits & 32 != 0,
            owned_dialog: bits & 64 != 0,
            dialog: bits & 128 != 0,
            no_activate: bits & 256 != 0,
        };
        let win_facts = overlay_free_facts(&facts);
        let focus_ok = win::classify_focus(&win_facts).is_ok();
        let classify_ok = win::classify(&win_facts).is_ok();
        assert_eq!(focus_ok, classify_ok, "bits={bits:09b}");
        assert_eq!(core::safety_eligible(&facts), focus_ok, "bits={bits:09b}");
        assert_eq!(
            core::admission_clear_eligible(&facts),
            focus_ok,
            "bits={bits:09b}"
        );
        assert_eq!(
            core::focus_actuation_eligible(&facts),
            focus_ok,
            "bits={bits:09b}"
        );
        assert_eq!(
            core::geometry_eligible(&facts, false, false),
            classify_ok,
            "bits={bits:09b}"
        );
    }
}

#[test]
fn classify_overlay_order_matches_windows_token_precedence() {
    // Actual Windows ordering evidence: overlays refuse geometry, fullscreen
    // wins when both hold, and no_activate orders after overlays.
    let plain = overlay_free_facts(&core::SafetyFacts {
        visible: true,
        ..core::SafetyFacts::default()
    });
    assert!(win::classify(&plain).is_ok());
    assert!(win::classify_focus(&plain).is_ok());

    let maximized = win::WindowFacts {
        maximized: true,
        ..plain
    };
    assert_eq!(win::classify(&maximized), Err(SkipReason::Maximized));
    assert!(win::classify_focus(&maximized).is_ok());

    let fullscreen = win::WindowFacts {
        captionless_fullscreen: true,
        ..plain
    };
    assert_eq!(win::classify(&fullscreen), Err(SkipReason::Fullscreen));
    assert!(win::classify_focus(&fullscreen).is_ok());

    let both = win::WindowFacts {
        maximized: true,
        captionless_fullscreen: true,
        ..plain
    };
    assert_eq!(win::classify(&both), Err(SkipReason::Fullscreen));

    // no_activate orders after overlays in Windows classify: a no-activate
    // maximized window reports Maximized, not NoActivate. The core asserts
    // boolean eligibility only and makes no token-order claim.
    let no_activate_maximized = win::WindowFacts {
        maximized: true,
        no_activate: true,
        ..plain
    };
    assert_eq!(
        win::classify(&no_activate_maximized),
        Err(SkipReason::Maximized)
    );
    let no_activate_plain = win::WindowFacts {
        no_activate: true,
        ..plain
    };
    assert_eq!(
        win::classify(&no_activate_plain),
        Err(SkipReason::NoActivate)
    );
    assert_eq!(
        win::classify_focus(&no_activate_plain),
        Err(SkipReason::NoActivate)
    );
}

#[test]
fn shared_fixture_matches_both_sides() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../test-fixtures/window-policy.json"))
            .expect("shared window-policy fixture parses");
    let cases = fixture
        .get("cases")
        .and_then(serde_json::Value::as_array)
        .expect("fixture carries a case table");
    assert!(!cases.is_empty(), "fixture case table is non-empty");
    for case in cases {
        let name = case
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<unnamed>");
        let gate = case
            .get("gate")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<missing gate>");
        let bit = |key: &str| {
            case.get(key)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or_else(|| panic!("{name} carries `{key}`"))
        };
        let refusal = |key: &str| case.get(key).and_then(serde_json::Value::as_str);
        match gate {
            "overlay_refusal" => {
                let expected = refusal("refusal");
                assert_eq!(
                    core::overlay_refusal(bit("fullscreen"), bit("maximized")),
                    expected,
                    "{name} core"
                );
                assert_eq!(
                    win::overlay_refusal(bit("fullscreen"), bit("maximized")),
                    expected,
                    "{name} windows"
                );
            }
            "float_toggle_refusal" => {
                let expected = refusal("refusal");
                assert_eq!(
                    core::float_toggle_refusal(bit("fullscreen"), bit("maximized")),
                    expected,
                    "{name} core"
                );
                assert_eq!(
                    win::float_toggle_refusal(bit("fullscreen"), bit("maximized")),
                    expected,
                    "{name} windows"
                );
            }
            "sticky_toggle_refusal" => {
                let expected = refusal("refusal");
                assert_eq!(
                    core::sticky_toggle_refusal(bit("fullscreen"), bit("maximized")),
                    expected,
                    "{name} core"
                );
                assert_eq!(
                    win::sticky_toggle_refusal(bit("fullscreen"), bit("maximized")),
                    expected,
                    "{name} windows"
                );
            }
            "orientation_toggle_refusal" => {
                let expected = refusal("refusal");
                assert_eq!(
                    core::orientation_toggle_refusal(bit("fullscreen"), bit("maximized")),
                    expected,
                    "{name} core"
                );
                assert_eq!(
                    win::orientation_toggle_refusal(bit("fullscreen"), bit("maximized")),
                    expected,
                    "{name} windows"
                );
            }
            "unfloat_lane" => {
                let expected = bit("eligible");
                assert_eq!(
                    core::unfloat_maximize_clear_eligible(
                        bit("fullscreen"),
                        bit("maximized"),
                        bit("is_float"),
                        bit("is_sticky_origin")
                    ),
                    expected,
                    "{name} core"
                );
                assert_eq!(
                    win::unfloat_maximize_clear_eligible(
                        bit("fullscreen"),
                        bit("maximized"),
                        bit("is_float"),
                        bit("is_sticky_origin")
                    ),
                    expected,
                    "{name} windows"
                );
            }
            "focus_fence" => {
                let expected = bit("fenced");
                assert_eq!(
                    core::maximized_focus_fence(bit("maximized"), bit("fullscreen")),
                    expected,
                    "{name} core"
                );
                assert_eq!(
                    win::maximized_focus_fence(bit("maximized"), bit("fullscreen")),
                    expected,
                    "{name} windows"
                );
            }
            "born_hold" => {
                let expected = bit("hold");
                assert_eq!(
                    core::should_hold_born_fullscreen(
                        bit("fullscreen"),
                        bit("known_slot"),
                        bit("seen_nonfullscreen")
                    ),
                    expected,
                    "{name} core"
                );
                assert_eq!(
                    win::should_hold_born_fullscreen(
                        bit("fullscreen"),
                        bit("known_slot"),
                        bit("seen_nonfullscreen")
                    ),
                    expected,
                    "{name} windows"
                );
            }
            "admission_clear" => {
                let expected = bit("clear");
                assert_eq!(
                    core::should_clear_maximize_at_admission_with_mode(
                        bit("fullscreen"),
                        bit("maximized"),
                        bit("known_slot"),
                        bit("attempted"),
                        bit("tiled"),
                        bit("keep_overlay")
                    ),
                    expected,
                    "{name} core"
                );
                assert_eq!(
                    win::should_clear_maximize_at_admission_with_mode(
                        bit("fullscreen"),
                        bit("maximized"),
                        bit("known_slot"),
                        bit("attempted"),
                        bit("tiled"),
                        bit("keep_overlay")
                    ),
                    expected,
                    "{name} windows"
                );
            }
            "slotless" => {
                let expected = bit("slotless");
                assert_eq!(
                    core::should_admit_slotless_maximized(
                        bit("maximized"),
                        bit("fullscreen"),
                        bit("is_member"),
                        bit("has_slot")
                    ),
                    expected,
                    "{name} core"
                );
                assert_eq!(
                    win::should_admit_slotless_maximized(
                        bit("maximized"),
                        bit("fullscreen"),
                        bit("is_member"),
                        bit("has_slot")
                    ),
                    expected,
                    "{name} windows"
                );
            }
            "born_latch" => {
                let expected = bit("latch");
                assert_eq!(
                    core::born_keep_latch(bit("tiled_first"), bit("keep_setting")),
                    expected,
                    "{name} core"
                );
                assert_eq!(
                    win::born_keep_latch(bit("tiled_first"), bit("keep_setting")),
                    expected,
                    "{name} windows"
                );
            }
            "safety" => {
                let facts = core::SafetyFacts {
                    visible: bit("visible"),
                    minimized: bit("minimized"),
                    cloaked: bit("cloaked"),
                    privileged: bit("privileged"),
                    shell: bit("shell"),
                    tool: bit("tool"),
                    owned_dialog: bit("owned_dialog"),
                    dialog: bit("dialog"),
                    no_activate: bit("no_activate"),
                };
                let expected = bit("eligible");
                assert_eq!(core::safety_eligible(&facts), expected, "{name} core");
                let win_facts = overlay_free_facts(&facts);
                assert_eq!(
                    win::classify_focus(&win_facts).is_ok(),
                    expected,
                    "{name} windows focus"
                );
                assert_eq!(
                    win::classify(&win_facts).is_ok(),
                    expected,
                    "{name} windows classify"
                );
                if let Some(geometry) = case
                    .get("geometry_when_plain")
                    .and_then(serde_json::Value::as_bool)
                {
                    assert_eq!(
                        core::geometry_eligible(&facts, false, false),
                        geometry,
                        "{name} core"
                    );
                    assert_eq!(
                        win::classify(&win_facts).is_ok(),
                        geometry,
                        "{name} windows"
                    );
                }
                if let Some(geometry) = case
                    .get("geometry_when_maximized")
                    .and_then(serde_json::Value::as_bool)
                {
                    assert_eq!(
                        core::geometry_eligible(&facts, false, true),
                        geometry,
                        "{name} core"
                    );
                    let maximized = win::WindowFacts {
                        maximized: true,
                        ..win_facts
                    };
                    assert_eq!(
                        win::classify(&maximized).is_ok(),
                        geometry,
                        "{name} windows"
                    );
                }
                if let Some(focus) = case
                    .get("focus_when_maximized")
                    .and_then(serde_json::Value::as_bool)
                {
                    assert_eq!(core::focus_actuation_eligible(&facts), focus, "{name} core");
                    let maximized = win::WindowFacts {
                        maximized: true,
                        ..win_facts
                    };
                    assert_eq!(
                        win::classify_focus(&maximized).is_ok(),
                        focus,
                        "{name} windows"
                    );
                }
            }
            "first_exit" => {
                let bound = |key: &str| match case.get(key) {
                    None | Some(serde_json::Value::Null) => None,
                    Some(serde_json::Value::Number(n)) => {
                        n.as_i64().and_then(|v| i32::try_from(v).ok())
                    }
                    _ => panic!("{name} carries numeric-or-null `{key}`"),
                };
                let hints = WindowSizeHints {
                    min_w: bound("min_w"),
                    min_h: bound("min_h"),
                    max_w: bound("max_w"),
                    max_h: bound("max_h"),
                };
                let predicate = match case.get("predicate").and_then(serde_json::Value::as_str) {
                    Some("either-axis-fixed") => FixedSizePredicate::EitherAxis,
                    _ => FixedSizePredicate::BothAxes,
                };
                let expected = case
                    .get("disposition")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| panic!("{name} carries `disposition`"));
                let disposition = core::classify_born_fullscreen_exit(hints, predicate);
                assert_eq!(
                    match disposition {
                        core::FirstExitDisposition::FloatAutomatic => "float-automatic",
                        core::FirstExitDisposition::Tile => "tile",
                    },
                    expected,
                    "{name} core"
                );
                // Windows classifies the same exit through the shared
                // predicate: assert the identical boolean on the real path.
                assert_eq!(
                    tiler_core::size_hints::is_fixed_size_with(hints, predicate),
                    disposition == core::FirstExitDisposition::FloatAutomatic,
                    "{name} shared predicate"
                );
            }
            other => panic!("{name} names an unknown gate `{other}`"),
        }
    }
}
