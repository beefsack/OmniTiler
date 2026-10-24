#![forbid(unsafe_code)]
//! Windows-behavior conformance for the shared safety vocabulary.
//!
//! Portable (runs on Linux/macOS/Windows CI without live windows): the
//! Windows crate's portable classifiers compile everywhere, so this test
//! calls the real Windows functions, never a transcription. Coverage is
//! the all-512-combos agreement between `win::classify` /
//! `win::classify_focus` and the core eligibility predicates, the Windows
//! token order, and the shared JSON fixture rows that have a public
//! Windows API (`safety`, `geometry_classification`,
//! `focus_classification`).
//!
//! Gates without a public Windows API are skipped, not asserted. The
//! retired per-predicate Windows wrappers do not exist since the
//! share-policy commit, and asserting `core::` against itself would be
//! tautological, so core semantics stay owned by core's own tests.

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
fn shared_fixture_matches_windows_classify() {
    // Only gates with a public Windows API ride here (`win::classify` /
    // `win::classify_focus` against the shared fixture). Every other gate
    // is core-owned since the share-policy commit: the retired Windows
    // wrappers do not exist, so comparing `core::` to itself would be
    // tautological evidence and is not done. Core semantics stay owned by
    // core's own fixture tests.
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
        match gate {
            "overlay_refusal" => {
                // No public Windows API: core-owned since the
                // share-policy commit. Covered by core's own fixture tests.
                continue;
            }
            "float_toggle_refusal" => {
                // No public Windows API: core-owned since the
                // share-policy commit. Covered by core's own fixture tests.
                continue;
            }
            "sticky_toggle_refusal" => {
                // No public Windows API: core-owned since the
                // share-policy commit. Covered by core's own fixture tests.
                continue;
            }
            "orientation_toggle_refusal" => {
                // No public Windows API: core-owned since the
                // share-policy commit. Covered by core's own fixture tests.
                continue;
            }
            "unfloat_lane" => {
                // No public Windows API: core-owned since the
                // share-policy commit. Covered by core's own fixture tests.
                continue;
            }
            "focus_fence" => {
                // No public Windows API: core-owned since the
                // share-policy commit. Covered by core's own fixture tests.
                continue;
            }
            "born_hold" => {
                // No public Windows API: core-owned since the
                // share-policy commit. Covered by core's own fixture tests.
                continue;
            }
            "admission_clear" => {
                // No public Windows API: core-owned since the
                // share-policy commit. Covered by core's own fixture tests.
                continue;
            }
            "slotless" => {
                // No public Windows API: core-owned since the
                // share-policy commit. Covered by core's own fixture tests.
                continue;
            }
            "born_latch" => {
                // No public Windows API: core-owned since the
                // share-policy commit. Covered by core's own fixture tests.
                continue;
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
                        win::classify(&win_facts).is_ok(),
                        geometry,
                        "{name} windows"
                    );
                }
                if let Some(geometry) = case
                    .get("geometry_when_maximized")
                    .and_then(serde_json::Value::as_bool)
                {
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
            "geometry_classification" => {
                // The real Windows `classify` must agree with the shared
                // fixture row. Unlike the overlay-free helper above,
                // geometry rows carry the overlays: the Windows classifier
                // reads them from the facts. (Core-vs-fixture for this
                // gate lives in core's own fixture test, not here.)
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
                let expected = case.get("refusal").and_then(serde_json::Value::as_str);
                // Windows reports the same verdict in native vocabulary
                // (`privileged` for `elevated`).
                let neutral = |reason: win::SkipReason| match reason {
                    win::SkipReason::Elevated => "privileged",
                    other => other.as_str(),
                };
                let mut win_facts = overlay_free_facts(&facts);
                win_facts.maximized = bit("maximized");
                win_facts.captionless_fullscreen = bit("fullscreen");
                assert_eq!(
                    win::classify(&win_facts).err().map(neutral),
                    expected,
                    "{name} windows"
                );
            }
            "focus_classification" => {
                // Focus takes no overlay input: cases may carry held
                // overlays and still expect the non-overlay verdict.
                // (Core-vs-fixture for this gate lives in core's own
                // fixture test, not here.)
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
                let expected = case.get("refusal").and_then(serde_json::Value::as_str);
                assert_eq!(
                    win::classify_focus(&overlay_free_facts(&facts)).err().map(
                        |reason| match reason {
                            win::SkipReason::Elevated => "privileged",
                            other => other.as_str(),
                        }
                    ),
                    expected,
                    "{name} windows"
                );
            }
            "first_exit" => {
                // No public Windows API: core-owned since the
                // share-policy commit. Covered by core's own fixture tests.
                continue;
            }
            other => panic!("{name} names an unknown gate `{other}`"),
        }
    }
}
