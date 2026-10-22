//! Shared frame-convergence fixture runners (stage 3b): the SAME
//! `test-fixtures/frame-convergence.json` sections the Windows production
//! sequencer consumes, driven here through raw core reports with the exact
//! seam capture facts (flagged report at ms 2000 carrying the pre-flag raw
//! attempt from ms 1000, recovery at ms 3000). Helpers come from the parent
//! frame-policy module; no public API, no test features.

use super::world_frame_policy_tests::{
    accepted, attempt, fact, frame_row, learned_of, minimized_fact, report_with_screens, screen,
    two_facts,
};
use crate::geometry::Rect;
use crate::world_engine::WorldEngine;
use crate::world_report::{FrameSetter, HoldReason, ScopeAuthority, ScreenReport, WindowFact};

fn screen_incomplete() -> ScreenReport {
    ScreenReport {
        complete: false,
        ..screen()
    }
}

fn unreadable_fact(window: &str) -> WindowFact {
    fact(window, None)
}

/// Tabled `expect_learned` axis array as genuine (width, height) proof:
/// only the portable floor axes may appear, so a typo fails loudly
/// instead of asserting nothing.
fn expect_learned_axes(case: &serde_json::Value) -> (bool, bool) {
    let axes = case["expect_learned"].as_array().expect("expect_learned");
    for axis in axes {
        let axis = axis.as_str().expect("axis string");
        assert!(
            axis == "min_w" || axis == "min_h",
            "unknown learned axis {axis}"
        );
    }
    let has = |axis: &str| axes.iter().any(|v| v.as_str() == Some(axis));
    (has("min_w"), has("min_h"))
}

fn expect_authority(name: &str, value: &serde_json::Value) -> ScopeAuthority {
    match value.as_str().unwrap_or("controlled") {
        "controlled" => ScopeAuthority::Controlled,
        "hold-clamp" => ScopeAuthority::Hold(HoldReason::ConstraintClamp),
        "hold-unreadable" => ScopeAuthority::Hold(HoldReason::Unreadable),
        "hold-minimized" => ScopeAuthority::Hold(HoldReason::Minimized),
        "hold-incomplete" => ScopeAuthority::Hold(HoldReason::IncompleteInventory),
        other => panic!("{name}: unknown expect_authority {other}"),
    }
}

#[test]
fn learned_shared_fixture_converges_through_raw_reports() {
    let raw = include_str!("../../../test-fixtures/frame-convergence.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).expect("valid fixture");
    assert_eq!(fixture["version"], serde_json::Value::from(1));
    let cases = fixture["cases"].as_array().expect("cases array");
    assert_eq!(cases.len(), 5);
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let window = case["window"].as_str().expect("window");
        let extra_w = case["extra_w"].as_i64().expect("extra_w") as i32;
        let extra_h = case["extra_h"].as_i64().expect("extra_h") as i32;
        let setter = match case["setter"].as_str().unwrap_or("accepted") {
            "accepted" => FrameSetter::Accepted,
            "refused" => FrameSetter::Refused,
            "threw" => FrameSetter::Threw,
            other => panic!("{name}: unknown setter {other}"),
        };
        let expect_retry = case["expect_retry"].as_bool().unwrap_or(false);
        let expect_reapply = case["expect_reapply"].as_bool().unwrap_or(false);
        let (exp_w, exp_h) = expect_learned_axes(case);
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let first = accepted(
            &mut engine,
            &report_with_screens(1, vec![screen()], two_facts(), vec![], vec![], 0, true),
        );
        let baseline = frame_row(&first, window).effective_frame.expect("tiled");
        let other = if window == "w1" { "w2" } else { "w1" };
        let other_base = frame_row(&first, other).effective_frame.expect("tiled");
        let grown = Rect {
            x: baseline.x,
            y: baseline.y,
            w: baseline.w + extra_w,
            h: baseline.h + extra_h,
        };
        let mut wfact = fact(window, Some(grown));
        if case["declared_max"].as_str() == Some("baseline-w") {
            wfact.hints = crate::size_hints::WindowSizeHints {
                min_w: None,
                min_h: None,
                max_w: Some(baseline.w),
                max_h: None,
            };
        }
        let second = accepted(
            &mut engine,
            &report_with_screens(
                2,
                vec![screen()],
                vec![wfact, fact(other, Some(other_base))],
                vec![],
                vec![attempt(window, baseline, setter, Some(grown), 1000)],
                1000,
                true,
            ),
        );
        let expect_grows = case["expect_grows"].as_bool().expect("expect_grows");
        let next = frame_row(&second, window).effective_frame.expect("tiled");
        if expect_grows {
            assert!(
                next.w >= grown.w && next.h == grown.h,
                "{name}: learned target {next:?} covers {grown:?}"
            );
            assert_eq!(
                learned_of(&engine, window),
                (exp_w.then_some(grown.w), exp_h.then_some(grown.h)),
                "{name}: tabled learned axes prove exactly"
            );
            assert_eq!(
                frame_row(&second, window).frame_authority,
                ScopeAuthority::Controlled,
                "{name}: learned target is actionable, not a hold"
            );
        } else if expect_retry {
            // Refused/throwing setters never learn: the transient lane
            // holds the stable pair inside the 500ms backoff, then the
            // adapter may retry controlled past it.
            assert_eq!(
                (exp_w, exp_h),
                (false, false),
                "{name}: table learns nothing on retry lanes"
            );
            assert!(
                engine.learned_mins.is_empty(),
                "{name}: refusal learns nothing"
            );
            assert_eq!(
                frame_row(&second, window).frame_authority,
                ScopeAuthority::Hold(HoldReason::ConstraintClamp),
                "{name}: transient holds"
            );
            let third = accepted(
                &mut engine,
                &report_with_screens(
                    3,
                    vec![screen()],
                    vec![fact(window, Some(grown)), fact(other, Some(other_base))],
                    vec![],
                    vec![],
                    1200,
                    true,
                ),
            );
            assert_eq!(
                frame_row(&third, window).frame_authority,
                ScopeAuthority::Hold(HoldReason::ConstraintClamp),
                "{name}: inside backoff still holds"
            );
            assert!(
                engine.learned_mins.is_empty(),
                "{name}: still learns nothing"
            );
            let fourth = accepted(
                &mut engine,
                &report_with_screens(
                    4,
                    vec![screen()],
                    vec![fact(window, Some(grown)), fact(other, Some(other_base))],
                    vec![],
                    vec![],
                    2000,
                    true,
                ),
            );
            assert_eq!(
                frame_row(&fourth, window).frame_authority,
                ScopeAuthority::Controlled,
                "{name}: past backoff retries controlled"
            );
            assert!(
                engine.learned_mins.is_empty(),
                "{name}: retry learns nothing"
            );
        } else {
            assert_eq!(
                (exp_w, exp_h),
                (false, false),
                "{name}: table learns nothing past the contradiction"
            );
            assert!(
                engine.learned_mins.is_empty(),
                "{name}: contradiction learns nothing"
            );
            assert_eq!(next, baseline, "{name}: target unchanged");
            assert_eq!(
                frame_row(&second, window).frame_authority,
                ScopeAuthority::Hold(HoldReason::ConstraintClamp),
                "{name}: stable pair still suppresses"
            );
        }
        if expect_reapply {
            // Exact re-observation of the learned target converges: the
            // suppression clears but the persistent floor survives for the
            // next layout.
            assert!(expect_grows, "{name}: reapply needs a learned target");
            let other_next = frame_row(&second, other).effective_frame.expect("tiled");
            let third = accepted(
                &mut engine,
                &report_with_screens(
                    3,
                    vec![screen()],
                    vec![fact(window, Some(next)), fact(other, Some(other_next))],
                    vec![],
                    vec![],
                    2000,
                    true,
                ),
            );
            assert_eq!(
                frame_row(&third, window).frame_authority,
                ScopeAuthority::Controlled,
                "{name}: converged target actionable"
            );
            assert!(engine.frames.is_empty(), "{name}: suppression clears");
            assert_eq!(
                learned_of(&engine, window),
                (exp_w.then_some(grown.w), exp_h.then_some(grown.h)),
                "{name}: floor persists"
            );
        }
    }
}

#[test]
fn replacement_shared_fixture_reuses_retired_token_fresh() {
    // A retired token reused as a fresh native lifetime inherits nothing:
    // the close prunes the learned floor and suppression, and the reused
    // token re-admits at the un-floored desire, controlled.
    let raw = include_str!("../../../test-fixtures/frame-convergence.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).expect("valid fixture");
    let cases = fixture["replacement_cases"]
        .as_array()
        .expect("replacement_cases array");
    assert_eq!(cases.len(), 1);
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let window = case["window"].as_str().expect("window");
        let extra_w = case["extra_w"].as_i64().expect("extra_w") as i32;
        let extra_h = case["extra_h"].as_i64().expect("extra_h") as i32;
        let other = if window == "w1" { "w2" } else { "w1" };
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let first = accepted(
            &mut engine,
            &report_with_screens(1, vec![screen()], two_facts(), vec![], vec![], 0, true),
        );
        let baseline = frame_row(&first, window).effective_frame.expect("tiled");
        let other_base = frame_row(&first, other).effective_frame.expect("tiled");
        let grown = Rect {
            x: baseline.x,
            y: baseline.y,
            w: baseline.w + extra_w,
            h: baseline.h + extra_h,
        };
        accepted(
            &mut engine,
            &report_with_screens(
                2,
                vec![screen()],
                vec![fact(window, Some(grown)), fact(other, Some(other_base))],
                vec![],
                vec![attempt(
                    window,
                    baseline,
                    FrameSetter::Accepted,
                    Some(grown),
                    1000,
                )],
                1000,
                true,
            ),
        );
        assert_eq!(learned_of(&engine, window).0, Some(grown.w));
        // Retire the token: the explicit close prunes its floor and pair.
        accepted(
            &mut engine,
            &report_with_screens(
                3,
                vec![screen()],
                vec![fact(other, Some(other_base))],
                vec![window],
                vec![],
                2000,
                true,
            ),
        );
        assert!(engine.learned_mins.is_empty(), "{name}: close prunes floor");
        assert!(engine.frames.is_empty(), "{name}: close prunes pair");
        // Reuse the token as a fresh lifetime at the same observed rect:
        // the desire is the un-floored baseline, controlled, with nothing
        // retained and no suppression resurrected.
        let readmitted = accepted(
            &mut engine,
            &report_with_screens(
                4,
                vec![screen()],
                vec![fact(window, Some(grown)), fact(other, Some(other_base))],
                vec![],
                vec![],
                3000,
                true,
            ),
        );
        let row = frame_row(&readmitted, window);
        assert_eq!(
            row.frame_authority,
            expect_authority(name, &case["expect_authority"]),
            "{name}: reused token actionable"
        );
        assert_eq!(
            row.effective_frame.expect("tiled"),
            baseline,
            "{name}: reused token takes the un-floored desire"
        );
        assert_eq!(
            expect_learned_axes(case),
            (false, false),
            "{name}: table learns nothing on reuse"
        );
        assert!(engine.learned_mins.is_empty(), "{name}: learns nothing");
        assert!(engine.frames.is_empty(), "{name}: suppresses nothing");
        let retained = !engine.learned_mins.is_empty() || !engine.frames.is_empty();
        assert_eq!(
            case["expect_retained"].as_bool().expect("expect_retained"),
            retained,
            "{name}: reuse retains nothing"
        );
    }
}

#[test]
fn observation_shared_fixture_records_no_bounds_without_proof() {
    // Incomplete, hidden, and unreadable observations mirror the exact seam
    // capture facts: the flagged report (ms 2000) carries the pre-flag raw
    // attempt (accepted, grown readback, ms 1000) with every fact unreadable
    // on incomplete screens (incomplete), a minimized fact on a complete
    // inventory (hidden), or w1 unreadable on complete screens with an
    // incomplete inventory (unreadable). No lane learns a floor; the clamp
    // pair records exactly when tabled, and the recovery observation (ms
    // 3000, complete inventory, no attempts) converges as tabled.
    let raw = include_str!("../../../test-fixtures/frame-convergence.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).expect("valid fixture");
    let cases = fixture["observation_cases"]
        .as_array()
        .expect("observation_cases array");
    assert_eq!(cases.len(), 3);
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let window = case["window"].as_str().expect("window");
        let extra_w = case["extra_w"].as_i64().expect("extra_w") as i32;
        let extra_h = case["extra_h"].as_i64().expect("extra_h") as i32;
        let condition = case["condition"].as_str().expect("condition");
        let suppressed = case["expect_suppressed"]
            .as_bool()
            .expect("expect_suppressed");
        let other = if window == "w1" { "w2" } else { "w1" };
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let first = accepted(
            &mut engine,
            &report_with_screens(1, vec![screen()], two_facts(), vec![], vec![], 0, true),
        );
        let baseline = frame_row(&first, window).effective_frame.expect("tiled");
        let other_base = frame_row(&first, other).effective_frame.expect("tiled");
        let grown = Rect {
            x: baseline.x,
            y: baseline.y,
            w: baseline.w + extra_w,
            h: baseline.h + extra_h,
        };
        let (windows, screens, complete) = match condition {
            "incomplete" => (
                vec![unreadable_fact(window), unreadable_fact(other)],
                vec![screen_incomplete()],
                false,
            ),
            "hidden" => (
                vec![minimized_fact(window), fact(other, Some(other_base))],
                vec![screen()],
                true,
            ),
            "unreadable" => (
                vec![unreadable_fact(window), fact(other, Some(other_base))],
                vec![screen()],
                false,
            ),
            other => panic!("{name}: unknown condition {other}"),
        };
        let second = accepted(
            &mut engine,
            &report_with_screens(
                2,
                screens,
                windows,
                vec![],
                vec![attempt(
                    window,
                    baseline,
                    FrameSetter::Accepted,
                    Some(grown),
                    1000,
                )],
                2000,
                complete,
            ),
        );
        assert_eq!(
            complete,
            case["expect_complete"].as_bool().expect("expect_complete"),
            "{name}: flagged completeness as tabled"
        );
        assert_eq!(
            expect_learned_axes(case),
            (false, false),
            "{name}: table learns nothing without proof"
        );
        assert!(
            engine.learned_mins.is_empty(),
            "{name}: unproven growth learns nothing"
        );
        if suppressed {
            // The readable readback still pins the stable pair (suppression
            // without a floor); hidden evidence records nothing at all.
            assert_eq!(engine.frames.len(), 1, "{name}: stable pair recorded");
        } else {
            assert!(
                engine.frames.is_empty(),
                "{name}: unproven growth suppresses nothing"
            );
        }
        assert_eq!(
            frame_row(&second, window).frame_authority,
            expect_authority(name, &case["expect_authority"]),
            "{name}: flagged observation settles as tabled"
        );
        // Recovery: a complete readable observation converges as tabled.
        // The incomplete lane re-observes the un-floored desire exactly
        // (clearing the recorded pair and proving no floor was learned);
        // hidden and unreadable lanes re-observe the growth as a fresh
        // deviation with no new attempt behind it.
        let recover_is_baseline = match case["recover_rect"].as_str().expect("recover_rect") {
            "baseline" => true,
            "grown" => false,
            other => panic!("{name}: unknown recover_rect {other}"),
        };
        let recover_rect = if recover_is_baseline { baseline } else { grown };
        let third = accepted(
            &mut engine,
            &report_with_screens(
                3,
                vec![screen()],
                vec![
                    fact(window, Some(recover_rect)),
                    fact(other, Some(other_base)),
                ],
                vec![],
                vec![],
                3000,
                true,
            ),
        );
        assert_eq!(
            frame_row(&third, window).frame_authority,
            expect_authority(name, &case["recover_authority"]),
            "{name}: recovery converges as tabled"
        );
        match (suppressed, recover_is_baseline) {
            (true, true) => assert!(
                engine.frames.is_empty(),
                "{name}: exact recovery clears the recorded pair"
            ),
            (true, false) => assert_eq!(
                engine.frames.len(),
                1,
                "{name}: grown recovery re-holds the recorded pair"
            ),
            (false, _) => assert!(
                engine.frames.is_empty(),
                "{name}: unrecorded lane recovers clean"
            ),
        }
        assert!(
            engine.learned_mins.is_empty(),
            "{name}: recovery learns nothing without proof"
        );
    }
}
