//! v4 staging intake regressions: legacy-versus-v4 staged equality over the
//! shared lanes, plus verbatim retention and lane order for the v4-only
//! lanes (`held_native`, `held_attempts`, `presentation_v4`,
//! `renderer_input`).

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::world_directional::tests::{accepted, fact, report_full, screen};
use crate::world_engine::WorldEngine;
use crate::world_report::{
    AttemptLaneV4, AuthorityScope, HeldAttemptRow, HeldNativeRow, HeldPresentationRow,
    HeldScreenRow, HeldWindowRow, PresentationIngressV4, PresentationRowV4, RenderTarget,
    RendererInput, RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane, ScreenReport,
    WORLD_CONTRACT_VERSION_V4, WindowFact, WorldReportV4,
};

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
