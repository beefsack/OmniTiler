//! v4 staging intake regressions: legacy-versus-v4 staged equality over the
//! shared lanes, plus verbatim retention and lane order for the v4-only
//! lanes (`held_native`, `held_attempts`, `presentation_v4`,
//! `renderer_input`).

use crate::connection::{FactName, HostCapabilities};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::world_directional::tests::{accepted, fact, report_full, screen};
use crate::world_engine::WorldEngine;
use crate::world_report::{
    AttemptLaneV4, AuthorityScope, FrameSetter, HeldAttemptRow, HeldNativeRow, HeldPresentationRow,
    HeldScreenRow, HeldWindowRow, PresentationIngressV4, PresentationRowV4, RenderTarget,
    RendererInput, RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane, ScreenReport, StateAttempt,
    StateProperty, WORLD_CONTRACT_VERSION_V4, WindowFact, WorldReportV4,
};
use crate::world_state::state_property_supported;

fn report_v4(
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    closed: Vec<WindowId>,
    held_screens: Vec<HeldScreenRow>,
    held_windows: Vec<HeldWindowRow>,
    gesture_barrier: bool,
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq: None,
        command: None,
        screens,
        held_screens,
        windows,
        held_windows,
        closed,
        native: Vec::new(),
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        inventory_complete: true,
        gesture_barrier,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn diagnostic(output: &str, row: RowDiagnosticLane, id: Option<&str>) -> RowDiagnostic {
    RowDiagnostic {
        scope: AuthorityScope::Output(OutputId(output.to_owned())),
        row,
        id: id.map(str::to_owned),
        kind: RowDiagnosticKind::InvalidFact,
    }
}

fn held_screen(output: &str) -> HeldScreenRow {
    HeldScreenRow {
        output: OutputId(output.to_owned()),
        ordinal: 0,
        fields: Vec::new(),
        diagnostic: diagnostic(output, RowDiagnosticLane::Screen, Some(output)),
    }
}

fn held_window(window: &str, output: Option<&str>) -> HeldWindowRow {
    HeldWindowRow {
        window: WindowId(window.to_owned()),
        ordinal: 0,
        output: output.map(|out| OutputId(out.to_owned())),
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }
}

#[test]
fn parity_legacy_v4_shared_fields() {
    // Seed both engines with the same admitted member so the output-less
    // held row below resolves through the retained home on both intakes.
    let mut legacy = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let mut v4 = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let seed = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-r", "out-a", 0), fact("win-keep", "out-a", 40)],
        Vec::new(),
        "",
        None,
        None,
    );
    accepted(&mut legacy, &seed);
    accepted(&mut v4, &seed);

    let mut minimized = fact("win-b", "out-a", 40);
    minimized.minimized = Some(true);
    let windows = vec![fact("win-a", "out-a", 0), minimized];
    let held_screens = vec![held_screen("out-a")];
    let mut held_scoped = held_window("win-h", Some("out-a"));
    held_scoped.ordinal = 1;
    let held_windows = vec![held_scoped, held_window("win-r", None)];
    let closed = vec![WindowId("win-c".to_owned())];
    let legacy_report = {
        let mut report = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            windows.clone(),
            Vec::new(),
            "",
            None,
            None,
        );
        report.held_screens = held_screens.clone();
        report.held_windows = held_windows.clone();
        report.closed = closed.clone();
        report.gesture_barrier = true;
        report
    };
    let v4_report = report_v4(
        2,
        vec![screen("out-a", 0)],
        windows,
        closed,
        held_screens,
        held_windows,
        true,
    );
    legacy.stage_facts(&legacy_report);
    v4.stage_facts_v4(&v4_report);

    assert_eq!(legacy.staged.screens, v4.staged.screens);
    assert_eq!(legacy.staged.held_screens, v4.staged.held_screens);
    assert_eq!(legacy.staged.held_windows, v4.staged.held_windows);
    assert_eq!(legacy.staged.excluded, v4.staged.excluded);
    assert_eq!(legacy.staged.diagnostics, v4.staged.diagnostics);
    assert_eq!(legacy.staged.fact_output, v4.staged.fact_output);
    assert_eq!(legacy.staged.facts, v4.staged.facts);
    assert_eq!(legacy.staged.derived, v4.staged.derived);
    assert_eq!(legacy.staged.closes, v4.staged.closes);
    assert_eq!(legacy.staged.gesture_barrier, v4.staged.gesture_barrier);
    assert!(legacy.staged.echoes.is_empty());
    assert!(v4.staged.echoes.is_empty());

    // Shared content pins: minimized and normal facts stage, the closed id
    // stages, and the output-less row resolves through the retained home.
    assert!(v4.staged.facts.contains_key(&WindowId("win-a".to_owned())));
    assert!(v4.staged.facts.contains_key(&WindowId("win-b".to_owned())));
    assert!(v4.staged.closes.contains(&WindowId("win-c".to_owned())));
    let resolved = v4
        .staged
        .held_windows
        .iter()
        .find(|held| held.window.0 == "win-r")
        .expect("held win-r staged");
    assert_eq!(
        resolved.diagnostic,
        Some(diagnostic(
            "out-a",
            RowDiagnosticLane::Window,
            Some("win-r")
        ))
    );

    // The legacy intake leaves every v4-only field at its default.
    assert!(legacy.staged.held_native.is_empty());
    assert!(legacy.staged.held_attempts.is_empty());
    assert!(legacy.staged.presentation_v4.is_none());
    assert!(legacy.staged.renderer_input.is_none());
    // The v4 side carries no new-lane rows here but stores its
    // presentation verbatim.
    assert!(v4.staged.held_native.is_empty());
    assert!(v4.staged.held_attempts.is_empty());
    assert_eq!(
        v4.staged.presentation_v4,
        Some(PresentationRowV4::Valid(PresentationIngressV4::default()))
    );
    assert!(v4.staged.renderer_input.is_none());
}

#[test]
fn legacy_intake_ignores_new_fields() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        Vec::new(),
        "",
        None,
        None,
    );
    engine.stage_facts(&report);
    assert!(engine.staged.held_native.is_empty());
    assert!(engine.staged.held_attempts.is_empty());
    assert!(engine.staged.presentation_v4.is_none());
    assert!(engine.staged.renderer_input.is_none());
}

#[test]
fn v4_stages_new_lanes_verbatim() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let mut report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        Vec::new(),
        vec![held_screen("out-a")],
        vec![held_window("win-h", Some("out-a"))],
        false,
    );
    report.held_native = vec![HeldNativeRow {
        output: OutputId("out-a".to_owned()),
        ordinal: 0,
        fields: Vec::new(),
        diagnostic: diagnostic("out-a", RowDiagnosticLane::Native, None),
    }];
    report.held_attempts = vec![
        HeldAttemptRow {
            lane: AttemptLaneV4::Frame,
            ordinal: 0,
            identity: Vec::new(),
            fields: Vec::new(),
            diagnostic: None,
            kind: RowDiagnosticKind::InvalidFact,
        },
        HeldAttemptRow {
            lane: AttemptLaneV4::Member,
            ordinal: 1,
            identity: vec!["win-a".to_owned()],
            fields: Vec::new(),
            diagnostic: Some(diagnostic(
                "out-a",
                RowDiagnosticLane::Attempt,
                Some("win-a"),
            )),
            kind: RowDiagnosticKind::InvalidFact,
        },
    ];
    let ingress = PresentationIngressV4 {
        foreground: "win-a".to_owned(),
        chord_held: true,
        ..PresentationIngressV4::default()
    };
    report.presentation = PresentationRowV4::Valid(ingress.clone());
    report.renderer_input = Some(RendererInput {
        rev: 7,
        target: RenderTarget::Window {
            out: OutputId("out-a".to_owned()),
            ws: WorkspaceId("ws-1".to_owned()),
            win: WindowId("win-a".to_owned()),
        },
    });
    engine.stage_facts_v4(&report);

    assert_eq!(engine.staged.held_native, report.held_native);
    assert_eq!(engine.staged.held_attempts, report.held_attempts);
    assert_eq!(
        engine.staged.presentation_v4,
        Some(PresentationRowV4::Valid(ingress))
    );
    assert_eq!(engine.staged.renderer_input, report.renderer_input);
    assert!(engine.staged.echoes.is_empty());
    // Window diagnostics first, then the native diagnostic, then the
    // attempt row carrying `Some`; the `None` attempt row emits nothing.
    let lanes: Vec<RowDiagnosticLane> = engine
        .staged
        .diagnostics
        .iter()
        .map(|item| item.row)
        .collect();
    assert_eq!(
        lanes,
        vec![
            RowDiagnosticLane::Screen,
            RowDiagnosticLane::Window,
            RowDiagnosticLane::Native,
            RowDiagnosticLane::Attempt,
        ]
    );
}

#[test]
fn v4_lane_order_with_held_presentation_last() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let mut report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        Vec::new(),
        vec![held_screen("out-a")],
        vec![held_window("win-h", Some("out-a"))],
        false,
    );
    report.held_native = vec![HeldNativeRow {
        output: OutputId("out-a".to_owned()),
        ordinal: 0,
        fields: Vec::new(),
        diagnostic: diagnostic("out-a", RowDiagnosticLane::Native, None),
    }];
    report.held_attempts = vec![HeldAttemptRow {
        lane: AttemptLaneV4::Clear,
        ordinal: 0,
        identity: Vec::new(),
        fields: Vec::new(),
        diagnostic: Some(diagnostic(
            "out-a",
            RowDiagnosticLane::Attempt,
            Some("win-a"),
        )),
        kind: RowDiagnosticKind::InvalidFact,
    }];
    let held_presentation = HeldPresentationRow {
        fields: Vec::new(),
        kind: RowDiagnosticKind::InvalidFact,
        diagnostic: Some(diagnostic("out-a", RowDiagnosticLane::Presentation, None)),
    };
    report.presentation = PresentationRowV4::Held(held_presentation.clone());
    engine.stage_facts_v4(&report);

    // One held row per lane stages its diagnostic in lane order, with the
    // held presentation last; the stored row is verbatim, never defaulted.
    assert_eq!(
        engine.staged.presentation_v4,
        Some(PresentationRowV4::Held(held_presentation.clone()))
    );
    let lanes: Vec<RowDiagnosticLane> = engine
        .staged
        .diagnostics
        .iter()
        .map(|item| item.row)
        .collect();
    assert_eq!(
        lanes,
        vec![
            RowDiagnosticLane::Screen,
            RowDiagnosticLane::Window,
            RowDiagnosticLane::Native,
            RowDiagnosticLane::Attempt,
            RowDiagnosticLane::Presentation,
        ]
    );

    // A held presentation without a scope stages the row but no diagnostic.
    let mut scopeless = report.clone();
    scopeless.presentation = PresentationRowV4::Held(HeldPresentationRow {
        diagnostic: None,
        ..held_presentation
    });
    engine.stage_facts_v4(&scopeless);
    assert!(matches!(
        engine.staged.presentation_v4,
        Some(PresentationRowV4::Held(_))
    ));
    let lanes: Vec<RowDiagnosticLane> = engine
        .staged
        .diagnostics
        .iter()
        .map(|item| item.row)
        .collect();
    assert_eq!(
        lanes,
        vec![
            RowDiagnosticLane::Screen,
            RowDiagnosticLane::Window,
            RowDiagnosticLane::Native,
            RowDiagnosticLane::Attempt,
        ]
    );
}

// --- Sub-increment 3: capability-conflict grading and state offroute ---

fn caps_sticky_native_free() -> HostCapabilities {
    HostCapabilities {
        maximize: false,
        fullscreen: false,
        native_sticky: false,
        above: false,
        window_suppression: true,
        ..HostCapabilities::default()
    }
}

fn caps_permissive() -> HostCapabilities {
    HostCapabilities {
        maximize: true,
        fullscreen: true,
        native_sticky: true,
        above: true,
        window_suppression: true,
        ..HostCapabilities::default()
    }
}

fn engine_with_caps(caps: HostCapabilities) -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", caps).expect("generation")
}

fn state_attempt(
    window: &str,
    property: StateProperty,
    target: bool,
    revision: u64,
) -> StateAttempt {
    StateAttempt {
        window: WindowId(window.to_owned()),
        property,
        target,
        revision,
        setter: FrameSetter::Threw,
        pre: None,
        post: None,
        attempt_ms: 20,
    }
}

fn conflicting_diagnostic(output: &str, lane: RowDiagnosticLane, id: &str) -> RowDiagnostic {
    RowDiagnostic {
        scope: AuthorityScope::Output(OutputId(output.to_owned())),
        row: lane,
        id: Some(id.to_owned()),
        kind: RowDiagnosticKind::ConflictingFact,
    }
}

#[test]
fn unbound_capability_defaults_fail_closed_for_all_state_properties() {
    for property in [
        StateProperty::Fullscreen,
        StateProperty::Maximized,
        StateProperty::Sticky,
        StateProperty::Above,
    ] {
        assert!(!state_property_supported(None, property), "{property:?}");
        assert!(state_property_supported(Some(&caps_permissive()), property));
        assert!(!state_property_supported(
            Some(&HostCapabilities::default()),
            property
        ));
    }
    // Bound true/false passes through per property.
    let mut caps = caps_permissive();
    caps.fullscreen = false;
    assert!(!state_property_supported(
        Some(&caps),
        StateProperty::Fullscreen
    ));
    assert!(state_property_supported(
        Some(&caps),
        StateProperty::Maximized
    ));
    caps.maximize = false;
    caps.native_sticky = false;
    caps.above = false;
    for property in [
        StateProperty::Fullscreen,
        StateProperty::Maximized,
        StateProperty::Sticky,
        StateProperty::Above,
    ] {
        assert!(
            !state_property_supported(Some(&caps), property),
            "{property:?}"
        );
    }
}

#[test]
fn v4_grades_unavailable_fact_contradiction_to_held() {
    let mut engine = engine_with_caps(HostCapabilities {
        unavailable_facts: vec![FactName::Managed],
        ..caps_permissive()
    });
    let mut contradictory = fact("win-a", "out-a", 0);
    contradictory.managed = Some(true);
    let report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![contradictory],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    // Only Managed is unavailable, so the hold is attributable to it.
    engine.stage_facts_v4(&report);
    assert!(engine.staged.facts.is_empty());
    assert_eq!(engine.staged.held_windows.len(), 1);
    assert_eq!(
        engine.staged.diagnostics,
        vec![conflicting_diagnostic(
            "out-a",
            RowDiagnosticLane::Window,
            "win-a"
        )]
    );
    // Supported control: permissive capabilities keep the same fact live.
    let mut supported = engine_with_caps(caps_permissive());
    supported.stage_facts_v4(&report);
    assert_eq!(supported.staged.facts.len(), 1);
    assert!(supported.staged.held_windows.is_empty());
    assert!(supported.staged.diagnostics.is_empty());
}

#[test]
fn v4_grades_executable_unavailable_value_to_held() {
    let mut engine = engine_with_caps(HostCapabilities {
        unavailable_facts: vec![FactName::Executable],
        ..caps_permissive()
    });
    let mut with_exe = fact("win-a", "out-a", 0);
    with_exe.executable = Some("c:\\synthetic\\example.exe".to_owned());
    let report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![with_exe],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    engine.stage_facts_v4(&report);
    assert!(engine.staged.facts.is_empty());
    assert_eq!(engine.staged.held_windows.len(), 1);
    assert_eq!(
        engine.staged.diagnostics,
        vec![conflicting_diagnostic(
            "out-a",
            RowDiagnosticLane::Window,
            "win-a"
        )]
    );
    let mut supported = engine_with_caps(caps_permissive());
    supported.stage_facts_v4(&report);
    assert_eq!(supported.staged.facts.len(), 1);
    assert!(supported.staged.diagnostics.is_empty());
}

#[test]
fn v4_grades_sticky_projected_fact_to_held() {
    let mut engine = engine_with_caps(caps_sticky_native_free());
    let mut sticky = fact("win-a", "out-a", 0);
    sticky.sticky = Some(true);
    // Isolate the sticky conflict: maximized null never grades.
    sticky.maximized = None;
    let report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![sticky],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    engine.stage_facts_v4(&report);
    assert!(engine.staged.facts.is_empty());
    assert_eq!(engine.staged.held_windows.len(), 1);
    assert_eq!(
        engine.staged.diagnostics,
        vec![conflicting_diagnostic(
            "out-a",
            RowDiagnosticLane::Window,
            "win-a"
        )]
    );
    // Sticky null never grades, even without the capability.
    let mut null_sticky = fact("win-a", "out-a", 0);
    null_sticky.sticky = None;
    null_sticky.maximized = None;
    let null_report = report_v4(
        2,
        vec![screen("out-a", 0)],
        vec![null_sticky],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    let mut native_free = engine_with_caps(caps_sticky_native_free());
    native_free.stage_facts_v4(&null_report);
    assert_eq!(native_free.staged.facts.len(), 1);
    assert!(native_free.staged.held_windows.is_empty());
    // Supported control keeps the non-null sticky live and classified.
    let mut supported = engine_with_caps(caps_permissive());
    supported.stage_facts_v4(&report);
    assert_eq!(supported.staged.facts.len(), 1);
    assert!(supported.staged.diagnostics.is_empty());
}

#[test]
fn v4_cursor_hit_on_capability_held_fact_keeps_hold() {
    use crate::world_report_v4_checks::validate_report_v4_reason;
    let mut engine = engine_with_caps(caps_sticky_native_free());
    let mut sticky = fact("win-a", "out-a", 0);
    sticky.sticky = Some(true);
    // Isolate the sticky conflict: maximized null never grades.
    sticky.maximized = None;
    let mut report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![sticky],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    report.cursor = Some((10, 20));
    report.cursor_hit = Some(WindowId("win-a".to_owned()));
    // The initial report is valid: the hit names a typed window.
    assert_eq!(validate_report_v4_reason(&report), None);
    let original_windows = report.windows.clone();
    engine.stage_facts_v4(&report);
    // Raw capture preserves the original facts verbatim, with the native
    // cursor lanes intact; the held-target clearing applies to the
    // effective copy only, with no surrogate target.
    assert_eq!(engine.staged.observation.raw_windows, original_windows);
    assert_eq!(engine.staged.observation.cursor, Some((10, 20)));
    assert_eq!(
        engine.staged.observation.cursor_hit,
        Some(WindowId("win-a".to_owned()))
    );
    // The conflicting typed fact must not survive to staging: classified
    // fact absence is the holding invariant (no full v4 hover path is
    // wired yet).
    assert!(engine.staged.facts.is_empty());
    assert_eq!(engine.staged.held_windows.len(), 1);
    assert_eq!(
        engine.staged.diagnostics,
        vec![conflicting_diagnostic(
            "out-a",
            RowDiagnosticLane::Window,
            "win-a"
        )]
    );
}

#[test]
fn v4_grade_clears_cursor_hit_naming_newly_held_window() {
    use crate::world_report_v4_checks::{grade_conflicting_windows_v4, validate_report_v4_reason};
    let mut sticky = fact("win-a", "out-a", 0);
    sticky.sticky = Some(true);
    // Isolate the sticky conflict: maximized null never grades.
    sticky.maximized = None;
    let mut report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![sticky],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    report.cursor = Some((10, 20));
    report.cursor_hit = Some(WindowId("win-a".to_owned()));
    assert_eq!(validate_report_v4_reason(&report), None);
    let graded =
        grade_conflicting_windows_v4(&report, &[], false, false).expect("grades with cursor clear");
    // Input report intact: cursor, hit and typed row untouched.
    assert_eq!(report.cursor, Some((10, 20)));
    assert_eq!(report.cursor_hit, Some(WindowId("win-a".to_owned())));
    assert_eq!(report.windows.len(), 1);
    // Effective rows: cursor kept, dangling hit cleared, hold retained.
    assert_eq!(graded.cursor, Some((10, 20)));
    assert_eq!(graded.cursor_hit, None);
    assert!(graded.windows.is_empty());
    assert_eq!(graded.held_windows.len(), 1);
    assert_eq!(graded.held_windows[0].window.0, "win-a");
    assert_eq!(graded.held_windows[0].ordinal, 0);
    assert_eq!(
        graded.held_windows[0].kind,
        RowDiagnosticKind::ConflictingFact
    );
    assert_eq!(validate_report_v4_reason(&graded), None);
}

#[test]
fn v4_grade_keeps_hit_on_unaffected_survivor() {
    use crate::world_report_v4_checks::{grade_conflicting_windows_v4, validate_report_v4_reason};
    let mut sticky = fact("win-a", "out-a", 0);
    sticky.sticky = Some(true);
    sticky.maximized = None;
    let mut plain = fact("win-b", "out-a", 40);
    plain.sticky = None;
    plain.maximized = None;
    let mut report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![sticky, plain],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    report.cursor = Some((10, 20));
    report.cursor_hit = Some(WindowId("win-b".to_owned()));
    assert_eq!(validate_report_v4_reason(&report), None);
    let graded =
        grade_conflicting_windows_v4(&report, &[], false, false).expect("grades survivor report");
    // The survivor hit is not dangling: it stays verbatim.
    assert_eq!(graded.cursor_hit, Some(WindowId("win-b".to_owned())));
    assert_eq!(graded.windows.len(), 1);
    assert_eq!(graded.windows[0].window.0, "win-b");
    assert_eq!(graded.held_windows.len(), 1);
    assert_eq!(graded.held_windows[0].window.0, "win-a");
    assert_eq!(validate_report_v4_reason(&graded), None);
    // Supported control: no conflict, no grading, input untouched.
    let ungraded = grade_conflicting_windows_v4(&report, &[], true, true);
    assert!(ungraded.is_none());
    assert_eq!(report.cursor_hit, Some(WindowId("win-b".to_owned())));
}

#[test]
fn v4_grade_refuses_malformed_cursor_hit_originals() {
    use crate::world_report_v4_checks::{grade_conflicting_windows_v4, validate_report_v4_reason};
    fn conflicting_report() -> WorldReportV4 {
        let mut sticky = fact("win-a", "out-a", 0);
        sticky.sticky = Some(true);
        sticky.maximized = None;
        report_v4(
            1,
            vec![screen("out-a", 0)],
            vec![sticky],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            false,
        )
    }
    // Unknown hit: clearing must not mask the invalid original.
    let mut unknown = conflicting_report();
    unknown.cursor = Some((10, 20));
    unknown.cursor_hit = Some(WindowId("win-ghost".to_owned()));
    assert_eq!(
        validate_report_v4_reason(&unknown),
        Some("cursor-hit-unknown")
    );
    assert!(grade_conflicting_windows_v4(&unknown, &[], false, false).is_none());
    // Hit without cursor: clearing must not mask the invalid original.
    let mut cursorless = conflicting_report();
    cursorless.cursor_hit = Some(WindowId("win-a".to_owned()));
    assert_eq!(
        validate_report_v4_reason(&cursorless),
        Some("cursor-hit-without-cursor")
    );
    assert!(grade_conflicting_windows_v4(&cursorless, &[], false, false).is_none());
    // Unrelated invalid original: a dangling hit cleared on the effective
    // copy must not rescue grading when another lane still refuses.
    let mut unpaired = conflicting_report();
    unpaired.cursor = Some((10, 20));
    unpaired.cursor_hit = Some(WindowId("win-a".to_owned()));
    unpaired.command_seq = Some(1);
    assert_eq!(
        validate_report_v4_reason(&unpaired),
        Some("command-pair-invalid")
    );
    assert!(grade_conflicting_windows_v4(&unpaired, &[], false, false).is_none());
}

#[test]
fn v4_grade_conserves_existing_held_rows_and_ordinals() {
    use crate::world_report_v4_checks::{grade_conflicting_windows_v4, validate_report_v4_reason};
    let mut sticky = fact("win-a", "out-a", 0);
    sticky.sticky = Some(true);
    sticky.maximized = None;
    let mut scoped = held_window("win-h", Some("out-a"));
    scoped.ordinal = 1;
    let mut report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![sticky],
        Vec::new(),
        Vec::new(),
        vec![scoped],
        false,
    );
    report.cursor = Some((10, 20));
    report.cursor_hit = Some(WindowId("win-a".to_owned()));
    assert_eq!(validate_report_v4_reason(&report), None);
    let graded =
        grade_conflicting_windows_v4(&report, &[], false, false).expect("grades with prior hold");
    // Pre-existing held row conserved verbatim; the new hold takes the
    // free slot and rows stay ordinal-ordered.
    assert_eq!(graded.held_windows.len(), 2);
    assert_eq!(
        graded
            .held_windows
            .iter()
            .map(|held| (held.window.0.clone(), held.ordinal))
            .collect::<Vec<_>>(),
        vec![("win-a".to_owned(), 0), ("win-h".to_owned(), 1)]
    );
    assert_eq!(graded.held_windows[1], report.held_windows[0]);
    assert_eq!(graded.cursor_hit, None);
    assert_eq!(validate_report_v4_reason(&graded), None);
}

#[test]
fn v4_unsupported_sticky_attempt_emits_diagnostic_and_preserves_raw() {
    let mut engine = engine_with_caps(caps_sticky_native_free());
    let mut live = fact("win-a", "out-a", 0);
    live.sticky = None;
    live.maximized = None;
    let mut report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![live],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    report.state_attempts = vec![state_attempt("win-a", StateProperty::Sticky, true, 1)];
    engine.stage_facts_v4(&report);
    // Raw window stays live; raw attempt stays intact, never a held row.
    assert_eq!(engine.staged.facts.len(), 1);
    assert!(engine.staged.held_windows.is_empty());
    assert!(engine.staged.held_attempts.is_empty());
    assert_eq!(
        engine.staged.diagnostics,
        vec![conflicting_diagnostic(
            "out-a",
            RowDiagnosticLane::Attempt,
            "win-a"
        )]
    );
    // Supported control: same attempt is silent.
    let mut supported = engine_with_caps(caps_permissive());
    // Permissive sticky fact must be non-null to stay comparable; reuse null
    // live fact would hold classification, so use a sticky-false fact.
    let mut supported_live = fact("win-a", "out-a", 0);
    supported_live.sticky = Some(false);
    let mut supported_report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![supported_live],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    supported_report.state_attempts = vec![state_attempt("win-a", StateProperty::Sticky, true, 1)];
    supported.stage_facts_v4(&supported_report);
    assert!(supported.staged.diagnostics.is_empty());
    assert!(supported.staged.held_attempts.is_empty());
}

#[test]
fn v4_unsupported_attempt_does_not_settle() {
    use crate::world_report::AttemptOutcome;
    use crate::world_state::{RetainedStateBinding, RetainedStateTarget};
    let mut engine = engine_with_caps(caps_sticky_native_free());
    engine.retained_states.insert(
        (WindowId("win-a".to_owned()), StateProperty::Sticky),
        RetainedStateBinding {
            target: RetainedStateTarget {
                window: WindowId("win-a".to_owned()),
                property: StateProperty::Sticky,
                value: true,
                revision: 1,
            },
            pending: None,
            spent: false,
            disposition: AttemptOutcome::Unattempted,
            sticky_meta: None,
            fullscreen_meta: None,
            maximize_meta: None,
            above_meta: None,
        },
    );
    let attempt = state_attempt("win-a", StateProperty::Sticky, true, 1);
    let mut report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    report.state_attempts = vec![attempt.clone()];
    engine.stage_facts_v4(&report);
    // The real staged v4 attempt is present in the capture; settlement
    // still ignores it for the unsupported property.
    assert_eq!(engine.staged.observation.state_attempts, vec![attempt]);
    engine.settle_states();
    let binding = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding retained");
    assert!(!binding.spent);
    assert!(binding.pending.is_none());
    assert_eq!(binding.disposition, AttemptOutcome::Unattempted);
}

#[test]
fn v4_state_offroute_scope_precedence_and_unresolved() {
    // Carried valid output wins over held output.
    let mut engine = engine_with_caps(HostCapabilities::default());
    let mut valid = fact("win-a", "out-a", 0);
    valid.sticky = None;
    valid.maximized = None;
    let held = HeldWindowRow {
        window: WindowId("win-a".to_owned()),
        ordinal: 0,
        output: Some(OutputId("out-b".to_owned())),
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::ConflictingFact,
    };
    let mut report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![valid],
        Vec::new(),
        Vec::new(),
        vec![held],
        false,
    );
    report.state_attempts = vec![state_attempt("win-a", StateProperty::Above, true, 1)];
    engine.stage_facts_v4(&report);
    assert_eq!(
        engine.staged.diagnostics,
        vec![
            conflicting_diagnostic("out-b", RowDiagnosticLane::Window, "win-a"),
            conflicting_diagnostic("out-a", RowDiagnosticLane::Attempt, "win-a"),
        ]
    );
    // Held output wins when no valid fact carries the window.
    let mut held_only = engine_with_caps(HostCapabilities::default());
    let held_row = HeldWindowRow {
        window: WindowId("win-held".to_owned()),
        ordinal: 0,
        output: Some(OutputId("out-b".to_owned())),
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    };
    let mut held_report = report_v4(
        1,
        vec![screen("out-a", 0)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![held_row],
        false,
    );
    held_report.state_attempts = vec![state_attempt("win-held", StateProperty::Above, true, 1)];
    held_only.stage_facts_v4(&held_report);
    assert_eq!(
        held_only.staged.diagnostics,
        vec![
            RowDiagnostic {
                scope: AuthorityScope::Output(OutputId("out-b".to_owned())),
                row: RowDiagnosticLane::Window,
                id: Some("win-held".to_owned()),
                kind: RowDiagnosticKind::InvalidFact,
            },
            conflicting_diagnostic("out-b", RowDiagnosticLane::Attempt, "win-held"),
        ]
    );
    // Unresolvable ghost emits nothing and fabricates no scope.
    let mut ghost_engine = engine_with_caps(HostCapabilities::default());
    let mut ghost = report_v4(
        1,
        vec![screen("out-a", 0)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    ghost.state_attempts = vec![state_attempt("ghost", StateProperty::Above, true, 1)];
    ghost_engine.stage_facts_v4(&ghost);
    assert!(ghost_engine.staged.diagnostics.is_empty());
}

#[test]
fn v4_state_offroute_uses_retained_home_when_carried_output_missing() {
    // Seed a retained home via an accepted legacy run, then stage a v4
    // report with no carried output for the same lifetime.
    let mut engine = engine_with_caps(caps_permissive());
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-r", "out-a", 0)],
            Vec::new(),
            "",
            None,
            None,
        ),
    );
    let mut engine = {
        let mut with_caps = engine_with_caps(HostCapabilities::default());
        // Carry over the retained membership so core_homing resolves.
        with_caps.members = engine.members.clone();
        with_caps.engine = engine.engine.clone();
        with_caps
    };
    let mut report = report_v4(
        2,
        vec![screen("out-a", 0)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    report.state_attempts = vec![state_attempt("win-r", StateProperty::Above, true, 1)];
    engine.stage_facts_v4(&report);
    assert_eq!(
        engine.staged.diagnostics,
        vec![conflicting_diagnostic(
            "out-a",
            RowDiagnosticLane::Attempt,
            "win-r"
        )]
    );
}

#[test]
fn v4_supported_attempt_emits_no_offroute() {
    let mut engine = engine_with_caps(caps_permissive());
    let mut report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    report.state_attempts = vec![
        state_attempt("win-a", StateProperty::Fullscreen, true, 1),
        state_attempt("win-a", StateProperty::Above, true, 1),
    ];
    engine.stage_facts_v4(&report);
    assert!(engine.staged.diagnostics.is_empty());
}

#[test]
fn v4_held_row_interaction_uses_graded_held_output() {
    // A conflicting sticky fact grades to held on out-a; the unsupported
    // sticky attempt for the same window must scope through that held row.
    // Maximized null isolates the sticky conflict (non-null maximized would
    // grade independently under maximize=false).
    let mut engine = engine_with_caps(caps_sticky_native_free());
    let mut sticky = fact("win-a", "out-a", 0);
    sticky.sticky = Some(true);
    sticky.maximized = None;
    let mut report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![sticky],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    report.state_attempts = vec![state_attempt("win-a", StateProperty::Sticky, true, 1)];
    engine.stage_facts_v4(&report);
    assert!(engine.staged.facts.is_empty());
    assert_eq!(engine.staged.held_windows.len(), 1);
    assert_eq!(engine.staged.diagnostics.len(), 2);
    assert_eq!(
        engine.staged.diagnostics[0],
        conflicting_diagnostic("out-a", RowDiagnosticLane::Window, "win-a")
    );
    assert_eq!(
        engine.staged.diagnostics[1],
        conflicting_diagnostic("out-a", RowDiagnosticLane::Attempt, "win-a")
    );
}

#[test]
fn v4_staging_twice_emits_no_duplicates() {
    let mut engine = engine_with_caps(caps_sticky_native_free());
    let mut sticky = fact("win-a", "out-a", 0);
    sticky.sticky = Some(true);
    // Maximized null keeps the hold attributable to sticky alone.
    sticky.maximized = None;
    let mut report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![sticky],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    report.state_attempts = vec![state_attempt("win-a", StateProperty::Sticky, true, 1)];
    engine.stage_facts_v4(&report);
    let first = engine.staged.diagnostics.clone();
    engine.stage_facts_v4(&report);
    assert_eq!(engine.staged.diagnostics, first);
}

#[test]
fn v4_offroute_orders_before_presentation_last() {
    let mut engine = engine_with_caps(HostCapabilities {
        maximize: true,
        native_sticky: true,
        fullscreen: true,
        above: false,
        ..HostCapabilities::default()
    });
    let mut live = fact("win-a", "out-a", 0);
    live.sticky = Some(false);
    let mut report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![live],
        Vec::new(),
        vec![held_screen("out-a")],
        vec![held_window("win-h", Some("out-a"))],
        false,
    );
    report.held_native = vec![HeldNativeRow {
        output: OutputId("out-a".to_owned()),
        ordinal: 0,
        fields: Vec::new(),
        diagnostic: diagnostic("out-a", RowDiagnosticLane::Native, None),
    }];
    report.held_attempts = vec![HeldAttemptRow {
        lane: AttemptLaneV4::Clear,
        ordinal: 0,
        identity: Vec::new(),
        fields: Vec::new(),
        diagnostic: Some(diagnostic(
            "out-a",
            RowDiagnosticLane::Attempt,
            Some("win-a"),
        )),
        kind: RowDiagnosticKind::InvalidFact,
    }];
    report.state_attempts = vec![state_attempt("win-a", StateProperty::Above, true, 1)];
    let held_presentation = HeldPresentationRow {
        fields: Vec::new(),
        kind: RowDiagnosticKind::InvalidFact,
        diagnostic: Some(diagnostic("out-a", RowDiagnosticLane::Presentation, None)),
    };
    report.presentation = PresentationRowV4::Held(held_presentation);
    engine.stage_facts_v4(&report);
    let lanes: Vec<RowDiagnosticLane> = engine
        .staged
        .diagnostics
        .iter()
        .map(|item| item.row)
        .collect();
    assert_eq!(
        lanes,
        vec![
            RowDiagnosticLane::Screen,
            RowDiagnosticLane::Window,
            RowDiagnosticLane::Native,
            RowDiagnosticLane::Attempt,
            RowDiagnosticLane::Attempt,
            RowDiagnosticLane::Presentation,
        ]
    );
    assert_eq!(
        engine.staged.diagnostics[4],
        conflicting_diagnostic("out-a", RowDiagnosticLane::Attempt, "win-a")
    );
}

#[test]
fn unbound_grading_withholds_non_null_sticky_and_maximize() {
    let unbound = WorldEngine::new("gen-1").expect("generation");
    assert!(unbound.capabilities().is_none());
    let mut sticky_fact = fact("win-a", "out-a", 0);
    sticky_fact.sticky = Some(true);
    // Isolate each conflict to its own property: null never grades.
    sticky_fact.maximized = None;
    let mut maximized_fact = fact("win-b", "out-a", 0);
    maximized_fact.maximized = Some(true);
    maximized_fact.sticky = None;
    // Unbound intake grades both to held: absent binding means no support.
    // This proves grading only; classification defaults are pinned below.
    let mut engine = unbound;
    let report = report_v4(
        1,
        vec![screen("out-a", 0)],
        vec![sticky_fact, maximized_fact],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
    );
    engine.stage_facts_v4(&report);
    assert!(engine.staged.facts.is_empty());
    assert_eq!(engine.staged.held_windows.len(), 2);
    assert!(engine.staged.diagnostics.iter().all(|item| {
        item.kind == RowDiagnosticKind::ConflictingFact && item.row == RowDiagnosticLane::Window
    }));
}

#[test]
fn unbound_classification_matches_unsupported_and_rejects_permissive_default() {
    // Null sticky/maximized facts pass grading under every binding, so the
    // legacy intake (which never grades) exercises the shared classification
    // defaults in `stage_window_facts` directly.
    fn classified_hold(engine: &mut WorldEngine) -> bool {
        let mut null_fact = fact("win-a", "out-a", 0);
        null_fact.sticky = None;
        null_fact.maximized = None;
        let legacy = report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![null_fact],
            Vec::new(),
            "",
            None,
            None,
        );
        engine.stage_facts(&legacy);
        assert_eq!(engine.staged.facts.len(), 1);
        assert!(engine.staged.held_windows.is_empty());
        assert!(engine.staged.diagnostics.is_empty());
        engine
            .staged
            .derived
            .get(&WindowId("win-a".to_owned()))
            .expect("classified")
            .hold
    }
    let mut unbound = WorldEngine::new("gen-1").expect("generation");
    let mut unsupported = engine_with_caps(HostCapabilities::default());
    let mut supported = engine_with_caps(caps_permissive());
    let unbound_hold = classified_hold(&mut unbound);
    let unsupported_hold = classified_hold(&mut unsupported);
    let supported_hold = classified_hold(&mut supported);
    // Fail closed without a binding, exactly like an explicit unsupported
    // binding: null facts stay live. A supported binding holds the same
    // nulls (composed-route unknowns gate membership), so the old
    // permissive `unwrap_or(true)` classification default would fail the
    // first assertion.
    assert!(!unbound_hold);
    assert_eq!(unbound_hold, unsupported_hold);
    assert!(supported_hold);
}
