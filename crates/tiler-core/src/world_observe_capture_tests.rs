//! Raw observation capture regressions (unit 6 sub-increment 4a): both
//! intakes stage the same verbatim observation boundary before any
//! classification, intent ingestion consumes the staged capture before the
//! classifier runs, and capture never mutates retained routing state.

use std::collections::{BTreeMap, BTreeSet};

use crate::connection::{HostCapabilities, RawSettingValue};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::gesture::{GestureIngress, GestureKind, GesturePhase};
use crate::ids::{CorrelationId, OwnerId};
use crate::intent_data::{IntentData, encode_intent_data};
use crate::seed::EngineWindow;
use crate::session::DomainKey;
use crate::world_directional::tests::{accepted, fact, float_fact, rect, report_full, screen};
use crate::world_engine::{
    ExcludedIntent, RetainedBornClear, RetainedTransfer, StagedDomain, StagedObservation,
    WorldEngine,
};
use crate::world_focus::{FocusVerdict, RetainedFocusGrant};
use crate::world_gesture::GestureOutcome;
use crate::world_members::MemberState;
use crate::world_report::{
    ArrivalBracket, ArrivalLocation, AttemptLaneV4, AttemptOutcome, AuthorityScope, ClearAttempt,
    CommandResult, EffectCause, FocusFeedback, FrameAttempt, FrameSetter, HeldAttemptRow,
    HeldNativeRow, HeldScreenRow, HeldWindowRow, HoldReason, IntentRecord, IntentWrite, LastWrite,
    LastWritePath, LastWriteProperty, MemberAttempt, NativeCreationAttempt, NativeObservation,
    NativeOccupancy, OneShotEffect, PresentationIngressV4, PresentationRowV4, RenderTarget,
    RendererInput, RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane, ScopeAuthority,
    ScreenReport, ScreenView, StateAttempt, StateProperty, SwitchOutcome, TransferKind,
    TransferTarget, ViewAttempt, VisibilityFact, VisibilityReason, VisibilitySample,
    VisibilitySetter, WORLD_CONTRACT_VERSION_V4, WindowDesire, WindowFact, WorkspaceRef,
    WorldReport, WorldReportV4,
};
use crate::world_selection::{
    FrozenArrivalMember, RetainedSelection, SelectionScope, SelectionTarget, SwitchMember,
};
use crate::world_state::{RetainedStateBinding, RetainedStateTarget};

fn engine() -> WorldEngine {
    WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
}

fn diag(output: &str, row: RowDiagnosticLane, id: Option<&str>) -> RowDiagnostic {
    RowDiagnostic {
        scope: AuthorityScope::Output(OutputId(output.to_owned())),
        row,
        id: id.map(str::to_owned),
        kind: RowDiagnosticKind::InvalidFact,
    }
}

fn held_screen_row(output: &str, ordinal: usize) -> HeldScreenRow {
    HeldScreenRow {
        output: OutputId(output.to_owned()),
        ordinal,
        fields: Vec::new(),
        diagnostic: diag(output, RowDiagnosticLane::Screen, Some(output)),
    }
}

fn held_window_row(window: &str, output: &str, ordinal: usize) -> HeldWindowRow {
    HeldWindowRow {
        window: WindowId(window.to_owned()),
        ordinal,
        output: Some(OutputId(output.to_owned())),
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }
}

fn held_native_row(output: &str, ordinal: usize) -> HeldNativeRow {
    HeldNativeRow {
        output: OutputId(output.to_owned()),
        ordinal,
        fields: Vec::new(),
        diagnostic: diag(output, RowDiagnosticLane::Native, None),
    }
}

fn held_frame_row(ordinal: usize) -> HeldAttemptRow {
    HeldAttemptRow {
        lane: AttemptLaneV4::Frame,
        ordinal,
        identity: Vec::new(),
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }
}

fn held_member_row(ordinal: usize) -> HeldAttemptRow {
    HeldAttemptRow {
        lane: AttemptLaneV4::Member,
        ordinal,
        identity: vec!["win-a".to_owned()],
        fields: Vec::new(),
        diagnostic: Some(diag("out-a", RowDiagnosticLane::Attempt, Some("win-a"))),
        kind: RowDiagnosticKind::InvalidFact,
    }
}

fn native_row(output: &str) -> NativeObservation {
    NativeObservation {
        output: OutputId(output.to_owned()),
        native_order: vec![format!("native-{output}-0")],
        current_native: Some(format!("native-{output}-0")),
        occupancy: vec![NativeOccupancy {
            native: format!("native-{output}-0"),
            count: None,
        }],
    }
}

fn gesture_row() -> GestureIngress {
    GestureIngress {
        id: 7,
        window: WindowId("win-a".to_owned()),
        kind: GestureKind::Move,
        cursor: Some((10, 20)),
        start_cursor: None,
        phase: GesturePhase::Started,
        esc: false,
        modal_move: true,
        start_frame: rect(0, 0, 40, 90),
    }
}

fn frame_row(window: &str) -> FrameAttempt {
    FrameAttempt {
        window: WindowId(window.to_owned()),
        target: rect(0, 0, 40, 90),
        setter: FrameSetter::Accepted,
        readback: None,
        dpi_before: None,
        dpi_after: None,
        attempt_ms: 11,
        operation_id: None,
        prewrite: None,
    }
}

fn member_row(operation: &str, window: &str) -> MemberAttempt {
    MemberAttempt {
        operation_id: operation.to_owned(),
        window: WindowId(window.to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: "ws-9".to_owned(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId("ws-9".to_owned()),
        }),
        attempt_ms: 12,
    }
}

fn view_row(operation: &str) -> ViewAttempt {
    ViewAttempt {
        operation_id: operation.to_owned(),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: None,
        baseline: None,
        attempt_ms: 13,
    }
}

fn clear_row(operation: &str, window: &str) -> ClearAttempt {
    ClearAttempt {
        operation_id: operation.to_owned(),
        window: WindowId(window.to_owned()),
        setter: FrameSetter::Accepted,
        pre_maximized: Some(true),
        post_maximized: Some(true),
        attempt_ms: 14,
    }
}

fn creation_row(operation: &str, workspace: &str, output: &str) -> NativeCreationAttempt {
    NativeCreationAttempt {
        operation_id: operation.to_owned(),
        logical_workspace: workspace.to_owned(),
        target_output: OutputId(output.to_owned()),
        setter: FrameSetter::Accepted,
        order_before: vec![format!("native-{output}-0")],
        order_after: vec![format!("native-{output}-0")],
        created_native: None,
        attempt_ms: 15,
    }
}

fn state_row(window: &str, property: StateProperty) -> StateAttempt {
    StateAttempt {
        window: WindowId(window.to_owned()),
        property,
        target: true,
        revision: 1,
        setter: FrameSetter::Accepted,
        pre: Some(false),
        post: Some(false),
        attempt_ms: 16,
    }
}

fn feedback_row(id: &str, target: &str) -> FocusFeedback {
    FocusFeedback {
        id: id.to_owned(),
        target: WindowId(target.to_owned()),
        revision: 1,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: None,
        post: None,
        foreground_proven: false,
        arrival: Vec::<ArrivalBracket>::new(),
        attempt_ms: 17,
    }
}

fn record(win: &str, data: &str) -> IntentRecord {
    IntentRecord {
        win: WindowId(win.to_owned()),
        data: data.to_owned(),
    }
}

fn float_true() -> String {
    encode_intent_data(&IntentData::new(Some(true), None))
}

fn tile_false() -> String {
    encode_intent_data(&IntentData::new(Some(false), None))
}

fn stage_ingested(engine: &mut WorldEngine, report: &WorldReport) {
    // Initial intake ordering: capture, ingest once, then classify.
    // Restages below reuse classification-only `restage_classification`
    // and must never ingest again.
    let observation = WorldEngine::capture_observation(report);
    engine.ingest_staged_intent(&observation);
    engine.stage_facts(report);
}

fn stage_v4_ingested(engine: &mut WorldEngine, report: &WorldReportV4) {
    let observation = WorldEngine::capture_observation_v4(report);
    engine.ingest_staged_intent(&observation);
    engine.stage_facts_v4(report);
}

#[test]
fn restage_after_command_mutation_does_not_reingest() {
    let mut subject = engine();
    let mut report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        Vec::new(),
        "",
        None,
        None,
    );
    report.intent_records = vec![record("win-a", &float_true())];
    stage_ingested(&mut subject, &report);
    let win = WindowId("win-a".to_owned());
    assert!(subject.pending_intent.contains_key(&win));
    // Command mutation clears the imported intent and records its own
    // save, mirroring the sticky probes before their restage.
    subject.pending_intent.remove(&win);
    subject.intent_conflicts.remove(&win);
    let save = IntentData::new(Some(false), None);
    subject.sticky_saved.insert(win.clone(), save);
    // Restaging the same report (still carrying the import row) must
    // not re-add pending, latch a conflict, or disturb the command's
    // saved state.
    subject.stage_facts(&report);
    assert!(
        !subject.pending_intent.contains_key(&win),
        "restage must not reingest cleared intent"
    );
    assert!(!subject.intent_conflicts.contains(&win));
    assert_eq!(subject.sticky_saved.get(&win), Some(&save));
    // Classification still runs against the command-cleared ledgers:
    // with no pending float the fact classifies tiled.
    assert!(
        !subject
            .staged
            .derived
            .get(&win)
            .expect("classified")
            .floating
    );
}

/// Classification-only restage parity: `restage_classification` rebuilds
/// exactly what the intake staged (diagnostics, homing, facts, derived)
/// from the retained effective rows, conserves the raw/effective
/// distinction (raw keeps the original conflicted row, effective the
/// capability-graded hold), and never reingests intent the command side
/// already cleared. Managed-gated host so the v4 leg actually grades.
#[test]
fn restage_classification_matches_intake_and_conserves_effective() {
    for v4 in [false, true] {
        let mut engine = WorldEngine::new_test_bound_with_capabilities(
            "gen-1",
            "adapter-1",
            HostCapabilities {
                fullscreen: true,
                maximize: true,
                native_sticky: true,
                above: true,
                unavailable_facts: vec![crate::connection::FactName::Managed],
                ..Default::default()
            },
        )
        .expect("generation");
        let (mut legacy, mut v4report) =
            pair_reports(1, vec![fact("win-a", "out-a", 0)], Vec::new());
        legacy.intent_records = vec![record("win-a", &float_true())];
        v4report.intent_records = vec![record("win-a", &float_true())];
        stage_either(&mut engine, v4, &legacy, &v4report);
        let win = WindowId("win-a".to_owned());
        // Raw/effective conservation straight from intake.
        assert_eq!(engine.staged.observation.raw_windows.len(), 1);
        if v4 {
            assert!(
                engine.staged.effective_windows.is_empty(),
                "graded conflict leaves no effective row (v4={v4})"
            );
            assert!(
                engine
                    .staged
                    .held_windows
                    .iter()
                    .any(|held| held.window == win),
                "graded conflict retained as held (v4={v4})"
            );
        } else {
            assert_eq!(
                engine.staged.effective_windows, engine.staged.observation.raw_windows,
                "legacy classifies verbatim (v4={v4})"
            );
        }
        // Exactly-once ingestion at intake.
        assert!(engine.pending_intent.contains_key(&win));
        let diagnostics = engine.staged.diagnostics.clone();
        let homing = engine.staged.homing.clone();
        let effective = engine.staged.effective_windows.clone();
        let held_windows = engine.staged.held_windows.clone();
        let facts = engine.staged.facts.clone();
        let derived = engine.staged.derived.clone();
        // Clean restage first: with untouched ledgers the rebuild must
        // reproduce the intake exactly.
        engine.restage_classification();
        assert_eq!(
            engine.staged.diagnostics, diagnostics,
            "diagnostics parity (v4={v4})"
        );
        assert_eq!(engine.staged.homing, homing, "homing parity (v4={v4})");
        assert_eq!(
            engine.staged.effective_windows, effective,
            "effective parity (v4={v4})"
        );
        assert_eq!(
            engine.staged.held_windows, held_windows,
            "held parity (v4={v4})"
        );
        assert_eq!(engine.staged.facts, facts, "facts parity (v4={v4})");
        assert_eq!(engine.staged.derived, derived, "derived parity (v4={v4})");
        assert!(
            engine.pending_intent.contains_key(&win),
            "clean restage keeps ingested intent (v4={v4})"
        );
        // Command-side mutation mirrors the sticky probes: clear the
        // imported intent, record the command's own save. The restage
        // must follow the cleared ledgers (no pending float classifies
        // tiled) and must not resurrect the cleared import row.
        engine.pending_intent.remove(&win);
        engine.intent_conflicts.remove(&win);
        let save = IntentData::new(Some(false), None);
        engine.sticky_saved.insert(win.clone(), save);
        engine.restage_classification();
        if v4 {
            assert!(
                engine
                    .staged
                    .held_windows
                    .iter()
                    .any(|held| held.window == win),
                "restage retains the graded hold (v4={v4})"
            );
            assert!(
                !engine.staged.facts.contains_key(&win),
                "held conflict stages no fact (v4={v4})"
            );
        } else {
            assert!(
                !engine
                    .staged
                    .derived
                    .get(&win)
                    .expect("classified")
                    .floating,
                "restage follows cleared ledgers (v4={v4})"
            );
        }
        assert!(
            !engine.pending_intent.contains_key(&win),
            "restage must not reingest cleared intent (v4={v4})"
        );
        assert!(!engine.intent_conflicts.contains(&win));
        assert_eq!(engine.sticky_saved.get(&win), Some(&save));
    }
}

/// Every raw observation lane with at least two distinct rows per
/// order-sensitive lane. Shared by both intake constructors so the two
/// boundaries cannot drift.
struct RichLanes {
    windows: Vec<WindowFact>,
    closed: Vec<WindowId>,
    native: Vec<NativeObservation>,
    held_screens: Vec<HeldScreenRow>,
    held_windows: Vec<HeldWindowRow>,
    held_native: Vec<HeldNativeRow>,
    held_attempts: Vec<HeldAttemptRow>,
    cursor: Option<(i32, i32)>,
    cursor_hit: Option<WindowId>,
    focused: String,
    action_foreground: Option<OutputId>,
    action_cached: Option<OutputId>,
    action_outputs: Vec<OutputId>,
    primary: Option<OutputId>,
    gesture: Option<GestureIngress>,
    monotonic: u64,
    frames: Vec<FrameAttempt>,
    member: Vec<MemberAttempt>,
    view: Vec<ViewAttempt>,
    clear: Vec<ClearAttempt>,
    creation: Vec<NativeCreationAttempt>,
    state: Vec<StateAttempt>,
    feedback: Vec<FocusFeedback>,
    records: Vec<IntentRecord>,
}

fn rich_lanes() -> RichLanes {
    RichLanes {
        windows: vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        closed: vec![
            WindowId("win-gone".to_owned()),
            WindowId("win-old".to_owned()),
        ],
        native: vec![native_row("out-a"), native_row("out-b")],
        // Held ordinals retain original report positions: valid rows
        // precede held rows in each lane. Valid and held native rows
        // share one per-output namespace, so with three screens the two
        // valid rows take out-a/out-b and the single held row takes
        // out-c; every other held lane witnesses two-row ordering.
        held_screens: vec![held_screen_row("out-h1", 3), held_screen_row("out-h2", 4)],
        held_windows: vec![
            held_window_row("win-h1", "out-a", 2),
            held_window_row("win-h2", "out-b", 3),
        ],
        held_native: vec![held_native_row("out-c", 2)],
        // Held attempt ordinals index their own lane array: two valid
        // frame/member rows precede one held row each, so ordinal 2.
        held_attempts: vec![held_frame_row(2), held_member_row(2)],
        cursor: Some((10, 20)),
        cursor_hit: Some(WindowId("win-a".to_owned())),
        focused: "win-a".to_owned(),
        action_foreground: Some(OutputId("out-a".to_owned())),
        action_cached: Some(OutputId("out-b".to_owned())),
        action_outputs: vec![
            OutputId("out-a".to_owned()),
            OutputId("out-b".to_owned()),
            OutputId("out-c".to_owned()),
        ],
        primary: Some(OutputId("out-a".to_owned())),
        gesture: Some(gesture_row()),
        monotonic: 1234,
        frames: vec![frame_row("win-a"), frame_row("win-b")],
        member: vec![member_row("op-11", "win-a"), member_row("op-12", "win-b")],
        view: vec![view_row("op-view-1"), view_row("op-view-2")],
        clear: vec![clear_row("op-3", "win-a"), clear_row("op-4", "win-b")],
        creation: vec![
            creation_row("op-native-1", "ws-1", "out-a"),
            creation_row("op-native-2", "ws-2", "out-b"),
        ],
        state: vec![
            state_row("win-a", StateProperty::Sticky),
            state_row("win-b", StateProperty::Maximized),
        ],
        feedback: vec![feedback_row("ff-1", "win-a"), feedback_row("ff-2", "win-b")],
        records: vec![
            record("win-a", &float_true()),
            record("win-b", &tile_false()),
        ],
    }
}

/// Exact conservation: every capture field equals the source lane.
/// Whole-vector equality fails on any omission or order reversal; every
/// order-sensitive lane carries at least two distinct rows.
fn assert_conservation(captured: &StagedObservation, lanes: &RichLanes, seq: u64) {
    assert!(captured.inventory_complete);
    assert_eq!(captured.observation_seq, seq);
    assert_eq!(captured.raw_windows, lanes.windows);
    assert_eq!(
        captured
            .raw_windows
            .iter()
            .map(|fact| fact.window.0.clone())
            .collect::<Vec<_>>(),
        vec!["win-a".to_owned(), "win-b".to_owned()]
    );
    assert_eq!(captured.closed, lanes.closed);
    assert_eq!(
        captured
            .closed
            .iter()
            .map(|id| id.0.clone())
            .collect::<Vec<_>>(),
        vec!["win-gone".to_owned(), "win-old".to_owned()]
    );
    assert_eq!(captured.native, lanes.native);
    assert_eq!(
        captured
            .native
            .iter()
            .map(|row| row.output.0.clone())
            .collect::<Vec<_>>(),
        vec!["out-a".to_owned(), "out-b".to_owned()]
    );
    assert_eq!(captured.cursor, lanes.cursor);
    assert_eq!(captured.cursor_hit, lanes.cursor_hit);
    assert_eq!(captured.focused_global, lanes.focused);
    assert_eq!(captured.action_foreground, lanes.action_foreground);
    assert_eq!(captured.action_cached, lanes.action_cached);
    assert_eq!(captured.action_outputs, lanes.action_outputs);
    assert_eq!(
        captured
            .action_outputs
            .iter()
            .map(|out| out.0.clone())
            .collect::<Vec<_>>(),
        vec!["out-a".to_owned(), "out-b".to_owned(), "out-c".to_owned()]
    );
    assert_eq!(captured.primary_output, lanes.primary);
    assert_eq!(captured.gesture, lanes.gesture);
    assert_eq!(captured.monotonic_ms, lanes.monotonic);
    assert_eq!(captured.frames, lanes.frames);
    assert_eq!(
        captured
            .frames
            .iter()
            .map(|row| row.window.0.clone())
            .collect::<Vec<_>>(),
        vec!["win-a".to_owned(), "win-b".to_owned()]
    );
    assert_eq!(captured.member_attempts, lanes.member);
    assert_eq!(
        captured
            .member_attempts
            .iter()
            .map(|row| row.operation_id.clone())
            .collect::<Vec<_>>(),
        vec!["op-11".to_owned(), "op-12".to_owned()]
    );
    assert_eq!(captured.view_attempts, lanes.view);
    assert_eq!(
        captured
            .view_attempts
            .iter()
            .map(|row| row.operation_id.clone())
            .collect::<Vec<_>>(),
        vec!["op-view-1".to_owned(), "op-view-2".to_owned()]
    );
    assert_eq!(captured.clear_attempts, lanes.clear);
    assert_eq!(
        captured
            .clear_attempts
            .iter()
            .map(|row| row.operation_id.clone())
            .collect::<Vec<_>>(),
        vec!["op-3".to_owned(), "op-4".to_owned()]
    );
    assert_eq!(captured.creation_attempts, lanes.creation);
    assert_eq!(
        captured
            .creation_attempts
            .iter()
            .map(|row| row.operation_id.clone())
            .collect::<Vec<_>>(),
        vec!["op-native-1".to_owned(), "op-native-2".to_owned()]
    );
    assert_eq!(captured.state_attempts, lanes.state);
    assert_eq!(
        captured
            .state_attempts
            .iter()
            .map(|row| row.window.0.clone())
            .collect::<Vec<_>>(),
        vec!["win-a".to_owned(), "win-b".to_owned()]
    );
    assert_eq!(captured.focus_feedback, lanes.feedback);
    assert_eq!(
        captured
            .focus_feedback
            .iter()
            .map(|row| row.id.clone())
            .collect::<Vec<_>>(),
        vec!["ff-1".to_owned(), "ff-2".to_owned()]
    );
    assert_eq!(captured.intent_records, lanes.records);
    assert_eq!(
        captured
            .intent_records
            .iter()
            .map(|row| row.win.0.clone())
            .collect::<Vec<_>>(),
        vec!["win-a".to_owned(), "win-b".to_owned()]
    );
}

/// A legacy report carrying the shared rich lanes.
fn rich_legacy(seq: u64, lanes: RichLanes) -> WorldReport {
    let mut report = report_full(
        seq,
        seq,
        vec![
            screen("out-a", 0),
            screen("out-b", 800),
            screen("out-c", 1600),
        ],
        lanes.windows,
        vec![("out-a", "ws-1")],
        &lanes.focused,
        None,
        None,
    );
    report.held_screens = lanes.held_screens;
    report.held_windows = lanes.held_windows;
    report.closed = lanes.closed;
    report.native = lanes.native;
    report.cursor = lanes.cursor;
    report.cursor_hit = lanes.cursor_hit;
    report.action_foreground = lanes.action_foreground;
    report.action_cached = lanes.action_cached;
    report.action_outputs = lanes.action_outputs;
    report.primary_output = lanes.primary;
    report.gesture = lanes.gesture;
    report.monotonic_ms = lanes.monotonic;
    report.frames = lanes.frames;
    report.member_attempts = lanes.member;
    report.view_attempts = lanes.view;
    report.clear_attempts = lanes.clear;
    report.creation_attempts = lanes.creation;
    report.state_attempts = lanes.state;
    report.focus_feedback = lanes.feedback;
    report.intent_records = lanes.records;
    report
}

/// A v4 report carrying the shared rich lanes plus the v4-only rows.
/// Renderer input requires a carried command, so the report carries a
/// bare by-id focus (valid without a direction); staging ignores it.
fn rich_v4(seq: u64, lanes: RichLanes) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq: Some(9),
        command: Some(crate::world_report::WorldCommand::Focus {
            window: WindowId("win-a".to_owned()),
            direction: String::new(),
        }),
        screens: vec![
            screen("out-a", 0),
            screen("out-b", 800),
            screen("out-c", 1600),
        ],
        held_screens: lanes.held_screens,
        windows: lanes.windows,
        held_windows: lanes.held_windows,
        closed: lanes.closed,
        native: lanes.native,
        held_native: lanes.held_native,
        cursor: lanes.cursor,
        cursor_hit: lanes.cursor_hit,
        focused_global: lanes.focused,
        action_foreground: lanes.action_foreground,
        action_cached: lanes.action_cached,
        action_outputs: lanes.action_outputs,
        primary_output: lanes.primary,
        inventory_complete: true,
        gesture_barrier: false,
        gesture: lanes.gesture,
        monotonic_ms: lanes.monotonic,
        frames: lanes.frames,
        member_attempts: lanes.member,
        view_attempts: lanes.view,
        clear_attempts: lanes.clear,
        creation_attempts: lanes.creation,
        held_attempts: lanes.held_attempts,
        state_attempts: lanes.state,
        focus_feedback: lanes.feedback,
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: lanes.records,
        renderer_input: Some(RendererInput {
            rev: 7,
            target: RenderTarget::Window {
                out: OutputId("out-a".to_owned()),
                ws: WorkspaceId("ws-1".to_owned()),
                win: WindowId("win-a".to_owned()),
            },
        }),
    }
}

fn diagnostic_lanes(staged: &crate::world_engine::StagedReport) -> Vec<RowDiagnosticLane> {
    staged
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.row)
        .collect()
}

#[test]
fn legacy_capture_conserves_raw_lanes_in_order() {
    let mut subject = engine();
    let lanes = rich_lanes();
    let report = rich_legacy(3, lanes);
    let expected = rich_lanes();
    stage_ingested(&mut subject, &report);
    assert_conservation(&subject.staged.observation, &expected, 3);
    // Ordered valid screens stay staged alongside the capture.
    assert_eq!(subject.staged.screens, report.screens);
    // Held screens and windows stage in report order with retained
    // ordinals and diagnostics; the legacy intake never fills the
    // v4-only held lanes.
    assert_eq!(subject.staged.held_screens, expected.held_screens);
    assert_eq!(
        subject
            .staged
            .held_screens
            .iter()
            .map(|held| held.ordinal)
            .collect::<Vec<_>>(),
        vec![3, 4]
    );
    assert_eq!(subject.staged.held_windows.len(), 2);
    // Scoped held rows resolve their diagnostic scope at staging;
    // order, identity, ordinal, and output stay verbatim.
    assert_eq!(
        subject
            .staged
            .held_windows
            .iter()
            .map(|held| (held.window.0.clone(), held.ordinal, held.output.clone()))
            .collect::<Vec<_>>(),
        vec![
            ("win-h1".to_owned(), 2, Some(OutputId("out-a".to_owned()))),
            ("win-h2".to_owned(), 3, Some(OutputId("out-b".to_owned()))),
        ]
    );
    assert_eq!(
        subject
            .staged
            .held_windows
            .iter()
            .map(|held| held.diagnostic.clone().expect("resolved scope").scope)
            .collect::<Vec<_>>(),
        vec![
            crate::world_report::AuthorityScope::Output(OutputId("out-a".to_owned())),
            crate::world_report::AuthorityScope::Output(OutputId("out-b".to_owned())),
        ]
    );
    assert!(subject.staged.held_native.is_empty());
    assert!(subject.staged.held_attempts.is_empty());
    // Diagnostics follow lane order: held screens then held windows.
    // Both attempt lanes are capability-supported, so no offroute rows.
    assert_eq!(
        diagnostic_lanes(&subject.staged),
        vec![
            RowDiagnosticLane::Screen,
            RowDiagnosticLane::Screen,
            RowDiagnosticLane::Window,
            RowDiagnosticLane::Window,
        ]
    );
    // Same-report intent ingestion feeds classification for both
    // recorded subjects.
    assert!(
        subject
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        subject
            .pending_intent
            .contains_key(&WindowId("win-b".to_owned()))
    );
    assert!(
        subject
            .staged
            .derived
            .get(&WindowId("win-a".to_owned()))
            .expect("classified")
            .floating,
        "same-report pending float must classify floating"
    );
}

#[test]
fn v4_capture_conserves_raw_lanes_in_order() {
    let mut subject = engine();
    let lanes = rich_lanes();
    let report = rich_v4(5, lanes);
    // The rich report must stay genuinely valid: unrelated report
    // invalidity refuses grading, which would hide a lane regression
    // behind an unchanged staging.
    assert!(
        crate::world_report_v4_checks::validate_report_v4_reason(&report).is_none(),
        "rich v4 report must validate"
    );
    let expected = rich_lanes();
    stage_v4_ingested(&mut subject, &report);
    assert_conservation(&subject.staged.observation, &expected, 5);
    assert_eq!(subject.staged.screens, report.screens);
    // Every held lane stages verbatim in report order with retained
    // ordinals and diagnostics.
    assert_eq!(subject.staged.held_screens, expected.held_screens);
    // Scoped held rows resolve their diagnostic scope at staging;
    // order, identity, ordinal, and output stay verbatim.
    assert_eq!(
        subject
            .staged
            .held_windows
            .iter()
            .map(|held| (held.window.0.clone(), held.ordinal, held.output.clone()))
            .collect::<Vec<_>>(),
        vec![
            ("win-h1".to_owned(), 2, Some(OutputId("out-a".to_owned()))),
            ("win-h2".to_owned(), 3, Some(OutputId("out-b".to_owned()))),
        ]
    );
    assert_eq!(
        subject
            .staged
            .held_windows
            .iter()
            .map(|held| held.diagnostic.clone().expect("resolved scope").scope)
            .collect::<Vec<_>>(),
        vec![
            crate::world_report::AuthorityScope::Output(OutputId("out-a".to_owned())),
            crate::world_report::AuthorityScope::Output(OutputId("out-b".to_owned())),
        ]
    );
    assert_eq!(subject.staged.held_native, expected.held_native);
    assert_eq!(
        subject
            .staged
            .held_native
            .iter()
            .map(|held| (held.output.0.clone(), held.ordinal))
            .collect::<Vec<_>>(),
        vec![("out-c".to_owned(), 2)]
    );
    assert_eq!(subject.staged.held_attempts, expected.held_attempts);
    assert_eq!(
        subject
            .staged
            .held_attempts
            .iter()
            .map(|held| (held.lane, held.ordinal))
            .collect::<Vec<_>>(),
        vec![(AttemptLaneV4::Frame, 2), (AttemptLaneV4::Member, 2)]
    );
    // Diagnostics follow lane order: screens, windows, the native row,
    // then the attempt row carrying a diagnostic (the diagnostic-less
    // frame row emits nothing).
    assert_eq!(
        diagnostic_lanes(&subject.staged),
        vec![
            RowDiagnosticLane::Screen,
            RowDiagnosticLane::Screen,
            RowDiagnosticLane::Window,
            RowDiagnosticLane::Window,
            RowDiagnosticLane::Native,
            RowDiagnosticLane::Attempt,
        ]
    );
    // v4-only rows stay verbatim with exact content, and echoes stay
    // empty (v4 carries no views).
    assert_eq!(
        subject.staged.presentation_v4,
        Some(report.presentation.clone())
    );
    assert_eq!(subject.staged.renderer_input, report.renderer_input);
    assert!(subject.staged.echoes.is_empty());
    // Same-report intent ingestion feeds classification on v4 too.
    assert!(
        subject
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        subject
            .pending_intent
            .contains_key(&WindowId("win-b".to_owned()))
    );
    assert!(
        subject
            .staged
            .derived
            .get(&WindowId("win-a".to_owned()))
            .expect("classified")
            .floating,
        "same-report pending float must classify floating on v4"
    );
}

/// Rich v4 restage: the full lane set survives `restage_classification`.
/// One live row, one capability-graded hold, and one own-process row
/// prove raw/effective separation; held/source lanes, v4-only rows, and
/// the exact lane diagnostic order are retained; a clean restage reproduces
/// the whole staged report; repeated build/restage/build never accumulates
/// the use-time scale hold.
#[test]
fn v4_restage_conserves_rich_lanes_without_accumulating_scale_hold() {
    let mut engine = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            native_sticky: true,
            maximize: true,
            above: true,
            unavailable_facts: vec![crate::connection::FactName::Managed],
            ..Default::default()
        },
    )
    .expect("generation");
    // win-a stays live, win-b grades to held on the managed conflict,
    // win-own exercises the own-process drop with no grading conflict.
    let mut windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)];
    windows[0].managed = None;
    let mut own = fact("win-own", "out-a", 400);
    own.process_id = Some("test-own-pid".to_owned());
    own.managed = None;
    windows.insert(1, own);
    let mut states = vec![
        state_row("win-a", StateProperty::Sticky),
        state_row("win-b", StateProperty::Maximized),
    ];
    // Fullscreen is unsupported under these caps, so this attempt stages
    // the offroute diagnostic while the supported lanes stay silent.
    states.push(state_row("win-a", StateProperty::Fullscreen));
    let mut lanes = rich_lanes();
    let expected = RichLanes {
        windows: windows.clone(),
        state: states.clone(),
        ..rich_lanes()
    };
    lanes.windows = windows;
    lanes.state = states;
    let mut report = rich_v4(7, lanes);
    report.screens[0].scale = 1e308;
    stage_v4_ingested(&mut engine, &report);
    let win_a = WindowId("win-a".to_owned());
    let win_b = WindowId("win-b".to_owned());
    let win_own = WindowId("win-own".to_owned());
    // Raw capture keeps every row in source order.
    assert_eq!(engine.staged.observation.raw_windows, expected.windows);
    assert_eq!(engine.staged.observation.closed, expected.closed);
    assert_eq!(engine.staged.observation.intent_records, expected.records);
    // Effective rows keep source order minus the graded hold; the
    // own-process row is dropped only at classification, never here.
    assert_eq!(
        engine
            .staged
            .effective_windows
            .iter()
            .map(|fact| fact.window.0.clone())
            .collect::<Vec<_>>(),
        vec!["win-a".to_owned(), "win-own".to_owned()]
    );
    // The graded row stays held with its conflict diagnostic.
    let graded = engine
        .staged
        .held_windows
        .iter()
        .find(|held| held.window == win_b)
        .expect("graded hold retained");
    assert_eq!(graded.kind, RowDiagnosticKind::ConflictingFact);
    assert_eq!(
        graded.diagnostic.clone().expect("graded scope").id,
        Some("win-b".to_owned())
    );
    // Classification drops the own-process row into the excluded set.
    assert!(!engine.staged.facts.contains_key(&win_own));
    assert!(engine.staged.excluded.contains(&win_own));
    assert!(engine.staged.facts.contains_key(&win_a));
    // Held/source lanes and v4-only rows stay verbatim.
    assert_eq!(engine.staged.held_native, expected.held_native);
    assert_eq!(engine.staged.held_attempts, expected.held_attempts);
    assert_eq!(
        engine.staged.presentation_v4,
        Some(report.presentation.clone())
    );
    assert_eq!(engine.staged.renderer_input, report.renderer_input);
    // Exact lane diagnostic order: screens, windows (source-held then
    // graded), native, attempt with a diagnostic (the diagnostic-less
    // frame row emits nothing), unsupported fullscreen attempt.
    assert_eq!(
        diagnostic_lanes(&engine.staged),
        vec![
            RowDiagnosticLane::Screen,
            RowDiagnosticLane::Screen,
            RowDiagnosticLane::Window,
            RowDiagnosticLane::Window,
            RowDiagnosticLane::Window,
            RowDiagnosticLane::Native,
            RowDiagnosticLane::Attempt,
            RowDiagnosticLane::Attempt,
        ]
    );
    // A clean restage reproduces the whole staged report.
    let before = engine.staged.clone();
    engine.restage_classification();
    assert_eq!(engine.staged, before);
    // A held singleton retains its row and appends its diagnostic last.
    report.presentation = PresentationRowV4::Held(crate::world_report::HeldPresentationRow {
        fields: Vec::new(),
        kind: RowDiagnosticKind::InvalidFact,
        diagnostic: Some(crate::world_report::RowDiagnostic {
            scope: crate::world_report::AuthorityScope::Output(OutputId("out-a".to_owned())),
            row: RowDiagnosticLane::Presentation,
            id: None,
            kind: RowDiagnosticKind::InvalidFact,
        }),
    });
    stage_v4_ingested(&mut engine, &report);
    let before = engine.staged.clone();
    engine.restage_classification();
    assert_eq!(engine.staged, before);
    assert_eq!(
        diagnostic_lanes(&engine.staged).last(),
        Some(&RowDiagnosticLane::Presentation)
    );
    // Use-time scale hold: one overflowing output stages exactly one
    // diagnostic across repeated build/restage/build cycles.
    engine
        .mapping
        .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
    let scale_diags = |engine: &WorldEngine| {
        engine
            .staged
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.row == RowDiagnosticLane::Screen
                    && diagnostic.kind == RowDiagnosticKind::Unreadable
                    && diagnostic.id.as_deref() == Some("out-a")
            })
            .count()
    };
    engine.build_staged_domains();
    assert_eq!(scale_diags(&engine), 1);
    engine.build_staged_domains();
    assert_eq!(scale_diags(&engine), 1);
    engine.restage_classification();
    assert_eq!(scale_diags(&engine), 0);
    engine.build_staged_domains();
    assert_eq!(scale_diags(&engine), 1);
}

#[test]
fn raw_keeps_excluded_subjects_apart_from_classified_facts() {
    let mut subject = engine();
    let reply = subject.handshake(&crate::connection::HandshakeRequest {
        adapter: "adapter-1".to_owned(),
        request: "handshake-1".to_owned(),
        application_identity: crate::connection::ApplicationIdentity {
            app_id: None,
            executable: Some("synthetic.exe".to_owned()),
            process_id: Some("pid-own-1".to_owned()),
        },
        capabilities: HostCapabilities {
            native_sticky: true,
            maximize: true,
            ..HostCapabilities::default()
        },
    });
    assert!(reply.kind.is_none(), "handshake binds");
    let mut own_fact = fact("win-own", "out-a", 0);
    own_fact.process_id = Some("pid-own-1".to_owned());
    let mut safety_fact = fact("win-risky", "out-a", 40);
    safety_fact.managed = Some(false);
    let mut report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 80), own_fact, safety_fact],
        Vec::new(),
        "",
        None,
        None,
    );
    // An intent row for the own-process subject must never import.
    report.intent_records = vec![
        record("win-own", &float_true()),
        record("win-a", &float_true()),
    ];
    stage_ingested(&mut subject, &report);
    let captured = &subject.staged.observation;
    // Raw capture keeps every typed row, including excluded subjects.
    assert_eq!(
        captured
            .raw_windows
            .iter()
            .map(|fact| fact.window.0.clone())
            .collect::<Vec<_>>(),
        vec![
            "win-a".to_owned(),
            "win-own".to_owned(),
            "win-risky".to_owned()
        ]
    );
    // Classification drops own-process facts but keeps safety-excluded
    // facts staged under the exclusion ledger.
    assert!(
        subject
            .staged
            .facts
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !subject
            .staged
            .facts
            .contains_key(&WindowId("win-own".to_owned()))
    );
    assert!(
        subject
            .staged
            .facts
            .contains_key(&WindowId("win-risky".to_owned()))
    );
    assert!(
        subject
            .staged
            .excluded
            .contains(&WindowId("win-own".to_owned()))
    );
    assert!(
        subject
            .staged
            .excluded
            .contains(&WindowId("win-risky".to_owned()))
    );
    // Intent imports the managed subject only.
    assert!(
        subject
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !subject
            .pending_intent
            .contains_key(&WindowId("win-own".to_owned()))
    );
}

#[test]
fn capture_mutates_no_retained_routing() {
    let mut subject = engine();
    let seed = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        vec![("out-a", "ws-1")],
        "win-a",
        None,
        None,
    );
    accepted(&mut subject, &seed);
    let mapping = subject.mapping.clone();
    let views = subject.views.clone();
    let members = subject.members.clone();
    let prior_homing = subject.staged.homing.clone();
    assert!(!prior_homing.is_empty());
    // Raw capture alone is pure: no retained mutation, no homing change.
    let observation = WorldEngine::capture_observation(&rich_legacy(2, rich_lanes()));
    assert_eq!(subject.mapping, mapping);
    assert_eq!(subject.views, views);
    assert!(subject.staged.homing == prior_homing);
    assert_eq!(observation.observation_seq, 2);
    // Full staging replaces the staged view but keeps retained routing.
    // Classification-only staging never ingests: the rich report's
    // import rows leave no ledger trace here.
    subject.stage_facts(&rich_legacy(2, rich_lanes()));
    assert_eq!(subject.mapping, mapping);
    assert_eq!(subject.views, views);
    assert_eq!(subject.members, members);
    assert!(subject.pending_intent.is_empty());
}

#[test]
fn held_scope_keeps_prior_staged_homing() {
    let mut subject = engine();
    let seed = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-keep", "out-a", 0)],
        vec![("out-a", "ws-1")],
        "",
        None,
        None,
    );
    accepted(&mut subject, &seed);
    let home = subject
        .staged
        .homing
        .get(&WindowId("win-keep".to_owned()))
        .expect("seeded home")
        .clone();
    // The follow-up report carries the kept lifetime as an unscoped held
    // row only: the resolved scope must reuse the prior staged home.
    let mut held = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        Vec::new(),
        vec![("out-a", "ws-1")],
        "",
        None,
        None,
    );
    held.held_windows = vec![crate::world_report::HeldWindowRow {
        window: WindowId("win-keep".to_owned()),
        ordinal: 0,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: crate::world_report::RowDiagnosticKind::InvalidFact,
    }];
    subject.stage_facts(&held);
    let resolved = subject
        .staged
        .held_windows
        .iter()
        .find(|held| held.window.0 == "win-keep")
        .expect("held row staged");
    let scope = resolved.diagnostic.clone().expect("resolved scope").scope;
    assert_eq!(
        scope,
        crate::world_report::AuthorityScope::Output(home.output.clone())
    );
}

#[test]
fn same_report_intent_shapes_classification() {
    // Without an intent row the synthetic fact classifies tiled.
    let mut plain = engine();
    plain.stage_facts(&report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        Vec::new(),
        "",
        None,
        None,
    ));
    assert!(
        !plain
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !plain
            .staged
            .derived
            .get(&WindowId("win-a".to_owned()))
            .expect("classified")
            .floating
    );
    // The same report with an intent row ingests before classification,
    // so the pending float win is visible to the classifier immediately.
    let mut restored = engine();
    let mut report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        Vec::new(),
        "",
        None,
        None,
    );
    report.intent_records = vec![record("win-a", &float_true())];
    stage_ingested(&mut restored, &report);
    assert!(
        restored
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        restored
            .staged
            .derived
            .get(&WindowId("win-a".to_owned()))
            .expect("classified")
            .floating,
        "same-report pending float must classify floating"
    );
}

#[test]
fn intent_pending_survives_hold_cleared_by_close_and_late_fenced() {
    let mut subject = engine();
    let mut open = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        Vec::new(),
        "",
        None,
        None,
    );
    open.intent_records = vec![record("win-a", &float_true())];
    stage_ingested(&mut subject, &open);
    assert!(
        subject
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    // Pending survives a report where the lifetime is only a held row.
    let mut held = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        Vec::new(),
        Vec::new(),
        "",
        None,
        None,
    );
    held.held_windows = vec![crate::world_report::HeldWindowRow {
        window: WindowId("win-a".to_owned()),
        ordinal: 0,
        output: Some(OutputId("out-a".to_owned())),
        fields: Vec::new(),
        diagnostic: None,
        kind: crate::world_report::RowDiagnosticKind::InvalidFact,
    }];
    subject.stage_facts(&held);
    assert!(
        subject
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    // An admitted lifetime ignores late rows forever.
    subject.intent_admitted.insert(WindowId("win-a".to_owned()));
    let mut late = report_full(
        3,
        3,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        Vec::new(),
        "",
        None,
        None,
    );
    late.intent_records = vec![record("win-a", &tile_false())];
    stage_ingested(&mut subject, &late);
    assert!(
        subject
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !subject
            .intent_conflicts
            .contains(&WindowId("win-a".to_owned()))
    );
    // An explicit close ends the lifetime and drops the ledgers.
    let mut closed = report_full(
        4,
        4,
        vec![screen("out-a", 0)],
        Vec::new(),
        Vec::new(),
        "",
        None,
        None,
    );
    closed.closed = vec![WindowId("win-a".to_owned())];
    stage_ingested(&mut subject, &closed);
    assert!(
        !subject
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
    assert!(
        !subject
            .intent_admitted
            .contains(&WindowId("win-a".to_owned()))
    );
    assert_eq!(
        subject.staged.observation.closed,
        vec![WindowId("win-a".to_owned())]
    );
}

#[test]
fn v4_capture_keeps_original_rows_through_capability_grading() {
    // Default capabilities bind no native state support, so the sticky
    // fact grades to a narrow conflicting hold. Unrelated report
    // invalidity refuses grading; grading keeps the native hit in the raw
    // capture but clears a held target on the effective copy, so every
    // lane here references the ungraded survivor lifetime only.
    let mut subject = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities::default(),
    )
    .expect("generation");
    let mut sticky = fact("win-a", "out-a", 0);
    sticky.sticky = Some(true);
    // The ungraded lifetime carries null sticky/maximized: under these
    // capabilities any non-null value grades, so only the sticky row
    // must move.
    let mut plain = fact("win-b", "out-a", 40);
    plain.sticky = None;
    plain.maximized = None;
    let report = WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: 1,
        request_seq: 1,
        request_id: "req-1".to_owned(),
        command_seq: None,
        command: None,
        screens: vec![screen("out-a", 0)],
        held_screens: Vec::new(),
        windows: vec![sticky, plain],
        held_windows: Vec::new(),
        closed: Vec::new(),
        native: Vec::new(),
        held_native: Vec::new(),
        cursor: Some((10, 20)),
        cursor_hit: Some(WindowId("win-b".to_owned())),
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: vec![frame_row("win-b")],
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: vec![feedback_row("ff-1", "win-b")],
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: vec![record("win-a", &float_true())],
        renderer_input: None,
    };
    assert!(
        crate::world_report_v4_checks::validate_report_v4_reason(&report).is_none(),
        "grading probe must validate"
    );
    let original_windows = report.windows.clone();
    stage_v4_ingested(&mut subject, &report);
    // Raw capture keeps the original typed rows in source order even
    // though grading moves one row into the held set.
    assert_eq!(subject.staged.observation.raw_windows, original_windows);
    assert_eq!(
        subject
            .staged
            .observation
            .raw_windows
            .iter()
            .map(|fact| fact.window.0.clone())
            .collect::<Vec<_>>(),
        vec!["win-a".to_owned(), "win-b".to_owned()]
    );
    assert!(
        subject
            .staged
            .held_windows
            .iter()
            .any(|held| held.window.0 == "win-a"),
        "conflicting sticky grades to a held row"
    );
    assert!(
        subject
            .staged
            .diagnostics
            .iter()
            .any(
                |diagnostic| diagnostic.kind == RowDiagnosticKind::ConflictingFact
                    && diagnostic.id.as_deref() == Some("win-a")
            ),
        "graded hold emits its conflicting-fact diagnostic"
    );
    assert!(
        !subject
            .staged
            .facts
            .contains_key(&WindowId("win-a".to_owned())),
        "graded rows classify from the effective set, not the raw capture"
    );
    // Intent ingestion runs from the raw capture before grading, so the
    // graded-held subject still imports its record.
    assert!(
        subject
            .pending_intent
            .contains_key(&WindowId("win-a".to_owned()))
    );
}
// 4b-1: retained-authority lifetime pruning consumes the staged capture,
// never a report object. Both intakes drive every pruner; raw own-process
// and safety-excluded subjects stay lifetime evidence despite exclusion
// from control, while capability-held rows keep presence without authority.
fn insert_state_binding(engine: &mut WorldEngine, window: &str) {
    engine.retained_states.insert(
        (WindowId(window.to_owned()), StateProperty::Sticky),
        RetainedStateBinding {
            target: RetainedStateTarget {
                window: WindowId(window.to_owned()),
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
}

fn seed_lifetime_ledgers(engine: &mut WorldEngine, window: &str, output: &str) {
    let id = WindowId(window.to_owned());
    let home = DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    insert_state_binding(engine, window);
    engine.automatic_focus_tokens.insert(id.clone(), 7);
    engine.retained_focus.push(RetainedFocusGrant {
        effect: OneShotEffect::explicit_focus(3, id.clone(), 1),
        feedback: None,
        verdict: FocusVerdict::Pending,
    });
    engine.composed_maximized.insert(id.clone());
    engine.selection_targets.insert(
        id.clone(),
        SelectionTarget {
            home: home.clone(),
            shown: true,
            rev: 1,
        },
    );
    engine.selection_scopes.insert(
        OutputId(output.to_owned()),
        SelectionScope {
            selected: vec![home.clone()],
            departing: Vec::new(),
            rev: 1,
        },
    );
    engine.retained_selection.insert(
        OutputId(output.to_owned()),
        RetainedSelection {
            effect: Some(OneShotEffect::explicit_focus(3, id.clone(), 1)),
            parent_op: "op-3".to_owned(),
            home: home.clone(),
            rev: 1,
            obs: 1,
            members: vec![FrozenArrivalMember {
                window: id.clone(),
                home: home.clone(),
                frame: None,
            }],
            switch_members: vec![SwitchMember {
                window: id.clone(),
                home: home.clone(),
                shown: true,
                rev: 1,
            }],
            switch_evidence: BTreeMap::new(),
            feedback: None,
            foreground_proven: false,
            arrival_ready: false,
            view_targets: BTreeMap::new(),
            view_raw: BTreeMap::new(),
            selected: vec![home.clone()],
            affected: vec![OutputId(output.to_owned())],
        },
    );
}

fn assert_window_ledgers_hold(engine: &WorldEngine, window: &str) {
    let id = WindowId(window.to_owned());
    assert!(
        engine
            .retained_states
            .contains_key(&(id.clone(), StateProperty::Sticky)),
        "state binding survives"
    );
    assert!(
        engine.automatic_focus_tokens.contains_key(&id),
        "automatic-focus token survives"
    );
    assert!(
        engine
            .retained_focus
            .iter()
            .any(|grant| grant.effect.target == id),
        "focus grant survives"
    );
    assert!(
        engine.composed_maximized.contains(&id),
        "composed maximize survives"
    );
    assert!(
        engine.selection_targets.contains_key(&id),
        "selection target survives"
    );
    assert!(
        engine
            .retained_selection
            .values()
            .flat_map(|binding| binding
                .effect
                .as_ref()
                .map(|effect| effect.target.clone())
                .into_iter()
                .chain(binding.members.iter().map(|member| member.window.clone())))
            .any(|target| target == id),
        "selection grant survives"
    );
}

fn assert_window_ledgers_dropped(engine: &WorldEngine, window: &str) {
    let id = WindowId(window.to_owned());
    assert!(
        !engine
            .retained_states
            .contains_key(&(id.clone(), StateProperty::Sticky)),
        "state binding prunes on departure"
    );
    assert!(
        !engine.automatic_focus_tokens.contains_key(&id),
        "automatic-focus token prunes on departure"
    );
    assert!(
        engine
            .retained_focus
            .iter()
            .all(|grant| grant.effect.target != id),
        "no stale focus grant survives departure"
    );
    assert!(
        !engine.composed_maximized.contains(&id),
        "composed maximize prunes on departure"
    );
    assert!(
        !engine.selection_targets.contains_key(&id),
        "no stale selection target survives departure"
    );
    assert!(
        engine
            .retained_selection
            .values()
            .flat_map(|binding| binding
                .effect
                .as_ref()
                .map(|effect| effect.target.clone())
                .into_iter()
                .chain(binding.members.iter().map(|member| member.window.clone())))
            .all(|target| target != id),
        "no stale selection grant survives departure"
    );
}

fn report_v4_simple(
    seq: u64,
    screens: Vec<ScreenReport>,
    held_screens: Vec<HeldScreenRow>,
    windows: Vec<WindowFact>,
    held_windows: Vec<HeldWindowRow>,
    closed: Vec<WindowId>,
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
        gesture_barrier: false,
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

#[test]
fn lifetime_prune_drops_true_loss_on_both_intakes() {
    for v4 in [false, true] {
        let mut engine = engine();
        seed_lifetime_ledgers(&mut engine, "win-a", "out-a");
        if v4 {
            let report = report_v4_simple(
                1,
                vec![screen("out-a", 0)],
                Vec::new(),
                vec![fact("win-b", "out-a", 100)],
                Vec::new(),
                Vec::new(),
            );
            stage_v4_ingested(&mut engine, &report);
        } else {
            let report = report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-b", "out-a", 100)],
                Vec::new(),
                "",
                None,
                None,
            );
            stage_ingested(&mut engine, &report);
        }
        engine.prune_lifetime_state();
        assert_window_ledgers_dropped(&engine, "win-a");
        // The output itself stays live, so its scope survives: only true
        // output loss drops scopes, never member departure.
        assert!(
            engine
                .selection_scopes
                .contains_key(&OutputId("out-a".to_owned())),
            "live output scope survives member loss (v4={v4})"
        );
    }
}

#[test]
fn lifetime_prune_preserves_held_windows_and_held_screens_on_both_intakes() {
    for v4 in [false, true] {
        let mut engine = engine();
        seed_lifetime_ledgers(&mut engine, "win-a", "out-a");
        if v4 {
            let report = report_v4_simple(
                1,
                Vec::new(),
                vec![held_screen_row("out-a", 0)],
                Vec::new(),
                vec![held_window_row("win-a", "out-a", 0)],
                Vec::new(),
            );
            stage_v4_ingested(&mut engine, &report);
        } else {
            let mut report = report_full(1, 1, Vec::new(), Vec::new(), Vec::new(), "", None, None);
            report.held_screens = vec![held_screen_row("out-a", 0)];
            report.held_windows = vec![held_window_row("win-a", "out-a", 0)];
            stage_ingested(&mut engine, &report);
        }
        engine.prune_lifetime_state();
        assert_window_ledgers_hold(&engine, "win-a");
        assert!(
            engine
                .selection_scopes
                .contains_key(&OutputId("out-a".to_owned())),
            "held screen is never true output loss (v4={v4})"
        );
    }
}

#[test]
fn lifetime_prune_explicit_close_wins_over_carried_presence_on_both_intakes() {
    // Synthetic overlap: a carried live fact plus an explicit close for the
    // same id exercises the pruner-direct close-wins fence only, not codec
    // acceptance of close/live overlap.
    for v4 in [false, true] {
        let mut engine = engine();
        seed_lifetime_ledgers(&mut engine, "win-a", "out-a");
        if v4 {
            let report = report_v4_simple(
                1,
                vec![screen("out-a", 0)],
                Vec::new(),
                vec![fact("win-a", "out-a", 0)],
                Vec::new(),
                vec![WindowId("win-a".to_owned())],
            );
            stage_v4_ingested(&mut engine, &report);
        } else {
            let mut report = report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                Vec::new(),
                "",
                None,
                None,
            );
            report.closed = vec![WindowId("win-a".to_owned())];
            stage_ingested(&mut engine, &report);
        }
        engine.prune_lifetime_state();
        assert_window_ledgers_dropped(&engine, "win-a");
    }
}

#[test]
fn lifetime_prune_incomplete_gap_preserves_without_close() {
    let mut engine = engine();
    seed_lifetime_ledgers(&mut engine, "win-a", "out-a");
    let mut report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-b", "out-a", 100)],
        Vec::new(),
        "",
        None,
        None,
    );
    report.inventory_complete = false;
    stage_ingested(&mut engine, &report);
    engine.prune_lifetime_state();
    assert_window_ledgers_hold(&engine, "win-a");
    assert!(
        engine
            .selection_scopes
            .contains_key(&OutputId("out-a".to_owned())),
        "incomplete inventory never implies output loss"
    );
}

#[test]
fn lifetime_prune_own_process_raw_presence_prunes_without_inventory() {
    let mut engine = engine();
    insert_state_binding(&mut engine, "win-own");
    let mut own_fact = fact("win-own", "out-a", 0);
    own_fact.process_id = Some("test-own-pid".to_owned());
    let mut report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![own_fact],
        Vec::new(),
        "",
        None,
        None,
    );
    report.inventory_complete = false;
    stage_ingested(&mut engine, &report);
    // Classification drops the own-process subject, but the raw capture
    // keeps it: the own-process filter must read the raw capture, where a
    // classified-facts-only filter would find nothing on this incomplete
    // report and wrongly retain the binding.
    assert!(
        !engine
            .staged
            .facts
            .contains_key(&WindowId("win-own".to_owned())),
        "own-process subject classifies out of facts"
    );
    assert!(
        engine
            .staged
            .observation
            .raw_windows
            .iter()
            .any(|fact| fact.window.0 == "win-own"),
        "raw capture retains the own-process subject"
    );
    engine.prune_state_bindings();
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-own".to_owned()), StateProperty::Sticky)),
        "own-process binding prunes from raw evidence without inventory"
    );
}

#[test]
fn lifetime_prune_member_known_lifetime_survives_complete_absence() {
    let mut engine = engine();
    insert_state_binding(&mut engine, "win-a");
    engine.members.insert(
        WindowId("win-a".to_owned()),
        MemberState {
            home: DomainKey {
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
            },
            floating: false,
            minimized: false,
            geometry: None,
        },
    );
    let report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-b", "out-a", 100)],
        Vec::new(),
        "",
        None,
        None,
    );
    stage_ingested(&mut engine, &report);
    engine.prune_state_bindings();
    assert!(
        engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Sticky)),
        "engine/member-known lifetime survives complete absence"
    );
}

#[test]
fn lifetime_prune_uses_current_staged_capture() {
    let mut engine = engine();
    seed_lifetime_ledgers(&mut engine, "win-a", "out-a");
    let live = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        Vec::new(),
        "",
        None,
        None,
    );
    stage_ingested(&mut engine, &live);
    engine.prune_lifetime_state();
    assert_window_ledgers_hold(&engine, "win-a");
    // A later stage observing departure must prune: the helper reads the
    // current capture, never a cached older report.
    let gone = report_full(
        2,
        2,
        vec![screen("out-a", 0)],
        vec![fact("win-b", "out-a", 100)],
        Vec::new(),
        "",
        None,
        None,
    );
    stage_ingested(&mut engine, &gone);
    engine.prune_lifetime_state();
    assert_window_ledgers_dropped(&engine, "win-a");
}

fn home_key(output: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    }
}

fn seed_canonical_ledgers(engine: &mut WorldEngine, window: &str, output: &str) {
    let id = WindowId(window.to_owned());
    engine
        .last_active_window
        .insert(home_key(output), id.clone());
    engine
        .pending_intent
        .insert(id.clone(), IntentData::new(Some(true), None));
    engine
        .excluded_intent
        .insert(id.clone(), ExcludedIntent::default());
    engine.sticky_rollback_saves.insert(
        id.clone(),
        IntentWrite {
            win: id,
            data: None,
        },
    );
}

#[test]
fn lifetime_prune_canonical_and_intent_ledger_discriminators() {
    // Deliberate predicate differences on a held-only lifetime: canonical
    // last_active_window and excluded_intent need live (or excluded)
    // presence, while pending intent and rollback saves keep held rows.
    // Member knowledge saves neither canonical nor excluded ledgers.
    let mut engine = engine();
    seed_canonical_ledgers(&mut engine, "win-a", "out-a");
    engine.members.insert(
        WindowId("win-a".to_owned()),
        MemberState {
            home: home_key("out-a"),
            floating: false,
            minimized: false,
            geometry: None,
        },
    );
    let mut report = report_full(1, 1, Vec::new(), Vec::new(), Vec::new(), "", None, None);
    report.held_screens = vec![held_screen_row("out-a", 0)];
    report.held_windows = vec![held_window_row("win-a", "out-a", 0)];
    stage_ingested(&mut engine, &report);
    engine.prune_lifetime_state();
    let id = WindowId("win-a".to_owned());
    assert!(
        !engine.last_active_window.values().any(|held| held == &id),
        "canonical memory prunes held-only lifetimes"
    );
    assert!(
        engine.pending_intent.contains_key(&id),
        "pending intent retains held rows"
    );
    assert!(
        !engine.excluded_intent.contains_key(&id),
        "exclusion ledger keeps live/excluded only, never held-only"
    );
    assert!(
        engine.sticky_rollback_saves.contains_key(&id),
        "rollback saves retain held rows"
    );
}

#[test]
fn lifetime_prune_known_arm_saves_pending_only() {
    // Engine/member-known presence retains pending intent but neither
    // rollback saves nor the exclusion ledger: those two have no known
    // arm by design.
    let mut engine = engine();
    seed_canonical_ledgers(&mut engine, "win-a", "out-a");
    engine.members.insert(
        WindowId("win-a".to_owned()),
        MemberState {
            home: home_key("out-a"),
            floating: false,
            minimized: false,
            geometry: None,
        },
    );
    let report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-b", "out-a", 100)],
        Vec::new(),
        "",
        None,
        None,
    );
    stage_ingested(&mut engine, &report);
    engine.prune_lifetime_state();
    let id = WindowId("win-a".to_owned());
    assert!(
        engine.pending_intent.contains_key(&id),
        "member-known lifetime keeps pending intent"
    );
    assert!(
        !engine.sticky_rollback_saves.contains_key(&id),
        "rollback saves have no known-only retention"
    );
    assert!(
        !engine.excluded_intent.contains_key(&id),
        "exclusion ledger has no known-only retention"
    );
}

#[test]
fn lifetime_prune_own_process_exclusion_keeps_intent_ledgers() {
    // The own-process subject classifies out of facts (staged.facts lacks
    // it) but stays raw lifetime evidence with an exclusion mark, so both
    // intent ledgers retain it on a complete report.
    let mut engine = engine();
    let id = WindowId("win-b".to_owned());
    engine
        .pending_intent
        .insert(id.clone(), IntentData::new(Some(true), None));
    engine
        .excluded_intent
        .insert(id.clone(), ExcludedIntent::default());
    let mut own_fact = fact("win-b", "out-a", 100);
    own_fact.process_id = Some("test-own-pid".to_owned());
    let report = report_full(
        1,
        1,
        vec![screen("out-a", 0)],
        vec![own_fact],
        Vec::new(),
        "",
        None,
        None,
    );
    // Stage without intent ingestion: ingestion owns own-process pending
    // removal (`ingest_staged_intent` drops proven-own pending/conflict
    // entries), so this test isolates the prune-time excluded arm only.
    engine.stage_facts(&report);
    assert!(
        !engine.staged.facts.contains_key(&id),
        "own-process subject classifies out of facts"
    );
    assert!(
        engine.staged.excluded.contains(&id),
        "own-process subject marks the exclusion set"
    );
    engine.prune_lifetime_state();
    assert!(
        engine.pending_intent.contains_key(&id),
        "pending intent retains excluded lifetimes"
    );
    assert!(
        engine.excluded_intent.contains_key(&id),
        "exclusion ledger retains excluded lifetimes"
    );
}

#[test]
fn lifetime_prune_true_output_loss_drops_scopes_targets_and_grants() {
    // Window loss alone (covered elsewhere) keeps output policy; only a
    // complete inventory with neither a live nor a held screen for the
    // output drops scopes, homed targets, and the finite grant.
    let mut engine = engine();
    seed_lifetime_ledgers(&mut engine, "win-b", "out-b");
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
    stage_ingested(&mut engine, &report);
    engine.prune_lifetime_state();
    assert!(
        !engine
            .selection_scopes
            .contains_key(&OutputId("out-b".to_owned())),
        "missing screen with no held twin drops the scope"
    );
    assert!(
        !engine
            .selection_targets
            .contains_key(&WindowId("win-b".to_owned())),
        "target homed on the lost output drops"
    );
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-b".to_owned())),
        "finite grant on the lost output drops"
    );
    assert_window_ledgers_dropped(&engine, "win-b");
}

#[test]
fn lifetime_prune_v4_graded_hold_keeps_presence_without_eligibility() {
    // A genuinely capability-graded v4 hold (not a synthetic held row):
    // the raw capture keeps the original fact, so raw-presence ledgers
    // retain it, while classification withholds it from facts (presence
    // without authority). Canonical memory follows the same raw-presence
    // predicate the legacy lane used; no v4 accepted-run assembly is tested.
    let mut engine = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities::default(),
    )
    .expect("generation");
    let mut sticky = fact("win-a", "out-a", 0);
    sticky.sticky = Some(true);
    let report = report_v4_simple(
        1,
        vec![screen("out-a", 0)],
        Vec::new(),
        vec![sticky],
        Vec::new(),
        Vec::new(),
    );
    stage_v4_ingested(&mut engine, &report);
    assert!(
        engine
            .staged
            .held_windows
            .iter()
            .any(|held| held.window.0 == "win-a"),
        "conflicting sticky grades to a held row"
    );
    assert!(
        engine
            .staged
            .observation
            .raw_windows
            .iter()
            .any(|fact| fact.window.0 == "win-a"),
        "raw capture keeps the graded original"
    );
    insert_state_binding(&mut engine, "win-a");
    engine
        .last_active_window
        .insert(home_key("out-a"), WindowId("win-a".to_owned()));
    engine.prune_lifetime_state();
    assert!(
        engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Sticky)),
        "graded held row counts as lifetime presence"
    );
    assert!(
        !engine
            .staged
            .facts
            .contains_key(&WindowId("win-a".to_owned())),
        "graded hold carries no classified authority"
    );
    assert!(
        engine
            .last_active_window
            .values()
            .any(|held| held.0 == "win-a"),
        "canonical memory follows preserved raw presence"
    );
}

// 4b-2: state, ordinary-focus and frame settlement consume the staged
// capture on both intakes. The v4 accepted-run domain assembly is unbuilt,
// so tests seed the post-rollback staged join directly (homed window plus
// one readable domain; no conversion, no runtime change) and drive settle_*().
fn engine_settle() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            fullscreen: true,
            maximize: true,
            native_sticky: true,
            above: true,
            ..Default::default()
        },
    )
    .expect("generation")
}

fn seed_readable_domain(engine: &mut WorldEngine, window: &str) {
    let key = home_key("out-a");
    engine
        .staged
        .homing
        .insert(WindowId(window.to_owned()), key.clone());
    engine.staged.domains.push(StagedDomain {
        key,
        bounds: rect(0, 0, 800, 560),
        gap: 0,
        outer_gap: 0,
        windows: Vec::new(),
        focused: WindowId(String::new()),
        minimized: Vec::new(),
        unreadable: Vec::new(),
        closed: Vec::new(),
        readable: true,
        hold: None,
    });
}

/// Independently constructed same-content pair per intake (no conversion).
/// Tests set lane fields on each member; `stage_either` only picks intake.
fn pair_reports(
    seq: u64,
    windows: Vec<WindowFact>,
    closed: Vec<WindowId>,
) -> (WorldReport, WorldReportV4) {
    let mut legacy = report_full(
        seq,
        seq,
        vec![screen("out-a", 0)],
        windows.clone(),
        Vec::new(),
        "",
        None,
        None,
    );
    legacy.closed = closed.clone();
    let v4 = report_v4_simple(
        seq,
        vec![screen("out-a", 0)],
        Vec::new(),
        windows,
        Vec::new(),
        closed,
    );
    (legacy, v4)
}

fn stage_either(
    engine: &mut WorldEngine,
    v4: bool,
    legacy: &WorldReport,
    v4report: &WorldReportV4,
) {
    if v4 {
        stage_v4_ingested(engine, v4report);
    } else {
        stage_ingested(engine, legacy);
    }
}

fn sticky_fact(window: &str, value: Option<bool>) -> WindowFact {
    let mut row = fact(window, "out-a", 0);
    row.sticky = value;
    row
}

fn state_attempt(
    target: bool,
    revision: u64,
    setter: FrameSetter,
    pre: Option<bool>,
    post: Option<bool>,
) -> StateAttempt {
    StateAttempt {
        window: WindowId("win-a".to_owned()),
        property: StateProperty::Sticky,
        target,
        revision,
        setter,
        pre,
        post,
        attempt_ms: 16,
    }
}

fn assert_state(
    engine: &WorldEngine,
    v4: bool,
    spent: bool,
    pending: Option<StateAttempt>,
    disposition: AttemptOutcome,
    what: &str,
) {
    let got = engine
        .retained_states
        .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
        .expect("binding retained");
    assert_eq!(
        (got.spent, got.pending.clone(), got.disposition),
        (spent, pending, disposition),
        "{what} (v4={v4})"
    );
}

#[allow(clippy::too_many_arguments)]
fn stage_state(
    engine: &mut WorldEngine,
    v4: bool,
    seq: u64,
    windows: Vec<WindowFact>,
    attempts: Vec<StateAttempt>,
    complete: bool,
    barrier: bool,
    held: Vec<HeldWindowRow>,
) {
    let (mut legacy, mut v4report) = pair_reports(seq, windows, Vec::new());
    legacy.inventory_complete = complete;
    v4report.inventory_complete = complete;
    legacy.gesture_barrier = barrier;
    v4report.gesture_barrier = barrier;
    legacy.held_windows = held.clone();
    v4report.held_windows = held;
    legacy.state_attempts = attempts.clone();
    v4report.state_attempts = attempts;
    stage_either(engine, v4, &legacy, &v4report);
    seed_readable_domain(engine, "win-a");
}

#[test]
fn settle_state_gates_latch_first_evidence_then_recovers() {
    for v4 in [false, true] {
        let refused = state_attempt(true, 1, FrameSetter::Refused, Some(false), Some(false));
        let mut engine = engine_settle();
        insert_state_binding(&mut engine, "win-a");
        // Superseded empty stage: settle must read the current capture.
        let (old_leg, old_v4) = pair_reports(0, Vec::new(), Vec::new());
        stage_either(&mut engine, v4, &old_leg, &old_v4);
        // An unmatched tuple never spends.
        let stray = state_attempt(true, 2, FrameSetter::Accepted, Some(false), Some(true));
        stage_state(
            &mut engine,
            v4,
            1,
            vec![sticky_fact("win-a", Some(true))],
            vec![stray],
            true,
            false,
            Vec::new(),
        );
        engine.settle_states();
        assert_state(
            &engine,
            v4,
            false,
            None,
            AttemptOutcome::Unattempted,
            "mismatch",
        );
        // First exact refused receipt DURING incomplete inventory latches
        // spent with PossiblyExecuted: raw evidence records first, the
        // incomplete gate alone denies readability.
        stage_state(
            &mut engine,
            v4,
            2,
            vec![sticky_fact("win-a", Some(false))],
            vec![refused.clone()],
            false,
            false,
            Vec::new(),
        );
        engine.settle_states();
        assert_state(
            &engine,
            v4,
            true,
            Some(refused.clone()),
            AttemptOutcome::PossiblyExecuted,
            "incomplete latch",
        );
        // Barrier and held-only receipts latch the same way on isolated
        // engines: each gate alone denies readability.
        for (what, complete, barrier, windows, held) in [
            (
                "barrier latch",
                true,
                true,
                vec![sticky_fact("win-a", Some(false))],
                Vec::new(),
            ),
            (
                "hold latch",
                true,
                false,
                Vec::new(),
                vec![held_window_row("win-a", "out-a", 0)],
            ),
        ] {
            let mut gated = engine_settle();
            insert_state_binding(&mut gated, "win-a");
            stage_state(
                &mut gated,
                v4,
                2,
                windows,
                vec![refused.clone()],
                complete,
                barrier,
                held,
            );
            gated.settle_states();
            assert_state(
                &gated,
                v4,
                true,
                Some(refused.clone()),
                AttemptOutcome::PossiblyExecuted,
                what,
            );
        }
        // A contradictory SAME-tuple accepted row cannot replace the latch.
        let contra = state_attempt(true, 1, FrameSetter::Accepted, Some(false), Some(true));
        stage_state(
            &mut engine,
            v4,
            3,
            vec![sticky_fact("win-a", Some(true))],
            vec![contra],
            true,
            false,
            Vec::new(),
        );
        engine.settle_states();
        assert_state(
            &engine,
            v4,
            true,
            Some(refused.clone()),
            AttemptOutcome::PossiblyExecuted,
            "same tuple kept",
        );
        // Recovery: readable sticky=false proves the ORIGINAL refusal with
        // no new attempt row.
        stage_state(
            &mut engine,
            v4,
            4,
            vec![sticky_fact("win-a", Some(false))],
            Vec::new(),
            true,
            false,
            Vec::new(),
        );
        engine.settle_states();
        assert_state(
            &engine,
            v4,
            true,
            Some(refused.clone()),
            AttemptOutcome::RefusedProven,
            "recovery proves",
        );
    }
}

fn focus_grant(engine: &mut WorldEngine) {
    engine.retained_focus.push(RetainedFocusGrant {
        effect: OneShotEffect::explicit_focus(3, WindowId("win-a".to_owned()), 1),
        feedback: None,
        verdict: FocusVerdict::Pending,
    });
}

fn focus_row(id: &str, revision: u64) -> FocusFeedback {
    FocusFeedback {
        id: id.to_owned(),
        target: WindowId("win-a".to_owned()),
        revision,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: None,
        post: Some(WindowId("win-a".to_owned())),
        foreground_proven: true,
        arrival: Vec::new(),
        attempt_ms: 17,
    }
}

fn assert_grant(
    engine: &WorldEngine,
    v4: bool,
    verdict: FocusVerdict,
    feedback: Option<FocusFeedback>,
    what: &str,
) {
    let got = engine.retained_focus.first().expect("grant retained");
    assert_eq!(
        (got.verdict, got.feedback.clone()),
        (verdict, feedback),
        "{what} (v4={v4})"
    );
}

#[allow(clippy::too_many_arguments)]
fn stage_focus(
    engine: &mut WorldEngine,
    v4: bool,
    seq: u64,
    windows: Vec<WindowFact>,
    feedback: Vec<FocusFeedback>,
    complete: bool,
    barrier: bool,
    held: Vec<HeldWindowRow>,
) {
    let (mut legacy, mut v4report) = pair_reports(seq, windows, Vec::new());
    legacy.inventory_complete = complete;
    v4report.inventory_complete = complete;
    legacy.gesture_barrier = barrier;
    v4report.gesture_barrier = barrier;
    legacy.held_windows = held.clone();
    v4report.held_windows = held;
    legacy.focus_feedback = feedback.clone();
    v4report.focus_feedback = feedback;
    stage_either(engine, v4, &legacy, &v4report);
}

#[test]
fn settle_focus_first_match_under_hold_beats_same_tuple() {
    for v4 in [false, true] {
        let mut engine = engine_settle();
        focus_grant(&mut engine);
        // Mismatched tuples precede the match and mint nothing.
        stage_focus(
            &mut engine,
            v4,
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![focus_row("fx-9", 1), focus_row("fx-3", 2)],
            true,
            false,
            Vec::new(),
        );
        engine.settle_focus_feedback();
        assert_grant(&engine, v4, FocusVerdict::Pending, None, "mismatch");
        // The first exact match arrives under held/incomplete/barrier and
        // still latches history: no readability gate.
        let matched = focus_row("fx-3", 1);
        stage_focus(
            &mut engine,
            v4,
            2,
            Vec::new(),
            vec![matched.clone()],
            false,
            true,
            vec![held_window_row("win-a", "out-a", 0)],
        );
        engine.settle_focus_feedback();
        assert_grant(
            &engine,
            v4,
            FocusVerdict::Proven,
            Some(matched.clone()),
            "hold latch",
        );
        // A contradictory SAME-tuple row (refused shape, same opaque
        // identity) cannot overwrite the latched history.
        let same_contra = FocusFeedback {
            setter: VisibilitySetter::Refused,
            pre: Some(WindowId("win-a".to_owned())),
            post: Some(WindowId("win-a".to_owned())),
            foreground_proven: false,
            ..matched.clone()
        };
        stage_focus(
            &mut engine,
            v4,
            3,
            vec![fact("win-a", "out-a", 0)],
            vec![same_contra],
            true,
            false,
            Vec::new(),
        );
        engine.settle_focus_feedback();
        assert_grant(
            &engine,
            v4,
            FocusVerdict::Proven,
            Some(matched.clone()),
            "same tuple kept",
        );
        // Negative control: mismatches alone on a fresh grant mint nothing.
        let mut fresh = engine_settle();
        focus_grant(&mut fresh);
        stage_focus(
            &mut fresh,
            v4,
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![focus_row("fx-9", 1)],
            true,
            false,
            Vec::new(),
        );
        fresh.settle_focus_feedback();
        assert_grant(&fresh, v4, FocusVerdict::Pending, None, "control");
    }
}

fn frame_attempt(
    target: crate::geometry::Rect,
    readback: Option<crate::geometry::Rect>,
    setter: FrameSetter,
    ms: u64,
) -> FrameAttempt {
    FrameAttempt {
        window: WindowId("win-a".to_owned()),
        target,
        setter,
        readback,
        dpi_before: None,
        dpi_after: None,
        attempt_ms: ms,
        operation_id: None,
        prewrite: None,
    }
}

fn stage_frames(
    engine: &mut WorldEngine,
    v4: bool,
    seq: u64,
    staged: crate::geometry::Rect,
    attempts: Vec<FrameAttempt>,
    ms: u64,
) {
    let mut staged_fact = fact("win-a", "out-a", 0);
    staged_fact.rect = Some(staged);
    let (mut legacy, mut v4report) = pair_reports(seq, vec![staged_fact], Vec::new());
    legacy.frames = attempts.clone();
    v4report.frames = attempts;
    legacy.monotonic_ms = ms;
    v4report.monotonic_ms = ms;
    stage_either(engine, v4, &legacy, &v4report);
    seed_readable_domain(engine, "win-a");
}

fn burst_of(engine: &WorldEngine) -> (u32, u64) {
    let burst = engine
        .frame_bursts
        .get(&WindowId("win-a".to_owned()))
        .expect("burst spent");
    (burst.attempts, burst.last_change_ms)
}

fn learned_of(engine: &WorldEngine) -> Option<(Option<i32>, Option<i32>)> {
    engine
        .learned_mins
        .get(&WindowId("win-a".to_owned()))
        .map(|learned| (learned.min_w, learned.min_h))
}

#[test]
fn settle_frames_replay_never_renews_suppression_or_budget() {
    for v4 in [false, true] {
        let mut engine = engine_settle();
        let target = rect(0, 0, 200, 90);
        let observed = rect(0, 0, 40, 90);
        let attempt = frame_attempt(target, Some(observed), FrameSetter::Accepted, 1000);
        stage_frames(&mut engine, v4, 1, observed, vec![attempt.clone()], 1000);
        engine.settle_frames();
        assert_eq!(
            engine.staged.observation.frames,
            vec![attempt.clone()],
            "current capture (v4={v4})"
        );
        let memory = engine
            .frames
            .get(&WindowId("win-a".to_owned()))
            .expect("clamp recorded")
            .clone();
        assert_eq!(
            (
                memory.desired,
                memory.observed,
                memory.clamped,
                memory.last_ms
            ),
            (target, observed, true, 1000),
            "v4={v4}"
        );
        assert_eq!(burst_of(&engine), (1, 1000), "captured clock (v4={v4})");
        // Identical restage is a lost-ack redelivery: nothing renews.
        stage_frames(&mut engine, v4, 2, observed, vec![attempt.clone()], 1000);
        engine.settle_frames();
        assert_eq!(
            engine.frames.get(&WindowId("win-a".to_owned())),
            Some(&memory),
            "exact replay (v4={v4})"
        );
        assert_eq!(burst_of(&engine).0, 1, "no re-spend (v4={v4})");
        // An older stamp for the same pair is stale, not fresh proof.
        let older = frame_attempt(target, Some(observed), FrameSetter::Accepted, 500);
        stage_frames(&mut engine, v4, 3, observed, vec![older], 1500);
        engine.settle_frames();
        assert_eq!(
            engine.frames.get(&WindowId("win-a".to_owned())),
            Some(&memory),
            "older replay (v4={v4})"
        );
        // Fresh engine control consumes from the current capture alone.
        let mut fresh = engine_settle();
        stage_frames(&mut fresh, v4, 2, observed, vec![attempt.clone()], 1000);
        fresh.settle_frames();
        assert_eq!(
            fresh.frames.get(&WindowId("win-a".to_owned())),
            Some(&memory),
            "no-history consumption (v4={v4})"
        );
    }
}

#[test]
fn settle_frames_learned_counterevidence_vetoes_and_relearns() {
    for v4 in [false, true] {
        let mut engine = engine_settle();
        let grown = rect(0, 0, 120, 90);
        let target = rect(0, 0, 40, 90);
        let small = rect(0, 0, 40, 90);
        // Async clamp shape: readback proves dispatch at target, the later
        // staged fact holds the larger stable size.
        let learn = frame_attempt(target, Some(target), FrameSetter::Accepted, 1000);
        stage_frames(&mut engine, v4, 1, grown, vec![learn], 1000);
        engine.settle_frames();
        assert_eq!(
            learned_of(&engine),
            Some((Some(120), None)),
            "learn (v4={v4})"
        );
        // A smaller observation with a fresh refused attempt is transient
        // preimage: the floor survives while the attempt still spends.
        let refused = frame_attempt(target, Some(small), FrameSetter::Refused, 2000);
        stage_frames(&mut engine, v4, 2, small, vec![refused], 2000);
        engine.settle_frames();
        assert_eq!(
            learned_of(&engine),
            Some((Some(120), None)),
            "veto (v4={v4})"
        );
        assert_eq!(burst_of(&engine).0, 2, "vetoed spends (v4={v4})");
        // A stable eligible smaller observation with only a clockless
        // attempt (no ordering claim) clears the floor.
        let clockless = frame_attempt(target, None, FrameSetter::Accepted, 0);
        stage_frames(&mut engine, v4, 3, small, vec![clockless], 3000);
        engine.settle_frames();
        assert_eq!(learned_of(&engine), None, "cleared (v4={v4})");
        // The same growth evidence re-establishes the floor.
        let relearn = frame_attempt(target, Some(target), FrameSetter::Accepted, 4000);
        stage_frames(&mut engine, v4, 4, grown, vec![relearn], 4000);
        engine.settle_frames();
        assert_eq!(
            learned_of(&engine),
            Some((Some(120), None)),
            "relearn (v4={v4})"
        );
        // Explicit close retires all four maps even though retained
        // membership would otherwise keep the lifetime.
        let id = WindowId("win-a".to_owned());
        engine.members.insert(
            id.clone(),
            MemberState {
                home: home_key("out-a"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        assert!(
            engine.frames.contains_key(&id)
                && engine.consumed_frames.contains_key(&id)
                && engine.learned_mins.contains_key(&id)
                && engine.frame_bursts.contains_key(&id),
            "populated before close (v4={v4})"
        );
        let (close_leg, close_v4) = pair_reports(5, Vec::new(), vec![id.clone()]);
        stage_either(&mut engine, v4, &close_leg, &close_v4);
        engine.settle_frames();
        assert!(
            !engine.frames.contains_key(&id)
                && !engine.consumed_frames.contains_key(&id)
                && !engine.learned_mins.contains_key(&id)
                && !engine.frame_bursts.contains_key(&id),
            "close retires all four (v4={v4})"
        );
        assert!(
            engine.members.contains_key(&id),
            "close loop retired, not prune (v4={v4})"
        );
    }
}

#[test]
fn settle_frames_burst_quiet_gap_rearms_from_captured_clock() {
    for v4 in [false, true] {
        let mut engine = engine_settle();
        let target = rect(0, 0, 200, 90);
        let first = rect(0, 0, 40, 90);
        let drifted = rect(0, 0, 60, 90);
        let one = frame_attempt(target, Some(first), FrameSetter::Accepted, 1000);
        stage_frames(&mut engine, v4, 1, first, vec![one], 1000);
        engine.settle_frames();
        assert_eq!(burst_of(&engine), (1, 1000), "v4={v4}");
        // Drift after a proven >=500ms quiet gap re-arms then spends once.
        let two = frame_attempt(target, Some(drifted), FrameSetter::Accepted, 1800);
        stage_frames(&mut engine, v4, 2, drifted, vec![two], 1800);
        engine.settle_frames();
        let rearmed = *engine
            .frame_bursts
            .get(&WindowId("win-a".to_owned()))
            .expect("burst retained");
        assert_eq!(
            (rearmed.attempts, rearmed.last_change_ms),
            (1, 1800),
            "v4={v4}"
        );
        assert_eq!(rearmed.observed, drifted, "v4={v4}");
        // Inside the burst the count accumulates with no quiet gap.
        let three = frame_attempt(target, Some(drifted), FrameSetter::Accepted, 1900);
        stage_frames(&mut engine, v4, 3, drifted, vec![three], 1900);
        engine.settle_frames();
        assert_eq!(burst_of(&engine).0, 2, "accumulates (v4={v4})");
    }
}

// --- Unit 4b-2b: `settle_selection_feedback` reads the staged capture ---
//
// The seeds below are test-only retained bindings and homing marks (NOT v4
// runtime or scenario execution): each intake stages real legacy/v4 lanes
// and the no-arg settle consumes only the current staged observation.

fn sel_feedback(id: &str, revision: u64, arrival: Vec<ArrivalBracket>) -> FocusFeedback {
    FocusFeedback {
        id: id.to_owned(),
        target: WindowId("win-a".to_owned()),
        revision,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: None,
        post: Some(WindowId("win-a".to_owned())),
        foreground_proven: true,
        arrival,
        attempt_ms: 17,
    }
}

fn sel_sample(obs: u64) -> VisibilitySample {
    VisibilitySample {
        obs,
        lifetime_matched: Some(true),
        visibility: VisibilityFact {
            visible: Some(true),
            suppression_owned: Some(false),
        },
        minimized: Some(false),
        host_hidden: Some(false),
    }
}

fn sel_bracket_output(obs: u64) -> ArrivalBracket {
    ArrivalBracket {
        window: WindowId("win-a".to_owned()),
        obs,
        before: sel_sample(obs),
        location: ArrivalLocation::Output {
            out: Some(OutputId("out-a".to_owned())),
        },
        frame: Some(rect(0, 0, 40, 90)),
        after: sel_sample(obs),
    }
}

fn sel_bracket_native(native: &str, obs: u64) -> ArrivalBracket {
    let mut bracket = sel_bracket_output(obs);
    bracket.location = ArrivalLocation::Native {
        home: Some(WorkspaceRef::Native {
            output: OutputId("out-a".to_owned()),
            workspace: crate::ids::NativeWorkspaceId::parse(native).expect("native id"),
        }),
    };
    bracket
}

/// Test-only retained binding seed (NOT runtime scenario execution).
/// Focus-bearing seeds freeze one shown member and mint the producer's
/// `fx-3` arrival grant (`await_shown`, see `resolve_selection_focus`);
/// focusless seeds mint no effect and freeze no members.
fn seed_selection_binding(
    engine: &mut WorldEngine,
    parent_op: &str,
    effect: bool,
    outputs: &[&str],
) {
    let id = WindowId("win-a".to_owned());
    let mut view_targets = BTreeMap::new();
    for output in outputs {
        view_targets.insert(OutputId((*output).to_owned()), home_key(output));
    }
    engine.retained_selection.insert(
        OutputId("out-a".to_owned()),
        RetainedSelection {
            effect: effect.then(|| {
                let mut effect = OneShotEffect::explicit_focus(3, id.clone(), 1);
                effect.await_shown = true;
                effect
            }),
            parent_op: parent_op.to_owned(),
            home: home_key("out-a"),
            rev: 1,
            obs: 1,
            members: if effect {
                vec![FrozenArrivalMember {
                    window: id.clone(),
                    home: home_key("out-a"),
                    frame: None,
                }]
            } else {
                Vec::new()
            },
            switch_members: Vec::new(),
            switch_evidence: BTreeMap::new(),
            feedback: None,
            foreground_proven: false,
            arrival_ready: false,
            view_targets,
            view_raw: BTreeMap::new(),
            selected: outputs.iter().map(|o| home_key(o)).collect(),
            affected: outputs.iter().map(|o| OutputId((*o).to_owned())).collect(),
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn stage_selection(
    engine: &mut WorldEngine,
    v4: bool,
    seq: u64,
    views: Vec<ViewAttempt>,
    feedback: Vec<FocusFeedback>,
    complete: bool,
    barrier: bool,
    held: Vec<HeldWindowRow>,
) {
    let (mut legacy, mut v4report) = pair_reports(seq, Vec::new(), Vec::new());
    // Both obligated outputs are known carried screens on each intake, so
    // wrong-output tuples are genuine mismatches, not unknown outputs.
    legacy.screens.push(screen("out-b", 800));
    v4report.screens.push(screen("out-b", 800));
    legacy.inventory_complete = complete;
    v4report.inventory_complete = complete;
    legacy.gesture_barrier = barrier;
    v4report.gesture_barrier = barrier;
    legacy.held_windows = held.clone();
    v4report.held_windows = held;
    legacy.view_attempts = views.clone();
    v4report.view_attempts = views;
    legacy.focus_feedback = feedback.clone();
    v4report.focus_feedback = feedback;
    stage_either(engine, v4, &legacy, &v4report);
    // Test-only homing seed (NOT runtime execution): marks win-a homed so
    // the arrival join evaluates against the staged capture.
    engine
        .staged
        .homing
        .insert(WindowId("win-a".to_owned()), home_key("out-a"));
}

fn assert_selection(
    engine: &WorldEngine,
    v4: bool,
    views: usize,
    feedback: Option<FocusFeedback>,
    proven: bool,
    ready: bool,
    what: &str,
) {
    let binding = engine
        .retained_selection
        .get(&OutputId("out-a".to_owned()))
        .expect("binding retained");
    assert_eq!(binding.view_raw.len(), views, "{what} views (v4={v4})");
    assert_eq!(binding.feedback, feedback, "{what} feedback (v4={v4})");
    assert_eq!(
        (binding.foreground_proven, binding.arrival_ready),
        (proven, ready),
        "{what} flags (v4={v4})"
    );
    // The receipt latch alone mints no terminal: the switch settle owns
    // proof assembly, and the binding stays retained.
    assert!(
        engine.switch_settled.is_empty(),
        "{what} no terminal (v4={v4})"
    );
}

#[test]
fn settle_selection_feedback_first_exact_tuples_latch_from_staged_capture() {
    for v4 in [false, true] {
        let mut engine = engine_settle();
        seed_selection_binding(&mut engine, "op-3", true, &["out-a"]);
        // Wrong tuples correlate with nothing and latch nothing. The
        // decoder rejects duplicate view operation ids even across
        // outputs, so each wrong tuple stages alone on an advancing
        // sequence.
        let wrong_views = vec![
            ViewAttempt {
                operation_id: "op-9".to_owned(),
                ..view_row("op-3")
            },
            ViewAttempt {
                target_workspace: "ws-9".to_owned(),
                ..view_row("op-3")
            },
            ViewAttempt {
                target_output: OutputId("out-b".to_owned()),
                ..view_row("op-3")
            },
        ];
        let wrong_feedback = vec![
            focus_row("fx-9", 1),
            focus_row("fx-3", 2),
            FocusFeedback {
                target: WindowId("win-b".to_owned()),
                ..focus_row("fx-3", 1)
            },
        ];
        for (index, wrong) in wrong_views.into_iter().enumerate() {
            let seq = index as u64 + 1;
            stage_selection(
                &mut engine,
                v4,
                seq,
                vec![wrong],
                wrong_feedback.clone(),
                true,
                false,
                Vec::new(),
            );
            engine.settle_selection_feedback();
            assert_selection(&engine, v4, 0, None, false, false, "mismatch");
        }
        // The first exact tuples latch under held/incomplete/barrier: no
        // readability gate, no sweep authority.
        let matched = sel_feedback("fx-3", 1, vec![sel_bracket_output(4)]);
        stage_selection(
            &mut engine,
            v4,
            4,
            vec![view_row("op-3")],
            vec![matched.clone()],
            false,
            true,
            vec![held_window_row("win-a", "out-a", 0)],
        );
        engine.settle_selection_feedback();
        assert_eq!(
            engine
                .retained_selection
                .get(&OutputId("out-a".to_owned()))
                .expect("binding")
                .view_raw
                .get(&OutputId("out-a".to_owned())),
            Some(&view_row("op-3")),
            "exact view tuple (v4={v4})"
        );
        assert_selection(
            &engine,
            v4,
            1,
            Some(matched.clone()),
            true,
            true,
            "hold latch",
        );
        // A contradictory same-tuple row cannot replace the original.
        stage_selection(
            &mut engine,
            v4,
            5,
            vec![ViewAttempt {
                setter: FrameSetter::Refused,
                ..view_row("op-3")
            }],
            vec![FocusFeedback {
                setter: VisibilitySetter::Refused,
                foreground_proven: false,
                arrival: Vec::new(),
                ..matched.clone()
            }],
            true,
            false,
            Vec::new(),
        );
        engine.settle_selection_feedback();
        assert_selection(
            &engine,
            v4,
            1,
            Some(matched.clone()),
            true,
            true,
            "same tuple kept",
        );
        assert_eq!(
            engine
                .retained_selection
                .get(&OutputId("out-a".to_owned()))
                .expect("binding")
                .view_raw
                .get(&OutputId("out-a".to_owned())),
            Some(&view_row("op-3")),
            "original view row kept (v4={v4})"
        );
        // Same-lane focus ordering: the first exact row wins within one
        // lane. Repeated focus rows are decoder-preserved facts (no
        // duplicate restriction on the lane); duplicate view attempts are
        // decoder-rejected per operation id, so views keep cross-report
        // coverage only.
        let mut ordered = engine_settle();
        seed_selection_binding(&mut ordered, "op-3", true, &["out-a"]);
        let first = sel_feedback("fx-3", 1, vec![sel_bracket_output(6)]);
        stage_selection(
            &mut ordered,
            v4,
            6,
            vec![view_row("op-3")],
            vec![
                first.clone(),
                FocusFeedback {
                    setter: VisibilitySetter::Refused,
                    foreground_proven: false,
                    arrival: Vec::new(),
                    ..first.clone()
                },
            ],
            true,
            false,
            Vec::new(),
        );
        ordered.settle_selection_feedback();
        assert_selection(&ordered, v4, 1, Some(first), true, true, "lane order");
        // Empty later lanes never clear the latch: the current capture
        // replaces the older staged observation, never rereads it.
        stage_selection(
            &mut engine,
            v4,
            7,
            Vec::new(),
            Vec::new(),
            true,
            false,
            Vec::new(),
        );
        engine.settle_selection_feedback();
        assert_selection(
            &engine,
            v4,
            1,
            Some(matched.clone()),
            true,
            true,
            "empty restage keeps",
        );
        // A future-dated bracket latches feedback without arrival. The
        // bound itself is already discriminated by the first latch
        // (in-window proves, future-dated holds); the later receipt only
        // shows historical no-regrade: its watermark now covers the old
        // sample, yet the latched row is never re-graded, even with a
        // fresh in-window row in the lane.
        let mut fresh = engine_settle();
        seed_selection_binding(&mut fresh, "op-3", true, &["out-a"]);
        let future = sel_feedback("fx-3", 1, vec![sel_bracket_output(9)]);
        stage_selection(
            &mut fresh,
            v4,
            2,
            vec![view_row("op-3")],
            vec![future.clone()],
            true,
            false,
            Vec::new(),
        );
        fresh.settle_selection_feedback();
        assert_selection(
            &fresh,
            v4,
            1,
            Some(future.clone()),
            true,
            false,
            "future bracket holds",
        );
        let late = sel_feedback("fx-3", 1, vec![sel_bracket_output(10)]);
        stage_selection(
            &mut fresh,
            v4,
            10,
            vec![view_row("op-3")],
            vec![late],
            true,
            false,
            Vec::new(),
        );
        fresh.settle_selection_feedback();
        assert_selection(&fresh, v4, 1, Some(future), true, false, "no upgrade");
    }
}

fn engine_settle_host() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            host_owned: true,
            fullscreen: true,
            maximize: true,
            native_sticky: true,
            above: true,
            ..Default::default()
        },
    )
    .expect("generation")
}

#[test]
fn settle_selection_feedback_view_receipts_independent_focusless_and_host_owned() {
    for v4 in [false, true] {
        // Focusless native obligation on a host-owned engine, the realistic
        // retained shape (direct seed bypasses native/domain assembly, so
        // no codec-validity claim): the view raw latches per output while
        // no feedback gate exists without an effect.
        let mut engine = engine_settle_host();
        seed_selection_binding(&mut engine, "op-4", false, &["out-a", "out-b"]);
        let exact_b = ViewAttempt {
            target_output: OutputId("out-b".to_owned()),
            ..view_row("op-4")
        };
        stage_selection(
            &mut engine,
            v4,
            1,
            vec![view_row("op-4")],
            vec![focus_row("fx-4", 1)],
            true,
            false,
            Vec::new(),
        );
        engine.settle_selection_feedback();
        assert_selection(&engine, v4, 1, None, false, false, "focusless partial");
        {
            let binding = engine
                .retained_selection
                .get(&OutputId("out-a".to_owned()))
                .expect("binding");
            assert_eq!(
                binding.view_raw.get(&OutputId("out-a".to_owned())),
                Some(&view_row("op-4")),
                "out-a latched (v4={v4})"
            );
            assert!(
                !binding.view_raw.contains_key(&OutputId("out-b".to_owned())),
                "out-b still pending (v4={v4})"
            );
        }
        // The second output latches on its own exact tuple; a later
        // same-tuple out-a redelivery cannot replace the original (each
        // lane carries one operation id per the decoder rule).
        stage_selection(
            &mut engine,
            v4,
            2,
            vec![exact_b.clone()],
            Vec::new(),
            true,
            false,
            Vec::new(),
        );
        engine.settle_selection_feedback();
        assert_selection(&engine, v4, 2, None, false, false, "focusless complete");
        {
            let binding = engine
                .retained_selection
                .get(&OutputId("out-a".to_owned()))
                .expect("binding");
            assert_eq!(
                binding.view_raw.get(&OutputId("out-a".to_owned())),
                Some(&view_row("op-4")),
                "original out-a kept (v4={v4})"
            );
            assert_eq!(
                binding.view_raw.get(&OutputId("out-b".to_owned())),
                Some(&exact_b),
                "out-b latched (v4={v4})"
            );
        }
        stage_selection(
            &mut engine,
            v4,
            3,
            vec![ViewAttempt {
                setter: FrameSetter::Refused,
                ..view_row("op-4")
            }],
            Vec::new(),
            true,
            false,
            Vec::new(),
        );
        engine.settle_selection_feedback();
        assert_selection(&engine, v4, 2, None, false, false, "focusless redelivery");
        assert_eq!(
            engine
                .retained_selection
                .get(&OutputId("out-a".to_owned()))
                .expect("binding")
                .view_raw
                .get(&OutputId("out-a".to_owned())),
            Some(&view_row("op-4")),
            "original out-a survives redelivery (v4={v4})"
        );
        // Host-owned native-map translation: the mapped native home proves
        // arrival through the engine's own bindings.
        let mut host = engine_settle_host();
        host.native_map
            .insert(("out-a".to_owned(), "n1".to_owned()), "ws-1".to_owned());
        seed_selection_binding(&mut host, "op-3", true, &["out-a"]);
        let native_ok = sel_feedback("fx-3", 1, vec![sel_bracket_native("n1", 2)]);
        stage_selection(
            &mut host,
            v4,
            2,
            Vec::new(),
            vec![native_ok.clone()],
            true,
            false,
            Vec::new(),
        );
        host.settle_selection_feedback();
        assert_selection(&host, v4, 0, Some(native_ok), true, true, "native mapped");
        // An unmapped native home latches feedback without arrival.
        let mut wrong = engine_settle_host();
        wrong
            .native_map
            .insert(("out-a".to_owned(), "n1".to_owned()), "ws-1".to_owned());
        seed_selection_binding(&mut wrong, "op-3", true, &["out-a"]);
        let native_bad = sel_feedback("fx-3", 1, vec![sel_bracket_native("n0", 2)]);
        stage_selection(
            &mut wrong,
            v4,
            2,
            Vec::new(),
            vec![native_bad.clone()],
            true,
            false,
            Vec::new(),
        );
        wrong.settle_selection_feedback();
        assert_selection(
            &wrong,
            v4,
            0,
            Some(native_bad),
            true,
            false,
            "native unmapped holds",
        );
    }
}

// 4b-2c: cursor focus/warp observation family on the staged capture, both
// intakes. Tests drive the real `stage_facts`/`stage_facts_v4` lanes, then
// seed the post-rollback staged join directly (TEST-ONLY retained
// view/domain/membership seeds: the v4 accepted-run domain assembly is
// unbuilt, so homing, views, and domains below are seeds, never runtime
// proof), then call the shared staged helpers. No conversion, no runtime
// change, no new files.
fn cursor_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            warp: true,
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![
            ("focusFollowsCursor".to_owned(), RawSettingValue::Bool(true)),
            ("cursorWarp".to_owned(), RawSettingValue::Bool(true)),
        ],
    )
    .expect("generation")
}

#[allow(clippy::too_many_arguments)]
fn stage_cursor(
    engine: &mut WorldEngine,
    v4: bool,
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    cursor: Option<(i32, i32)>,
    hit: Option<&str>,
    complete: bool,
    barrier: bool,
) {
    let mut legacy = report_full(
        seq,
        seq,
        vec![screen("out-a", 0), screen("out-b", 800)],
        windows.clone(),
        Vec::new(),
        focused,
        None,
        None,
    );
    legacy.cursor = cursor;
    legacy.cursor_hit = hit.map(|hit| WindowId(hit.to_owned()));
    legacy.focused_global = focused.to_owned();
    legacy.inventory_complete = complete;
    legacy.gesture_barrier = barrier;
    let mut v4report = report_v4_simple(
        seq,
        vec![screen("out-a", 0), screen("out-b", 800)],
        Vec::new(),
        windows,
        Vec::new(),
        Vec::new(),
    );
    v4report.cursor = cursor;
    v4report.cursor_hit = hit.map(|hit| WindowId(hit.to_owned()));
    v4report.focused_global = focused.to_owned();
    v4report.inventory_complete = complete;
    v4report.gesture_barrier = barrier;
    stage_either(engine, v4, &legacy, &v4report);
    seed_cursor_join(engine);
}

/// TEST-ONLY staged join seed: homes every staged fact with a reported
/// screen to that screen's current-view domain and plants one readable
/// domain per output. Stands in for the unbuilt v4 accepted-run domain
/// assembly; the classification under test still comes from the real
/// staged lanes above.
fn seed_cursor_join(engine: &mut WorldEngine) {
    engine
        .mapping
        .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
    engine
        .mapping
        .insert("out-b".to_owned(), vec!["ws-2".to_owned()]);
    engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
    engine.views.insert("out-b".to_owned(), "ws-2".to_owned());
    let home = |output: &str| DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(if output == "out-a" { "ws-1" } else { "ws-2" }.to_owned()),
    };
    let mut ids: Vec<WindowId> = engine.staged.facts.keys().cloned().collect();
    ids.sort_by(|a, b| a.0.cmp(&b.0));
    let mut homing = BTreeMap::new();
    let mut carriers_a = Vec::new();
    let mut carriers_b = Vec::new();
    for id in &ids {
        let Some(fact) = engine.staged.facts.get(id) else {
            continue;
        };
        let output = fact
            .output
            .as_ref()
            .map(|output| output.0.clone())
            .unwrap_or_else(|| "out-a".to_owned());
        if output != "out-a" && output != "out-b" {
            continue;
        }
        let key = home(&output);
        homing.insert(id.clone(), key.clone());
        if let Some(rect) = fact.rect {
            let carrier = EngineWindow {
                window: id.clone(),
                output: key.output.clone(),
                workspace: key.workspace.clone(),
                rect,
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: fact
                    .hints
                    .unwrap_or(crate::size_hints::WindowSizeHints::none()),
            };
            if output == "out-a" {
                carriers_a.push(carrier);
            } else {
                carriers_b.push(carrier);
            }
        }
    }
    engine.staged.homing = homing;
    engine.staged.domains = vec![
        StagedDomain {
            key: home("out-a"),
            bounds: rect(0, 0, 800, 560),
            gap: 0,
            outer_gap: 0,
            windows: carriers_a,
            focused: WindowId(String::new()),
            minimized: Vec::new(),
            unreadable: Vec::new(),
            closed: Vec::new(),
            readable: true,
            hold: None,
        },
        StagedDomain {
            key: home("out-b"),
            bounds: rect(800, 0, 800, 560),
            gap: 0,
            outer_gap: 0,
            windows: carriers_b,
            focused: WindowId(String::new()),
            minimized: Vec::new(),
            unreadable: Vec::new(),
            closed: Vec::new(),
            readable: true,
            hold: None,
        },
    ];
}

#[test]
fn cursor_staged_pause_and_hover_parity() {
    for v4 in [false, true] {
        let mut engine = cursor_engine();
        let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
        // Baseline: complete, unbarred, focus on win-a, cursor moved onto
        // the win-b hit. Both intakes stage the same facts first.
        engine.prev_cursor = Some((10, 10));
        stage_cursor(
            &mut engine,
            v4,
            1,
            windows(),
            "win-a",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
        );
        for window in ["win-a", "win-b"] {
            assert!(
                engine
                    .staged
                    .facts
                    .contains_key(&WindowId(window.to_owned())),
                "both intakes stage {window} (v4={v4})"
            );
        }
        assert!(
            !crate::world_cursor::cursor_game_paused(&engine),
            "no pause without fullscreen (v4={v4})"
        );
        let effect = engine
            .take_focus_follows(&BTreeSet::new())
            .expect("moved cursor over an eligible hit couples");
        assert_eq!(effect.target.0, "win-b", "hover target (v4={v4})");
        assert_eq!(
            effect.cause,
            EffectCause::FocusFollowsCursor,
            "hover cause (v4={v4})"
        );
        // Arrival: focus catches up to the hover; the same cursor and hit
        // mint nothing more.
        engine.prev_cursor = Some((110, 10));
        stage_cursor(
            &mut engine,
            v4,
            2,
            windows(),
            "win-b",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
        );
        assert!(
            engine.take_focus_follows(&BTreeSet::new()).is_none(),
            "arrival stops minting (v4={v4})"
        );
        // Static cursor over an unchanged hover never re-steals after a
        // keyboard move of focus elsewhere.
        engine.prev_cursor = Some((110, 10));
        stage_cursor(
            &mut engine,
            v4,
            3,
            windows(),
            "win-a",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
        );
        assert!(
            engine.take_focus_follows(&BTreeSet::new()).is_none(),
            "static hover never re-steals (v4={v4})"
        );
        // Movement alone couples: the real seq-1/seq-3 calls recorded the
        // win-b hover, so the same hover with a moved cursor mints even
        // with no hover transition.
        engine.prev_cursor = Some((110, 10));
        stage_cursor(
            &mut engine,
            v4,
            4,
            windows(),
            "win-a",
            Some((120, 10)),
            Some("win-b"),
            true,
            false,
        );
        let effect = engine
            .take_focus_follows(&BTreeSet::new())
            .expect("moved cursor over the same hit couples");
        assert_eq!(effect.target.0, "win-b", "movement-only target (v4={v4})");
        // Transition alone couples: the real seq-4 call recorded the win-b
        // hover, so a new hit with a static cursor mints even with no
        // cursor movement.
        engine.prev_cursor = Some((120, 10));
        stage_cursor(
            &mut engine,
            v4,
            5,
            windows(),
            "win-b",
            Some((120, 10)),
            Some("win-a"),
            true,
            false,
        );
        let effect = engine
            .take_focus_follows(&BTreeSet::new())
            .expect("new hit under a static cursor couples");
        assert_eq!(effect.target.0, "win-a", "transition-only target (v4={v4})");
        // Static counterpart: the same cursor over the same recorded hover
        // mints nothing.
        engine.prev_cursor = Some((120, 10));
        stage_cursor(
            &mut engine,
            v4,
            6,
            windows(),
            "win-b",
            Some((120, 10)),
            Some("win-a"),
            true,
            false,
        );
        assert!(
            engine.take_focus_follows(&BTreeSet::new()).is_none(),
            "static cursor over the recorded hover mints nothing (v4={v4})"
        );
        // Incomplete inventory, gesture barrier, and a held home scope all
        // gate, with the cursor still moved onto an eligible hit.
        engine.prev_cursor = Some((10, 10));
        stage_cursor(
            &mut engine,
            v4,
            7,
            windows(),
            "win-a",
            Some((110, 10)),
            Some("win-b"),
            false,
            false,
        );
        assert!(
            engine.take_focus_follows(&BTreeSet::new()).is_none(),
            "incomplete inventory gates (v4={v4})"
        );
        stage_cursor(
            &mut engine,
            v4,
            8,
            windows(),
            "win-a",
            Some((110, 10)),
            Some("win-b"),
            true,
            true,
        );
        assert!(
            engine.take_focus_follows(&BTreeSet::new()).is_none(),
            "gesture barrier gates (v4={v4})"
        );
        stage_cursor(
            &mut engine,
            v4,
            9,
            windows(),
            "win-a",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
        );
        let mut held = BTreeSet::new();
        held.insert(home_key("out-a"));
        assert!(
            engine.take_focus_follows(&held).is_none(),
            "held home scope gates (v4={v4})"
        );
        // Game pause reads the CURRENT staged capture: a focused
        // fullscreen game pauses hover despite the moved cursor and hit.
        let mut game_windows = windows();
        let mut game = fact("game", "out-a", 200);
        game.fullscreen = Some(true);
        game_windows.push(game);
        engine.prev_cursor = Some((10, 10));
        stage_cursor(
            &mut engine,
            v4,
            10,
            game_windows,
            "game",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
        );
        assert!(
            crate::world_cursor::cursor_game_paused(&engine),
            "focused fullscreen pauses (v4={v4})"
        );
        assert!(
            engine.take_focus_follows(&BTreeSet::new()).is_none(),
            "game pause gates hover (v4={v4})"
        );
        // Prior capture does not leak: refocusing away clears the pause on
        // the next staged observation.
        engine.prev_cursor = Some((110, 10));
        stage_cursor(
            &mut engine,
            v4,
            11,
            windows(),
            "win-a",
            Some((120, 10)),
            Some("win-b"),
            true,
            false,
        );
        assert!(
            !crate::world_cursor::cursor_game_paused(&engine),
            "pause clears on refocus (v4={v4})"
        );
        // No gates hold and the cursor moved: hover couples again on the
        // current staged observation. The hovered win-b differs from both
        // the cached win-a hover and the win-a focus, so this is no stale
        // cached hover.
        let effect = engine
            .take_focus_follows(&BTreeSet::new())
            .expect("refocus with a moved cursor couples");
        assert_eq!(effect.target.0, "win-b", "refocus target (v4={v4})");
    }
}

#[test]
fn cursor_v4_capability_hold_kills_hover() {
    // v4-only: a capability-held cursor-hit subject has no classified
    // eligibility, isolated to the sticky conflict. Both engines bind
    // maximize support and stage the identical v4 report; only the held
    // engine withholds native_sticky support, so only its sticky hit
    // grades. The same TEST-ONLY join seeds both engines and the seeded
    // rings/views match, so the mint/no-mint split comes from
    // classification, never seed drift.
    let caps = |native_sticky: bool| HostCapabilities {
        native_sticky,
        maximize: true,
        ..Default::default()
    };
    let settings = || vec![("focusFollowsCursor".to_owned(), RawSettingValue::Bool(true))];
    let mut supported = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        caps(true),
        settings(),
    )
    .expect("generation");
    let mut held = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        caps(false),
        settings(),
    )
    .expect("generation");
    let mut sticky = fact("win-a", "out-a", 0);
    sticky.sticky = Some(true);
    let mut report = report_v4_simple(
        1,
        vec![screen("out-a", 0)],
        Vec::new(),
        vec![sticky],
        Vec::new(),
        Vec::new(),
    );
    report.cursor = Some((10, 10));
    report.cursor_hit = Some(WindowId("win-a".to_owned()));
    report.focused_global = "win-b".to_owned();
    stage_v4_ingested(&mut supported, &report);
    stage_v4_ingested(&mut held, &report);
    // Raw lanes retain the original rows on both engines.
    for engine in [&supported, &held] {
        assert_eq!(
            engine.staged.observation.cursor_hit,
            Some(WindowId("win-a".to_owned())),
            "raw capture keeps the original hit"
        );
        assert!(
            engine
                .staged
                .observation
                .raw_windows
                .iter()
                .any(|fact| fact.window.0 == "win-a"),
            "raw capture keeps the original fact"
        );
    }
    // The held engine grades the sticky conflict to a ConflictingFact row
    // with no classified fact; the supported engine classifies the same
    // fact.
    let held_row = held
        .staged
        .held_windows
        .iter()
        .find(|held| held.window.0 == "win-a")
        .expect("sticky conflict grades to a held row");
    assert_eq!(
        held_row.kind,
        RowDiagnosticKind::ConflictingFact,
        "graded hold carries the conflicting-fact kind"
    );
    assert!(
        !held
            .staged
            .facts
            .contains_key(&WindowId("win-a".to_owned())),
        "graded hold leaves no classified fact"
    );
    assert!(
        supported
            .staged
            .facts
            .contains_key(&WindowId("win-a".to_owned())),
        "supported intake classifies the same fact"
    );
    // Same TEST-ONLY join on both engines; the seeded rings/views match.
    seed_cursor_join(&mut supported);
    seed_cursor_join(&mut held);
    assert_eq!(supported.views, held.views, "seeded views match");
    assert_eq!(supported.mapping, held.mapping, "seeded rings match");
    assert_eq!(
        supported
            .staged
            .domains
            .iter()
            .map(|domain| domain.key.clone())
            .collect::<Vec<_>>(),
        held.staged
            .domains
            .iter()
            .map(|domain| domain.key.clone())
            .collect::<Vec<_>>(),
        "seeded domain keys match"
    );
    supported.prev_cursor = Some((0, 0));
    held.prev_cursor = Some((0, 0));
    let effect = supported
        .take_focus_follows(&BTreeSet::new())
        .expect("supported hit couples over the eligible join");
    assert_eq!(effect.target.0, "win-a");
    assert_eq!(effect.cause, EffectCause::FocusFollowsCursor);
    assert!(
        held.take_focus_follows(&BTreeSet::new()).is_none(),
        "capability-held hit mints nothing"
    );
}

fn warp_row(
    window: &str,
    output: &str,
    workspace: &str,
    frame: Option<crate::geometry::Rect>,
) -> WindowDesire {
    WindowDesire {
        lifetime: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: workspace.to_owned(),
        allocation: frame,
        effective_frame: frame,
        frame_authority: ScopeAuthority::Controlled,
        membership_authority: ScopeAuthority::Controlled,
        floating: false,
        float_origin: None,
        frame_operation: None,
        clear_operation: None,
        command_seq: None,
        visibility: None,
        visibility_authority: ScopeAuthority::Untouched,
        state: Vec::new(),
    }
}

#[test]
fn warp_staged_clamp_and_last_effect_parity() {
    for v4 in [false, true] {
        let mut engine = cursor_engine();
        let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)];
        // Cursor on out-a; win-b's effective-frame centre (860, 570) falls
        // below out-b's staged work area, so the staged work area clamps it
        // to (860, 559). out-a's staged full rect proves the cursor output.
        stage_cursor(
            &mut engine,
            v4,
            1,
            windows(),
            "win-a",
            Some((10, 10)),
            None,
            true,
            false,
        );
        let rows = vec![
            warp_row("win-a", "out-a", "ws-1", Some(rect(0, 0, 40, 90))),
            warp_row("win-b", "out-b", "ws-2", Some(rect(810, 520, 100, 100))),
        ];
        let cross = vec![OneShotEffect::explicit_focus(
            11,
            WindowId("win-b".to_owned()),
            3,
        )];
        assert_eq!(
            engine.warp_for_publish(&cross, &rows, &BTreeSet::new()),
            Some((860, 559)),
            "cross-output centre clamps into staged work area (v4={v4})"
        );
        // The LAST focus effect in order decides: an earlier cross-output
        // effect followed by a same-output one warps nothing.
        let vetoed = vec![
            OneShotEffect::explicit_focus(11, WindowId("win-b".to_owned()), 3),
            OneShotEffect::explicit_focus(11, WindowId("win-a".to_owned()), 4),
        ];
        assert_eq!(
            engine.warp_for_publish(&vetoed, &rows, &BTreeSet::new()),
            None,
            "same-output last effect vetoes (v4={v4})"
        );
        // Held destination scope withholds the point.
        let mut held = BTreeSet::new();
        held.insert(DomainKey {
            output: OutputId("out-b".to_owned()),
            workspace: WorkspaceId("ws-2".to_owned()),
        });
        assert_eq!(
            engine.warp_for_publish(&cross, &rows, &held),
            None,
            "held destination withholds warp (v4={v4})"
        );
        // Incomplete inventory, gesture barrier, and a focused fullscreen
        // game each pause the point on the current staged capture.
        stage_cursor(
            &mut engine,
            v4,
            2,
            windows(),
            "win-a",
            Some((10, 10)),
            None,
            false,
            false,
        );
        assert_eq!(
            engine.warp_for_publish(&cross, &rows, &BTreeSet::new()),
            None,
            "incomplete inventory pauses warp (v4={v4})"
        );
        stage_cursor(
            &mut engine,
            v4,
            3,
            windows(),
            "win-a",
            Some((10, 10)),
            None,
            true,
            true,
        );
        assert_eq!(
            engine.warp_for_publish(&cross, &rows, &BTreeSet::new()),
            None,
            "gesture barrier pauses warp (v4={v4})"
        );
        let mut game_windows = windows();
        let mut game = fact("game", "out-a", 200);
        game.fullscreen = Some(true);
        game_windows.push(game);
        stage_cursor(
            &mut engine,
            v4,
            4,
            game_windows,
            "game",
            Some((10, 10)),
            None,
            true,
            false,
        );
        assert!(
            crate::world_cursor::cursor_game_paused(&engine),
            "warp report runs under the game pause (v4={v4})"
        );
        assert_eq!(
            engine.warp_for_publish(&cross, &rows, &BTreeSet::new()),
            None,
            "game pause withholds warp (v4={v4})"
        );
        // Unknown and off-screen cursors warp nothing.
        stage_cursor(
            &mut engine,
            v4,
            5,
            windows(),
            "win-a",
            None,
            None,
            true,
            false,
        );
        assert_eq!(
            engine.warp_for_publish(&cross, &rows, &BTreeSet::new()),
            None,
            "cursorless report warps nothing (v4={v4})"
        );
        stage_cursor(
            &mut engine,
            v4,
            6,
            windows(),
            "win-a",
            Some((5000, 5000)),
            None,
            true,
            false,
        );
        assert_eq!(
            engine.warp_for_publish(&cross, &rows, &BTreeSet::new()),
            None,
            "off-screen cursor warps nothing (v4={v4})"
        );
    }
}

// 4b-2d: settlement consumes staged observation lanes on both intakes.
// Tests drive the real `stage_facts`/`stage_facts_v4` lanes, seed the
// post-rollback staged join directly (TEST-ONLY retained
// homing/view/domain seeds: the v4 accepted-run domain assembly is
// unbuilt, so homing, views, and domains below are seeds, never runtime
// proof), then call the shared settlement entry points. Attempt evidence,
// inventory, barrier, screens, and focus all read from the current staged
// capture; the passed legacy report only carries command supersession and
// native/view proofs where the callee still takes one. No conversion, no
// runtime change, no new files.
fn settle_engine_4b2d() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            birth_clear: true,
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
    )
    .expect("generation")
}

/// TEST-ONLY staged join seed: ring, views, homing, and one readable
/// domain for `window`. Stands in for the unbuilt v4 accepted-run domain
/// assembly; the classification and attempt lanes under test still come
/// from the real staged lanes above.
fn seed_join_4b2d(engine: &mut WorldEngine, window: &str, workspace: &str) {
    engine.mapping.insert(
        "out-a".to_owned(),
        vec!["ws-1".to_owned(), "ws-9".to_owned()],
    );
    engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
    engine
        .last_views
        .insert(OutputId("out-a".to_owned()), "ws-1".to_owned());
    let key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    };
    engine
        .staged
        .homing
        .insert(WindowId(window.to_owned()), key.clone());
    engine.staged.domains.push(StagedDomain {
        key,
        bounds: rect(0, 0, 800, 560),
        gap: 0,
        outer_gap: 0,
        windows: Vec::new(),
        focused: WindowId(String::new()),
        minimized: Vec::new(),
        unreadable: Vec::new(),
        closed: Vec::new(),
        readable: true,
        hold: None,
    });
}

#[allow(clippy::too_many_arguments)]
fn stage_attempts_4b2d(
    engine: &mut WorldEngine,
    v4: bool,
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    views: Vec<(&str, &str)>,
    complete: bool,
    barrier: bool,
    held: Vec<HeldWindowRow>,
    clears: Vec<ClearAttempt>,
    members: Vec<MemberAttempt>,
    view_attempts: Vec<ViewAttempt>,
    frames: Vec<FrameAttempt>,
    closed: Vec<WindowId>,
) {
    let mut legacy = report_full(
        seq,
        seq,
        screens.clone(),
        windows.clone(),
        views,
        focused,
        None,
        None,
    );
    legacy.inventory_complete = complete;
    legacy.gesture_barrier = barrier;
    legacy.held_windows = held.clone();
    legacy.clear_attempts = clears.clone();
    legacy.member_attempts = members.clone();
    legacy.view_attempts = view_attempts.clone();
    legacy.frames = frames.clone();
    legacy.closed = closed.clone();
    let mut v4report = report_v4_simple(seq, screens, Vec::new(), windows, held, closed);
    v4report.focused_global = focused.to_owned();
    v4report.inventory_complete = complete;
    v4report.gesture_barrier = barrier;
    v4report.clear_attempts = clears;
    v4report.member_attempts = members;
    v4report.view_attempts = view_attempts;
    v4report.frames = frames;
    stage_either(engine, v4, &legacy, &v4report);
}

/// Passed report for callees that still take one: same screens, no
/// command, empty attempt lanes, and deliberately wrong focus/views so
/// only staged inputs can settle.
fn passed_report_4b2d(seq: u64) -> WorldReport {
    report_full(
        seq,
        seq,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        vec![("out-a", "ws-9")],
        "win-other",
        None,
        None,
    )
}

/// 5a seam for the 4b2d view tests below: the host-owned view echo reads
/// the staged native current, so staging carries the same current the
/// passed report names. Attempt lanes stay exactly as staged.
#[allow(clippy::too_many_arguments)]
fn stage_views_pre_4b2d(
    engine: &mut WorldEngine,
    v4: bool,
    seq: u64,
    screens: Vec<ScreenReport>,
    complete: bool,
    barrier: bool,
    view_attempts: Vec<ViewAttempt>,
    current: &str,
) {
    let native = vec![NativeObservation {
        output: OutputId("out-a".to_owned()),
        native_order: vec!["n9".to_owned(), "n1".to_owned()],
        current_native: Some(current.to_owned()),
        occupancy: vec![
            NativeOccupancy {
                native: "n9".to_owned(),
                count: None,
            },
            NativeOccupancy {
                native: "n1".to_owned(),
                count: None,
            },
        ],
    }];
    let mut legacy = report_full(
        seq,
        seq,
        screens.clone(),
        vec![fact("win-a", "out-a", 0)],
        Vec::new(),
        "win-a",
        None,
        None,
    );
    legacy.inventory_complete = complete;
    legacy.gesture_barrier = barrier;
    legacy.view_attempts = view_attempts.clone();
    legacy.native = native.clone();
    let mut v4report = report_v4_simple(
        seq,
        screens,
        Vec::new(),
        vec![fact("win-a", "out-a", 0)],
        Vec::new(),
        Vec::new(),
    );
    v4report.focused_global = "win-a".to_owned();
    v4report.inventory_complete = complete;
    v4report.gesture_barrier = barrier;
    v4report.view_attempts = view_attempts;
    v4report.native = native;
    stage_either(engine, v4, &legacy, &v4report);
}

fn max_fact_4b2d(window: &str) -> WindowFact {
    WindowFact {
        maximized: Some(true),
        ..fact(window, "out-a", 0)
    }
}

fn clear_4b2d(
    operation: &str,
    window: &str,
    setter: FrameSetter,
    pre: Option<bool>,
    post: Option<bool>,
) -> ClearAttempt {
    ClearAttempt {
        operation_id: operation.to_owned(),
        window: WindowId(window.to_owned()),
        setter,
        pre_maximized: pre,
        post_maximized: post,
        attempt_ms: 7,
    }
}

fn member_4b2d(
    operation: &str,
    window: &str,
    target_workspace: &str,
    setter: FrameSetter,
    pre: Option<WorkspaceRef>,
    post: Option<WorkspaceRef>,
) -> MemberAttempt {
    MemberAttempt {
        operation_id: operation.to_owned(),
        window: WindowId(window.to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: target_workspace.to_owned(),
        setter,
        pre,
        post,
        attempt_ms: 7,
    }
}

fn logical_4b2d(workspace: &str) -> WorkspaceRef {
    WorkspaceRef::Logical {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn view_4b2d(operation: &str, target_workspace: &str) -> ViewAttempt {
    ViewAttempt {
        operation_id: operation.to_owned(),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: target_workspace.to_owned(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: None,
        baseline: None,
        attempt_ms: 7,
    }
}

fn frame_4b2d(
    target: crate::geometry::Rect,
    setter: FrameSetter,
    ms: u64,
    dpi: (Option<u32>, Option<u32>),
) -> FrameAttempt {
    FrameAttempt {
        window: WindowId("win-a".to_owned()),
        target,
        setter,
        readback: None,
        dpi_before: dpi.0,
        dpi_after: dpi.1,
        attempt_ms: ms,
        operation_id: Some("op-f".to_owned()),
        prewrite: None,
    }
}

fn seed_clear_4b2d(engine: &mut WorldEngine, operation: &str) {
    engine
        .retained_clears
        .push(crate::world_engine::RetainedClear {
            operation_id: operation.to_owned(),
            command_seq: 13,
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            home: home_key("out-a"),
            view: Some("ws-1".to_owned()),
            focus: "win-a".to_owned(),
            float_route: false,
            same_axis: crate::directional::SameAxisMove::default(),
            pending: None,
            ambiguous: false,
        });
}

fn seed_born_4b2d(engine: &mut WorldEngine, operation: &str) {
    engine
        .retained_born
        .push(crate::world_engine::RetainedBornClear {
            operation_id: operation.to_owned(),
            window: WindowId("win-a".to_owned()),
            home: home_key("out-a"),
            pending: None,
            ambiguous: false,
        });
}

fn seed_window_transfer_4b2d(engine: &mut WorldEngine, operation: &str) {
    engine.retained.push(crate::world_engine::RetainedTransfer {
        target: crate::world_report::TransferTarget {
            operation_id: operation.to_owned(),
            kind: crate::world_report::TransferKind::Window(WindowId("win-a".to_owned())),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: "ws-1".to_owned(),
            follow: false,
            held: false,
        },
        policy: false,
        command_seq: None,
        source_output: OutputId("out-a".to_owned()),
        source_workspace: "ws-9".to_owned(),
        members: Vec::new(),
        migration_refill: None,
        migration_initial_empty: false,
        migration_source_index: None,
        migration_source_view: None,
        migration_target_view: None,
        migration_terminal: None,
        migration_selection_rev: None,
        pending_member: None,
        pending_members: BTreeMap::new(),
        pending_view: None,
    });
}

fn seed_view_transfer_4b2d(engine: &mut WorldEngine, operation: &str) {
    engine.retained.push(crate::world_engine::RetainedTransfer {
        target: crate::world_report::TransferTarget {
            operation_id: operation.to_owned(),
            kind: crate::world_report::TransferKind::View,
            target_output: OutputId("out-a".to_owned()),
            target_workspace: "ws-1".to_owned(),
            follow: false,
            held: false,
        },
        policy: false,
        command_seq: None,
        source_output: OutputId("out-a".to_owned()),
        source_workspace: "ws-1".to_owned(),
        members: Vec::new(),
        migration_refill: None,
        migration_initial_empty: false,
        migration_source_index: None,
        migration_source_view: None,
        migration_target_view: None,
        migration_terminal: None,
        migration_selection_rev: None,
        pending_member: None,
        pending_members: BTreeMap::new(),
        pending_view: None,
    });
}

/// TEST-ONLY migration join seed: two-output rings, a refill child
/// (`{op}-refill` toward the source view) pushed before its migration
/// parent so `settle_migration` reconciles them together. Staged homing
/// is seeded per report below: the v4 accepted-run domain assembly is
/// unbuilt, so this is a seed, never runtime proof.
fn seed_migration_4b2d(engine: &mut WorldEngine, operation: &str) {
    engine.mapping.insert(
        "out-a".to_owned(),
        vec!["ws-1".to_owned(), "ws-9".to_owned()],
    );
    engine.mapping.insert(
        "out-b".to_owned(),
        vec!["ws-1".to_owned(), "ws-8".to_owned()],
    );
    engine.retained.push(crate::world_engine::RetainedTransfer {
        target: crate::world_report::TransferTarget {
            operation_id: format!("{operation}-refill"),
            kind: crate::world_report::TransferKind::View,
            target_output: OutputId("out-a".to_owned()),
            target_workspace: "ws-9".to_owned(),
            follow: false,
            held: false,
        },
        policy: false,
        command_seq: None,
        source_output: OutputId("out-a".to_owned()),
        source_workspace: "ws-1".to_owned(),
        members: Vec::new(),
        migration_refill: None,
        migration_initial_empty: false,
        migration_source_index: None,
        migration_source_view: None,
        migration_target_view: None,
        migration_terminal: None,
        migration_selection_rev: None,
        pending_member: None,
        pending_members: BTreeMap::new(),
        pending_view: None,
    });
    engine.retained.push(crate::world_engine::RetainedTransfer {
        target: crate::world_report::TransferTarget {
            operation_id: operation.to_owned(),
            kind: crate::world_report::TransferKind::Migration,
            target_output: OutputId("out-b".to_owned()),
            target_workspace: "ws-1".to_owned(),
            follow: false,
            held: false,
        },
        policy: false,
        command_seq: None,
        source_output: OutputId("out-a".to_owned()),
        source_workspace: "ws-1".to_owned(),
        members: vec![WindowId("win-a".to_owned())],
        migration_refill: Some("ws-9".to_owned()),
        migration_initial_empty: false,
        migration_source_index: Some(0),
        migration_source_view: Some("ws-1".to_owned()),
        migration_target_view: Some("ws-1".to_owned()),
        migration_terminal: None,
        migration_selection_rev: None,
        pending_member: None,
        pending_members: BTreeMap::new(),
        pending_view: None,
    });
}

fn home_migration_4b2d(engine: &mut WorldEngine, output: &str, workspace: &str) {
    engine.staged.homing.insert(
        WindowId("win-a".to_owned()),
        DomainKey {
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
        },
    );
}

fn member_out_4b2d(
    operation: &str,
    window: &str,
    output: &str,
    target_workspace: &str,
    setter: FrameSetter,
    pre: Option<WorkspaceRef>,
    post: Option<WorkspaceRef>,
) -> MemberAttempt {
    MemberAttempt {
        operation_id: operation.to_owned(),
        window: WindowId(window.to_owned()),
        target_output: OutputId(output.to_owned()),
        target_workspace: target_workspace.to_owned(),
        setter,
        pre,
        post,
        attempt_ms: 7,
    }
}

fn view_out_4b2d(operation: &str, output: &str, target_workspace: &str) -> ViewAttempt {
    ViewAttempt {
        operation_id: operation.to_owned(),
        target_output: OutputId(output.to_owned()),
        target_workspace: target_workspace.to_owned(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: None,
        baseline: None,
        attempt_ms: 7,
    }
}

/// Host-owned 4b2d engine: same caps plus `host_owned`, required only
/// for the view-refusal lane (effective view pre/post translate natives
/// solely under the capability; the tiler-owned engine can never take
/// the `settle_views_pre` Refused branch).
fn settle_engine_4b2d_host() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            birth_clear: true,
            native_sticky: true,
            maximize: true,
            host_owned: true,
            ..Default::default()
        },
    )
    .expect("generation")
}

fn native_ref_4b2d(native: &str) -> WorkspaceRef {
    WorkspaceRef::Native {
        output: OutputId("out-a".to_owned()),
        workspace: crate::ids::NativeWorkspaceId::parse(native).expect("native id"),
    }
}

fn view_native_refused_4b2d(
    operation: &str,
    target_workspace: &str,
    pre_native: &str,
) -> ViewAttempt {
    ViewAttempt {
        operation_id: operation.to_owned(),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: target_workspace.to_owned(),
        setter: FrameSetter::Refused,
        pre: Some(native_ref_4b2d(pre_native)),
        post: Some(native_ref_4b2d(pre_native)),
        baseline: None,
        attempt_ms: 7,
    }
}

fn seed_native_views_4b2d(engine: &mut WorldEngine) {
    engine.mapping.insert(
        "out-a".to_owned(),
        vec!["ws-1".to_owned(), "ws-9".to_owned()],
    );
    engine.views.insert("out-a".to_owned(), "ws-9".to_owned());
    engine
        .last_views
        .insert(OutputId("out-a".to_owned()), "ws-9".to_owned());
    engine
        .native_map
        .insert(("out-a".to_owned(), "n1".to_owned()), "ws-1".to_owned());
    engine
        .native_map
        .insert(("out-a".to_owned(), "n9".to_owned()), "ws-9".to_owned());
}

fn seed_float_clear_4b2d(engine: &mut WorldEngine, operation: &str) {
    engine
        .retained_clears
        .push(crate::world_engine::RetainedClear {
            operation_id: operation.to_owned(),
            command_seq: 13,
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            home: home_key("out-a"),
            view: Some("ws-1".to_owned()),
            focus: "win-a".to_owned(),
            float_route: true,
            same_axis: crate::directional::SameAxisMove::default(),
            pending: None,
            ambiguous: false,
        });
}

fn seed_float_4b2d(
    engine: &mut WorldEngine,
    target: crate::geometry::Rect,
    source: crate::geometry::Rect,
) {
    engine
        .retained_floats
        .push(crate::world_engine::RetainedFloat {
            operation_id: "op-f".to_owned(),
            command_seq: 11,
            window: WindowId("win-a".to_owned()),
            target,
            source: Some(source),
            output: OutputId("out-a".to_owned()),
            workspace: "ws-1".to_owned(),
            ambiguous: false,
            pending: None,
        });
}

#[test]
fn settle_clear_uses_staged_lane_not_passed_report() {
    use crate::world_report::OperationOutcome;
    for v4 in [false, true] {
        // Exact staged refusal retires even though the passed report
        // carries no attempts and names the wrong focus/view.
        let mut engine = settle_engine_4b2d();
        let refused = clear_4b2d(
            "op-1",
            "win-a",
            FrameSetter::Refused,
            Some(true),
            Some(true),
        );
        stage_attempts_4b2d(
            &mut engine,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![max_fact_4b2d("win-a")],
            "win-a",
            vec![("out-a", "ws-1")],
            true,
            false,
            Vec::new(),
            vec![refused],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        seed_join_4b2d(&mut engine, "win-a", "ws-1");
        seed_clear_4b2d(&mut engine, "op-1");
        engine.settle_clears(
            passed_report_4b2d(90).command.as_ref(),
            &test_issue(&passed_report_4b2d(90)),
            false,
        );
        assert!(
            engine.retained_clears.is_empty(),
            "staged refusal retires despite contradictory report (v4={v4})"
        );
        assert!(
            engine
                .operation_settled
                .iter()
                .any(|event| event.operation_id == "op-1"
                    && event.window.0 == "win-a"
                    && event.outcome == OperationOutcome::ClearRefusedProven),
            "refused-proven event carries op/window/outcome, got {:?} (v4={v4})",
            engine.operation_settled,
            v4 = v4,
        );
        // Forged rows (operation or window mismatch) read as absent: the
        // flight holds with nothing staged and no event.
        for (what, forged) in [
            (
                "wrong-op",
                clear_4b2d(
                    "op-X",
                    "win-a",
                    FrameSetter::Refused,
                    Some(true),
                    Some(true),
                ),
            ),
            (
                "wrong-window",
                clear_4b2d(
                    "op-1",
                    "win-b",
                    FrameSetter::Refused,
                    Some(true),
                    Some(true),
                ),
            ),
        ] {
            let mut forged_engine = settle_engine_4b2d();
            stage_attempts_4b2d(
                &mut forged_engine,
                v4,
                1,
                vec![screen("out-a", 0)],
                vec![max_fact_4b2d("win-a")],
                "win-a",
                vec![("out-a", "ws-1")],
                true,
                false,
                Vec::new(),
                vec![forged],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            seed_join_4b2d(&mut forged_engine, "win-a", "ws-1");
            seed_clear_4b2d(&mut forged_engine, "op-1");
            forged_engine.settle_clears(
                passed_report_4b2d(90).command.as_ref(),
                &test_issue(&passed_report_4b2d(90)),
                false,
            );
            assert_eq!(
                forged_engine.retained_clears.len(),
                1,
                "{what} holds (v4={v4})",
                v4 = v4,
            );
            assert!(
                forged_engine.operation_settled.is_empty(),
                "{what} emits nothing (v4={v4})",
                v4 = v4,
            );
            assert!(
                forged_engine.retained_clears[0].pending.is_none(),
                "{what} stages nothing (v4={v4})",
                v4 = v4,
            );
        }
        // Receipt before each gate latches evidence, then a readable
        // recovery with an empty lane still retires via the latch.
        for (what, complete, barrier, windows, held) in [
            (
                "incomplete",
                false,
                false,
                vec![max_fact_4b2d("win-a")],
                Vec::new(),
            ),
            (
                "barrier",
                true,
                true,
                vec![max_fact_4b2d("win-a")],
                Vec::new(),
            ),
            (
                "held",
                true,
                false,
                Vec::new(),
                vec![held_window_row("win-a", "out-a", 0)],
            ),
        ] {
            let mut gated = settle_engine_4b2d();
            stage_attempts_4b2d(
                &mut gated,
                v4,
                1,
                vec![screen("out-a", 0)],
                windows,
                "win-a",
                vec![("out-a", "ws-1")],
                complete,
                barrier,
                held,
                vec![clear_4b2d(
                    "op-1",
                    "win-a",
                    FrameSetter::Refused,
                    Some(true),
                    Some(true),
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            seed_join_4b2d(&mut gated, "win-a", "ws-1");
            seed_clear_4b2d(&mut gated, "op-1");
            gated.settle_clears(
                passed_report_4b2d(90).command.as_ref(),
                &test_issue(&passed_report_4b2d(90)),
                false,
            );
            assert_eq!(
                gated.retained_clears.len(),
                1,
                "{what} holds (v4={v4})",
                v4 = v4,
            );
            assert!(
                gated.operation_settled.is_empty(),
                "{what} emits nothing (v4={v4})",
                v4 = v4,
            );
            assert!(
                gated.retained_clears[0].pending.is_some(),
                "{what} latches evidence before the hold (v4={v4})",
                v4 = v4,
            );
            // Readable recovery carries no fresh attempt: the latch
            // reconciles on readable facts.
            stage_attempts_4b2d(
                &mut gated,
                v4,
                2,
                vec![screen("out-a", 0)],
                vec![max_fact_4b2d("win-a")],
                "win-a",
                vec![("out-a", "ws-1")],
                true,
                false,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            seed_join_4b2d(&mut gated, "win-a", "ws-1");
            gated.settle_clears(
                passed_report_4b2d(91).command.as_ref(),
                &test_issue(&passed_report_4b2d(91)),
                false,
            );
            assert!(
                gated.retained_clears.is_empty(),
                "{what} recovers readable (v4={v4})",
                v4 = v4,
            );
            assert!(
                gated
                    .operation_settled
                    .iter()
                    .any(|event| event.operation_id == "op-1"
                        && event.outcome == OperationOutcome::ClearRefusedProven),
                "{what} recovery retires refused-proven (v4={v4})",
                v4 = v4,
            );
        }
    }
}

#[test]
fn settle_born_uses_staged_lane_with_gates_and_recovery() {
    use crate::world_report::OperationOutcome;
    for v4 in [false, true] {
        // settle_born_clears takes no report: the staged capture alone
        // decides. Exact staged refusal retires.
        let mut engine = settle_engine_4b2d();
        stage_attempts_4b2d(
            &mut engine,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![max_fact_4b2d("win-a")],
            "win-a",
            vec![("out-a", "ws-1")],
            true,
            false,
            Vec::new(),
            vec![clear_4b2d(
                "op-born-1",
                "win-a",
                FrameSetter::Refused,
                Some(true),
                Some(true),
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        seed_join_4b2d(&mut engine, "win-a", "ws-1");
        seed_born_4b2d(&mut engine, "op-born-1");
        engine.settle_born_clears();
        assert!(
            engine.retained_born.is_empty(),
            "staged refusal retires (v4={v4})"
        );
        assert!(
            engine
                .operation_settled
                .iter()
                .any(|event| event.operation_id == "op-born-1"
                    && event.window.0 == "win-a"
                    && event.outcome == OperationOutcome::ClearRefusedProven),
            "refused-proven event carries op/window/outcome, got {:?} (v4={v4})",
            engine.operation_settled,
            v4 = v4,
        );
        for (what, forged) in [
            (
                "wrong-op",
                clear_4b2d(
                    "op-born-X",
                    "win-a",
                    FrameSetter::Refused,
                    Some(true),
                    Some(true),
                ),
            ),
            (
                "wrong-window",
                clear_4b2d(
                    "op-born-1",
                    "win-b",
                    FrameSetter::Refused,
                    Some(true),
                    Some(true),
                ),
            ),
        ] {
            let mut forged_engine = settle_engine_4b2d();
            stage_attempts_4b2d(
                &mut forged_engine,
                v4,
                1,
                vec![screen("out-a", 0)],
                vec![max_fact_4b2d("win-a")],
                "win-a",
                vec![("out-a", "ws-1")],
                true,
                false,
                Vec::new(),
                vec![forged],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            seed_join_4b2d(&mut forged_engine, "win-a", "ws-1");
            seed_born_4b2d(&mut forged_engine, "op-born-1");
            forged_engine.settle_born_clears();
            assert_eq!(
                forged_engine.retained_born.len(),
                1,
                "{what} holds (v4={v4})",
                v4 = v4,
            );
            assert!(
                forged_engine.operation_settled.is_empty(),
                "{what} emits nothing (v4={v4})",
                v4 = v4,
            );
        }
        for (what, complete, barrier, windows, held) in [
            (
                "incomplete",
                false,
                false,
                vec![max_fact_4b2d("win-a")],
                Vec::new(),
            ),
            (
                "barrier",
                true,
                true,
                vec![max_fact_4b2d("win-a")],
                Vec::new(),
            ),
            (
                "held",
                true,
                false,
                Vec::new(),
                vec![held_window_row("win-a", "out-a", 0)],
            ),
        ] {
            let mut gated = settle_engine_4b2d();
            stage_attempts_4b2d(
                &mut gated,
                v4,
                1,
                vec![screen("out-a", 0)],
                windows,
                "win-a",
                vec![("out-a", "ws-1")],
                complete,
                barrier,
                held,
                vec![clear_4b2d(
                    "op-born-1",
                    "win-a",
                    FrameSetter::Refused,
                    Some(true),
                    Some(true),
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            seed_join_4b2d(&mut gated, "win-a", "ws-1");
            seed_born_4b2d(&mut gated, "op-born-1");
            gated.settle_born_clears();
            assert_eq!(
                gated.retained_born.len(),
                1,
                "{what} holds (v4={v4})",
                v4 = v4,
            );
            assert!(
                gated.retained_born[0].pending.is_some(),
                "{what} latches evidence before the hold (v4={v4})",
                v4 = v4,
            );
            stage_attempts_4b2d(
                &mut gated,
                v4,
                2,
                vec![screen("out-a", 0)],
                vec![max_fact_4b2d("win-a")],
                "win-a",
                vec![("out-a", "ws-1")],
                true,
                false,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            seed_join_4b2d(&mut gated, "win-a", "ws-1");
            gated.settle_born_clears();
            assert!(
                gated.retained_born.is_empty(),
                "{what} recovers readable (v4={v4})",
                v4 = v4,
            );
        }
    }
}

#[test]
fn settle_transfer_uses_staged_member_lane_not_passed_report() {
    for v4 in [false, true] {
        // A staged accepted logical post proves arrival even though the
        // staged homing still names the source and the passed report
        // carries no member attempts at all.
        let mut engine = settle_engine_4b2d();
        let arrival = member_4b2d(
            "op-t",
            "win-a",
            "ws-1",
            FrameSetter::Accepted,
            None,
            Some(logical_4b2d("ws-1")),
        );
        stage_attempts_4b2d(
            &mut engine,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            vec![("out-a", "ws-1")],
            true,
            false,
            Vec::new(),
            Vec::new(),
            vec![arrival],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        seed_join_4b2d(&mut engine, "win-a", "ws-9");
        seed_window_transfer_4b2d(&mut engine, "op-t");
        engine.settle_rest(
            passed_report_4b2d(90).command.as_ref(),
            &test_issue(&passed_report_4b2d(90)),
            false,
        );
        assert!(
            engine.retained.is_empty(),
            "staged arrival proof settles despite contradictory report (v4={v4})"
        );
        // Forged member rows read as absent: the flight holds at the
        // source with nothing staged.
        for (what, forged) in [
            (
                "wrong-op",
                member_4b2d(
                    "op-X",
                    "win-a",
                    "ws-1",
                    FrameSetter::Accepted,
                    None,
                    Some(logical_4b2d("ws-1")),
                ),
            ),
            (
                "wrong-window",
                member_4b2d(
                    "op-t",
                    "win-b",
                    "ws-1",
                    FrameSetter::Accepted,
                    None,
                    Some(logical_4b2d("ws-1")),
                ),
            ),
            (
                "wrong-target",
                member_4b2d(
                    "op-t",
                    "win-a",
                    "ws-9",
                    FrameSetter::Accepted,
                    None,
                    Some(logical_4b2d("ws-9")),
                ),
            ),
        ] {
            let mut forged_engine = settle_engine_4b2d();
            stage_attempts_4b2d(
                &mut forged_engine,
                v4,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                "win-a",
                vec![("out-a", "ws-1")],
                true,
                false,
                Vec::new(),
                Vec::new(),
                vec![forged],
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            seed_join_4b2d(&mut forged_engine, "win-a", "ws-9");
            seed_window_transfer_4b2d(&mut forged_engine, "op-t");
            forged_engine.settle_rest(
                passed_report_4b2d(90).command.as_ref(),
                &test_issue(&passed_report_4b2d(90)),
                false,
            );
            assert_eq!(
                forged_engine.retained.len(),
                1,
                "{what} holds (v4={v4})",
                v4 = v4,
            );
            assert!(
                forged_engine.retained[0].pending_member.is_none(),
                "{what} stages nothing (v4={v4})",
                v4 = v4,
            );
        }
        // Proven refusal reconciles against staged homing: the incomplete
        // gate holds with the evidence latched, then the readable
        // recovery with an empty lane settles to observed.
        let refused = member_4b2d(
            "op-t",
            "win-a",
            "ws-1",
            FrameSetter::Refused,
            Some(logical_4b2d("ws-9")),
            Some(logical_4b2d("ws-9")),
        );
        let mut gated = settle_engine_4b2d();
        stage_attempts_4b2d(
            &mut gated,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            vec![("out-a", "ws-1")],
            false,
            false,
            Vec::new(),
            Vec::new(),
            vec![refused],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        seed_join_4b2d(&mut gated, "win-a", "ws-9");
        seed_window_transfer_4b2d(&mut gated, "op-t");
        gated.settle_rest(
            passed_report_4b2d(90).command.as_ref(),
            &test_issue(&passed_report_4b2d(90)),
            false,
        );
        assert_eq!(
            gated.retained.len(),
            1,
            "incomplete holds the refused flight (v4={v4})",
            v4 = v4,
        );
        assert!(
            gated.retained[0].pending_member.is_some(),
            "refusal evidence latches before the hold (v4={v4})",
            v4 = v4,
        );
        assert!(
            gated.retained[0].target.held,
            "gate marks held (v4={v4})",
            v4 = v4,
        );
        stage_attempts_4b2d(
            &mut gated,
            v4,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            vec![("out-a", "ws-1")],
            true,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        seed_join_4b2d(&mut gated, "win-a", "ws-9");
        gated.settle_rest(
            passed_report_4b2d(91).command.as_ref(),
            &test_issue(&passed_report_4b2d(91)),
            false,
        );
        assert!(
            gated.retained.is_empty(),
            "readable recovery settles the refused flight (v4={v4})",
            v4 = v4,
        );
        assert_eq!(
            gated.pending_forget.len(),
            1,
            "refusal defers the target forget (v4={v4})",
            v4 = v4,
        );
    }
}

#[test]
fn settle_view_stages_pending_across_stale_echo_on_both_intakes() {
    for v4 in [false, true] {
        // Native-free view proof is the frozen selection binding, never
        // the echo lane: the flight is kept for its binding terminal and
        // no staged attempt latches here, on both intakes.
        let mut engine = settle_engine_4b2d();
        engine.views.insert("out-a".to_owned(), "ws-9".to_owned());
        engine
            .last_views
            .insert(OutputId("out-a".to_owned()), "ws-9".to_owned());
        stage_attempts_4b2d(
            &mut engine,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            vec![("out-a", "ws-9")],
            true,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![view_4b2d("op-v", "ws-1")],
            Vec::new(),
            Vec::new(),
        );
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-9".to_owned()],
        );
        seed_view_transfer_4b2d(&mut engine, "op-v");
        engine.settle_views_pre();
        assert_eq!(
            engine.retained.len(),
            1,
            "native-free flight kept for its binding (v4={v4})",
            v4 = v4,
        );
        assert!(
            engine.retained[0].pending_view.is_none(),
            "native-free latches no echo attempt (v4={v4})",
            v4 = v4,
        );
        assert_eq!(
            engine.views.get("out-a"),
            Some(&"ws-9".to_owned()),
            "no echo confirm moves the native-free view (v4={v4})",
            v4 = v4,
        );
        // Forged view rows read as absent: same keep, no latch.
        for (what, forged) in [
            ("wrong-op", view_4b2d("op-X", "ws-1")),
            ("wrong-target", view_4b2d("op-v", "ws-9")),
        ] {
            let mut forged_engine = settle_engine_4b2d();
            forged_engine
                .views
                .insert("out-a".to_owned(), "ws-9".to_owned());
            forged_engine
                .last_views
                .insert(OutputId("out-a".to_owned()), "ws-9".to_owned());
            stage_attempts_4b2d(
                &mut forged_engine,
                v4,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                "win-a",
                vec![("out-a", "ws-9")],
                true,
                false,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                vec![forged],
                Vec::new(),
                Vec::new(),
            );
            forged_engine.mapping.insert(
                "out-a".to_owned(),
                vec!["ws-1".to_owned(), "ws-9".to_owned()],
            );
            seed_view_transfer_4b2d(&mut forged_engine, "op-v");
            forged_engine.settle_views_pre();
            assert_eq!(
                forged_engine.retained.len(),
                1,
                "{what} keeps the flight (v4={v4})",
                v4 = v4,
            );
            assert!(
                forged_engine.retained[0].pending_view.is_none(),
                "{what} stages nothing (v4={v4})",
                v4 = v4,
            );
        }
    }
    // Exact echo no longer confirms native-free: the flight is kept for
    // the frozen selection binding terminal, never settled from echoes.
    let mut engine = settle_engine_4b2d();
    engine.views.insert("out-a".to_owned(), "ws-9".to_owned());
    engine
        .last_views
        .insert(OutputId("out-a".to_owned()), "ws-9".to_owned());
    stage_attempts_4b2d(
        &mut engine,
        false,
        1,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        vec![("out-a", "ws-1")],
        true,
        false,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    engine.mapping.insert(
        "out-a".to_owned(),
        vec!["ws-1".to_owned(), "ws-9".to_owned()],
    );
    seed_view_transfer_4b2d(&mut engine, "op-v");
    engine.settle_views_pre();
    assert_eq!(
        engine.retained.len(),
        1,
        "exact echo keeps the native-free flight for its binding"
    );
    assert!(
        engine.retained[0].pending_view.is_none(),
        "exact echo latches nothing native-free"
    );
    assert_eq!(
        engine.views.get("out-a"),
        Some(&"ws-9".to_owned()),
        "echo alone never moves the native-free view"
    );
}

#[test]
fn settle_float_uses_staged_frames_with_replay_dpi_and_close() {
    use crate::world_report::OperationOutcome;
    let target = rect(404, 8, 388, 544);
    let source = rect(100, 100, 200, 200);
    let external = rect(10, 10, 50, 50);
    let seed_float = |engine: &mut WorldEngine| {
        engine
            .retained_floats
            .push(crate::world_engine::RetainedFloat {
                operation_id: "op-f".to_owned(),
                command_seq: 11,
                window: WindowId("win-a".to_owned()),
                target,
                source: Some(source),
                output: OutputId("out-a".to_owned()),
                workspace: "ws-1".to_owned(),
                ambiguous: false,
                pending: None,
            });
    };
    let fact_at = |rect: crate::geometry::Rect| {
        let mut row = fact("win-a", "out-a", 0);
        row.rect = Some(rect);
        row
    };
    for v4 in [false, true] {
        // Observation alone at the target confirms with no attempts.
        let mut engine = settle_engine_4b2d();
        stage_attempts_4b2d(
            &mut engine,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![fact_at(target)],
            "win-a",
            Vec::new(),
            true,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        seed_float(&mut engine);
        engine.settle_floats();
        assert!(
            engine.retained_floats.is_empty(),
            "arrival confirms (v4={v4})",
            v4 = v4,
        );
        assert!(
            engine
                .operation_settled
                .iter()
                .any(|event| event.operation_id == "op-f"
                    && event.window.0 == "win-a"
                    && event.outcome == OperationOutcome::FloatFrameProven),
            "frame-proven event carries op/window/outcome (v4={v4})",
            v4 = v4,
        );
        // A forged frame tuple (wrong operation) reads as absent: the
        // source frame holds with nothing staged.
        let mut forged = frame_4b2d(target, FrameSetter::Accepted, 5, (None, None));
        forged.operation_id = Some("op-X".to_owned());
        let mut forged_engine = settle_engine_4b2d();
        stage_attempts_4b2d(
            &mut forged_engine,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![fact_at(source)],
            "win-a",
            Vec::new(),
            true,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![forged],
            Vec::new(),
        );
        seed_float(&mut forged_engine);
        forged_engine.settle_floats();
        assert_eq!(
            forged_engine.retained_floats.len(),
            1,
            "forged frame holds (v4={v4})",
            v4 = v4,
        );
        assert!(
            forged_engine.retained_floats[0].pending.is_none(),
            "forged frame stages nothing (v4={v4})",
            v4 = v4,
        );
        // Stale replay is ignored: the third-location frame abandons as
        // external input instead of latching ambiguity.
        let prior = frame_4b2d(target, FrameSetter::Accepted, 5, (None, None));
        let mut replayed = settle_engine_4b2d();
        stage_attempts_4b2d(
            &mut replayed,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![fact_at(external)],
            "win-a",
            Vec::new(),
            true,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![prior.clone()],
            Vec::new(),
        );
        seed_float(&mut replayed);
        replayed
            .consumed_frames
            .insert(WindowId("win-a".to_owned()), prior);
        replayed.settle_floats();
        assert!(
            replayed.retained_floats.is_empty(),
            "replay keeps external abandonment (v4={v4})",
            v4 = v4,
        );
        assert!(
            replayed
                .operation_settled
                .iter()
                .any(|event| event.operation_id == "op-f"
                    && event.outcome == OperationOutcome::FloatAbandoned),
            "replay abandons as external (v4={v4})",
            v4 = v4,
        );
        // Fresh evidence on the same shape latches ambiguity instead.
        let mut fresh = settle_engine_4b2d();
        stage_attempts_4b2d(
            &mut fresh,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![fact_at(external)],
            "win-a",
            Vec::new(),
            true,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![frame_4b2d(target, FrameSetter::Accepted, 6, (None, None))],
            Vec::new(),
        );
        seed_float(&mut fresh);
        fresh.consumed_frames.insert(
            WindowId("win-a".to_owned()),
            frame_4b2d(target, FrameSetter::Accepted, 5, (None, None)),
        );
        fresh.settle_floats();
        assert_eq!(
            fresh.retained_floats.len(),
            1,
            "fresh evidence holds (v4={v4})",
            v4 = v4,
        );
        assert!(
            fresh.retained_floats[0].ambiguous,
            "fresh evidence latches ambiguity (v4={v4})",
            v4 = v4,
        );
        assert!(
            fresh.operation_settled.is_empty(),
            "fresh evidence emits nothing (v4={v4})",
            v4 = v4,
        );
        // A fresh DPI transition suspends same-report confirmation; the
        // next stable observation confirms.
        let mut dpi = settle_engine_4b2d();
        stage_attempts_4b2d(
            &mut dpi,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![fact_at(target)],
            "win-a",
            Vec::new(),
            true,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![frame_4b2d(
                target,
                FrameSetter::Accepted,
                5,
                (Some(96), Some(192)),
            )],
            Vec::new(),
        );
        seed_float(&mut dpi);
        dpi.settle_floats();
        assert_eq!(
            dpi.retained_floats.len(),
            1,
            "DPI transition suspends confirmation (v4={v4})",
            v4 = v4,
        );
        assert!(
            dpi.operation_settled.is_empty(),
            "DPI transition emits nothing (v4={v4})",
            v4 = v4,
        );
        stage_attempts_4b2d(
            &mut dpi,
            v4,
            2,
            vec![screen("out-a", 0)],
            vec![fact_at(target)],
            "win-a",
            Vec::new(),
            true,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        dpi.settle_floats();
        assert!(
            dpi.retained_floats.is_empty(),
            "stable recovery confirms (v4={v4})",
            v4 = v4,
        );
        assert!(
            dpi.operation_settled
                .iter()
                .any(|event| event.operation_id == "op-f"
                    && event.outcome == OperationOutcome::FloatFrameProven),
            "stable recovery proves the frame (v4={v4})",
            v4 = v4,
        );
        // Explicit close settles even across an inventory gap.
        let mut closed = settle_engine_4b2d();
        stage_attempts_4b2d(
            &mut closed,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![fact_at(source)],
            "win-a",
            Vec::new(),
            false,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![WindowId("win-a".to_owned())],
        );
        seed_float(&mut closed);
        closed.settle_floats();
        assert!(
            closed.retained_floats.is_empty(),
            "close settles across gaps (v4={v4})",
            v4 = v4,
        );
        assert!(
            closed
                .operation_settled
                .iter()
                .any(|event| event.operation_id == "op-f"
                    && event.outcome == OperationOutcome::FloatAbandoned
                    && event.cause.as_deref() == Some("closed")),
            "close abandons with cause (v4={v4})",
            v4 = v4,
        );
    }
}

#[test]
fn settle_migration_uses_staged_lanes_not_passed_report() {
    let screens = || vec![screen("out-a", 0), screen("out-b", 800)];
    let views = || vec![("out-a", "ws-9"), ("out-b", "ws-1")];
    let windows = || vec![fact("win-a", "out-b", 800)];
    let arrival = || {
        member_out_4b2d(
            "op-m",
            "win-a",
            "out-b",
            "ws-1",
            FrameSetter::Accepted,
            None,
            Some(WorkspaceRef::Logical {
                output: OutputId("out-b".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
            }),
        )
    };
    let parent_view = || view_out_4b2d("op-m", "out-b", "ws-1");
    let refill_view = || view_4b2d("op-m-refill", "ws-9");
    let parent_of = |engine: &WorldEngine| {
        engine
            .retained
            .iter()
            .find(|row| row.target.operation_id == "op-m")
            .expect("migration parent held")
            .clone()
    };
    let child_of = |engine: &WorldEngine| {
        engine
            .retained
            .iter()
            .find(|row| row.target.operation_id == "op-m-refill")
            .expect("refill child held")
            .clone()
    };
    for v4 in [false, true] {
        // The v4 wire carries no explicit views lane, so staged echoes
        // stay unknown there: echo-gated settlement below holds on v4
        // while receipt latching is intake-independent. The legacy legs
        // prove the full settle; the v4 legs prove the hold keeps every
        // staged receipt.
        // Staged arrival at the target homing settles on both wires even
        // though the passed report carries no attempts: membership arrival
        // retires the native-free parent with no echo proof (v4 carries
        // no explicit views lane, so no binding ever reverts to the echo
        // path there either).
        let mut engine = settle_engine_4b2d();
        stage_attempts_4b2d(
            &mut engine,
            v4,
            1,
            screens(),
            windows(),
            "win-a",
            views(),
            true,
            false,
            Vec::new(),
            Vec::new(),
            vec![arrival()],
            vec![parent_view(), refill_view()],
            Vec::new(),
            Vec::new(),
        );
        seed_migration_4b2d(&mut engine, "op-m");
        home_migration_4b2d(&mut engine, "out-b", "ws-1");
        engine.settle_rest(
            passed_report_4b2d(90).command.as_ref(),
            &test_issue(&passed_report_4b2d(90)),
            false,
        );
        if !v4 {
            assert!(
                engine.retained.is_empty(),
                "staged arrival plus exact echoes settles despite empty passed lanes"
            );
        } else {
            // The native-free binding owns both views through the member
            // settlement: staged arrival retires the parent with no echo
            // proof, so v4 settles exactly like legacy (no binding ever
            // reverts to the echo path).
            assert!(
                engine.retained.is_empty(),
                "staged arrival settles without echoes (v4={v4})",
                v4 = v4,
            );
        }
        // Forged member rows read as absent: homing stays at the source,
        // the flight holds with no member evidence staged.
        for (what, forged) in [
            (
                "wrong-op",
                member_out_4b2d(
                    "op-X",
                    "win-a",
                    "out-b",
                    "ws-1",
                    FrameSetter::Accepted,
                    None,
                    Some(logical_4b2d("ws-1")),
                ),
            ),
            (
                "wrong-target",
                member_out_4b2d(
                    "op-m",
                    "win-a",
                    "out-b",
                    "ws-9",
                    FrameSetter::Accepted,
                    None,
                    Some(logical_4b2d("ws-9")),
                ),
            ),
        ] {
            let mut forged_engine = settle_engine_4b2d();
            stage_attempts_4b2d(
                &mut forged_engine,
                v4,
                1,
                screens(),
                windows(),
                "win-a",
                views(),
                true,
                false,
                Vec::new(),
                Vec::new(),
                vec![forged],
                vec![parent_view(), refill_view()],
                Vec::new(),
                Vec::new(),
            );
            seed_migration_4b2d(&mut forged_engine, "op-m");
            home_migration_4b2d(&mut forged_engine, "out-a", "ws-1");
            forged_engine.settle_rest(
                passed_report_4b2d(90).command.as_ref(),
                &test_issue(&passed_report_4b2d(90)),
                false,
            );
            // The synthetic refill child holds echo-less on both intakes:
            // the staged echo lane carries no logical echoes native-free,
            // and production no longer mints refill children on this route
            // (the frozen selection binding owns both views). Only the
            // parent holds on the outstanding member with no member
            // evidence staged.
            assert_eq!(
                forged_engine.retained.len(),
                2,
                "{what} holds (v4={v4})",
                v4 = v4,
            );
            assert!(
                parent_of(&forged_engine).pending_members.is_empty(),
                "{what} stages no member (v4={v4})",
                v4 = v4,
            );
        }
        // Receipt latches before an incomplete hold; a later barrier
        // report replaces the member tuple outright (latest-wins, never
        // first-wins); a readable empty-lane recovery then settles.
        let mut gated = settle_engine_4b2d();
        stage_attempts_4b2d(
            &mut gated,
            v4,
            1,
            screens(),
            windows(),
            "win-a",
            views(),
            false,
            false,
            Vec::new(),
            Vec::new(),
            vec![arrival()],
            vec![parent_view(), refill_view()],
            Vec::new(),
            Vec::new(),
        );
        seed_migration_4b2d(&mut gated, "op-m");
        home_migration_4b2d(&mut gated, "out-b", "ws-1");
        gated.settle_rest(
            passed_report_4b2d(90).command.as_ref(),
            &test_issue(&passed_report_4b2d(90)),
            false,
        );
        // Eligible homing and echoes: only the incomplete gate holds, so
        // the hold discriminates the gate. The synthetic refill child
        // holds echo-less on both intakes (no logical echoes staged
        // native-free; production mints no refill children here).
        assert_eq!(
            gated.retained.len(),
            2,
            "incomplete holds (v4={v4})",
            v4 = v4,
        );
        assert_eq!(
            parent_of(&gated)
                .pending_members
                .get(&WindowId("win-a".to_owned())),
            Some(&arrival()),
            "member receipt latches before the hold (v4={v4})",
            v4 = v4,
        );
        assert_eq!(
            parent_of(&gated).pending_view,
            Some(parent_view()),
            "parent view receipt latches before the hold (v4={v4})",
            v4 = v4,
        );
        assert!(
            parent_of(&gated).target.held,
            "latched receipt marks held (v4={v4})",
            v4 = v4,
        );
        if v4 {
            assert_eq!(
                child_of(&gated).pending_view,
                Some(refill_view()),
                "refill receipt latches before the hold (v4={v4})",
                v4 = v4,
            );
        }
        let replacement = crate::world_report::MemberAttempt {
            attempt_ms: 9,
            ..arrival()
        };
        stage_attempts_4b2d(
            &mut gated,
            v4,
            2,
            screens(),
            windows(),
            "win-a",
            views(),
            true,
            true,
            Vec::new(),
            Vec::new(),
            vec![replacement.clone()],
            vec![parent_view(), refill_view()],
            Vec::new(),
            Vec::new(),
        );
        home_migration_4b2d(&mut gated, "out-b", "ws-1");
        gated.settle_rest(
            passed_report_4b2d(91).command.as_ref(),
            &test_issue(&passed_report_4b2d(91)),
            false,
        );
        assert_eq!(
            gated.retained.len(),
            2,
            "barrier holds with the echo-less refill child (v4={v4})",
            v4 = v4,
        );
        assert_eq!(
            parent_of(&gated)
                .pending_members
                .get(&WindowId("win-a".to_owned())),
            Some(&replacement),
            "later tuple replaces, never first-wins (v4={v4})",
            v4 = v4,
        );
        stage_attempts_4b2d(
            &mut gated,
            v4,
            3,
            screens(),
            windows(),
            "win-a",
            views(),
            true,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        home_migration_4b2d(&mut gated, "out-b", "ws-1");
        gated.settle_rest(
            passed_report_4b2d(92).command.as_ref(),
            &test_issue(&passed_report_4b2d(92)),
            false,
        );
        // The readable empty-lane recovery settles on both wires:
        // staged target homing already proved arrival, so the parent
        // retires with no echo proof (v4 settles exactly like legacy).
        assert!(
            gated.retained.is_empty(),
            "readable empty-lane recovery settles (v4={v4})",
            v4 = v4,
        );
    }
}

#[test]
fn settle_views_pre_refused_reads_staged_screens_not_passed_report() {
    for v4 in [false, true] {
        // View refusal needs the host-owned native lane: effective view
        // pre/post translate natives solely under the capability, so the
        // tiler-owned 4b2d engine can never take this branch.
        let refused = || view_native_refused_4b2d("op-v", "ws-1", "n9");
        // Each gap holds with the exact staged attempt latched, then a
        // readable empty-lane recovery settles to observed. The first row
        // is the discriminator: the staged screen is incomplete while the
        // passed report screen is complete, yet settlement still holds.
        for (what, complete, staged_complete, barrier) in [
            ("staged-screen", true, false, false),
            ("incomplete", false, true, false),
            ("barrier", true, true, true),
        ] {
            let mut engine = settle_engine_4b2d_host();
            let mut staged_screen = screen("out-a", 0);
            staged_screen.complete = staged_complete;
            stage_views_pre_4b2d(
                &mut engine,
                v4,
                1,
                vec![staged_screen],
                complete,
                barrier,
                vec![refused()],
                "n9",
            );
            seed_native_views_4b2d(&mut engine);
            seed_view_transfer_4b2d(&mut engine, "op-v");
            engine.settle_views_pre();
            assert_eq!(engine.retained.len(), 1, "{what} holds (v4={v4})", v4 = v4,);
            assert_eq!(
                engine.retained[0].pending_view,
                Some(refused()),
                "{what} latches before the hold (v4={v4})",
                v4 = v4,
            );
            stage_views_pre_4b2d(
                &mut engine,
                v4,
                2,
                vec![screen("out-a", 0)],
                true,
                false,
                Vec::new(),
                "n9",
            );
            engine.settle_views_pre();
            assert!(
                engine.retained.is_empty(),
                "{what} recovers readable (v4={v4})",
                v4 = v4,
            );
        }
        // Positive control: a complete staged capture settles outright.
        let mut engine = settle_engine_4b2d_host();
        stage_views_pre_4b2d(
            &mut engine,
            v4,
            1,
            vec![screen("out-a", 0)],
            true,
            false,
            vec![refused()],
            "n9",
        );
        seed_native_views_4b2d(&mut engine);
        seed_view_transfer_4b2d(&mut engine, "op-v");
        engine.settle_views_pre();
        assert!(
            engine.retained.is_empty(),
            "complete eligible refusal settles (v4={v4})",
            v4 = v4,
        );
        // A forged view row reads as absent: the stale echo holds with
        // nothing staged.
        let mut forged_engine = settle_engine_4b2d_host();
        stage_views_pre_4b2d(
            &mut forged_engine,
            v4,
            1,
            vec![screen("out-a", 0)],
            true,
            false,
            vec![view_native_refused_4b2d("op-X", "ws-1", "n9")],
            "n9",
        );
        seed_native_views_4b2d(&mut forged_engine);
        seed_view_transfer_4b2d(&mut forged_engine, "op-v");
        forged_engine.settle_views_pre();
        assert_eq!(
            forged_engine.retained.len(),
            1,
            "forged row holds (v4={v4})",
            v4 = v4,
        );
        assert!(
            forged_engine.retained[0].pending_view.is_none(),
            "forged row stages nothing (v4={v4})",
            v4 = v4,
        );
        // An echo naming the target confirms and adopts despite the same
        // staged attempt shape proving nothing there.
        let mut proving = settle_engine_4b2d_host();
        stage_views_pre_4b2d(
            &mut proving,
            v4,
            1,
            vec![screen("out-a", 0)],
            true,
            false,
            vec![refused()],
            "n1",
        );
        seed_native_views_4b2d(&mut proving);
        seed_view_transfer_4b2d(&mut proving, "op-v");
        proving.settle_views_pre();
        assert!(
            proving.retained.is_empty(),
            "exact echo confirms (v4={v4})",
            v4 = v4,
        );
        assert_eq!(
            proving.views.get("out-a"),
            Some(&"ws-1".to_owned()),
            "confirmation adopts the proven view (v4={v4})",
            v4 = v4,
        );
    }
}

#[test]
fn settle_clear_focus_view_home_read_staged_not_passed_report() {
    use crate::world_report::OperationOutcome;
    let refused = || {
        clear_4b2d(
            "op-1",
            "win-a",
            FrameSetter::Refused,
            Some(true),
            Some(true),
        )
    };
    for v4 in [false, true] {
        // Staged focus divergence abandons even though the passed report
        // names the bound subject: the staged gate fires before refusal.
        let mut engine = settle_engine_4b2d();
        stage_attempts_4b2d(
            &mut engine,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![max_fact_4b2d("win-a")],
            "win-b",
            vec![("out-a", "ws-1")],
            true,
            false,
            Vec::new(),
            vec![refused()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        seed_join_4b2d(&mut engine, "win-a", "ws-1");
        seed_clear_4b2d(&mut engine, "op-1");
        let mut proving_focus = passed_report_4b2d(90);
        proving_focus.focused_global = "win-a".to_owned();
        engine.settle_clears(
            proving_focus.command.as_ref(),
            &test_issue(&proving_focus),
            false,
        );
        assert!(
            engine.retained_clears.is_empty(),
            "staged focus divergence retires (v4={v4})",
            v4 = v4,
        );
        assert!(
            engine
                .operation_settled
                .iter()
                .any(|event| event.operation_id == "op-1"
                    && event.outcome == OperationOutcome::ClearAbandonedFocusChanged),
            "divergence abandons at the staged focus gate, got {:?} (v4={v4})",
            engine.operation_settled,
            v4 = v4,
        );
        // Staged view divergence abandons with the retained view intact:
        // the passed report carries no view authority for clears. The v4
        // wire has no explicit views lane, so there the divergent retained
        // view itself governs through the same effective-view fallback.
        let mut viewed = settle_engine_4b2d();
        stage_attempts_4b2d(
            &mut viewed,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![max_fact_4b2d("win-a")],
            "win-a",
            vec![("out-a", "ws-9")],
            true,
            false,
            Vec::new(),
            vec![refused()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        seed_join_4b2d(&mut viewed, "win-a", "ws-1");
        if v4 {
            viewed.views.insert("out-a".to_owned(), "ws-9".to_owned());
        }
        seed_clear_4b2d(&mut viewed, "op-1");
        viewed.settle_clears(
            passed_report_4b2d(90).command.as_ref(),
            &test_issue(&passed_report_4b2d(90)),
            false,
        );
        if v4 {
            assert!(
                viewed.retained_clears.is_empty(),
                "staged view divergence retires (v4={v4})",
                v4 = v4,
            );
            assert!(
                viewed
                    .operation_settled
                    .iter()
                    .any(|event| event.operation_id == "op-1"
                        && event.outcome == OperationOutcome::ClearAbandonedViewChanged),
                "divergence abandons at the staged view gate, got {:?} (v4={v4})",
                viewed.operation_settled,
                v4 = v4,
            );
        } else {
            // Legacy staged-only views row is inert native-free (no echo
            // ingress into the effective view): no abandon at the view
            // gate, so the staged refusal proof wins instead.
            assert!(
                viewed.retained_clears.is_empty(),
                "refusal proof retires (v4={v4})",
                v4 = v4,
            );
            assert!(
                viewed
                    .operation_settled
                    .iter()
                    .any(|event| event.operation_id == "op-1"
                        && event.outcome == OperationOutcome::ClearRefusedProven),
                "inert echo lane yields to refusal, got {:?} (v4={v4})",
                viewed.operation_settled,
                v4 = v4,
            );
        }
        // Staged home divergence abandons before the view gate.
        let mut homed = settle_engine_4b2d();
        stage_attempts_4b2d(
            &mut homed,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![max_fact_4b2d("win-a")],
            "win-a",
            vec![("out-a", "ws-9")],
            true,
            false,
            Vec::new(),
            vec![refused()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        seed_join_4b2d(&mut homed, "win-a", "ws-9");
        seed_clear_4b2d(&mut homed, "op-1");
        homed.settle_clears(
            passed_report_4b2d(90).command.as_ref(),
            &test_issue(&passed_report_4b2d(90)),
            false,
        );
        assert!(
            homed.retained_clears.is_empty(),
            "staged home divergence retires (v4={v4})",
            v4 = v4,
        );
        assert!(
            homed
                .operation_settled
                .iter()
                .any(|event| event.operation_id == "op-1"
                    && event.outcome == OperationOutcome::ClearAbandonedHomingChanged),
            "divergence abandons at the staged home gate, got {:?} (v4={v4})",
            homed.operation_settled,
            v4 = v4,
        );
    }
}

#[test]
fn settle_clear_float_continuation_uses_staged_geometry() {
    use crate::world_report::OperationOutcome;
    for v4 in [false, true] {
        // The float continuation derives its half from the staged work
        // area: the passed report below names a contradictory one.
        let mut engine = settle_engine_4b2d();
        stage_attempts_4b2d(
            &mut engine,
            v4,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            vec![("out-a", "ws-1")],
            true,
            false,
            Vec::new(),
            vec![clear_4b2d(
                "op-1",
                "win-a",
                FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        seed_join_4b2d(&mut engine, "win-a", "ws-1");
        seed_float_clear_4b2d(&mut engine, "op-1");
        let mut crossed = passed_report_4b2d(90);
        let mut crossed_screen = screen("out-a", 0);
        crossed_screen.work_area = Some(rect(0, 0, 400, 300));
        crossed.screens = vec![crossed_screen];
        engine.settle_clears(crossed.command.as_ref(), &test_issue(&crossed), false);
        assert!(
            engine.retained_clears.is_empty(),
            "proven clear continues (v4={v4})",
            v4 = v4,
        );
        assert!(
            engine
                .operation_settled
                .iter()
                .any(|event| event.operation_id == "op-1"
                    && event.outcome == OperationOutcome::ClearContinuedFloat),
            "continuation rides the original op, got {:?} (v4={v4})",
            engine.operation_settled,
            v4 = v4,
        );
        assert_eq!(
            engine.retained_floats.len(),
            1,
            "continuation retains one float (v4={v4})",
            v4 = v4,
        );
        assert_eq!(
            engine.retained_floats[0].target,
            rect(404, 8, 388, 544),
            "staged work area decides the half, not the passed report (v4={v4})",
            v4 = v4,
        );
    }
}

#[test]
fn settle_float_receipt_before_gap_with_empty_lane_recovery() {
    use crate::world_report::OperationOutcome;
    let target = rect(404, 8, 388, 544);
    let source = rect(100, 100, 200, 200);
    let fact_at = |rect: crate::geometry::Rect| {
        let mut row = fact("win-a", "out-a", 0);
        row.rect = Some(rect);
        row
    };
    for v4 in [false, true] {
        // Fresh frame evidence latches before each gap hold; a later
        // readable observation with an empty lane still confirms.
        for (what, complete, barrier, windows, held) in [
            (
                "incomplete",
                false,
                false,
                vec![fact_at(target)],
                Vec::new(),
            ),
            ("barrier", true, true, vec![fact_at(target)], Vec::new()),
            (
                "held",
                true,
                false,
                Vec::new(),
                vec![held_window_row("win-a", "out-a", 0)],
            ),
        ] {
            let mut engine = settle_engine_4b2d();
            stage_attempts_4b2d(
                &mut engine,
                v4,
                1,
                vec![screen("out-a", 0)],
                windows,
                "win-a",
                Vec::new(),
                complete,
                barrier,
                held,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                vec![frame_4b2d(target, FrameSetter::Accepted, 5, (None, None))],
                Vec::new(),
            );
            seed_float_4b2d(&mut engine, target, source);
            engine.settle_floats();
            assert_eq!(
                engine.retained_floats.len(),
                1,
                "{what} holds (v4={v4})",
                v4 = v4,
            );
            assert!(
                engine.retained_floats[0].pending.is_some(),
                "{what} latches receipt before the hold (v4={v4})",
                v4 = v4,
            );
            assert!(
                engine.retained_floats[0].ambiguous,
                "{what} marks no-retry ambiguity (v4={v4})",
                v4 = v4,
            );
            assert!(
                engine.operation_settled.is_empty(),
                "{what} emits nothing (v4={v4})",
                v4 = v4,
            );
            stage_attempts_4b2d(
                &mut engine,
                v4,
                2,
                vec![screen("out-a", 0)],
                vec![fact_at(target)],
                "win-a",
                Vec::new(),
                true,
                false,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            engine.settle_floats();
            assert!(
                engine.retained_floats.is_empty(),
                "{what} recovers readable (v4={v4})",
                v4 = v4,
            );
            assert!(
                engine
                    .operation_settled
                    .iter()
                    .any(|event| event.operation_id == "op-f"
                        && event.outcome == OperationOutcome::FloatFrameProven),
                "{what} recovery proves the frame (v4={v4})",
                v4 = v4,
            );
        }
    }
}

// 4b-2e: raw observation gates consume the staged capture on both intakes.
// Each test sets raw lanes on BOTH independently constructed legacy/v4
// reports BEFORE either intake, stages through the actual intake, then
// drives the real consumer; the staged capture wins. Only retained joins are seeded after capture (homing,
// domains, retained bindings, lifetime marks), explicitly TEST-ONLY, never
// v4 runtime. Gesture preview authority comes from an explicit legacy
// accepted() rig (Engine sessions, mapping, views); v4 iterations reuse
// that rig and stage only their own capture.

fn born_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            birth_clear: true,
            fullscreen: true,
            maximize: true,
            native_sticky: true,
            above: true,
            ..Default::default()
        },
    )
    .expect("generation")
}

fn staged_ingress(id: u64, phase: GesturePhase) -> GestureIngress {
    GestureIngress {
        id,
        phase,
        ..gesture_row()
    }
}

/// Started ingress with a moved cursor: the bind-only guard holds it at the
/// staged watermark, while a later watermark previews at (150, 50).
fn moved_started_ingress(id: u64) -> GestureIngress {
    GestureIngress {
        id,
        kind: GestureKind::Move,
        cursor: Some((150, 50)),
        start_cursor: None,
        phase: GesturePhase::Started,
        start_frame: rect(0, 0, 40, 90),
        ..gesture_row()
    }
}

fn contra_legacy(seq: u64, windows: Vec<WindowFact>) -> WorldReport {
    report_full(
        seq,
        seq,
        vec![screen("out-a", 0)],
        windows,
        Vec::new(),
        "",
        None,
        None,
    )
}

/// Explicit legacy setup (no v4 runtime claim): bootstrap two tiled windows
/// so Engine sessions, mapping, and views exist before test staging.
fn gesture_rig() -> WorldEngine {
    let mut engine = engine();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    engine
}

/// Stage one gesture report through the real intake: raw gesture, inventory,
/// and closes sit on both source reports BEFORE staging. The legacy twin
/// supplies the test-only domain join even on the v4 leg; this does not
/// establish v4 domain assembly. Topology retention reads staged screens.
fn stage_gesture(
    engine: &mut WorldEngine,
    v4: bool,
    seq: u64,
    windows: Vec<WindowFact>,
    ingress: Option<GestureIngress>,
    complete: bool,
    closed: Vec<WindowId>,
) {
    let (mut legacy, mut v4report) = pair_reports(seq, windows, closed);
    legacy.gesture = ingress.clone();
    v4report.gesture = ingress;
    legacy.inventory_complete = complete;
    v4report.inventory_complete = complete;
    stage_either(engine, v4, &legacy, &v4report);
    engine.build_staged_domains();
    engine.retain_live_topology();
}

#[test]
fn staged_gesture_bind_gates_win_from_staged_only() {
    for v4 in [false, true] {
        let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
        // Staged ingress binds with the staged watermark alone: no report
        // input reaches the helper, so staged state alone explains the bind.
        let mut engine = gesture_rig();
        stage_gesture(
            &mut engine,
            v4,
            5,
            windows(),
            Some(staged_ingress(7, GesturePhase::Started)),
            true,
            Vec::new(),
        );
        assert!(
            engine.gesture_bind(&BTreeSet::new()),
            "staged ingress binds (v4={v4})"
        );
        let retained = engine.gesture.clone().expect("session retained");
        assert_eq!(
            (retained.id, retained.start_seq),
            (7, 5),
            "identity from staged (v4={v4})"
        );
        // Negative: staged absent never binds even with a live staged
        // ingress shape on a complete inventory: binding reads staged
        // state only.
        let mut engine = gesture_rig();
        stage_gesture(&mut engine, v4, 5, windows(), None, true, Vec::new());
        assert!(
            !engine.gesture_bind(&BTreeSet::new()),
            "staged absent refuses (v4={v4})"
        );
        assert!(engine.gesture.is_none(), "no session (v4={v4})");
        // Staged inventory gate: staged incomplete refuses with the same
        // live ingress shape staged.
        let mut engine = gesture_rig();
        stage_gesture(
            &mut engine,
            v4,
            5,
            windows(),
            Some(staged_ingress(7, GesturePhase::Started)),
            false,
            Vec::new(),
        );
        assert!(
            !engine.gesture_bind(&BTreeSet::new()),
            "staged gap refuses (v4={v4})"
        );
        assert!(engine.gesture.is_none(), "no session (v4={v4})");
        // Staged close gate: staged explicit close refuses on a complete
        // inventory.
        let mut engine = gesture_rig();
        stage_gesture(
            &mut engine,
            v4,
            5,
            windows(),
            Some(staged_ingress(7, GesturePhase::Started)),
            true,
            vec![WindowId("win-a".to_owned())],
        );
        assert!(
            !engine.gesture_bind(&BTreeSet::new()),
            "staged close refuses (v4={v4})"
        );
        assert!(engine.gesture.is_none(), "no session (v4={v4})");
        // Fresh staged id holds the old session under a staged gap, then
        // supersedes once the staged inventory completes: the start gate,
        // not any report input, decides.
        let mut engine = gesture_rig();
        stage_gesture(
            &mut engine,
            v4,
            5,
            windows(),
            Some(staged_ingress(7, GesturePhase::Started)),
            true,
            Vec::new(),
        );
        assert!(engine.gesture_bind(&BTreeSet::new()));
        stage_gesture(
            &mut engine,
            v4,
            6,
            windows(),
            Some(staged_ingress(8, GesturePhase::Started)),
            false,
            Vec::new(),
        );
        assert!(
            !engine.gesture_bind(&BTreeSet::new()),
            "staged gap holds old session (v4={v4})"
        );
        assert_eq!(
            engine.gesture.clone().expect("old session held").id,
            7,
            "no silent replacement (v4={v4})"
        );
        stage_gesture(
            &mut engine,
            v4,
            7,
            windows(),
            Some(staged_ingress(8, GesturePhase::Started)),
            true,
            Vec::new(),
        );
        assert!(
            engine.gesture_bind(&BTreeSet::new()),
            "staged complete supersedes (v4={v4})"
        );
        assert_eq!(
            engine.gesture.clone().expect("new session").id,
            8,
            "fresh staged id wins (v4={v4})"
        );
    }
}

#[test]
fn staged_gesture_act_watermark_guard_drives_preview() {
    // Same Started ingress, only the staged watermark differs: seq 5 holds
    // bind-only with no overlay, seq 6 previews. The helper takes no report
    // input, so only the staged watermark explains the pair.
    for v4 in [false, true] {
        let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
        let mut engine = gesture_rig();
        stage_gesture(
            &mut engine,
            v4,
            5,
            windows(),
            Some(moved_started_ingress(7)),
            true,
            Vec::new(),
        );
        assert!(engine.gesture_bind(&BTreeSet::new()));
        // Same staged watermark: bind-only hold, no overlay, no terminal.
        match engine.gesture_act(
            &test_issue(&contra_legacy(99, windows())),
            &mut BTreeSet::new(),
        ) {
            GestureOutcome::Held => {}
            _ => panic!("same staged watermark holds bind-only (v4={v4})"),
        }
        assert!(
            engine.gesture_terminal.is_none()
                && engine.gesture.is_some()
                && engine.gesture.clone().expect("session").hover.is_none(),
            "no overlay, no terminal, session kept (v4={v4})"
        );
        // Next staged watermark, SAME Started ingress: the update path
        // previews.
        stage_gesture(
            &mut engine,
            v4,
            6,
            windows(),
            Some(moved_started_ingress(7)),
            true,
            Vec::new(),
        );
        match engine.gesture_act(
            &test_issue(&contra_legacy(5, windows())),
            &mut BTreeSet::new(),
        ) {
            GestureOutcome::Settled {
                committed: false,
                result: Some(boxed),
            } if matches!(*boxed, CommandResult::DragPreview { .. }) => {}
            _ => panic!("next staged watermark previews (v4={v4})"),
        }
        assert!(
            engine.gesture.is_some() && engine.gesture_terminal.is_none(),
            "preview retains the session with no terminal (v4={v4})"
        );
    }
}

#[test]
fn staged_gesture_continue_fences_gate_update_and_release() {
    // Genuine continue-fence discrimination through the real consumer with
    // Engine session authority: inventory comes from staged; no report
    // input reaches the helper.
    for v4 in [false, true] {
        let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
        let updated = || GestureIngress {
            phase: GesturePhase::Updated,
            ..moved_started_ingress(7)
        };
        // Updated under a staged gap holds with no preview.
        let mut engine = gesture_rig();
        stage_gesture(
            &mut engine,
            v4,
            5,
            windows(),
            Some(moved_started_ingress(7)),
            true,
            Vec::new(),
        );
        assert!(engine.gesture_bind(&BTreeSet::new()));
        stage_gesture(
            &mut engine,
            v4,
            6,
            windows(),
            Some(updated()),
            false,
            Vec::new(),
        );
        match engine.gesture_act(
            &test_issue(&contra_legacy(6, windows())),
            &mut BTreeSet::new(),
        ) {
            GestureOutcome::Held => {}
            _ => panic!("staged gap holds the update (v4={v4})"),
        }
        assert!(
            engine.gesture_terminal.is_none() && engine.gesture.is_some(),
            "no preview, no terminal, session kept (v4={v4})"
        );
        // Staged complete previews.
        stage_gesture(
            &mut engine,
            v4,
            7,
            windows(),
            Some(updated()),
            true,
            Vec::new(),
        );
        match engine.gesture_act(
            &test_issue(&contra_legacy(7, windows())),
            &mut BTreeSet::new(),
        ) {
            GestureOutcome::Settled {
                committed: false,
                result: Some(boxed),
            } if matches!(*boxed, CommandResult::DragPreview { .. }) => {}
            _ => panic!("staged complete previews (v4={v4})"),
        }
        // Released under a staged gap records uncommitted with no commit,
        // even with a moved release frame.
        let mut engine = gesture_rig();
        stage_gesture(
            &mut engine,
            v4,
            5,
            windows(),
            Some(moved_started_ingress(7)),
            true,
            Vec::new(),
        );
        assert!(engine.gesture_bind(&BTreeSet::new()));
        let moved_windows = || vec![fact("win-a", "out-a", 100), fact("win-b", "out-a", 100)];
        let released = || GestureIngress {
            phase: GesturePhase::Released,
            ..moved_started_ingress(7)
        };
        stage_gesture(
            &mut engine,
            v4,
            6,
            moved_windows(),
            Some(released()),
            false,
            Vec::new(),
        );
        match engine.gesture_act(
            &test_issue(&contra_legacy(6, moved_windows())),
            &mut BTreeSet::new(),
        ) {
            GestureOutcome::Settled {
                committed: false, ..
            } => {}
            _ => panic!("staged gap release never commits (v4={v4})"),
        }
        assert!(
            engine.gesture.is_none() && engine.gesture_terminal.is_some(),
            "gap terminal clears and records uncommitted (v4={v4})"
        );
        // Staged complete with the same moved frame commits.
        let mut engine = gesture_rig();
        stage_gesture(
            &mut engine,
            v4,
            5,
            windows(),
            Some(moved_started_ingress(7)),
            true,
            Vec::new(),
        );
        assert!(engine.gesture_bind(&BTreeSet::new()));
        stage_gesture(
            &mut engine,
            v4,
            6,
            moved_windows(),
            Some(released()),
            true,
            Vec::new(),
        );
        match engine.gesture_act(
            &test_issue(&contra_legacy(6, moved_windows())),
            &mut BTreeSet::new(),
        ) {
            GestureOutcome::Settled {
                committed: true, ..
            } => {}
            _ => panic!("staged complete release commits (v4={v4})"),
        }
        assert!(
            engine.gesture.is_none() && engine.gesture_terminal.is_some(),
            "commit terminal clears and records (v4={v4})"
        );
    }
}

#[test]
fn staged_gesture_act_close_follows_staged() {
    for v4 in [false, true] {
        let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
        // Staged explicit close clears with no ingress: staged closes
        // decide.
        let mut engine = gesture_rig();
        stage_gesture(
            &mut engine,
            v4,
            5,
            windows(),
            Some(moved_started_ingress(7)),
            true,
            Vec::new(),
        );
        let open = contra_legacy(5, windows());
        assert!(engine.gesture_bind(&BTreeSet::new()));
        stage_gesture(
            &mut engine,
            v4,
            6,
            windows(),
            None,
            true,
            vec![WindowId("win-a".to_owned())],
        );
        match engine.gesture_act(&test_issue(&open), &mut BTreeSet::new()) {
            GestureOutcome::Settled {
                committed: false, ..
            } => {}
            _ => panic!("staged close clears (v4={v4})"),
        }
        assert!(engine.gesture.is_none(), "closed session cleared (v4={v4})");
        // Staged without ingress or close holds the session.
        let mut engine = gesture_rig();
        stage_gesture(
            &mut engine,
            v4,
            5,
            windows(),
            Some(moved_started_ingress(7)),
            true,
            Vec::new(),
        );
        assert!(engine.gesture_bind(&BTreeSet::new()));
        stage_gesture(&mut engine, v4, 6, windows(), None, true, Vec::new());
        match engine.gesture_act(
            &test_issue(&contra_legacy(6, windows())),
            &mut BTreeSet::new(),
        ) {
            GestureOutcome::Held => {}
            _ => panic!("no staged close holds (v4={v4})"),
        }
        assert!(engine.gesture.is_some(), "session held (v4={v4})");
    }
}

/// Stage one born report through the real intake: raw inventory and barrier
/// sit on both source reports BEFORE staging. The retained binding below is
/// the TEST-ONLY seeded join.
fn stage_born(engine: &mut WorldEngine, v4: bool, complete: bool, barrier: bool) {
    let (mut legacy, mut v4report) = pair_reports(1, vec![fact("win-a", "out-a", 0)], Vec::new());
    legacy.inventory_complete = complete;
    v4report.inventory_complete = complete;
    legacy.gesture_barrier = barrier;
    v4report.gesture_barrier = barrier;
    stage_either(engine, v4, &legacy, &v4report);
    seed_readable_domain(engine, "win-a");
}

fn seed_born_desire(engine: &mut WorldEngine, v4: bool, complete: bool, barrier: bool) {
    stage_born(engine, v4, complete, barrier);
    engine.retained_born.push(RetainedBornClear {
        operation_id: "op-born-1".to_owned(),
        window: WindowId("win-a".to_owned()),
        home: home_key("out-a"),
        pending: None,
        ambiguous: false,
    });
}

fn touching_migration() -> RetainedTransfer {
    RetainedTransfer {
        target: TransferTarget {
            operation_id: "op-1".to_owned(),
            kind: TransferKind::Migration,
            target_output: OutputId("out-b".to_owned()),
            target_workspace: "ws-1".to_owned(),
            follow: false,
            held: false,
        },
        policy: false,
        command_seq: None,
        source_output: OutputId("out-a".to_owned()),
        source_workspace: "ws-1".to_owned(),
        members: Vec::new(),
        migration_refill: None,
        migration_initial_empty: true,
        migration_source_index: None,
        migration_source_view: None,
        migration_target_view: None,
        migration_terminal: None,
        migration_selection_rev: None,
        pending_member: None,
        pending_members: BTreeMap::new(),
        pending_view: None,
    }
}

#[test]
fn staged_born_authorization_gates_win_over_passed_report() {
    for v4 in [false, true] {
        let id = WindowId("win-a".to_owned());
        // Control: staged complete with no barrier desires despite a passed
        // incomplete/barrier report (no blanket inventory gate).
        let mut engine = born_engine();
        seed_born_desire(&mut engine, v4, true, false);
        let mut contra = contra_legacy(1, vec![fact("win-a", "out-a", 0)]);
        contra.inventory_complete = false;
        contra.gesture_barrier = true;
        assert_eq!(
            engine.born_desire_for(&id, contra.presentation.paused),
            Some("op-born-1".to_owned()),
            "staged complete desires (v4={v4})"
        );
        // Staged barrier vetoes despite a passed clean report.
        let mut engine = born_engine();
        seed_born_desire(&mut engine, v4, true, true);
        let contra = contra_legacy(1, vec![fact("win-a", "out-a", 0)]);
        assert_eq!(
            engine.born_desire_for(&id, contra.presentation.paused),
            None,
            "staged barrier holds (v4={v4})"
        );
        // Touching migration: staged incomplete holds despite a passed
        // complete report; staged complete desires despite passed incomplete.
        let mut engine = born_engine();
        seed_born_desire(&mut engine, v4, false, false);
        engine.retained.push(touching_migration());
        let contra = contra_legacy(1, vec![fact("win-a", "out-a", 0)]);
        assert_eq!(
            engine.born_desire_for(&id, contra.presentation.paused),
            None,
            "touching gap holds (v4={v4})"
        );
        // Restage staged complete through the real intake (retained binding
        // and touching migration survive): desires despite passed incomplete.
        stage_born(&mut engine, v4, true, false);
        let mut contra_gapped = contra_legacy(1, vec![fact("win-a", "out-a", 0)]);
        contra_gapped.inventory_complete = false;
        assert_eq!(
            engine.born_desire_for(&id, contra_gapped.presentation.paused),
            Some("op-born-1".to_owned()),
            "staged complete desires under touching migration (v4={v4})"
        );
        // Presentation pause stays report-based: a paused passed report
        // vetoes even with staged rails clear.
        let mut engine = born_engine();
        seed_born_desire(&mut engine, v4, true, false);
        let mut contra_paused = contra_legacy(1, vec![fact("win-a", "out-a", 0)]);
        contra_paused.presentation.paused = true;
        assert_eq!(
            engine.born_desire_for(&id, contra_paused.presentation.paused),
            None,
            "report pause vetoes (v4={v4})"
        );
    }
}

#[test]
fn staged_born_mint_departure_prune_reads_staged() {
    for v4 in [false, true] {
        let departed = WindowId("win-x".to_owned());
        // Staged complete with no barrier prunes the departed lifetime
        // despite a passed gapped/barrier report.
        let mut engine = born_engine();
        let (legacy, v4report) = pair_reports(1, vec![fact("win-a", "out-a", 0)], Vec::new());
        stage_either(&mut engine, v4, &legacy, &v4report);
        engine.born_attempted.insert(departed.clone());
        let mut contra = contra_legacy(1, vec![fact("win-a", "out-a", 0)]);
        contra.inventory_complete = false;
        contra.gesture_barrier = true;
        engine.mint_born_clears(&contra, &BTreeSet::new(), &BTreeSet::new());
        assert!(
            !engine.born_attempted.contains(&departed),
            "staged complete prunes departure (v4={v4})"
        );
        // Staged barrier preserves despite a passed clean report.
        let mut engine = born_engine();
        stage_born_mint(&mut engine, v4, true, true, Vec::new());
        engine.born_attempted.insert(departed.clone());
        let contra = contra_legacy(1, vec![fact("win-a", "out-a", 0)]);
        engine.mint_born_clears(&contra, &BTreeSet::new(), &BTreeSet::new());
        assert!(
            engine.born_attempted.contains(&departed),
            "staged barrier preserves (v4={v4})"
        );
        // Staged incomplete preserves despite a passed complete report.
        let mut engine = born_engine();
        stage_born_mint(&mut engine, v4, false, false, Vec::new());
        engine.born_attempted.insert(departed.clone());
        let contra = contra_legacy(1, vec![fact("win-a", "out-a", 0)]);
        engine.mint_born_clears(&contra, &BTreeSet::new(), &BTreeSet::new());
        assert!(
            engine.born_attempted.contains(&departed),
            "staged gap preserves (v4={v4})"
        );
        // Held rows are live presence (facts OR held): staged complete
        // keeps the held lifetime despite a passed report with no held rows.
        let mut engine = born_engine();
        let held = held_window_row("win-x", "out-a", 0);
        stage_born_mint(&mut engine, v4, true, false, vec![held]);
        engine.born_attempted.insert(departed.clone());
        let contra = contra_legacy(1, vec![fact("win-a", "out-a", 0)]);
        engine.mint_born_clears(&contra, &BTreeSet::new(), &BTreeSet::new());
        assert!(
            engine.born_attempted.contains(&departed),
            "held presence preserves (v4={v4})"
        );
    }
}

/// Stage one born-mint report through the real intake: raw inventory,
/// barrier, and held rows sit on both source reports BEFORE staging. The
/// departed-lifetime marks below are the TEST-ONLY seeded join.
fn stage_born_mint(
    engine: &mut WorldEngine,
    v4: bool,
    complete: bool,
    barrier: bool,
    held: Vec<HeldWindowRow>,
) {
    let (mut legacy, mut v4report) = pair_reports(1, vec![fact("win-a", "out-a", 0)], Vec::new());
    legacy.inventory_complete = complete;
    v4report.inventory_complete = complete;
    legacy.gesture_barrier = barrier;
    v4report.gesture_barrier = barrier;
    legacy.held_windows = held.clone();
    v4report.held_windows = held;
    stage_either(engine, v4, &legacy, &v4report);
}

fn confirm_last_write(target: bool, rev: u64, post_obs: u64) -> LastWrite {
    LastWrite {
        property: LastWriteProperty::Visibility,
        target,
        revision: rev,
        path: LastWritePath::Staged,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: None,
        post: Some(VisibilitySample {
            obs: post_obs,
            lifetime_matched: Some(true),
            visibility: VisibilityFact {
                visible: Some(target),
                suppression_owned: Some(false),
            },
            minimized: Some(false),
            host_hidden: Some(false),
        }),
    }
}

fn fact_with_lw(window: &str, last: LastWrite) -> WindowFact {
    let mut row = fact(window, "out-a", 0);
    row.last_write = Some(last);
    row
}

fn seed_switch_binding(engine: &mut WorldEngine, members: Vec<SwitchMember>) {
    let mut selected: Vec<DomainKey> = members.iter().map(|member| member.home.clone()).collect();
    selected.sort();
    selected.dedup();
    let mut affected: Vec<OutputId> = selected.iter().map(|home| home.output.clone()).collect();
    affected.sort();
    affected.dedup();
    engine.retained_selection.insert(
        OutputId("out-a".to_owned()),
        RetainedSelection {
            effect: None,
            parent_op: "op-4".to_owned(),
            home: home_key("out-a"),
            rev: 1,
            obs: 1,
            members: Vec::new(),
            switch_members: members,
            switch_evidence: BTreeMap::new(),
            feedback: None,
            foreground_proven: false,
            arrival_ready: false,
            view_targets: BTreeMap::new(),
            view_raw: BTreeMap::new(),
            selected,
            affected,
        },
    );
}

fn switch_member(window: &str, shown: bool) -> SwitchMember {
    SwitchMember {
        window: WindowId(window.to_owned()),
        home: home_key("out-a"),
        shown,
        rev: 1,
    }
}

#[test]
fn staged_switch_receipt_and_close_win_over_passed_report() {
    for v4 in [false, true] {
        // First LastWrite grades against the staged receipt watermark: a
        // post beyond the staged seq fails even though the passed report
        // seq would bracket it.
        let mut engine = engine_settle();
        seed_switch_binding(&mut engine, vec![switch_member("win-a", true)]);
        let (legacy, v4report) = pair_reports(
            2,
            vec![fact_with_lw("win-a", confirm_last_write(true, 1, 5))],
            Vec::new(),
        );
        stage_either(&mut engine, v4, &legacy, &v4report);
        let _contra = contra_legacy(99, vec![fact("win-a", "out-a", 0)]);
        engine.settle_selection_switch();
        let binding = engine.retained_selection.get(&OutputId("out-a".to_owned()));
        assert!(
            engine.switch_settled.len() == 1
                && binding.is_none()
                && engine
                    .switch_settled
                    .first()
                    .is_some_and(|event| event.outcome == SwitchOutcome::CompletedWithFailures),
            "staged out-of-bracket verdict terminates failed (v4={v4})"
        );
        // Later watermarks never upgrade the latched verdict: restage with
        // a would-confirm LastWrite plus the pending member, and the first
        // verdict stands until the terminal below.
        let mut engine = engine_settle();
        seed_switch_binding(
            &mut engine,
            vec![switch_member("win-a", true), switch_member("win-b", false)],
        );
        let (legacy, v4report) = pair_reports(
            2,
            vec![
                fact_with_lw("win-a", confirm_last_write(true, 1, 5)),
                fact("win-b", "out-a", 100),
            ],
            Vec::new(),
        );
        stage_either(&mut engine, v4, &legacy, &v4report);
        engine.settle_selection_switch();
        assert!(
            engine.switch_settled.is_empty(),
            "pending member holds terminal (v4={v4})"
        );
        let (legacy, v4report) = pair_reports(
            3,
            vec![
                fact_with_lw("win-a", confirm_last_write(true, 1, 2)),
                fact("win-b", "out-a", 100),
            ],
            Vec::new(),
        );
        stage_either(&mut engine, v4, &legacy, &v4report);
        engine.settle_selection_switch();
        let latched = engine
            .retained_selection
            .get(&OutputId("out-a".to_owned()))
            .and_then(|binding| {
                binding
                    .switch_evidence
                    .get(&WindowId("win-a".to_owned()))
                    .map(|evidence| (evidence.confirmed, evidence.receipt_obs))
            });
        assert_eq!(
            latched,
            Some((false, 2)),
            "first verdict never upgrades (v4={v4})"
        );
        // Staged close suppresses receipt and terminal despite passed empty
        // closes; a passed-only close changes nothing.
        let mut engine = engine_settle();
        seed_switch_binding(&mut engine, vec![switch_member("win-a", true)]);
        let (legacy, v4report) = pair_reports(
            2,
            vec![fact_with_lw("win-a", confirm_last_write(true, 1, 2))],
            vec![WindowId("win-a".to_owned())],
        );
        stage_either(&mut engine, v4, &legacy, &v4report);
        engine.settle_selection_switch();
        assert!(
            engine.switch_settled.is_empty()
                && engine
                    .retained_selection
                    .contains_key(&OutputId("out-a".to_owned())),
            "staged close suppresses (v4={v4})"
        );
        let mut engine = engine_settle();
        seed_switch_binding(&mut engine, vec![switch_member("win-a", true)]);
        let (legacy, v4report) = pair_reports(
            2,
            vec![fact_with_lw("win-a", confirm_last_write(true, 1, 2))],
            Vec::new(),
        );
        stage_either(&mut engine, v4, &legacy, &v4report);
        let mut _contra_closed = contra_legacy(2, Vec::new());
        _contra_closed.closed = vec![WindowId("win-a".to_owned())];
        engine.settle_selection_switch();
        assert_eq!(
            engine.switch_settled.len(),
            1,
            "passed-only close changes nothing (v4={v4})"
        );
    }
}

/// Stage one topology report through the real intake: raw screens, held
/// screens, and inventory sit on both source reports BEFORE staging.
/// Malformed typed screen rows below are defensive staged inputs only;
/// they are never codec-accepted.
fn stage_topology(
    engine: &mut WorldEngine,
    v4: bool,
    screens: Vec<ScreenReport>,
    held_screens: Vec<HeldScreenRow>,
    complete: bool,
) {
    let (mut legacy, mut v4report) = pair_reports(1, Vec::new(), Vec::new());
    legacy.screens = screens.clone();
    v4report.screens = screens;
    legacy.held_screens = held_screens.clone();
    v4report.held_screens = held_screens;
    legacy.inventory_complete = complete;
    v4report.inventory_complete = complete;
    stage_either(engine, v4, &legacy, &v4report);
}

#[test]
fn staged_topology_retention_reads_staged_screens() {
    for v4 in [false, true] {
        // Staged complete geometry retains under an incomplete staged
        // inventory: window inventory never gates topology.
        let mut engine = engine();
        stage_topology(&mut engine, v4, vec![screen("out-a", 0)], Vec::new(), false);
        engine.retain_live_topology();
        let entries = engine.live_topology_entries();
        assert_eq!(
            entries.len(),
            1,
            "complete topology retains under gap (v4={v4})"
        );
        assert_eq!(entries[0].id.0, "out-a", "staged output (v4={v4})");
        assert_eq!(
            entries[0].rect,
            rect(0, 0, 800, 600),
            "staged geometry (v4={v4})"
        );
        // Held rows never feed topology: a held-only output retains nothing
        // extra and never clears the staged map.
        stage_topology(
            &mut engine,
            v4,
            vec![screen("out-a", 0)],
            vec![held_screen_row("out-b", 0)],
            true,
        );
        engine.retain_live_topology();
        assert_eq!(
            engine.live_topology_entries().len(),
            1,
            "held rows ignored (v4={v4})"
        );
        // Each fence clears with no prior retention: incomplete, missing,
        // duplicate, and full-rect-invalid staged screens.
        let mut incomplete = screen("out-a", 0);
        incomplete.complete = false;
        stage_topology(&mut engine, v4, vec![incomplete], Vec::new(), true);
        engine.retain_live_topology();
        assert!(
            engine.live_topology_entries().is_empty(),
            "incomplete clears (v4={v4})"
        );
        stage_topology(&mut engine, v4, Vec::new(), Vec::new(), true);
        engine.retain_live_topology();
        assert!(
            engine.live_topology_entries().is_empty(),
            "missing clears (v4={v4})"
        );
        stage_topology(
            &mut engine,
            v4,
            vec![screen("out-a", 0), screen("out-a", 800)],
            Vec::new(),
            true,
        );
        engine.retain_live_topology();
        assert!(
            engine.live_topology_entries().is_empty(),
            "duplicate clears (v4={v4})"
        );
        let mut invalid = screen("out-a", 0);
        invalid.full_rect = Some(rect(0, 0, 0, 600));
        stage_topology(&mut engine, v4, vec![invalid], Vec::new(), true);
        engine.retain_live_topology();
        assert!(
            engine.live_topology_entries().is_empty(),
            "invalid full rect clears (v4={v4})"
        );
        let mut missing_rect = screen("out-a", 0);
        missing_rect.full_rect = None;
        stage_topology(&mut engine, v4, vec![missing_rect], Vec::new(), true);
        engine.retain_live_topology();
        assert!(
            engine.live_topology_entries().is_empty(),
            "missing full rect clears (v4={v4})"
        );
        // Repeated captures keep no prior map: a complete restage retains
        // current geometry again.
        stage_topology(&mut engine, v4, vec![screen("out-a", 0)], Vec::new(), true);
        engine.retain_live_topology();
        assert_eq!(
            engine.live_topology_entries().len(),
            1,
            "complete restage retains (v4={v4})"
        );
    }
}

// 4b-2f: logical-convergence and domain-staging reads on the staged
// observation, both intakes. Raw lanes sit on both source reports BEFORE
// intake; assertions read staged state or staged-fed results, with a
// divergent legacy proof join passed only where a signature still needs a
// report (native/homing/conflict proof). Retained mapping/view/member
// seeds below are TEST-ONLY authority stand-ins, never v4 runtime proof.

#[test]
fn staged_live_order_and_designated_primary_win_over_contra() {
    for v4 in [false, true] {
        let screens = vec![screen("out-a", 0), screen("out-c", 800)];
        let mut legacy = report_full(
            1,
            1,
            screens.clone(),
            Vec::new(),
            Vec::new(),
            "",
            None,
            None,
        );
        legacy.held_screens = vec![held_screen_row("out-b", 1)];
        legacy.primary_output = Some(OutputId("out-c".to_owned()));
        let mut v4report = report_v4_simple(
            1,
            screens.clone(),
            legacy.held_screens.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        v4report.primary_output = Some(OutputId("out-c".to_owned()));
        let mut engine = engine();
        stage_either(&mut engine, v4, &legacy, &v4report);
        // Valid rows keep staged order, the held row returns to its carried
        // ordinal: the merge reads staged slices on both intakes.
        assert_eq!(
            WorldEngine::live_output_order(&engine.staged.screens, &engine.staged.held_screens),
            vec!["out-a", "out-b", "out-c"],
            "staged valid+held merge (v4={v4})"
        );
        // No report parameter remains: convergence reads staged slices
        // only, so a divergent source cannot retarget live-set or primary.
        engine.converge_logical_pre();
        let mut keys: Vec<&str> = engine.output_facts.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["out-a", "out-b", "out-c"],
            "staged live set wins (v4={v4})"
        );
        assert!(
            engine
                .output_facts
                .get("out-c")
                .is_some_and(|facts| facts.primary),
            "staged designated primary wins (v4={v4})"
        );
        assert!(
            engine
                .output_facts
                .get("out-a")
                .is_some_and(|facts| !facts.primary),
            "contra first-valid refused (v4={v4})"
        );
        // Fallback: designated absent or held falls back to first valid in
        // staged order, never a held output. Observed through displacement:
        // the removed origin's units land on the resolved primary survivor.
        // The contra join designates out-c, which the staged fallback must
        // refuse. Removed origins carry no staged facts, so no survivor
        // rect outranks the primary.
        for (primary, expect) in [
            (None, "out-a"),
            (Some(OutputId("out-b".to_owned())), "out-a"),
        ] {
            let mut leg = report_full(
                1,
                1,
                screens.clone(),
                Vec::new(),
                Vec::new(),
                "",
                None,
                None,
            );
            leg.held_screens = vec![held_screen_row("out-b", 1)];
            leg.primary_output = primary.clone();
            let mut v4leg = report_v4_simple(
                1,
                screens.clone(),
                leg.held_screens.clone(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            v4leg.primary_output = primary;
            let mut fallback =
                WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
            fallback
                .mapping
                .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
            fallback
                .mapping
                .insert("out-c".to_owned(), vec!["ws-2".to_owned()]);
            fallback
                .mapping
                .insert("out-x".to_owned(), vec!["ws-9".to_owned()]);
            fallback.views.insert("out-a".to_owned(), "ws-1".to_owned());
            fallback.views.insert("out-c".to_owned(), "ws-2".to_owned());
            fallback.views.insert("out-x".to_owned(), "ws-9".to_owned());
            stage_either(&mut fallback, v4, &leg, &v4leg);
            // No report parameter remains: the staged fallback (never a
            // divergent source) owns displacement.
            fallback.converge_logical_pre();
            assert_eq!(
                fallback
                    .displaced
                    .get("out-x")
                    .map(|entry| entry.dest.clone()),
                Some(expect.to_owned()),
                "fallback lands on first valid {expect} (v4={v4})"
            );
            assert!(
                !fallback.mapping.contains_key("out-x"),
                "removed origin displaced (v4={v4})"
            );
        }
    }
}

#[test]
fn staged_logical_post_parity_pad_and_gap_hold() {
    for v4 in [false, true] {
        // Parity: the shorter ring pads to the longest on complete staged
        // inventory. Retained ring/view/member seeds stand in for ring
        // authority; occupancy proves through the real staged homing below.
        let mut engine = WorldEngine::new_test_bound_with_capabilities(
            "gen-1",
            "adapter-1",
            HostCapabilities {
                equal_count: true,
                fullscreen: true,
                maximize: true,
                native_sticky: true,
                above: true,
                ..Default::default()
            },
        )
        .expect("generation");
        engine
            .mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        engine.mapping.insert(
            "out-b".to_owned(),
            vec!["ws-1".to_owned(), "ws-2".to_owned()],
        );
        engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
        engine.views.insert("out-b".to_owned(), "ws-2".to_owned());
        engine.members.insert(
            WindowId("win-b".to_owned()),
            MemberState {
                home: DomainKey {
                    output: OutputId("out-b".to_owned()),
                    workspace: WorkspaceId("ws-1".to_owned()),
                },
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        let screens = vec![screen("out-a", 0), screen("out-b", 800)];
        let windows = vec![fact("win-b", "out-b", 800)];
        let (mut legacy, mut v4report) = (
            report_full(
                2,
                2,
                screens.clone(),
                windows.clone(),
                Vec::new(),
                "",
                None,
                None,
            ),
            report_v4_simple(2, screens, Vec::new(), windows, Vec::new(), Vec::new()),
        );
        legacy.inventory_complete = true;
        v4report.inventory_complete = true;
        stage_either(&mut engine, v4, &legacy, &v4report);
        engine.build_staged_domains();
        assert_eq!(
            engine.staged.homing.get(&WindowId("win-b".to_owned())),
            Some(&DomainKey {
                output: OutputId("out-b".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
            }),
            "resident keeps homing (v4={v4})"
        );
        engine.converge_logical_post();
        assert_eq!(
            engine.mapping.get("out-a").map(Vec::len),
            Some(2),
            "parity pads short ring (v4={v4})"
        );
        assert_eq!(
            engine.mapping.get("out-b").map(Vec::len),
            Some(2),
            "long ring kept (v4={v4})"
        );
        assert!(
            engine
                .mapping
                .get("out-a")
                .is_some_and(|ring| ring.contains(&"ws-1".to_owned())),
            "original id kept (v4={v4})"
        );
        // Gap: incomplete staged inventory blocks lifecycle and parity.
        let mut gapped = WorldEngine::new_test_bound_with_capabilities(
            "gen-1",
            "adapter-1",
            HostCapabilities {
                equal_count: true,
                fullscreen: true,
                maximize: true,
                native_sticky: true,
                above: true,
                ..Default::default()
            },
        )
        .expect("generation");
        gapped
            .mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        gapped.mapping.insert(
            "out-b".to_owned(),
            vec!["ws-1".to_owned(), "ws-2".to_owned()],
        );
        gapped.views.insert("out-a".to_owned(), "ws-1".to_owned());
        gapped.views.insert("out-b".to_owned(), "ws-2".to_owned());
        let (mut gap_leg, mut gap_v4) = pair_reports(3, Vec::new(), Vec::new());
        gap_leg.inventory_complete = false;
        gap_v4.inventory_complete = false;
        gap_leg.screens = vec![screen("out-a", 0), screen("out-b", 800)];
        gap_v4.screens = gap_leg.screens.clone();
        stage_either(&mut gapped, v4, &gap_leg, &gap_v4);
        assert!(!gapped.staged.observation.inventory_complete);
        let before = gapped.mapping.clone();
        gapped.converge_logical_post();
        assert_eq!(gapped.mapping, before, "staged gap holds (v4={v4})");
        // Host-owned native order-presence gates under equal_count: parity
        // pads only with a non-empty staged native order per live output on
        // a complete inventory. No retained native bindings or echoes are
        // seeded; post-convergence receives only the staged native lanes.
        let template = WorldEngine::new_test_bound_with_capabilities(
            "gen-1",
            "adapter-1",
            HostCapabilities {
                host_owned: true,
                equal_count: true,
                fullscreen: true,
                maximize: true,
                native_sticky: true,
                above: true,
                ..Default::default()
            },
        )
        .expect("generation");
        for (name, complete, native_kind, expect_pad) in [
            ("positive", true, 0u8, true),
            ("missing", true, 1u8, false),
            ("empty", true, 2u8, false),
            ("gapped", false, 0u8, false),
        ] {
            let mut host = template.clone();
            host.creation_counter = 100;
            host.mapping.insert(
                "out-a".to_owned(),
                vec!["ws-1".to_owned(), "ws-2".to_owned()],
            );
            host.mapping.insert(
                "out-b".to_owned(),
                vec!["ws-1".to_owned(), "ws-2".to_owned(), "ws-3".to_owned()],
            );
            host.views.insert("out-a".to_owned(), "ws-2".to_owned());
            host.views.insert("out-b".to_owned(), "ws-3".to_owned());
            host.members.insert(
                WindowId("win-a".to_owned()),
                MemberState {
                    home: DomainKey {
                        output: OutputId("out-a".to_owned()),
                        workspace: WorkspaceId("ws-1".to_owned()),
                    },
                    floating: false,
                    minimized: false,
                    geometry: None,
                },
            );
            host.members.insert(
                WindowId("win-b".to_owned()),
                MemberState {
                    home: DomainKey {
                        output: OutputId("out-b".to_owned()),
                        workspace: WorkspaceId("ws-1".to_owned()),
                    },
                    floating: false,
                    minimized: false,
                    geometry: None,
                },
            );
            host.members.insert(
                WindowId("win-c".to_owned()),
                MemberState {
                    home: DomainKey {
                        output: OutputId("out-b".to_owned()),
                        workspace: WorkspaceId("ws-2".to_owned()),
                    },
                    floating: false,
                    minimized: false,
                    geometry: None,
                },
            );
            let native = match native_kind {
                0 => vec![native_row("out-a"), native_row("out-b")],
                1 => Vec::new(),
                _ => {
                    let mut rows = vec![native_row("out-a"), native_row("out-b")];
                    rows[1].native_order = Vec::new();
                    rows
                }
            };
            let screens = vec![screen("out-a", 0), screen("out-b", 800)];
            let windows = vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-b", 800),
                fact("win-c", "out-b", 900),
            ];
            let mut leg = report_full(
                10,
                10,
                screens.clone(),
                windows.clone(),
                Vec::new(),
                "",
                None,
                None,
            );
            leg.inventory_complete = complete;
            leg.native = native.clone();
            let mut v4leg =
                report_v4_simple(10, screens, Vec::new(), windows, Vec::new(), Vec::new());
            v4leg.inventory_complete = complete;
            v4leg.native = native;
            stage_either(&mut host, v4, &leg, &v4leg);
            host.build_staged_domains();
            host.converge_logical_post();
            if expect_pad {
                assert_eq!(
                    host.mapping.get("out-a").map(Vec::len),
                    Some(3),
                    "{name} pads host ring (v4={v4})"
                );
                assert_eq!(
                    host.mapping.get("out-b").map(Vec::len),
                    Some(3),
                    "{name} keeps host ring (v4={v4})"
                );
                assert!(
                    host.mapping
                        .get("out-a")
                        .is_some_and(|ring| ring.iter().take(2).eq(["ws-1", "ws-2"].iter())),
                    "{name} keeps host ids (v4={v4})"
                );
            } else {
                assert_eq!(
                    host.mapping.get("out-a"),
                    Some(&vec!["ws-1".to_owned(), "ws-2".to_owned()]),
                    "{name} holds host short ring (v4={v4})"
                );
                assert_eq!(
                    host.mapping.get("out-b"),
                    Some(&vec![
                        "ws-1".to_owned(),
                        "ws-2".to_owned(),
                        "ws-3".to_owned()
                    ]),
                    "{name} holds host long ring (v4={v4})"
                );
            }
        }
    }
}

#[test]
fn staged_displacement_reads_staged_live_not_contra() {
    for v4 in [false, true] {
        // Retained rings seed the pre-report topology; the staged screens
        // drop out-a while the contra proof join still claims it live.
        let mut engine = engine();
        engine
            .mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        engine
            .mapping
            .insert("out-b".to_owned(), vec!["ws-2".to_owned()]);
        engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
        engine.views.insert("out-b".to_owned(), "ws-2".to_owned());
        let (mut legacy, mut v4report) = pair_reports(2, Vec::new(), Vec::new());
        legacy.screens = vec![screen("out-b", 800)];
        v4report.screens = legacy.screens.clone();
        stage_either(&mut engine, v4, &legacy, &v4report);
        // No report parameter remains: staged liveness (never a passed
        // live claim) owns displacement.
        engine.converge_logical_pre();
        assert_eq!(
            engine
                .displaced
                .get("out-a")
                .map(|entry| entry.dest.clone()),
            Some("out-b".to_owned()),
            "removed origin displaced to staged survivor (v4={v4})"
        );
        assert!(
            engine
                .mapping
                .get("out-b")
                .is_some_and(|ring| ring.contains(&"ws-1".to_owned())),
            "survivor hosts staged unit (v4={v4})"
        );
        assert!(
            !engine.mapping.contains_key("out-a"),
            "contra live claim refused (v4={v4})"
        );
        // Focused branch: staged focus on a window homed in the displaced
        // unit retains the survivor view; an unfocused staged report mints
        // no view transfer. V4 grades facts without sticky/maximize caps,
        // so this leg uses the settle-grade engine like the other
        // fact-bearing controls above.
        for (focused, expect_view) in [("win-a", true), ("", false)] {
            let mut focus_engine = engine_settle();
            focus_engine
                .mapping
                .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
            focus_engine
                .mapping
                .insert("out-b".to_owned(), vec!["ws-2".to_owned()]);
            focus_engine
                .views
                .insert("out-a".to_owned(), "ws-1".to_owned());
            focus_engine
                .views
                .insert("out-b".to_owned(), "ws-2".to_owned());
            focus_engine.members.insert(
                WindowId("win-a".to_owned()),
                MemberState {
                    home: home_key("out-a"),
                    floating: false,
                    minimized: false,
                    geometry: None,
                },
            );
            let (mut fleg, mut fv4) =
                pair_reports(2, vec![fact("win-a", "out-b", 800)], Vec::new());
            fleg.screens = vec![screen("out-b", 800)];
            fv4.screens = fleg.screens.clone();
            fleg.focused_global = focused.to_owned();
            fv4.focused_global = focused.to_owned();
            stage_either(&mut focus_engine, v4, &fleg, &fv4);
            // Proof-only join: homing proves through retained residency,
            // the view transfer must come from the staged focus above.
            focus_engine.converge_logical_pre();
            let has_view = focus_engine.retained.iter().any(|retained| {
                matches!(&retained.target.kind, TransferKind::View)
                    && retained.target.target_output == OutputId("out-b".to_owned())
                    && retained.target.target_workspace == "ws-1"
            });
            assert_eq!(
                has_view, expect_view,
                "staged focus drives survivor view (focused={focused:?}, v4={v4})"
            );
        }
    }
}

#[test]
fn staged_trailing_marker_reads_staged_gates_not_contra() {
    for v4 in [false, true] {
        // Retained ring/view/member seeds; occupancy proves through the
        // real staged homing. The contra proof join carries an incomplete
        // inventory, which the staged gates must override.
        let mut engine = engine_settle();
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-2".to_owned()],
        );
        engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
        engine.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key("out-a"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        let (mut legacy, mut v4report) =
            pair_reports(2, vec![fact("win-a", "out-a", 0)], Vec::new());
        legacy.inventory_complete = true;
        v4report.inventory_complete = true;
        stage_either(&mut engine, v4, &legacy, &v4report);
        engine.build_staged_domains();
        let mut _contra = contra_legacy(2, vec![fact("win-a", "out-a", 0)]);
        _contra.inventory_complete = false;
        // views: retained ws-1 shows, ws-2 tail spare is unoccupied.
        engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
        assert_eq!(
            engine.trailing_marked_id("out-a"),
            Some("ws-2".to_owned()),
            "staged gates mark the spare tail (v4={v4})"
        );
        // Staged gap fails closed despite a complete contra report.
        let (mut gap_leg, mut gap_v4) =
            pair_reports(3, vec![fact("win-a", "out-a", 0)], Vec::new());
        gap_leg.inventory_complete = false;
        gap_v4.inventory_complete = false;
        stage_either(&mut engine, v4, &gap_leg, &gap_v4);
        engine.build_staged_domains();
        let _complete = contra_legacy(3, vec![fact("win-a", "out-a", 0)]);
        assert_eq!(
            engine.trailing_marked_id("out-a"),
            None,
            "staged gap fails closed (v4={v4})"
        );
    }
}

#[test]
fn staged_domain_focus_and_host_readability_win_over_contra() {
    for v4 in [false, true] {
        // Focus: staged focus wins; the contra proof join names an unknown
        // window that must never surface as domain focus.
        let mut engine = engine_settle();
        engine
            .mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
        engine.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key("out-a"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        let (mut legacy, mut v4report) =
            pair_reports(2, vec![fact("win-a", "out-a", 0)], Vec::new());
        legacy.focused_global = "win-a".to_owned();
        v4report.focused_global = "win-a".to_owned();
        stage_either(&mut engine, v4, &legacy, &v4report);
        // Staged focus wins with no report parameter: a ghost focus has
        // no staged fact, so it can never surface as domain focus.
        engine.build_staged_domains();
        let domain = engine
            .staged
            .domains
            .iter()
            .find(|domain| domain.key == home_key("out-a"))
            .expect("domain staged");
        assert_eq!(
            domain.focused,
            WindowId("win-a".to_owned()),
            "staged focus wins (v4={v4})"
        );
        assert!(
            engine
                .staged
                .domains
                .iter()
                .all(|domain| domain.focused.0 != "win-ghost"),
            "contra focus refused (v4={v4})"
        );
        // Host-owned readability: staged gap holds despite a complete
        // contra report; staged completeness reads on a fresh engine.
        let mut host = engine_settle_host();
        host.mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        host.views.insert("out-a".to_owned(), "ws-1".to_owned());
        host.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key("out-a"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        let (mut gap_leg, mut gap_v4) =
            pair_reports(3, vec![fact("win-a", "out-a", 0)], Vec::new());
        gap_leg.inventory_complete = false;
        gap_v4.inventory_complete = false;
        stage_either(&mut host, v4, &gap_leg, &gap_v4);
        host.build_staged_domains();
        let gap_domain = host
            .staged
            .domains
            .iter()
            .find(|domain| domain.key == home_key("out-a"))
            .expect("host domain staged");
        assert!(
            !gap_domain.readable,
            "staged gap holds host domain (v4={v4})"
        );
        let mut clear = engine_settle_host();
        clear
            .mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        clear.views.insert("out-a".to_owned(), "ws-1".to_owned());
        clear.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key("out-a"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        let (mut ok_leg, mut ok_v4) = pair_reports(4, vec![fact("win-a", "out-a", 0)], Vec::new());
        ok_leg.inventory_complete = true;
        ok_v4.inventory_complete = true;
        stage_either(&mut clear, v4, &ok_leg, &ok_v4);
        clear.build_staged_domains();
        let clear_domain = clear
            .staged
            .domains
            .iter()
            .find(|domain| domain.key == home_key("out-a"))
            .expect("host domain staged");
        assert!(
            clear_domain.readable,
            "staged complete reads host domain (v4={v4})"
        );
    }
}

#[test]
fn staged_routing_owns_post_capture_fresh_placement() {
    for v4 in [false, true] {
        // Fresh-placement routing must come from the captured staged
        // observation even when the source report object changes
        // afterward: the accepted caller boundary already captured.
        let mut engine = engine_settle();
        accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0), screen("out-b", 800)],
                vec![fact("win-a", "out-a", 0)],
                Vec::new(),
                "",
                None,
                None,
            ),
        );
        engine
            .mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        engine
            .mapping
            .insert("out-b".to_owned(), vec!["ws-2".to_owned()]);
        engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
        engine.views.insert("out-b".to_owned(), "ws-2".to_owned());
        let screens = vec![screen("out-a", 0), screen("out-b", 800)];
        let (mut legacy, mut v4report) = pair_reports(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-new", "out-a", 0)],
            Vec::new(),
        );
        legacy.screens = screens.clone();
        v4report.screens = screens;
        legacy.inventory_complete = true;
        v4report.inventory_complete = true;
        legacy.cursor = Some((900, 100));
        v4report.cursor = Some((900, 100));
        stage_either(&mut engine, v4, &legacy, &v4report);
        // Mutate the source objects after capture: staged routing owns
        // post-capture fresh placement, never the mutated source.
        legacy.cursor = Some((100, 100));
        v4report.cursor = Some((100, 100));
        engine.converge_logical_pre();
        engine.build_staged_domains();
        assert_eq!(
            engine.staged.homing.get(&WindowId("win-new".to_owned())),
            Some(&DomainKey {
                output: OutputId("out-b".to_owned()),
                workspace: WorkspaceId("ws-2".to_owned()),
            }),
            "captured routing owns fresh placement (v4={v4})"
        );
    }
}

#[test]
fn staged_focused_policy_owns_post_capture_routing_and_trace() {
    for v4 in [false, true] {
        // Focused-output placement ignores the cursor: the captured
        // managed-focus leg must own fresh placement even when the
        // source objects mutate afterward. Cursor sits on out-a while
        // the focused resident lives on out-b, so policy (out-b) and
        // native fallback (out-a) disagree.
        let mut engine = WorldEngine::new_test_bound_with_initial_settings(
            "gen-1",
            "adapter-1",
            HostCapabilities {
                fullscreen: true,
                maximize: true,
                native_sticky: true,
                above: true,
                ..Default::default()
            },
            vec![(
                "freshPlacement".to_owned(),
                RawSettingValue::String("focused-output-last-focused".to_owned()),
            )],
        )
        .expect("generation");
        accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0), screen("out-b", 800)],
                vec![fact("win-focus", "out-b", 800)],
                Vec::new(),
                "",
                None,
                None,
            ),
        );
        engine
            .mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        engine
            .mapping
            .insert("out-b".to_owned(), vec!["ws-2".to_owned()]);
        engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
        engine.views.insert("out-b".to_owned(), "ws-2".to_owned());
        let screens = vec![screen("out-a", 0), screen("out-b", 800)];
        let (mut legacy, mut v4report) = pair_reports(
            2,
            vec![fact("win-focus", "out-b", 800), fact("win-new", "out-a", 0)],
            Vec::new(),
        );
        legacy.screens = screens.clone();
        v4report.screens = screens;
        legacy.inventory_complete = true;
        v4report.inventory_complete = true;
        legacy.focused_global = "win-focus".to_owned();
        v4report.focused_global = "win-focus".to_owned();
        legacy.cursor = Some((100, 100));
        v4report.cursor = Some((100, 100));
        legacy.primary_output = Some(OutputId("out-b".to_owned()));
        v4report.primary_output = Some(OutputId("out-b".to_owned()));
        stage_either(&mut engine, v4, &legacy, &v4report);
        // Mutate the sources after capture: ghost focus, retargeted
        // primary, cursor swung to the other output. Staged routing owns
        // post-capture fresh placement, never the mutated source.
        legacy.focused_global = "win-ghost".to_owned();
        v4report.focused_global = "win-ghost".to_owned();
        legacy.primary_output = Some(OutputId("out-a".to_owned()));
        v4report.primary_output = Some(OutputId("out-a".to_owned()));
        legacy.cursor = Some((900, 100));
        v4report.cursor = Some((900, 100));
        engine.converge_logical_pre();
        engine.build_staged_domains();
        let key = DomainKey {
            output: OutputId("out-b".to_owned()),
            workspace: WorkspaceId("ws-2".to_owned()),
        };
        assert_eq!(
            engine.staged.homing.get(&WindowId("win-new".to_owned())),
            Some(&key),
            "captured focused leg owns fresh placement (v4={v4})"
        );
        assert!(
            engine
                .staged
                .domains
                .iter()
                .any(|domain| domain.key == key && domain.readable),
            "captured routing assembles a readable domain (v4={v4})"
        );
        let trace = &engine.fresh_focus.trace;
        assert_eq!(
            trace.basis,
            crate::fresh_focus::FreshFocusBasis::Current,
            "trace basis (v4={v4})"
        );
        assert_eq!(
            trace.chosen,
            Some(OutputId("out-b".to_owned())),
            "trace chosen (v4={v4})"
        );
        assert_eq!(
            trace.current,
            Some(OutputId("out-b".to_owned())),
            "trace current (v4={v4})"
        );
    }
}

#[test]
fn staged_fresh_gate_holds_native_affinity_without_trace_update() {
    // Gate matrix on staged-only homing: unbootstrapped or incomplete
    // inventory keeps native affinity even with the cursor (and primary)
    // on the competing output, and leaves a preseeded trace untouched.
    for v4 in [false, true] {
        for (bootstrapped, complete) in [(false, true), (true, false)] {
            let mut engine = engine_settle();
            if bootstrapped {
                accepted(
                    &mut engine,
                    &report_full(
                        1,
                        1,
                        vec![screen("out-a", 0), screen("out-b", 800)],
                        vec![fact("win-a", "out-a", 0)],
                        Vec::new(),
                        "",
                        None,
                        None,
                    ),
                );
            }
            engine
                .mapping
                .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
            engine
                .mapping
                .insert("out-b".to_owned(), vec!["ws-2".to_owned()]);
            engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
            engine.views.insert("out-b".to_owned(), "ws-2".to_owned());
            let screens = vec![screen("out-a", 0), screen("out-b", 800)];
            let (mut legacy, mut v4report) = pair_reports(
                2,
                vec![fact("win-a", "out-a", 0), fact("win-new", "out-a", 0)],
                Vec::new(),
            );
            legacy.screens = screens.clone();
            v4report.screens = screens;
            legacy.inventory_complete = complete;
            v4report.inventory_complete = complete;
            legacy.cursor = Some((900, 100));
            v4report.cursor = Some((900, 100));
            legacy.primary_output = Some(OutputId("out-b".to_owned()));
            v4report.primary_output = Some(OutputId("out-b".to_owned()));
            stage_either(&mut engine, v4, &legacy, &v4report);
            engine.fresh_focus.trace = crate::fresh_focus::FreshFocusTrace {
                current: Some(OutputId("out-z".to_owned())),
                retained: Some(OutputId("out-a".to_owned())),
                chosen: Some(OutputId("out-z".to_owned())),
                basis: crate::fresh_focus::FreshFocusBasis::Current,
            };
            engine.converge_logical_pre();
            engine.build_staged_domains();
            assert_eq!(
                engine.staged.homing.get(&WindowId("win-new".to_owned())),
                Some(&DomainKey {
                    output: OutputId("out-a".to_owned()),
                    workspace: WorkspaceId("ws-1".to_owned()),
                }),
                "gate holds native affinity (bootstrapped={bootstrapped}, complete={complete}, v4={v4})"
            );
            let trace = &engine.fresh_focus.trace;
            assert_eq!(
                (
                    trace.current.clone(),
                    trace.retained.clone(),
                    trace.chosen.clone(),
                    trace.basis
                ),
                (
                    Some(OutputId("out-z".to_owned())),
                    Some(OutputId("out-a".to_owned())),
                    Some(OutputId("out-z".to_owned())),
                    crate::fresh_focus::FreshFocusBasis::Current,
                ),
                "gate leaves the trace untouched (bootstrapped={bootstrapped}, complete={complete}, v4={v4})"
            );
        }
    }
}

#[test]
fn staged_retained_home_survives_mutated_source() {
    for v4 in [false, true] {
        // A resident keeps its retained homing even when the source
        // report mutates after capture; routing is never consulted.
        let mut engine = engine_settle();
        accepted(
            &mut engine,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                Vec::new(),
                "",
                None,
                None,
            ),
        );
        engine
            .mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
        let (mut legacy, mut v4report) =
            pair_reports(2, vec![fact("win-a", "out-a", 0)], Vec::new());
        legacy.inventory_complete = true;
        v4report.inventory_complete = true;
        stage_either(&mut engine, v4, &legacy, &v4report);
        legacy.screens = vec![screen("out-z", 1600)];
        legacy.focused_global = "win-ghost".to_owned();
        v4report.screens = legacy.screens.clone();
        v4report.focused_global = "win-ghost".to_owned();
        engine.converge_logical_pre();
        engine.build_staged_domains();
        assert_eq!(
            engine.staged.homing.get(&WindowId("win-a".to_owned())),
            Some(&home_key("out-a")),
            "retained home survives mutated source (v4={v4})"
        );
    }
}

#[test]
fn v4_staging_drives_convergence_homing_and_resweep() {
    // Genuine v4 intake with no legacy conversion: staging, logical
    // convergence, domain assembly, homing, and a float-to-tiled
    // resweep all run from the staged v4 capture.
    let mut engine = engine_settle();
    accepted(
        &mut engine,
        &report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            Vec::new(),
            "",
            None,
            None,
        ),
    );
    engine.mapping.insert(
        "out-a".to_owned(),
        vec!["ws-1".to_owned(), "ws-2".to_owned()],
    );
    engine.views.insert("out-a".to_owned(), "ws-2".to_owned());
    let mut v4report = report_v4_simple(
        2,
        vec![screen("out-a", 0)],
        Vec::new(),
        vec![fact("win-a", "out-a", 0), fact("win-new", "out-a", 100)],
        Vec::new(),
        Vec::new(),
    );
    v4report.inventory_complete = true;
    stage_v4_ingested(&mut engine, &v4report);
    engine.converge_logical_pre();
    engine.build_staged_domains();
    let key = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId("ws-2".to_owned()),
    };
    assert_eq!(
        engine.staged.homing.get(&WindowId("win-new".to_owned())),
        Some(&key),
        "v4 capture homes the newcomer"
    );
    assert!(
        engine
            .staged
            .domains
            .iter()
            .any(|domain| domain.key == key && domain.readable),
        "v4 capture assembles a readable domain"
    );
    // Float-to-tiled resweep with explicit owner/correlation: the
    // sessionless second workspace converges through the real sweep.
    let owner = OwnerId::parse("adapter-1").expect("owner");
    let correlation = CorrelationId::parse("req-2").expect("correlation");
    let mut held_scopes = BTreeSet::new();
    let mut was_floating = BTreeSet::new();
    let spare = key.clone();
    was_floating.insert(spare.clone());
    assert!(
        !engine.engine.contains(&spare),
        "sessionless before resweep"
    );
    engine.resweep_retiled(&owner, &correlation, &mut held_scopes, &was_floating);
    assert!(
        engine.engine.contains(&spare),
        "resweep converges the retiled domain"
    );
    assert!(held_scopes.is_empty(), "resweep holds nothing");
}

fn admission_known_before(engine: &WorldEngine) -> BTreeMap<DomainKey, BTreeSet<String>> {
    // Mirrors the run_accepted pre-sweep snapshot positions: retained
    // sessions per readable staged domain only.
    let mut known_before: BTreeMap<DomainKey, BTreeSet<String>> = BTreeMap::new();
    for domain in &engine.staged.domains {
        if !domain.readable {
            continue;
        }
        let Some(session) = engine.engine.session(&domain.key) else {
            continue;
        };
        let mut known = BTreeSet::new();
        for link in session.snapshot().windows {
            if link.output == domain.key.output && link.workspace == domain.key.workspace {
                known.insert(link.window.0.clone());
            }
        }
        for observed in session.exception_observed() {
            if observed.output == domain.key.output && observed.workspace == domain.key.workspace {
                known.insert(observed.window.0.clone());
            }
        }
        for (window, output, workspace) in session.minimized_entries() {
            if output == domain.key.output && workspace == domain.key.workspace {
                known.insert(window.0.clone());
            }
        }
        known_before.insert(domain.key.clone(), known);
    }
    known_before
}

fn sweep_all(engine: &mut WorldEngine) {
    let owner = OwnerId::parse("adapter-1").expect("owner");
    let correlation = CorrelationId::parse("req-2").expect("correlation");
    let mut held_scopes = BTreeSet::new();
    let domains = engine.staged.domains.clone();
    for domain in &domains {
        if !domain.readable {
            continue;
        }
        engine.sweep_domain(domain, &owner, &correlation, &mut held_scopes);
    }
}

fn test_issue(report: &WorldReport) -> crate::world_engine::EventIssue {
    crate::world_engine::EventIssue {
        owner: OwnerId::parse("adapter-1").expect("owner"),
        correlation: CorrelationId::parse(&report.request_id).expect("correlation"),
    }
}

#[test]
fn staged_admission_focus_at_most_once_and_barrier_spend() {
    for v4 in [false, true] {
        // Explicit legacy baseline (no v4 runtime claim): sessions,
        // mapping, views, and the bootstrap watermark exist before the
        // staged newcomer reports below.
        let mut rig = engine_settle();
        accepted(
            &mut rig,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                Vec::new(),
                "",
                None,
                None,
            ),
        );
        // Newcomer raw lanes sit on both source reports BEFORE intake.
        let (mut legacy, mut v4report) = pair_reports(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            Vec::new(),
        );
        legacy.inventory_complete = true;
        v4report.inventory_complete = true;
        stage_either(&mut rig, v4, &legacy, &v4report);
        rig.converge_logical_pre();
        rig.build_staged_domains();
        let known_before = admission_known_before(&rig);
        sweep_all(&mut rig);
        let held = BTreeSet::new();
        let effects = rig.take_admission_focus(&known_before, &held);
        assert_eq!(effects.len(), 1, "newcomer mints once (v4={v4})");
        assert_eq!(effects[0].target, WindowId("win-b".to_owned()));
        assert_eq!(effects[0].cause, EffectCause::AutomaticAdmission);
        assert!(effects[0].id.starts_with("af-"));
        // Same eligible input replays: already spent, never replays.
        assert!(
            rig.take_admission_focus(&known_before, &held).is_empty(),
            "at-most-once holds (v4={v4})"
        );
        assert!(
            rig.admission_focus_spent.contains("win-b"),
            "watermark records the mint (v4={v4})"
        );
        // Barrier: a mid-gesture newcomer spends with no effect, and the
        // spend survives a later clean report.
        let mut barred = engine_settle();
        accepted(
            &mut barred,
            &report_full(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                Vec::new(),
                "",
                None,
                None,
            ),
        );
        let (mut bar_leg, mut bar_v4) = pair_reports(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            Vec::new(),
        );
        bar_leg.inventory_complete = true;
        bar_v4.inventory_complete = true;
        bar_leg.gesture_barrier = true;
        bar_v4.gesture_barrier = true;
        stage_either(&mut barred, v4, &bar_leg, &bar_v4);
        barred.converge_logical_pre();
        barred.build_staged_domains();
        let bar_known = admission_known_before(&barred);
        sweep_all(&mut barred);
        assert!(
            barred.take_admission_focus(&bar_known, &held).is_empty(),
            "barrier spends silently (v4={v4})"
        );
        assert!(
            barred.admission_focus_spent.contains("win-b"),
            "barrier spend watermarks (v4={v4})"
        );
        let (mut clean_leg, mut clean_v4) = pair_reports(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            Vec::new(),
        );
        clean_leg.inventory_complete = true;
        clean_v4.inventory_complete = true;
        stage_either(&mut barred, v4, &clean_leg, &clean_v4);
        barred.converge_logical_pre();
        barred.build_staged_domains();
        let clean_known = admission_known_before(&barred);
        sweep_all(&mut barred);
        assert!(
            barred.take_admission_focus(&clean_known, &held).is_empty(),
            "spent barrier never replays (v4={v4})"
        );
    }
}

// 4b-2f: membership, decorator, above, frame-clock, and output-plan reads
// use staged inputs on both intakes. Raw lanes sit on both source reports
// BEFORE intake; assertions read staged state or staged-fed results, with a
// divergent legacy proof join passed only where a signature still needs a
// report (native/homing/conflict proof). Retained mapping/view/member and
// domain/binding seeds below are TEST-ONLY authority stand-ins, never v4
// runtime proof.

fn fullscreen_binding(engine: &mut WorldEngine, window: &str, home: &DomainKey, command_seq: u64) {
    engine.retained_states.insert(
        (WindowId(window.to_owned()), StateProperty::Fullscreen),
        RetainedStateBinding {
            target: RetainedStateTarget {
                window: WindowId(window.to_owned()),
                property: StateProperty::Fullscreen,
                value: true,
                revision: 1,
            },
            pending: None,
            spent: false,
            disposition: AttemptOutcome::Unattempted,
            sticky_meta: None,
            fullscreen_meta: Some(crate::world_state::NativeFullscreenMeta {
                command_seq,
                home: home.clone(),
            }),
            maximize_meta: None,
            above_meta: None,
        },
    );
}

fn desire_row(window: &str, output: &str, workspace: &str) -> WindowDesire {
    WindowDesire {
        lifetime: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: workspace.to_owned(),
        allocation: None,
        effective_frame: None,
        frame_authority: ScopeAuthority::Controlled,
        membership_authority: ScopeAuthority::Controlled,
        floating: false,
        float_origin: None,
        frame_operation: None,
        clear_operation: None,
        command_seq: None,
        visibility: None,
        visibility_authority: ScopeAuthority::Untouched,
        state: Vec::new(),
    }
}

#[test]
fn staged_member_prune_gap_and_held_presence_win_over_contra() {
    for v4 in [false, true] {
        // Gap holds: staged incomplete retains an absent member even though
        // the divergent complete contra would prune it.
        let mut engine_gap = engine();
        engine_gap.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key("out-a"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        let (mut legacy, mut v4report) =
            pair_reports(1, vec![fact("win-b", "out-a", 100)], Vec::new());
        legacy.inventory_complete = false;
        v4report.inventory_complete = false;
        stage_either(&mut engine_gap, v4, &legacy, &v4report);
        let mut _contra = legacy.clone();
        _contra.inventory_complete = true;
        engine_gap.reconcile_members();
        assert!(
            engine_gap
                .members
                .contains_key(&WindowId("win-a".to_owned())),
            "staged gap blocks prune despite complete contra (v4={v4})"
        );
        // Positive prune: staged complete drops the absent member even
        // though the divergent gapped contra would hold it.
        let mut engine_prune = engine();
        engine_prune.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key("out-a"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        let (mut legacy, mut v4report) =
            pair_reports(1, vec![fact("win-b", "out-a", 100)], Vec::new());
        legacy.inventory_complete = true;
        v4report.inventory_complete = true;
        stage_either(&mut engine_prune, v4, &legacy, &v4report);
        let mut _contra = legacy.clone();
        _contra.inventory_complete = false;
        engine_prune.reconcile_members();
        assert!(
            !engine_prune
                .members
                .contains_key(&WindowId("win-a".to_owned())),
            "staged complete prunes despite gapped contra (v4={v4})"
        );
        // Held presence: staged complete with a held row retains the
        // member; the divergent contra without held rows would prune it.
        let mut engine_held = engine();
        engine_held.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key("out-a"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        let (mut legacy, mut v4report) =
            pair_reports(1, vec![fact("win-b", "out-a", 100)], Vec::new());
        legacy.inventory_complete = true;
        v4report.inventory_complete = true;
        legacy.held_windows = vec![held_window_row("win-a", "out-a", 1)];
        v4report.held_windows = vec![held_window_row("win-a", "out-a", 1)];
        stage_either(&mut engine_held, v4, &legacy, &v4report);
        let mut _contra = legacy.clone();
        _contra.held_windows = Vec::new();
        engine_held.reconcile_members();
        assert!(
            engine_held
                .members
                .contains_key(&WindowId("win-a".to_owned())),
            "staged held presence retains despite bare contra (v4={v4})"
        );
    }
}

#[test]
fn staged_fullscreen_decorator_controlled_and_reason_precedence() {
    for v4 in [false, true] {
        // Clean staged inventory authorizes Controlled (positive control).
        let mut engine = engine_settle();
        let (mut legacy, mut v4report) =
            pair_reports(1, vec![fact("win-a", "out-a", 0)], Vec::new());
        legacy.inventory_complete = true;
        v4report.inventory_complete = true;
        legacy.gesture_barrier = false;
        v4report.gesture_barrier = false;
        stage_either(&mut engine, v4, &legacy, &v4report);
        seed_readable_domain(&mut engine, "win-a");
        engine.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key("out-a"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        fullscreen_binding(&mut engine, "win-a", &home_key("out-a"), 7);
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_native_fullscreen_state(&BTreeSet::new(), &mut windows);
        let row = windows[0]
            .state
            .iter()
            .find(|row| row.property == StateProperty::Fullscreen)
            .expect("fullscreen row minted");
        assert_eq!(
            row.authority,
            ScopeAuthority::Controlled,
            "clean Controlled (v4={v4})"
        );
        assert_eq!(row.command_seq, Some(7));
        // Staged gap holds with IncompleteInventory.
        let mut engine = engine_settle();
        let (mut legacy, mut v4report) =
            pair_reports(1, vec![fact("win-a", "out-a", 0)], Vec::new());
        legacy.inventory_complete = false;
        v4report.inventory_complete = false;
        stage_either(&mut engine, v4, &legacy, &v4report);
        seed_readable_domain(&mut engine, "win-a");
        engine.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key("out-a"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        fullscreen_binding(&mut engine, "win-a", &home_key("out-a"), 7);
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_native_fullscreen_state(&BTreeSet::new(), &mut windows);
        let row = windows[0]
            .state
            .iter()
            .find(|row| row.property == StateProperty::Fullscreen)
            .expect("gap row minted");
        assert_eq!(
            row.authority,
            ScopeAuthority::Hold(HoldReason::IncompleteInventory),
            "gap holds (v4={v4})"
        );
        // Staged barrier holds with GestureBarrier, and wins over a
        // simultaneous gap (reason precedence).
        for (complete, what) in [(true, "barrier"), (false, "barrier-over-gap")] {
            let mut engine = engine_settle();
            let (mut legacy, mut v4report) =
                pair_reports(1, vec![fact("win-a", "out-a", 0)], Vec::new());
            legacy.inventory_complete = complete;
            v4report.inventory_complete = complete;
            legacy.gesture_barrier = true;
            v4report.gesture_barrier = true;
            stage_either(&mut engine, v4, &legacy, &v4report);
            seed_readable_domain(&mut engine, "win-a");
            engine.members.insert(
                WindowId("win-a".to_owned()),
                MemberState {
                    home: home_key("out-a"),
                    floating: false,
                    minimized: false,
                    geometry: None,
                },
            );
            fullscreen_binding(&mut engine, "win-a", &home_key("out-a"), 7);
            let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
            engine.decorate_native_fullscreen_state(&BTreeSet::new(), &mut windows);
            let row = windows[0]
                .state
                .iter()
                .find(|row| row.property == StateProperty::Fullscreen)
                .expect("barrier row minted");
            assert_eq!(
                row.authority,
                ScopeAuthority::Hold(HoldReason::GestureBarrier),
                "{what} precedence (v4={v4})"
            );
        }
        // Excluded lane stays Untouched even when complete and unbarriered.
        let mut engine = engine_settle();
        let mut excluded_fact = fact("win-a", "out-a", 0);
        excluded_fact.managed = Some(false);
        let (mut legacy, mut v4report) = pair_reports(1, vec![excluded_fact], Vec::new());
        legacy.inventory_complete = true;
        v4report.inventory_complete = true;
        stage_either(&mut engine, v4, &legacy, &v4report);
        seed_readable_domain(&mut engine, "win-a");
        engine.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key("out-a"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        fullscreen_binding(&mut engine, "win-a", &home_key("out-a"), 7);
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_native_fullscreen_state(&BTreeSet::new(), &mut windows);
        let row = windows[0]
            .state
            .iter()
            .find(|row| row.property == StateProperty::Fullscreen)
            .expect("excluded row minted");
        assert_eq!(
            row.authority,
            ScopeAuthority::Untouched,
            "excluded stays Untouched (v4={v4})"
        );
    }
}

fn selection_caps() -> HostCapabilities {
    HostCapabilities {
        window_suppression: true,
        fullscreen: true,
        maximize: true,
        native_sticky: true,
        above: true,
        ..HostCapabilities::default()
    }
}

fn composed_visibility_caps() -> HostCapabilities {
    HostCapabilities {
        window_suppression: true,
        maximize: false,
        ..HostCapabilities::default()
    }
}

fn seed_member(engine: &mut WorldEngine, window: &str) {
    engine.members.insert(
        WindowId(window.to_owned()),
        MemberState {
            home: home_key("out-a"),
            floating: false,
            minimized: false,
            geometry: None,
        },
    );
}

#[test]
fn staged_selection_visibility_gates_win_over_contra() {
    for v4 in [false, true] {
        // Show target on win-a: staged inventory/barrier lanes gate the
        // same Controlled target the clean intake authorizes, and the
        // excluded lane stays Untouched. Both intakes stage the same
        // lanes before intake; no passed report is read here.
        let setup = |complete: bool, barrier: bool, managed: Option<bool>| {
            let mut engine = WorldEngine::new_test_bound_with_capabilities(
                "gen-1",
                "adapter-1",
                selection_caps(),
            )
            .expect("generation");
            let mut window_fact = fact("win-a", "out-a", 0);
            window_fact.suppression_available = Some(true);
            window_fact.visibility.suppression_owned = Some(true);
            window_fact.managed = managed;
            let (mut legacy, mut v4report) = pair_reports(1, vec![window_fact], Vec::new());
            legacy.inventory_complete = complete;
            v4report.inventory_complete = complete;
            legacy.gesture_barrier = barrier;
            v4report.gesture_barrier = barrier;
            legacy.views = vec![ScreenView {
                output: OutputId("out-a".to_owned()),
                view: "ws-1".to_owned(),
            }];
            stage_either(&mut engine, v4, &legacy, &v4report);
            engine
                .mapping
                .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
            engine.build_staged_domains();
            seed_member(&mut engine, "win-a");
            engine.selection_targets.insert(
                WindowId("win-a".to_owned()),
                SelectionTarget {
                    home: home_key("out-a"),
                    shown: true,
                    rev: 1,
                },
            );
            engine
        };
        // Clean staged inventory authorizes Controlled (positive control).
        let engine = setup(true, false, Some(true));
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_selection_visibility(&BTreeSet::new(), &mut windows);
        let vis = windows[0].visibility.expect("clean visibility minted");
        assert!(vis.shown, "clean shows (v4={v4})");
        assert_eq!(
            vis.revision, 1,
            "clean keeps the retained revision (v4={v4})"
        );
        assert!(!vis.fallback, "clean is not a fallback (v4={v4})");
        assert_eq!(
            windows[0].visibility_authority,
            ScopeAuthority::Controlled,
            "clean Controlled (v4={v4})"
        );
        // Staged gap holds with IncompleteInventory.
        let engine = setup(false, false, Some(true));
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_selection_visibility(&BTreeSet::new(), &mut windows);
        assert_eq!(
            windows[0].visibility_authority,
            ScopeAuthority::Hold(HoldReason::IncompleteInventory),
            "gap holds (v4={v4})"
        );
        // Staged barrier holds with GestureBarrier, and wins over a
        // simultaneous gap (reason precedence).
        for (complete, what) in [(true, "barrier"), (false, "barrier-over-gap")] {
            let engine = setup(complete, true, Some(true));
            let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
            engine.decorate_selection_visibility(&BTreeSet::new(), &mut windows);
            assert_eq!(
                windows[0].visibility_authority,
                ScopeAuthority::Hold(HoldReason::GestureBarrier),
                "{what} precedence (v4={v4})"
            );
        }
        // Excluded lane stays Untouched even when complete and unbarriered.
        let engine = setup(true, false, Some(false));
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_selection_visibility(&BTreeSet::new(), &mut windows);
        assert_eq!(
            windows[0].visibility_authority,
            ScopeAuthority::Untouched,
            "excluded stays Untouched (v4={v4})"
        );
    }
}

#[test]
fn staged_sticky_visibility_gates_win_over_contra() {
    for v4 in [false, true] {
        // Composed sticky show on win-a: retained provenance plus a
        // visible revision decorates Controlled when clean, holds on
        // staged gap/barrier lanes, and stays Untouched when excluded.
        // Null sticky/maximized facts are the contract-valid absent
        // values on this route (non-null sticky would hold admission).
        let setup = |complete: bool, barrier: bool, managed: Option<bool>| {
            let mut engine = WorldEngine::new_test_bound_with_capabilities(
                "gen-1",
                "adapter-1",
                composed_visibility_caps(),
            )
            .expect("generation");
            let mut window_fact = fact("win-a", "out-a", 0);
            window_fact.sticky = None;
            window_fact.maximized = None;
            window_fact.suppression_available = Some(true);
            window_fact.visibility.suppression_owned = Some(true);
            window_fact.managed = managed;
            let (mut legacy, mut v4report) = pair_reports(1, vec![window_fact], Vec::new());
            legacy.inventory_complete = complete;
            v4report.inventory_complete = complete;
            legacy.gesture_barrier = barrier;
            v4report.gesture_barrier = barrier;
            legacy.views = vec![ScreenView {
                output: OutputId("out-a".to_owned()),
                view: "ws-1".to_owned(),
            }];
            stage_either(&mut engine, v4, &legacy, &v4report);
            engine
                .mapping
                .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
            engine.build_staged_domains();
            seed_member(&mut engine, "win-a");
            engine
                .restored_sticky
                .insert(WindowId("win-a".to_owned()), false);
            engine
                .sticky_visible_rev
                .insert(WindowId("win-a".to_owned()), 4);
            engine
        };
        let engine = setup(true, false, Some(true));
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_sticky_visibility(&BTreeSet::new(), &mut windows);
        let vis = windows[0].visibility.expect("clean visibility minted");
        assert!(vis.shown, "clean shows (v4={v4})");
        assert_eq!(
            vis.revision, 4,
            "clean keeps the retained revision (v4={v4})"
        );
        assert!(!vis.fallback, "clean is not a fallback (v4={v4})");
        assert_eq!(
            windows[0].visibility_authority,
            ScopeAuthority::Controlled,
            "clean Controlled (v4={v4})"
        );
        let engine = setup(false, false, Some(true));
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_sticky_visibility(&BTreeSet::new(), &mut windows);
        assert_eq!(
            windows[0].visibility_authority,
            ScopeAuthority::Hold(HoldReason::IncompleteInventory),
            "gap holds (v4={v4})"
        );
        for (complete, what) in [(true, "barrier"), (false, "barrier-over-gap")] {
            let engine = setup(complete, true, Some(true));
            let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
            engine.decorate_sticky_visibility(&BTreeSet::new(), &mut windows);
            assert_eq!(
                windows[0].visibility_authority,
                ScopeAuthority::Hold(HoldReason::GestureBarrier),
                "{what} precedence (v4={v4})"
            );
        }
        let engine = setup(true, false, Some(false));
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_sticky_visibility(&BTreeSet::new(), &mut windows);
        assert_eq!(
            windows[0].visibility_authority,
            ScopeAuthority::Untouched,
            "excluded stays Untouched (v4={v4})"
        );
    }
}

fn sticky_binding(engine: &mut WorldEngine, window: &str, home: &DomainKey, command_seq: u64) {
    engine.retained_states.insert(
        (WindowId(window.to_owned()), StateProperty::Sticky),
        RetainedStateBinding {
            target: RetainedStateTarget {
                window: WindowId(window.to_owned()),
                property: StateProperty::Sticky,
                value: true,
                revision: 1,
            },
            pending: None,
            spent: false,
            disposition: AttemptOutcome::Unattempted,
            sticky_meta: Some(crate::world_state::NativeStickyMeta {
                command_seq,
                prior_saved: None,
                prior_restored_sticky: None,
                prior_restored_floats: false,
                prior_float: false,
                home: home.clone(),
                dest: home.clone(),
                requested: false,
                requested_save: IntentData::new(None, None),
                is_toggle_float_clear: false,
                followup_done: false,
            }),
            fullscreen_meta: None,
            maximize_meta: None,
            above_meta: None,
        },
    );
}

fn maximize_binding(engine: &mut WorldEngine, window: &str, home: &DomainKey, command_seq: u64) {
    engine.retained_states.insert(
        (WindowId(window.to_owned()), StateProperty::Maximized),
        RetainedStateBinding {
            target: RetainedStateTarget {
                window: WindowId(window.to_owned()),
                property: StateProperty::Maximized,
                value: true,
                revision: 1,
            },
            pending: None,
            spent: false,
            disposition: AttemptOutcome::Unattempted,
            sticky_meta: None,
            fullscreen_meta: None,
            maximize_meta: Some(crate::world_state::NativeMaximizeMeta {
                command_seq,
                home: home.clone(),
                clear_fullscreen: false,
            }),
            above_meta: None,
        },
    );
}

#[test]
fn staged_native_sticky_state_gates_win_over_contra() {
    for v4 in [false, true] {
        let setup = |complete: bool, barrier: bool, managed: Option<bool>| {
            let mut engine = engine_settle();
            let mut window_fact = fact("win-a", "out-a", 0);
            window_fact.managed = managed;
            let (mut legacy, mut v4report) = pair_reports(1, vec![window_fact], Vec::new());
            legacy.inventory_complete = complete;
            v4report.inventory_complete = complete;
            legacy.gesture_barrier = barrier;
            v4report.gesture_barrier = barrier;
            stage_either(&mut engine, v4, &legacy, &v4report);
            seed_readable_domain(&mut engine, "win-a");
            seed_member(&mut engine, "win-a");
            sticky_binding(&mut engine, "win-a", &home_key("out-a"), 7);
            engine
        };
        // Clean staged inventory authorizes Controlled (positive control).
        let engine = setup(true, false, Some(true));
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_native_sticky_state(&BTreeSet::new(), &mut windows);
        let row = windows[0]
            .state
            .iter()
            .find(|row| row.property == StateProperty::Sticky)
            .expect("clean row minted");
        assert!(row.value, "clean keeps the retained value (v4={v4})");
        assert_eq!(
            row.authority,
            ScopeAuthority::Controlled,
            "clean Controlled (v4={v4})"
        );
        assert_eq!(row.command_seq, Some(7));
        // Staged gap holds with IncompleteInventory.
        let engine = setup(false, false, Some(true));
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_native_sticky_state(&BTreeSet::new(), &mut windows);
        let row = windows[0]
            .state
            .iter()
            .find(|row| row.property == StateProperty::Sticky)
            .expect("gap row minted");
        assert_eq!(
            row.authority,
            ScopeAuthority::Hold(HoldReason::IncompleteInventory),
            "gap holds (v4={v4})"
        );
        // Staged barrier holds with GestureBarrier, and wins over a
        // simultaneous gap (reason precedence).
        for (complete, what) in [(true, "barrier"), (false, "barrier-over-gap")] {
            let engine = setup(complete, true, Some(true));
            let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
            engine.decorate_native_sticky_state(&BTreeSet::new(), &mut windows);
            let row = windows[0]
                .state
                .iter()
                .find(|row| row.property == StateProperty::Sticky)
                .expect("barrier row minted");
            assert_eq!(
                row.authority,
                ScopeAuthority::Hold(HoldReason::GestureBarrier),
                "{what} precedence (v4={v4})"
            );
        }
        // Excluded lane stays Untouched even when complete and unbarriered.
        let engine = setup(true, false, Some(false));
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_native_sticky_state(&BTreeSet::new(), &mut windows);
        let row = windows[0]
            .state
            .iter()
            .find(|row| row.property == StateProperty::Sticky)
            .expect("excluded row minted");
        assert_eq!(
            row.authority,
            ScopeAuthority::Untouched,
            "excluded stays Untouched (v4={v4})"
        );
    }
}

#[test]
fn staged_native_maximize_state_gates_win_over_contra() {
    for v4 in [false, true] {
        let setup = |complete: bool, barrier: bool, managed: Option<bool>| {
            let mut engine = engine_settle();
            let mut window_fact = fact("win-a", "out-a", 0);
            window_fact.managed = managed;
            let (mut legacy, mut v4report) = pair_reports(1, vec![window_fact], Vec::new());
            legacy.inventory_complete = complete;
            v4report.inventory_complete = complete;
            legacy.gesture_barrier = barrier;
            v4report.gesture_barrier = barrier;
            stage_either(&mut engine, v4, &legacy, &v4report);
            seed_readable_domain(&mut engine, "win-a");
            seed_member(&mut engine, "win-a");
            maximize_binding(&mut engine, "win-a", &home_key("out-a"), 7);
            engine
        };
        // Clean staged inventory authorizes Controlled (positive control).
        let engine = setup(true, false, Some(true));
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_native_maximize_state(&BTreeSet::new(), &mut windows);
        let row = windows[0]
            .state
            .iter()
            .find(|row| row.property == StateProperty::Maximized)
            .expect("clean row minted");
        assert!(row.value, "clean keeps the retained value (v4={v4})");
        assert_eq!(
            row.authority,
            ScopeAuthority::Controlled,
            "clean Controlled (v4={v4})"
        );
        assert_eq!(row.command_seq, Some(7));
        // Staged gap holds with IncompleteInventory.
        let engine = setup(false, false, Some(true));
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_native_maximize_state(&BTreeSet::new(), &mut windows);
        let row = windows[0]
            .state
            .iter()
            .find(|row| row.property == StateProperty::Maximized)
            .expect("gap row minted");
        assert_eq!(
            row.authority,
            ScopeAuthority::Hold(HoldReason::IncompleteInventory),
            "gap holds (v4={v4})"
        );
        // Staged barrier holds with GestureBarrier, and wins over a
        // simultaneous gap (reason precedence).
        for (complete, what) in [(true, "barrier"), (false, "barrier-over-gap")] {
            let engine = setup(complete, true, Some(true));
            let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
            engine.decorate_native_maximize_state(&BTreeSet::new(), &mut windows);
            let row = windows[0]
                .state
                .iter()
                .find(|row| row.property == StateProperty::Maximized)
                .expect("barrier row minted");
            assert_eq!(
                row.authority,
                ScopeAuthority::Hold(HoldReason::GestureBarrier),
                "{what} precedence (v4={v4})"
            );
        }
        // Excluded lane stays Untouched even when complete and unbarriered.
        let engine = setup(true, false, Some(false));
        let mut windows = vec![desire_row("win-a", "out-a", "ws-1")];
        engine.decorate_native_maximize_state(&BTreeSet::new(), &mut windows);
        let row = windows[0]
            .state
            .iter()
            .find(|row| row.property == StateProperty::Maximized)
            .expect("excluded row minted");
        assert_eq!(
            row.authority,
            ScopeAuthority::Untouched,
            "excluded stays Untouched (v4={v4})"
        );
    }
}

#[test]
fn staged_above_suspends_focused_fullscreen_scope() {
    for v4 in [false, true] {
        // Clean: the same staged fullscreen fact as the suspended leg,
        // but with no focus, so no scope suspends. The healthy float
        // acquires a Controlled Above true (positive control). Focus is
        // the only difference between the legs.
        let mut engine = engine_settle();
        let mut full = fact("win-full", "out-a", 300);
        full.fullscreen = Some(true);
        let (mut legacy, mut v4report) =
            pair_reports(1, vec![fact("win-a", "out-a", 0), full], Vec::new());
        legacy.inventory_complete = true;
        v4report.inventory_complete = true;
        legacy.focused_global = String::new();
        v4report.focused_global = String::new();
        stage_either(&mut engine, v4, &legacy, &v4report);
        seed_readable_domain(&mut engine, "win-a");
        // Same homing seed as the suspended leg: identical staged
        // homing on both engines, only the focus differs.
        engine
            .staged
            .homing
            .insert(WindowId("win-full".to_owned()), home_key("out-a"));
        engine.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key("out-a"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        let mut windows = vec![
            {
                let mut row = desire_row("win-a", "out-a", "ws-1");
                row.floating = true;
                row
            },
            desire_row("win-full", "out-a", "ws-1"),
        ];
        engine.reconcile_and_decorate_native_above_state(&BTreeSet::new(), &mut windows);
        assert!(
            engine
                .retained_states
                .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Above)),
            "clean float acquires (v4={v4})"
        );
        let row = windows[0]
            .state
            .iter()
            .find(|row| row.property == StateProperty::Above)
            .expect("above row minted");
        assert!(row.value);
        assert_eq!(row.authority, ScopeAuthority::Controlled);
        // Suspended: staged focused fullscreen on the same scope blocks the
        // float acquire with no binding and no row.
        let mut engine = engine_settle();
        let mut full = fact("win-full", "out-a", 300);
        full.fullscreen = Some(true);
        let (mut legacy, mut v4report) =
            pair_reports(1, vec![fact("win-a", "out-a", 0), full], Vec::new());
        legacy.inventory_complete = true;
        v4report.inventory_complete = true;
        legacy.focused_global = "win-full".to_owned();
        v4report.focused_global = "win-full".to_owned();
        stage_either(&mut engine, v4, &legacy, &v4report);
        seed_readable_domain(&mut engine, "win-a");
        // TEST-ONLY homing seed: the focused fullscreen lifetime homes on
        // the same staged scope as the float candidate.
        engine
            .staged
            .homing
            .insert(WindowId("win-full".to_owned()), home_key("out-a"));
        engine.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key("out-a"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        let mut windows = vec![
            {
                let mut row = desire_row("win-a", "out-a", "ws-1");
                row.floating = true;
                row
            },
            desire_row("win-full", "out-a", "ws-1"),
        ];
        engine.reconcile_and_decorate_native_above_state(&BTreeSet::new(), &mut windows);
        assert!(
            !engine
                .retained_states
                .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Above)),
            "suspended scope mints nothing (v4={v4})"
        );
        assert!(
            windows[0]
                .state
                .iter()
                .all(|row| row.property != StateProperty::Above),
            "suspended scope publishes no row (v4={v4})"
        );
    }
}

#[test]
fn staged_above_health_fallback_holds_rows_over_contra() {
    for v4 in [false, true] {
        // Producer-owned Above false binding on the float candidate: the
        // staged health lanes decide the row authority on both intakes.
        // The binding is observation-only seeded (Unattempted, unlatched)
        // so only decoration asserts.
        let setup = |complete: bool, barrier: bool, managed: Option<bool>| {
            let mut engine = engine_settle();
            let mut window_fact = fact("win-a", "out-a", 0);
            window_fact.managed = managed;
            let (mut legacy, mut v4report) = pair_reports(1, vec![window_fact], Vec::new());
            legacy.inventory_complete = complete;
            v4report.inventory_complete = complete;
            legacy.gesture_barrier = barrier;
            v4report.gesture_barrier = barrier;
            legacy.focused_global = String::new();
            v4report.focused_global = String::new();
            stage_either(&mut engine, v4, &legacy, &v4report);
            seed_readable_domain(&mut engine, "win-a");
            seed_member(&mut engine, "win-a");
            engine.retained_states.insert(
                (WindowId("win-a".to_owned()), StateProperty::Above),
                RetainedStateBinding {
                    target: RetainedStateTarget {
                        window: WindowId("win-a".to_owned()),
                        property: StateProperty::Above,
                        value: false,
                        revision: 1,
                    },
                    pending: None,
                    spent: false,
                    disposition: AttemptOutcome::Unattempted,
                    sticky_meta: None,
                    fullscreen_meta: None,
                    maximize_meta: None,
                    above_meta: Some(crate::world_state::NativeAboveMeta {
                        home: home_key("out-a"),
                        latched: false,
                    }),
                },
            );
            engine
        };
        let float_row = || {
            let mut row = desire_row("win-a", "out-a", "ws-1");
            row.floating = true;
            row
        };
        // Clean staged inventory: the healthy binding publishes
        // Controlled false (positive control).
        let mut engine = setup(true, false, Some(true));
        let mut windows = vec![float_row()];
        engine.reconcile_and_decorate_native_above_state(&BTreeSet::new(), &mut windows);
        let row = windows[0]
            .state
            .iter()
            .find(|row| row.property == StateProperty::Above)
            .expect("clean row minted");
        assert!(!row.value, "clean keeps the retained value (v4={v4})");
        assert_eq!(
            row.authority,
            ScopeAuthority::Controlled,
            "clean Controlled (v4={v4})"
        );
        assert_eq!(row.command_seq, None);
        // Staged gap holds with IncompleteInventory.
        let mut engine = setup(false, false, Some(true));
        let mut windows = vec![float_row()];
        engine.reconcile_and_decorate_native_above_state(&BTreeSet::new(), &mut windows);
        let row = windows[0]
            .state
            .iter()
            .find(|row| row.property == StateProperty::Above)
            .expect("gap row minted");
        assert_eq!(
            row.authority,
            ScopeAuthority::Hold(HoldReason::IncompleteInventory),
            "gap holds (v4={v4})"
        );
        // Staged barrier holds with GestureBarrier, and wins over a
        // simultaneous gap (reason precedence).
        for (complete, what) in [(true, "barrier"), (false, "barrier-over-gap")] {
            let mut engine = setup(complete, true, Some(true));
            let mut windows = vec![float_row()];
            engine.reconcile_and_decorate_native_above_state(&BTreeSet::new(), &mut windows);
            let row = windows[0]
                .state
                .iter()
                .find(|row| row.property == StateProperty::Above)
                .expect("barrier row minted");
            assert_eq!(
                row.authority,
                ScopeAuthority::Hold(HoldReason::GestureBarrier),
                "{what} precedence (v4={v4})"
            );
        }
        // Excluded lane stays Untouched even when complete and unbarriered.
        let mut engine = setup(true, false, Some(false));
        let mut windows = vec![float_row()];
        engine.reconcile_and_decorate_native_above_state(&BTreeSet::new(), &mut windows);
        let row = windows[0]
            .state
            .iter()
            .find(|row| row.property == StateProperty::Above)
            .expect("excluded row minted");
        assert_eq!(
            row.authority,
            ScopeAuthority::Untouched,
            "excluded stays Untouched (v4={v4})"
        );
    }
}

#[test]
fn staged_native_sticky_followup_uses_staged_gates_and_original_seq() {
    for v4 in [false, true] {
        // Spent Applied off-binding awaiting its followup: the staged
        // lanes decide completion. The unfinished binding and prior
        // sticky provenance stand in for an earlier OFF producer.
        let setup = |complete: bool, sticky: Option<bool>| {
            let mut engine = engine_settle();
            let mut window_fact = fact("win-a", "out-a", 0);
            window_fact.sticky = sticky;
            let (mut legacy, mut v4report) = pair_reports(1, vec![window_fact], Vec::new());
            legacy.inventory_complete = complete;
            v4report.inventory_complete = complete;
            stage_either(&mut engine, v4, &legacy, &v4report);
            seed_readable_domain(&mut engine, "win-a");
            seed_member(&mut engine, "win-a");
            let home = home_key("out-a");
            engine
                .mapping
                .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
            engine.floating_scopes.insert(home.clone());
            engine
                .restored_sticky
                .insert(WindowId("win-a".to_owned()), false);
            engine
                .members
                .get_mut(&WindowId("win-a".to_owned()))
                .expect("member")
                .floating = true;
            engine.retained_states.insert(
                (WindowId("win-a".to_owned()), StateProperty::Sticky),
                RetainedStateBinding {
                    target: RetainedStateTarget {
                        window: WindowId("win-a".to_owned()),
                        property: StateProperty::Sticky,
                        value: false,
                        revision: 1,
                    },
                    pending: None,
                    spent: true,
                    disposition: AttemptOutcome::Applied,
                    sticky_meta: Some(crate::world_state::NativeStickyMeta {
                        command_seq: 7,
                        prior_saved: None,
                        prior_restored_sticky: None,
                        prior_restored_floats: false,
                        prior_float: false,
                        home: home.clone(),
                        dest: home.clone(),
                        requested: false,
                        requested_save: IntentData::new(None, None),
                        is_toggle_float_clear: false,
                        followup_done: false,
                    }),
                    fullscreen_meta: None,
                    maximize_meta: None,
                    above_meta: None,
                },
            );
            (engine, legacy)
        };
        let done = |engine: &WorldEngine| {
            engine
                .retained_states
                .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
                .expect("binding retained")
                .sticky_meta
                .as_ref()
                .expect("sticky meta")
                .followup_done
        };
        // Staged gap retains proof and sticky provenance.
        let (mut engine, legacy) = setup(false, Some(false));
        engine.run_native_sticky_followups(&test_issue(&legacy), &BTreeSet::new());
        assert!(!done(&engine), "staged gap holds (v4={v4})");
        // Staged restick retains proof while the current fact is sticky.
        let (mut engine, legacy) = setup(true, Some(true));
        engine.run_native_sticky_followups(&test_issue(&legacy), &BTreeSet::new());
        assert!(!done(&engine), "staged restick holds (v4={v4})");
        // Staged complete with a fresh non-sticky fact finishes the
        // followup (positive control).
        let (mut engine, legacy) = setup(true, Some(false));
        engine.run_native_sticky_followups(&test_issue(&legacy), &BTreeSet::new());
        assert!(
            done(&engine),
            "staged complete finishes the followup (v4={v4})"
        );
        assert!(
            !engine
                .restored_sticky
                .contains_key(&WindowId("win-a".to_owned()))
        );
        assert_eq!(engine.command_highwater, Some(7));
        assert!(engine.sticky_command_save.is_none());
        assert!(engine.command_result.is_none());
        let binding = engine
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
            .expect("binding retained");
        assert_eq!(binding.target.revision, 1);
        assert_eq!(
            binding
                .sticky_meta
                .as_ref()
                .expect("sticky meta")
                .command_seq,
            7
        );
    }
}

/// v4-staged proven-refusal rollback: `run_native_sticky_followups` driven
/// directly off a v4 capture restores the exact pre-command runtime, queues
/// one null-clear write for a prior-less producer, and leaves the original
/// producer seq/revision untouched under a distinct followup issue.
/// Held scopes and incomplete inventory retain proof with no write.
#[test]
fn v4_staged_native_sticky_refused_proven_rolls_back_with_original_seq() {
    for v4 in [false, true] {
        let win = WindowId("win-a".to_owned());
        let home = home_key("out-a");
        let setup = || {
            let mut engine = engine_settle();
            let (legacy, v4report) = pair_reports(1, vec![fact("win-a", "out-a", 0)], Vec::new());
            stage_either(&mut engine, v4, &legacy, &v4report);
            engine
                .mapping
                .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
            seed_readable_domain(&mut engine, "win-a");
            seed_member(&mut engine, "win-a");
            // Runtime after an ON producer from a tiled base.
            engine.restored_sticky.insert(win.clone(), false);
            engine
                .sticky_saved
                .insert(win.clone(), IntentData::new(Some(true), Some(false)));
            let member = engine.members.get_mut(&win).expect("member seeded");
            member.floating = true;
            engine.retained_states.insert(
                (win.clone(), StateProperty::Sticky),
                RetainedStateBinding {
                    target: RetainedStateTarget {
                        window: win.clone(),
                        property: StateProperty::Sticky,
                        value: true,
                        revision: 1,
                    },
                    pending: None,
                    spent: true,
                    disposition: AttemptOutcome::RefusedProven,
                    sticky_meta: Some(crate::world_state::NativeStickyMeta {
                        command_seq: 7,
                        prior_saved: None,
                        prior_restored_sticky: None,
                        prior_restored_floats: false,
                        prior_float: false,
                        home: home.clone(),
                        dest: home.clone(),
                        requested: true,
                        requested_save: IntentData::new(Some(true), Some(false)),
                        is_toggle_float_clear: false,
                        followup_done: false,
                    }),
                    fullscreen_meta: None,
                    maximize_meta: None,
                    above_meta: None,
                },
            );
            engine
        };
        let binding_of = |engine: &WorldEngine| {
            engine
                .retained_states
                .get(&(win.clone(), StateProperty::Sticky))
                .expect("binding retained")
                .clone()
        };
        let issue = crate::world_engine::EventIssue {
            owner: OwnerId::parse("adapter-1").expect("owner"),
            correlation: CorrelationId::parse("req-9").expect("correlation"),
        };
        // Held home retains proof with no write and no runtime change.
        let mut held_engine = setup();
        let mut held = BTreeSet::new();
        held.insert(home.clone());
        held_engine.run_native_sticky_followups(&issue, &held);
        let held_binding = binding_of(&held_engine);
        assert!(!held_binding.sticky_meta.expect("meta").followup_done);
        assert!(held_engine.sticky_rollback_saves.is_empty());
        assert!(held_engine.restored_sticky.contains_key(&win));
        // Incomplete inventory retains proof the same way.
        let mut gapped_engine = setup();
        gapped_engine.staged.observation.inventory_complete = false;
        gapped_engine.run_native_sticky_followups(&issue, &BTreeSet::new());
        let gapped_binding = binding_of(&gapped_engine);
        assert!(!gapped_binding.sticky_meta.expect("meta").followup_done);
        assert!(gapped_engine.sticky_rollback_saves.is_empty());
        // Healthy run: prior runtime restored through fresh restage.
        let mut engine = setup();
        engine.run_native_sticky_followups(&issue, &BTreeSet::new());
        let binding = binding_of(&engine);
        let meta = binding.sticky_meta.expect("meta");
        assert!(meta.followup_done);
        assert!(!engine.restored_sticky.contains_key(&win));
        assert!(!engine.restored_floats.contains(&win));
        let member = engine.members.get(&win).expect("member");
        assert_eq!(member.home, home);
        assert!(!member.floating);
        assert_eq!(engine.staged_homing(&win).as_ref(), Some(&home));
        // Prior-less producer: save slot returns to absence with exactly
        // one null-clear rollback write.
        assert!(!engine.sticky_saved.contains_key(&win));
        assert_eq!(
            engine.sticky_rollback_saves.get(&win),
            Some(&IntentWrite {
                win: win.clone(),
                data: None,
            })
        );
        // Original producer identity untouched by the distinct issue.
        assert_eq!(binding.target.revision, 1);
        assert_eq!(meta.command_seq, 7);
        // Exactly-once queue: a second run writes nothing new.
        engine.run_native_sticky_followups(&issue, &BTreeSet::new());
        assert_eq!(engine.sticky_rollback_saves.len(), 1);
    }
}

#[test]
fn staged_frame_clock_drives_suppression_and_burst() {
    // Publish-driven: the staged source clock drives the tiled, float
    // token, and composed-overlay frame gates through `publish`, even
    // when the passed contra report carries a rearming clock. A caller
    // reverted to the passed-report clock would publish Controlled
    // under the contra clock, so these rows pin the staged input.
    // Retained seeds (frames, bursts, float target, composed bit) are
    // disclosed below; raw lanes sit on source reports pre-intake.
    for v4 in [false, true] {
        let mut engine = engine();
        // Baseline: win-a tiles, win-b floats. The tiled row publishes
        // Controlled with its tile frame (positive unsuppressed
        // control); the float row carries no token yet.
        let mut base = report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![
                fact("win-a", "out-a", 0),
                float_fact("win-b", "out-a", 100, 100),
            ],
            vec![("out-a", "ws-1")],
            "",
            None,
            None,
        );
        base.monotonic_ms = 0;
        let first = accepted(&mut engine, &base);
        let tile = first
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-a")
            .expect("tiled baseline row")
            .effective_frame
            .expect("tiled baseline frame");
        assert_eq!(
            first
                .windows
                .iter()
                .find(|row| row.lifetime.0 == "win-a")
                .expect("tiled baseline row")
                .frame_authority,
            ScopeAuthority::Controlled,
            "baseline publishes Controlled (v4={v4})"
        );
        let float_row = first
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-b")
            .expect("float baseline row");
        assert!(float_row.floating, "win-b floats at baseline (v4={v4})");
        assert_eq!(
            float_row.frame_operation, None,
            "baseline carries no float token (v4={v4})"
        );
        let float_home = float_row.workspace.clone();
        // Retained float placement for the token lane: the desired
        // target is the seeded rect below, independent of the
        // (tokenless) baseline.
        let float_target = rect(100, 100, 200, 200);
        // Mismatched readbacks: the staged facts sit away from every
        // desired target so the seeded memories stay applicable.
        let mut fa = fact("win-a", "out-a", 0);
        fa.rect = Some(rect(10, 0, 40, 90));
        let mut fb = float_fact("win-b", "out-a", 100, 100);
        fb.rect = Some(rect(110, 110, 200, 200));
        let observed_a = rect(10, 0, 40, 90);
        let observed_b = rect(110, 110, 200, 200);
        let stage_source = |engine: &mut WorldEngine, clock: u64| {
            let (mut legacy, mut v4report) =
                pair_reports(2, vec![fa.clone(), fb.clone()], Vec::new());
            legacy.inventory_complete = true;
            v4report.inventory_complete = true;
            legacy.monotonic_ms = clock;
            v4report.monotonic_ms = clock;
            legacy.views = vec![ScreenView {
                output: OutputId("out-a".to_owned()),
                view: "ws-1".to_owned(),
            }];
            stage_either(engine, v4, &legacy, &v4report);
            assert_eq!(
                engine.staged.observation.monotonic_ms, clock,
                "staged clock stages on both intakes (v4={v4})"
            );
            // Rebuild domains against the converged mapping (same shape
            // as the accepted baseline) so `publish` resolves sessions.
            engine.build_staged_domains();
            legacy
        };
        // Staged 1200 sits 200ms after the seeded failure: inside the
        // 1000ms backoff on both lanes. The contra 5000 sits
        // outside, so a passed-report clock would not suppress.
        let legacy = stage_source(&mut engine, 1200);
        let id_a = WindowId("win-a".to_owned());
        let id_b = WindowId("win-b".to_owned());
        engine.frames.insert(
            id_a.clone(),
            crate::world_frames::FrameMemory {
                desired: tile,
                observed: observed_a,
                failures: 1,
                last_ms: 1000,
                clamped: false,
            },
        );
        engine.frames.insert(
            id_b.clone(),
            crate::world_frames::FrameMemory {
                desired: float_target,
                observed: observed_b,
                failures: 1,
                last_ms: 1000,
                clamped: false,
            },
        );
        engine
            .retained_floats
            .push(crate::world_engine::RetainedFloat {
                operation_id: "op-9".to_owned(),
                command_seq: 9,
                window: id_b.clone(),
                target: float_target,
                source: None,
                output: OutputId("out-a".to_owned()),
                workspace: float_home,
                ambiguous: false,
                pending: None,
            });
        let mut contra = legacy.clone();
        contra.monotonic_ms = 5000;
        let reply = engine.publish(&contra, false, Vec::new(), None, &BTreeSet::new());
        let row_a = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-a")
            .expect("published tiled row");
        assert_eq!(
            row_a.frame_authority,
            ScopeAuthority::Hold(HoldReason::ConstraintClamp),
            "staged 1200 suppresses tiled despite contra 5000 (v4={v4})"
        );
        assert_eq!(
            row_a.effective_frame,
            Some(tile),
            "suppressed desire stays intact (v4={v4})"
        );
        let row_b = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-b")
            .expect("published float row");
        assert_eq!(
            row_b.frame_operation, None,
            "staged 1200 withholds the float token despite contra 5000 (v4={v4})"
        );
        assert_eq!(
            row_b.effective_frame,
            Some(float_target),
            "withheld float keeps its base frame (v4={v4})"
        );
        // Staged 5000 re-arms both lanes: the tiled row publishes
        // Controlled and the float token rides again.
        let legacy = stage_source(&mut engine, 5000);
        let mut contra = legacy.clone();
        contra.monotonic_ms = 5000;
        let reply = engine.publish(&contra, false, Vec::new(), None, &BTreeSet::new());
        let row_a = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-a")
            .expect("rearmed tiled row");
        assert_eq!(
            row_a.frame_authority,
            ScopeAuthority::Controlled,
            "staged 5000 re-arms tiled (v4={v4})"
        );
        let row_b = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-b")
            .expect("rearmed float row");
        assert_eq!(
            row_b.frame_operation.as_deref(),
            Some("op-9"),
            "staged 5000 releases the float token (v4={v4})"
        );
    }
    // Composed-overlay lane: with native maximize unsupported the
    // retained bit selects the work-area frame. Same publish-driven
    // shape as above on a no-max engine with contract-valid null
    // sticky/maximized facts (a non-null sticky would hold admission
    // on this route, so the nulls are required, not incidental).
    for v4 in [false, true] {
        let mut engine = WorldEngine::new_test_bound_with_capabilities(
            "gen-1",
            "adapter-1",
            HostCapabilities {
                maximize: false,
                fullscreen: true,
                ..HostCapabilities::default()
            },
        )
        .expect("generation");
        let mut composed_fact = float_fact("win-c", "out-a", 300, 100);
        composed_fact.sticky = None;
        composed_fact.maximized = None;
        let mut base = report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![composed_fact.clone()],
            vec![("out-a", "ws-1")],
            "",
            None,
            None,
        );
        base.monotonic_ms = 0;
        let first = accepted(&mut engine, &base);
        let baseline_row = first
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-c")
            .expect("overlay baseline row");
        assert!(baseline_row.floating, "win-c floats at baseline (v4={v4})");
        engine
            .composed_maximized
            .insert(WindowId("win-c".to_owned()));
        let mut staged_fact = composed_fact.clone();
        staged_fact.rect = Some(rect(310, 110, 200, 200));
        let observed = rect(310, 110, 200, 200);
        let overlay = rect(0, 0, 800, 560);
        let stage_source = |engine: &mut WorldEngine, clock: u64| {
            let (mut legacy, mut v4report) = pair_reports(2, vec![staged_fact.clone()], Vec::new());
            legacy.inventory_complete = true;
            v4report.inventory_complete = true;
            legacy.monotonic_ms = clock;
            v4report.monotonic_ms = clock;
            legacy.views = vec![ScreenView {
                output: OutputId("out-a".to_owned()),
                view: "ws-1".to_owned(),
            }];
            stage_either(engine, v4, &legacy, &v4report);
            assert_eq!(
                engine.staged.observation.monotonic_ms, clock,
                "staged clock stages on both intakes (v4={v4})"
            );
            engine.build_staged_domains();
            legacy
        };
        let id = WindowId("win-c".to_owned());
        let legacy = stage_source(&mut engine, 1200);
        engine.frames.insert(
            id.clone(),
            crate::world_frames::FrameMemory {
                desired: overlay,
                observed,
                failures: 1,
                last_ms: 1000,
                clamped: false,
            },
        );
        let mut contra = legacy.clone();
        contra.monotonic_ms = 5000;
        let reply = engine.publish(&contra, false, Vec::new(), None, &BTreeSet::new());
        let row = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-c")
            .expect("published overlay row");
        assert_eq!(
            row.frame_authority,
            ScopeAuthority::Hold(HoldReason::ConstraintClamp),
            "staged 1200 suppresses the overlay despite contra 5000 (v4={v4})"
        );
        assert_eq!(
            row.effective_frame,
            Some(overlay),
            "suppressed overlay stays on the work area (v4={v4})"
        );
        let legacy = stage_source(&mut engine, 5000);
        let mut contra = legacy.clone();
        contra.monotonic_ms = 5000;
        let reply = engine.publish(&contra, false, Vec::new(), None, &BTreeSet::new());
        let row = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-c")
            .expect("rearmed overlay row");
        assert_eq!(
            row.frame_authority,
            ScopeAuthority::Controlled,
            "staged 5000 re-arms the overlay (v4={v4})"
        );
    }
    // Burst family on the tiled lane: staged 1200 sits 200ms after the
    // last change (inside the 500ms quiet window) so the exhausted
    // budget holds; staged 2000 proves quiet and re-arms. The observed
    // frame stays away from the desired target throughout (continuous
    // burst, never an exact arrival).
    for v4 in [false, true] {
        let mut engine = engine();
        let mut base = report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            vec![("out-a", "ws-1")],
            "",
            None,
            None,
        );
        base.monotonic_ms = 0;
        let first = accepted(&mut engine, &base);
        let tile = first
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-a")
            .expect("tiled baseline row")
            .effective_frame
            .expect("tiled baseline frame");
        assert_eq!(
            first
                .windows
                .iter()
                .find(|row| row.lifetime.0 == "win-a")
                .expect("tiled baseline row")
                .frame_authority,
            ScopeAuthority::Controlled,
            "baseline publishes Controlled (v4={v4})"
        );
        let mut fa = fact("win-a", "out-a", 0);
        fa.rect = Some(rect(10, 0, 40, 90));
        let burst_observed = rect(10, 0, 40, 90);
        let id = WindowId("win-a".to_owned());
        let stage_source = |engine: &mut WorldEngine, clock: u64| {
            let (mut legacy, mut v4report) = pair_reports(2, vec![fa.clone()], Vec::new());
            legacy.inventory_complete = true;
            v4report.inventory_complete = true;
            legacy.monotonic_ms = clock;
            v4report.monotonic_ms = clock;
            legacy.views = vec![ScreenView {
                output: OutputId("out-a".to_owned()),
                view: "ws-1".to_owned(),
            }];
            stage_either(engine, v4, &legacy, &v4report);
            engine.build_staged_domains();
            legacy
        };
        let legacy = stage_source(&mut engine, 1200);
        engine.frame_bursts.insert(
            id.clone(),
            crate::world_frames::FrameBurst {
                desired: tile,
                attempts: 2,
                observed: burst_observed,
                last_change_ms: 1000,
            },
        );
        let mut contra = legacy.clone();
        contra.monotonic_ms = 2000;
        let reply = engine.publish(&contra, false, Vec::new(), None, &BTreeSet::new());
        let row = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-a")
            .expect("published tiled row");
        assert_eq!(
            row.frame_authority,
            ScopeAuthority::Hold(HoldReason::ConstraintClamp),
            "staged 1200 holds the burst despite contra 2000 (v4={v4})"
        );
        assert_eq!(
            engine
                .frame_bursts
                .get(&id)
                .expect("burst retained")
                .attempts,
            2,
            "held burst spends no budget (v4={v4})"
        );
        // Proven quiet re-arms through the same publish path: the
        // budget resets and the row publishes Controlled.
        let legacy = stage_source(&mut engine, 2000);
        let mut contra = legacy.clone();
        contra.monotonic_ms = 2000;
        let reply = engine.publish(&contra, false, Vec::new(), None, &BTreeSet::new());
        let row = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-a")
            .expect("rearmed tiled row");
        assert_eq!(
            row.frame_authority,
            ScopeAuthority::Controlled,
            "staged 2000 re-arms the burst (v4={v4})"
        );
        assert_eq!(
            engine
                .frame_bursts
                .get(&id)
                .expect("burst retained")
                .attempts,
            0,
            "re-armed burst resets its budget (v4={v4})"
        );
    }
}

#[test]
fn staged_output_plan_follows_staged_screens_with_contra_views_proof() {
    for v4 in [false, true] {
        let (mut legacy, mut v4report) = pair_reports(1, Vec::new(), Vec::new());
        legacy.inventory_complete = true;
        v4report.inventory_complete = true;
        let mut engine = engine();
        stage_either(&mut engine, v4, &legacy, &v4report);
        // Divergent contra: extra screen plus an unknown view echo. The
        // screens must not leak in (staged wins) while the unknown echo
        // proof still reads the passed report (held).
        let mut contra = legacy.clone();
        contra.screens.push(screen("out-z", 1600));
        contra.views = vec![crate::world_report::ScreenView {
            output: OutputId("out-a".to_owned()),
            view: "ws-unknown".to_owned(),
        }];
        let rows = engine.plan_outputs(&contra, &BTreeSet::new());
        assert_eq!(rows.len(), 1, "staged screens win over contra (v4={v4})");
        assert_eq!(rows[0].output.0, "out-a");
        assert_eq!(
            rows[0].hold,
            Some(HoldReason::IncompleteInventory),
            "contra unknown echo still holds the row (v4={v4})"
        );
        // Clean contra (no echo): the staged complete screen authorizes
        // with no hold (positive control).
        let clean = legacy.clone();
        let rows = engine.plan_outputs(&clean, &BTreeSet::new());
        assert_eq!(rows.len(), 1, "staged screen plans (v4={v4})");
        assert_eq!(
            rows[0].hold, None,
            "clean staged screen holds nothing (v4={v4})"
        );
    }
}

// 4b-2f closure: `note_focused_output` reads the staged capture on both
// intakes (no accepted-run proof). Source focus/screens sit on both source
// reports BEFORE intake; retained seeds are disclosed below (member home
// for win-a plus a prior last_output). The finish call position
// (pre-reconcile, before `reconcile_members`) is source-reviewed, not
// executed here: this test drives stage + the no-arg latch directly.
#[test]
fn staged_focused_output_latch_reads_staged_capture() {
    for v4 in [false, true] {
        let mut subject = engine();
        // Retained seeds: win-a homes on out-a (so `core_homing` proves
        // genuine managed focus), win-new has no homing (a stealing
        // newcomer), and a prior latch predates every intake.
        subject.members.insert(
            WindowId("win-a".to_owned()),
            MemberState {
                home: home_key("out-a"),
                floating: false,
                minimized: false,
                geometry: None,
            },
        );
        // Retained ring so `core_homing` resolves the home (fresh engines
        // carry no rings; the accepted run converges them pre-reconcile).
        subject
            .mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        subject.fresh_focus.last_output = Some(OutputId("out-prior".to_owned()));
        // Genuine managed focus on a live screen latches, overwriting the
        // prior context.
        let (mut legacy, mut v4report) =
            pair_reports(1, vec![fact("win-a", "out-a", 0)], Vec::new());
        legacy.focused_global = "win-a".to_owned();
        v4report.focused_global = "win-a".to_owned();
        stage_either(&mut subject, v4, &legacy, &v4report);
        assert_eq!(
            subject.staged.observation.focused_global, "win-a",
            "staged focus stages (v4={v4})"
        );
        subject.note_focused_output();
        assert_eq!(
            subject.fresh_focus.last_output,
            Some(OutputId("out-a".to_owned())),
            "genuine managed focus on a live screen latches (v4={v4})"
        );
        // Unknown/newcomer focus (no retained homing) never overwrites.
        let (mut legacy, mut v4report) =
            pair_reports(2, vec![fact("win-a", "out-a", 0)], Vec::new());
        legacy.focused_global = "win-new".to_owned();
        v4report.focused_global = "win-new".to_owned();
        stage_either(&mut subject, v4, &legacy, &v4report);
        subject.note_focused_output();
        assert_eq!(
            subject.fresh_focus.last_output,
            Some(OutputId("out-a".to_owned())),
            "newcomer keeps the latch (v4={v4})"
        );
        // Empty focus never overwrites.
        let (mut legacy, mut v4report) =
            pair_reports(3, vec![fact("win-a", "out-a", 0)], Vec::new());
        legacy.focused_global = String::new();
        v4report.focused_global = String::new();
        stage_either(&mut subject, v4, &legacy, &v4report);
        subject.note_focused_output();
        assert_eq!(
            subject.fresh_focus.last_output,
            Some(OutputId("out-a".to_owned())),
            "empty focus keeps the latch (v4={v4})"
        );
        // Managed focus whose home left the live set never overwrites:
        // focus still names win-a, but only out-b stages live.
        let (mut legacy, mut v4report) =
            pair_reports(4, vec![fact("win-a", "out-a", 0)], Vec::new());
        legacy.focused_global = "win-a".to_owned();
        v4report.focused_global = "win-a".to_owned();
        legacy.screens = vec![screen("out-b", 800)];
        v4report.screens = legacy.screens.clone();
        stage_either(&mut subject, v4, &legacy, &v4report);
        subject.fresh_focus.last_output = Some(OutputId("out-prior".to_owned()));
        subject.note_focused_output();
        assert_eq!(
            subject.fresh_focus.last_output,
            Some(OutputId("out-prior".to_owned())),
            "absent live output keeps the latch (v4={v4})"
        );
        // Prior memory survival without any latch ever firing: a fresh
        // engine holding only prior context keeps it through empty focus.
        let mut prior = engine();
        prior.fresh_focus.last_output = Some(OutputId("out-prior".to_owned()));
        let (mut legacy, mut v4report) =
            pair_reports(5, vec![fact("win-a", "out-a", 0)], Vec::new());
        legacy.focused_global = String::new();
        v4report.focused_global = String::new();
        stage_either(&mut prior, v4, &legacy, &v4report);
        prior.note_focused_output();
        assert_eq!(
            prior.fresh_focus.last_output,
            Some(OutputId("out-prior".to_owned())),
            "prior memory survives empty focus (v4={v4})"
        );
    }
}

/// Unit 6 sub-increment 5a: stage one native lane through the real intake.
/// Source reports carry the native rows BEFORE staging on both intakes.
fn stage_native_5a(
    engine: &mut WorldEngine,
    v4: bool,
    seq: u64,
    native: Vec<NativeObservation>,
    views: Vec<ViewAttempt>,
    complete: bool,
) {
    let (mut legacy, mut v4report) = pair_reports(seq, Vec::new(), Vec::new());
    legacy.inventory_complete = complete;
    v4report.inventory_complete = complete;
    legacy.native = native.clone();
    v4report.native = native;
    legacy.view_attempts = views.clone();
    v4report.view_attempts = views;
    stage_either(engine, v4, &legacy, &v4report);
}

fn shared_host_engine_5a() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            host_owned: true,
            ..Default::default()
        },
        vec![(
            "mode".to_owned(),
            RawSettingValue::String("shared".to_owned()),
        )],
    )
    .expect("generation")
}

fn native_lane_5a(output: &str, order: Vec<&str>, current: &str) -> NativeObservation {
    NativeObservation {
        output: OutputId(output.to_owned()),
        native_order: order.iter().map(|id| (*id).to_owned()).collect(),
        current_native: Some(current.to_owned()),
        occupancy: order
            .iter()
            .map(|id| NativeOccupancy {
                native: (*id).to_owned(),
                count: None,
            })
            .collect(),
    }
}

#[test]
fn staged_native_completeness_echo_receipt_and_order_conflict() {
    for v4 in [false, true] {
        // Eligible positive: a complete staged list proves the current
        // echo and the native-derived member receipt.
        let mut engine = engine_settle_host();
        engine
            .mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        engine.native_map.insert(
            ("out-a".to_owned(), "native-out-a-0".to_owned()),
            "ws-1".to_owned(),
        );
        stage_native_5a(
            &mut engine,
            v4,
            2,
            vec![native_row("out-a")],
            Vec::new(),
            true,
        );
        assert!(
            engine.native_list_complete("out-a"),
            "complete list (v4={v4})"
        );
        assert_eq!(
            engine.native_echo_for(&OutputId("out-a".to_owned())),
            Some("ws-1".to_owned()),
            "staged current echo (v4={v4})"
        );
        let mut receipt_fact = fact("win-a", "out-a", 0);
        receipt_fact.native_workspace =
            crate::world_report::NativeWorkspaceFact::Value("native-out-a-0".to_owned());
        assert_eq!(
            engine.effective_logical_receipt(&receipt_fact),
            Some("ws-1".to_owned()),
            "staged member receipt (v4={v4})"
        );
        // Negative: an unmapped current proves nothing on the same joins.
        stage_native_5a(
            &mut engine,
            v4,
            3,
            vec![native_lane_5a(
                "out-a",
                vec!["native-unknown"],
                "native-unknown",
            )],
            Vec::new(),
            true,
        );
        assert_eq!(
            engine.native_echo_for(&OutputId("out-a".to_owned())),
            None,
            "unknown current holds echo (v4={v4})"
        );
        receipt_fact.native_workspace =
            crate::world_report::NativeWorkspaceFact::Value("native-unknown".to_owned());
        assert_eq!(
            engine.effective_logical_receipt(&receipt_fact),
            None,
            "unmapped receipt holds (v4={v4})"
        );
        // Negative: a staged inventory gap fails closed even with the
        // mapped rows restaged.
        stage_native_5a(
            &mut engine,
            v4,
            4,
            vec![native_row("out-a")],
            Vec::new(),
            false,
        );
        assert!(
            !engine.native_list_complete("out-a"),
            "staged gap fails list (v4={v4})"
        );
        assert_eq!(
            engine.native_echo_for(&OutputId("out-a".to_owned())),
            None,
            "staged gap holds echo (v4={v4})"
        );
        receipt_fact.native_workspace =
            crate::world_report::NativeWorkspaceFact::Value("native-out-a-0".to_owned());
        assert_eq!(
            engine.effective_logical_receipt(&receipt_fact),
            None,
            "staged gap holds receipt (v4={v4})"
        );
    }
}

#[test]
fn staged_native_shared_order_conflict_current_vs_stale() {
    for v4 in [false, true] {
        // Positive: first-appearance union across sorted outputs reverses
        // out-b's reported order, so the fresh snapshot is ambiguous.
        let mut engine = shared_host_engine_5a();
        let screens = vec![screen("out-a", 0), screen("out-b", 800)];
        let mut legacy = report_full(
            2,
            2,
            screens.clone(),
            Vec::new(),
            Vec::new(),
            "",
            None,
            None,
        );
        let mut v4report =
            report_v4_simple(2, screens, Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let conflict = vec![
            native_lane_5a("out-a", vec!["n1", "n2"], "n1"),
            native_lane_5a("out-b", vec!["n2", "n1"], "n2"),
        ];
        legacy.native = conflict.clone();
        v4report.native = conflict;
        stage_either(&mut engine, v4, &legacy, &v4report);
        assert!(
            engine.native_order_conflict(),
            "conflicting snapshot holds adoption (v4={v4})"
        );
        // Negative: identical lists are consistent on the same screens.
        let mut engine = shared_host_engine_5a();
        let screens = vec![screen("out-a", 0), screen("out-b", 800)];
        let mut legacy = report_full(
            2,
            2,
            screens.clone(),
            Vec::new(),
            Vec::new(),
            "",
            None,
            None,
        );
        let mut v4report =
            report_v4_simple(2, screens, Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let aligned = vec![
            native_lane_5a("out-a", vec!["n1", "n2"], "n1"),
            native_lane_5a("out-b", vec!["n1", "n2"], "n1"),
        ];
        legacy.native = aligned.clone();
        v4report.native = aligned;
        stage_either(&mut engine, v4, &legacy, &v4report);
        assert!(
            !engine.native_order_conflict(),
            "aligned snapshot consistent (v4={v4})"
        );
        // Negative: a staged inventory gap never conflicts even with the
        // conflicting rows restaged.
        let mut engine = shared_host_engine_5a();
        let screens = vec![screen("out-a", 0), screen("out-b", 800)];
        let mut legacy = report_full(
            2,
            2,
            screens.clone(),
            Vec::new(),
            Vec::new(),
            "",
            None,
            None,
        );
        let mut v4report =
            report_v4_simple(2, screens, Vec::new(), Vec::new(), Vec::new(), Vec::new());
        legacy.inventory_complete = false;
        v4report.inventory_complete = false;
        let conflict = vec![
            native_lane_5a("out-a", vec!["n1", "n2"], "n1"),
            native_lane_5a("out-b", vec!["n2", "n1"], "n2"),
        ];
        legacy.native = conflict.clone();
        v4report.native = conflict;
        stage_either(&mut engine, v4, &legacy, &v4report);
        assert!(
            !engine.native_order_conflict(),
            "staged gap never conflicts (v4={v4})"
        );
    }
}

/// Unit 6 sub-increment 5a: creation correlation reads the staged raw
/// creation lane with the exact op/logical/output tuple, bounds the
/// attempt stamp against the staged receipt clock and the mint clock,
/// and never upgrades the latched first receipt.
fn stage_creation_5a(
    engine: &mut WorldEngine,
    v4: bool,
    seq: u64,
    attempts: Vec<NativeCreationAttempt>,
    monotonic_ms: u64,
) {
    let (mut legacy, mut v4report) = pair_reports(seq, Vec::new(), Vec::new());
    legacy.monotonic_ms = monotonic_ms;
    v4report.monotonic_ms = monotonic_ms;
    legacy.native = vec![native_row("out-a")];
    v4report.native = vec![native_row("out-a")];
    legacy.creation_attempts = attempts.clone();
    v4report.creation_attempts = attempts;
    stage_either(engine, v4, &legacy, &v4report);
}

fn retained_creation_5a(minted_ms: u64) -> crate::world_engine::RetainedNativeCreation {
    crate::world_engine::RetainedNativeCreation {
        operation_id: "op-c".to_owned(),
        logical_workspace: "ws-9".to_owned(),
        target_output: OutputId("out-a".to_owned()),
        order_before: vec!["native-out-a-0".to_owned()],
        minted_ms,
        pending: None,
        ambiguous: false,
    }
}

#[test]
fn staged_native_creation_exact_tuple_clocks_and_first_receipt() {
    for v4 in [false, true] {
        // Eligible positive: the exact-tuple Accepted attempt inside both
        // clocks latches pending, and the unconsumed creation still reads
        // as a desire before settlement consumes the evidence.
        let mut engine = engine_settle_host();
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-9".to_owned()],
        );
        engine.native_map.insert(
            ("out-a".to_owned(), "native-out-a-0".to_owned()),
            "ws-1".to_owned(),
        );
        engine.retained_creations.push(retained_creation_5a(10));
        let mut attempt = creation_row("op-c", "ws-9", "out-a");
        attempt.attempt_ms = 15;
        stage_creation_5a(&mut engine, v4, 2, vec![attempt], 20);
        assert_eq!(
            engine.native_creation_desires().len(),
            1,
            "desire (v4={v4})"
        );
        engine.converge_native_pre();
        let retained = engine.retained_creations.first().expect("creation");
        assert!(retained.pending.is_some(), "exact tuple latches (v4={v4})");
        assert!(!retained.ambiguous, "in-bounds attempt clean (v4={v4})");
        assert!(
            engine.native_creation_desires().is_empty(),
            "consumed evidence leaves desires (v4={v4})"
        );
        // A contradictory replay never upgrades the latched first
        // receipt: the second tuple latches ambiguity, pending stays.
        let mut replay = creation_row("op-c", "ws-9", "out-a");
        replay.attempt_ms = 16;
        replay.order_after = vec!["native-out-a-0".to_owned(), "native-out-a-9".to_owned()];
        stage_creation_5a(&mut engine, v4, 3, vec![replay], 20);
        engine.converge_native_pre();
        let retained = engine.retained_creations.first().expect("creation");
        assert!(retained.ambiguous, "replay latches ambiguity (v4={v4})");
        assert_eq!(
            retained.pending.as_ref().map(|pending| pending.attempt_ms),
            Some(15),
            "first receipt never upgrades (v4={v4})"
        );
        // Negative: a wrong-tuple row reads as absent, never latches.
        // Same eligible baseline as the positive (bound head, no unmapped
        // candidate) so absence proves the tuple, not a race hold.
        let mut engine = engine_settle_host();
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-9".to_owned()],
        );
        engine.native_map.insert(
            ("out-a".to_owned(), "native-out-a-0".to_owned()),
            "ws-1".to_owned(),
        );
        engine.retained_creations.push(retained_creation_5a(10));
        let mut forged = creation_row("op-other", "ws-9", "out-a");
        forged.attempt_ms = 15;
        stage_creation_5a(&mut engine, v4, 2, vec![forged], 20);
        engine.converge_native_pre();
        let retained = engine.retained_creations.first().expect("creation");
        assert!(retained.pending.is_none(), "forged tuple absent (v4={v4})");
        assert!(!retained.ambiguous, "forged tuple clean (v4={v4})");
        // Negative: a stale stamp before the mint clock latches
        // ambiguity instead of proving the creation. Same eligible
        // baseline with no unmapped candidate: the positive above latches
        // clean at 15ms, so ambiguity is causal to the stamp.
        let mut engine = engine_settle_host();
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-9".to_owned()],
        );
        engine.native_map.insert(
            ("out-a".to_owned(), "native-out-a-0".to_owned()),
            "ws-1".to_owned(),
        );
        engine.retained_creations.push(retained_creation_5a(10));
        let mut stale = creation_row("op-c", "ws-9", "out-a");
        stale.attempt_ms = 5;
        stage_creation_5a(&mut engine, v4, 2, vec![stale], 20);
        engine.converge_native_pre();
        let retained = engine.retained_creations.first().expect("creation");
        assert!(
            retained.pending.is_some(),
            "stale receipt latched (v4={v4})"
        );
        assert!(retained.ambiguous, "stale stamp ambiguous (v4={v4})");
        // Negative: a future stamp past the staged receipt clock latches
        // ambiguity instead of proving the creation. Same eligible
        // baseline, so ambiguity is causal to the stamp.
        let mut engine = engine_settle_host();
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-9".to_owned()],
        );
        engine.native_map.insert(
            ("out-a".to_owned(), "native-out-a-0".to_owned()),
            "ws-1".to_owned(),
        );
        engine.retained_creations.push(retained_creation_5a(10));
        let mut future = creation_row("op-c", "ws-9", "out-a");
        future.attempt_ms = 25;
        stage_creation_5a(&mut engine, v4, 2, vec![future], 20);
        engine.converge_native_pre();
        let retained = engine.retained_creations.first().expect("creation");
        assert!(
            retained.pending.is_some(),
            "future receipt latched (v4={v4})"
        );
        assert!(retained.ambiguous, "future stamp ambiguous (v4={v4})");
    }
}

#[test]
fn staged_native_creation_hold_exact_tuple_and_unmapped_gate() {
    for v4 in [false, true] {
        // Positive: an outstanding creation faces a fresh unmapped
        // staged candidate, so the row holds.
        let mut engine = engine_settle_host();
        engine.retained_creations.push(retained_creation_5a(0));
        stage_native_5a(
            &mut engine,
            v4,
            2,
            vec![native_row("out-a")],
            Vec::new(),
            true,
        );
        assert!(
            engine.creation_hold_for("out-a", "ws-9"),
            "unmapped candidate holds (v4={v4})"
        );
        // Exact tuple: neighbouring rows never hold.
        assert!(
            !engine.creation_hold_for("out-a", "ws-8"),
            "wrong logical free (v4={v4})"
        );
        assert!(
            !engine.creation_hold_for("out-b", "ws-9"),
            "wrong output free (v4={v4})"
        );
        // Negative: once the staged candidate binds, the hold clears.
        engine.native_map.insert(
            ("out-a".to_owned(), "native-out-a-0".to_owned()),
            "ws-9".to_owned(),
        );
        assert!(
            !engine.creation_hold_for("out-a", "ws-9"),
            "bound candidate frees (v4={v4})"
        );
    }
}

#[test]
fn staged_selection_native_proof_success_refusal_and_latched_hold() {
    for v4 in [false, true] {
        // Positive: the staged current echo names the obligated home.
        let mut engine = engine_settle_host();
        seed_selection_binding(&mut engine, "op-4", false, &["out-a"]);
        engine
            .mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        engine.native_map.insert(
            ("out-a".to_owned(), "native-out-a-0".to_owned()),
            "ws-1".to_owned(),
        );
        stage_native_5a(
            &mut engine,
            v4,
            2,
            vec![native_row("out-a")],
            Vec::new(),
            true,
        );
        engine.settle_selection_switch();
        assert_eq!(
            engine.switch_settled.len(),
            1,
            "staged echo proves (v4={v4})"
        );
        assert_eq!(
            engine.switch_settled.first().map(|event| event.outcome),
            Some(SwitchOutcome::Complete),
            "proven outcome (v4={v4})"
        );
        // Negative: a staged gap holds the terminal with the binding kept.
        let mut engine = engine_settle_host();
        seed_selection_binding(&mut engine, "op-4", false, &["out-a"]);
        engine
            .mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        engine.native_map.insert(
            ("out-a".to_owned(), "native-out-a-0".to_owned()),
            "ws-1".to_owned(),
        );
        stage_native_5a(
            &mut engine,
            v4,
            2,
            vec![native_row("out-a")],
            Vec::new(),
            false,
        );
        engine.settle_selection_switch();
        assert!(
            engine.switch_settled.is_empty()
                && engine
                    .retained_selection
                    .contains_key(&OutputId("out-a".to_owned())),
            "staged gap holds terminal (v4={v4})"
        );
        // Proven refusal: the exact-tuple Refused attempt keeps its
        // readable preimage while the staged echo still names it.
        let mut engine = engine_settle_host();
        seed_selection_binding(&mut engine, "op-4", false, &["out-a"]);
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-2".to_owned()],
        );
        engine
            .native_map
            .insert(("out-a".to_owned(), "n0".to_owned()), "ws-2".to_owned());
        let refused = ViewAttempt {
            operation_id: "op-4".to_owned(),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: "ws-1".to_owned(),
            setter: FrameSetter::Refused,
            pre: Some(WorkspaceRef::Native {
                output: OutputId("out-a".to_owned()),
                workspace: crate::ids::NativeWorkspaceId::parse("n0").expect("native id"),
            }),
            post: Some(WorkspaceRef::Native {
                output: OutputId("out-a".to_owned()),
                workspace: crate::ids::NativeWorkspaceId::parse("n0").expect("native id"),
            }),
            baseline: None,
            attempt_ms: 7,
        };
        stage_native_5a(
            &mut engine,
            v4,
            2,
            vec![native_lane_5a("out-a", vec!["n0"], "n0")],
            vec![refused.clone()],
            true,
        );
        engine.settle_selection_switch();
        assert_eq!(
            engine.switch_settled.len(),
            1,
            "refusal terminates (v4={v4})"
        );
        assert_eq!(
            engine.switch_settled.first().map(|event| event.outcome),
            Some(SwitchOutcome::CompletedWithFailures),
            "refused outcome (v4={v4})"
        );
        // The identical refusal under a wrong operation never latches.
        let mut engine = engine_settle_host();
        seed_selection_binding(&mut engine, "op-4", false, &["out-a"]);
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-2".to_owned()],
        );
        engine
            .native_map
            .insert(("out-a".to_owned(), "n0".to_owned()), "ws-2".to_owned());
        let mut forged = refused;
        forged.operation_id = "op-other".to_owned();
        stage_native_5a(
            &mut engine,
            v4,
            2,
            vec![native_lane_5a("out-a", vec!["n0"], "n0")],
            vec![forged],
            true,
        );
        engine.settle_selection_feedback();
        assert!(
            engine
                .retained_selection
                .get(&OutputId("out-a".to_owned()))
                .expect("binding")
                .view_raw
                .is_empty(),
            "wrong-tuple source row never latches (v4={v4})"
        );
        engine.settle_selection_switch();
        assert!(
            engine.switch_settled.is_empty()
                && engine
                    .retained_selection
                    .contains_key(&OutputId("out-a".to_owned())),
            "wrong-tuple source row holds (v4={v4})"
        );
    }
}

fn native_occ_5a(
    output: &str,
    order: &[&str],
    current: &str,
    counts: &[Option<u32>],
) -> NativeObservation {
    assert_eq!(order.len(), counts.len(), "order/counts keyed coverage");
    NativeObservation {
        output: OutputId(output.to_owned()),
        native_order: order.iter().map(|id| (*id).to_owned()).collect(),
        current_native: Some(current.to_owned()),
        occupancy: order
            .iter()
            .zip(counts.iter())
            .map(|(id, count)| NativeOccupancy {
                native: (*id).to_owned(),
                count: *count,
            })
            .collect(),
    }
}

fn seed_domain_5a(engine: &mut WorldEngine, output: &str, workspace: &str, readable: bool) {
    engine.staged.domains.push(StagedDomain {
        key: DomainKey {
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
        },
        bounds: rect(0, 0, 800, 560),
        gap: 0,
        outer_gap: 0,
        windows: Vec::new(),
        focused: WindowId(String::new()),
        minimized: Vec::new(),
        unreadable: Vec::new(),
        closed: Vec::new(),
        readable,
        hold: None,
    });
}

/// Eligible four-row retirement seed: d1 viewed, d4 the bound trailing
/// spare, d2/d3 empty with every guard clear. Call after staging: the
/// intake rebuilds staged domains, so domain seeds would not survive.
fn seed_retirement_5a(engine: &mut WorldEngine) {
    engine.mapping.insert(
        "out-a".to_owned(),
        vec![
            "ws-1".to_owned(),
            "ws-2".to_owned(),
            "ws-3".to_owned(),
            "ws-4".to_owned(),
        ],
    );
    for (native, logical) in [
        ("d1", "ws-1"),
        ("d2", "ws-2"),
        ("d3", "ws-3"),
        ("d4", "ws-4"),
    ] {
        engine
            .native_map
            .insert(("out-a".to_owned(), native.to_owned()), logical.to_owned());
    }
    engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
    for workspace in ["ws-1", "ws-2", "ws-3", "ws-4"] {
        seed_domain_5a(engine, "out-a", workspace, true);
    }
}

fn stage_retire_5a(
    engine: &mut WorldEngine,
    v4: bool,
    seq: u64,
    native: Vec<NativeObservation>,
    complete: bool,
    barrier: bool,
    screen_complete: bool,
) {
    let (mut legacy, mut v4report) = pair_reports(seq, Vec::new(), Vec::new());
    legacy.inventory_complete = complete;
    v4report.inventory_complete = complete;
    legacy.gesture_barrier = barrier;
    v4report.gesture_barrier = barrier;
    legacy.screens[0].complete = screen_complete;
    v4report.screens[0].complete = screen_complete;
    legacy.native = native.clone();
    v4report.native = native;
    stage_either(engine, v4, &legacy, &v4report);
}

fn empty_retire_5a() -> NativeObservation {
    native_occ_5a(
        "out-a",
        &["d1", "d2", "d3", "d4"],
        "d1",
        &[Some(0), Some(0), Some(0), Some(0)],
    )
}

#[test]
fn staged_native_retirement_occupancy_join_and_holds() {
    use std::collections::BTreeSet;
    for v4 in [false, true] {
        // Eligible positive: zero-count d2/d3 with exact coverage join;
        // the viewed d1 and the trailing d4 veto.
        let mut engine = engine_settle_host();
        stage_retire_5a(
            &mut engine,
            v4,
            2,
            vec![empty_retire_5a()],
            true,
            false,
            true,
        );
        seed_retirement_5a(&mut engine);
        assert_eq!(
            engine.retirement_deletion_set(),
            BTreeSet::from([
                ("out-a".to_owned(), "ws-2".to_owned()),
                ("out-a".to_owned(), "ws-3".to_owned()),
            ]),
            "empty join retires (v4={v4})"
        );
        // Negative: staged inventory gap authorizes no deletion.
        let mut engine = engine_settle_host();
        stage_retire_5a(
            &mut engine,
            v4,
            2,
            vec![empty_retire_5a()],
            false,
            false,
            true,
        );
        seed_retirement_5a(&mut engine);
        assert!(
            engine.retirement_deletion_set().is_empty(),
            "inventory gap holds (v4={v4})"
        );
        // Negative: incomplete staged screen authorizes no deletion.
        let mut engine = engine_settle_host();
        stage_retire_5a(
            &mut engine,
            v4,
            2,
            vec![empty_retire_5a()],
            true,
            false,
            false,
        );
        seed_retirement_5a(&mut engine);
        assert!(
            engine.retirement_deletion_set().is_empty(),
            "screen gap holds (v4={v4})"
        );
        // Negative: inexact occupancy coverage authorizes no deletion.
        let mut engine = engine_settle_host();
        let mut bad = empty_retire_5a();
        bad.occupancy.pop();
        stage_retire_5a(&mut engine, v4, 2, vec![bad], true, false, true);
        seed_retirement_5a(&mut engine);
        assert!(
            engine.retirement_deletion_set().is_empty(),
            "coverage gap holds (v4={v4})"
        );
        // Negative: gesture barrier vetoes every retirement.
        let mut engine = engine_settle_host();
        stage_retire_5a(
            &mut engine,
            v4,
            2,
            vec![empty_retire_5a()],
            true,
            true,
            true,
        );
        seed_retirement_5a(&mut engine);
        assert!(
            engine.retirement_deletion_set().is_empty(),
            "barrier holds (v4={v4})"
        );
        // Occupied d2 survives while the eligible d3 still retires.
        let mut engine = engine_settle_host();
        stage_retire_5a(
            &mut engine,
            v4,
            2,
            vec![native_occ_5a(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                "d1",
                &[Some(0), Some(2), Some(0), Some(0)],
            )],
            true,
            false,
            true,
        );
        seed_retirement_5a(&mut engine);
        assert_eq!(
            engine.retirement_deletion_set(),
            BTreeSet::from([("out-a".to_owned(), "ws-3".to_owned())]),
            "occupied vetoes narrowly (v4={v4})"
        );
        // Unreadable d3 domain holds while the eligible d2 still retires.
        let mut engine = engine_settle_host();
        stage_retire_5a(
            &mut engine,
            v4,
            2,
            vec![empty_retire_5a()],
            true,
            false,
            true,
        );
        seed_retirement_5a(&mut engine);
        engine
            .staged
            .domains
            .retain(|domain| domain.key.workspace.0 != "ws-3");
        seed_domain_5a(&mut engine, "out-a", "ws-3", false);
        assert_eq!(
            engine.retirement_deletion_set(),
            BTreeSet::from([("out-a".to_owned(), "ws-2".to_owned())]),
            "unreadable holds narrowly (v4={v4})"
        );
    }
}

#[test]
fn staged_native_adoption_reorder_and_prune() {
    for v4 in [false, true] {
        // Outside desktop binds at native-order position behind the
        // retained head; the ring follows the staged current order.
        let mut engine = engine_settle_host();
        // Fresh mints count from ws-1: skip past the seeded row.
        engine.creation_counter = 100;
        engine
            .mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        engine
            .native_map
            .insert(("out-a".to_owned(), "d1".to_owned()), "ws-1".to_owned());
        stage_native_5a(
            &mut engine,
            v4,
            2,
            vec![native_lane_5a("out-a", vec!["d1", "d2"], "d2")],
            Vec::new(),
            true,
        );
        engine.converge_native_pre();
        let ring = engine.mapping.get("out-a").cloned().unwrap_or_default();
        assert_eq!(ring.len(), 2, "outside desktop binds (v4={v4})");
        assert_eq!(ring[0], "ws-1", "head keeps order (v4={v4})");
        let fresh = ring[1].clone();
        assert_ne!(fresh, "ws-1", "fresh id minted (v4={v4})");
        assert_eq!(
            engine
                .native_map
                .get(&("out-a".to_owned(), "d2".to_owned())),
            Some(&fresh),
            "outside desktop bound (v4={v4})"
        );
        // Reorder-only: the same identities follow the observed order.
        let mut engine = engine_settle_host();
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-2".to_owned(), "ws-1".to_owned()],
        );
        engine
            .native_map
            .insert(("out-a".to_owned(), "d1".to_owned()), "ws-1".to_owned());
        engine
            .native_map
            .insert(("out-a".to_owned(), "d2".to_owned()), "ws-2".to_owned());
        stage_native_5a(
            &mut engine,
            v4,
            2,
            vec![native_lane_5a("out-a", vec!["d1", "d2"], "d1")],
            Vec::new(),
            true,
        );
        engine.converge_native_pre();
        assert_eq!(
            engine.mapping.get("out-a"),
            Some(&vec!["ws-1".to_owned(), "ws-2".to_owned()]),
            "reorder follows native order (v4={v4})"
        );
        assert_eq!(
            engine
                .native_map
                .get(&("out-a".to_owned(), "d1".to_owned())),
            Some(&"ws-1".to_owned()),
            "identities preserved (v4={v4})"
        );
        // Vanished native prunes its pair on a complete snapshot.
        let mut engine = engine_settle_host();
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-9".to_owned()],
        );
        engine
            .native_map
            .insert(("out-a".to_owned(), "d1".to_owned()), "ws-1".to_owned());
        engine
            .native_map
            .insert(("out-a".to_owned(), "dx".to_owned()), "ws-9".to_owned());
        stage_native_5a(
            &mut engine,
            v4,
            2,
            vec![native_lane_5a("out-a", vec!["d1"], "d1")],
            Vec::new(),
            true,
        );
        engine.converge_native_pre();
        assert!(
            !engine
                .native_map
                .contains_key(&("out-a".to_owned(), "dx".to_owned())),
            "vanished pair prunes (v4={v4})"
        );
        assert_eq!(
            engine
                .native_map
                .get(&("out-a".to_owned(), "d1".to_owned())),
            Some(&"ws-1".to_owned()),
            "survivor keeps binding (v4={v4})"
        );
        // Negative: an incomplete staged list never prunes.
        let mut engine = engine_settle_host();
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-9".to_owned()],
        );
        engine
            .native_map
            .insert(("out-a".to_owned(), "d1".to_owned()), "ws-1".to_owned());
        engine
            .native_map
            .insert(("out-a".to_owned(), "dx".to_owned()), "ws-9".to_owned());
        stage_native_5a(
            &mut engine,
            v4,
            2,
            vec![native_lane_5a("out-a", vec!["d1"], "d1")],
            Vec::new(),
            false,
        );
        engine.converge_native_pre();
        assert_eq!(
            engine
                .native_map
                .get(&("out-a".to_owned(), "dx".to_owned())),
            Some(&"ws-9".to_owned()),
            "gap holds vanished pair (v4={v4})"
        );
    }
}

#[test]
fn staged_native_provisioning_mint_authorization_and_gates() {
    for v4 in [false, true] {
        // Eligible positive: the unbound demanded ws-9 mints one desire
        // stamped with the staged clock, stable across repeats.
        let mut engine = engine_settle_host();
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-9".to_owned()],
        );
        engine.views.insert("out-a".to_owned(), "ws-9".to_owned());
        stage_creation_5a(&mut engine, v4, 2, Vec::new(), 42);
        engine.ensure_native_creations("out-a");
        assert_eq!(engine.retained_creations.len(), 1, "demand mints (v4={v4})");
        let minted = engine.retained_creations.first().expect("minted").clone();
        assert_eq!(minted.logical_workspace, "ws-9", "mint target (v4={v4})");
        assert_eq!(minted.minted_ms, 42, "staged mint clock (v4={v4})");
        assert_eq!(
            minted.order_before,
            vec!["native-out-a-0".to_owned()],
            "mint baseline order (v4={v4})"
        );
        let desires = engine.native_creation_desires();
        assert_eq!(desires.len(), 1, "desire emitted (v4={v4})");
        assert_eq!(
            desires[0].logical_workspace, "ws-9",
            "desire target (v4={v4})"
        );
        assert_eq!(
            desires[0].operation_id, minted.operation_id,
            "desire operation (v4={v4})"
        );
        engine.ensure_native_creations("out-a");
        assert_eq!(
            engine.retained_creations.len(),
            1,
            "repeat mints nothing (v4={v4})"
        );
        assert_eq!(
            engine
                .retained_creations
                .first()
                .expect("minted")
                .operation_id,
            minted.operation_id,
            "stable operation id (v4={v4})"
        );
        // Negative: incomplete inventory mints nothing.
        let mut engine = engine_settle_host();
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-9".to_owned()],
        );
        engine.views.insert("out-a".to_owned(), "ws-9".to_owned());
        let (mut legacy, mut v4report) = pair_reports(2, Vec::new(), Vec::new());
        legacy.inventory_complete = false;
        v4report.inventory_complete = false;
        legacy.native = vec![native_row("out-a")];
        v4report.native = vec![native_row("out-a")];
        stage_either(&mut engine, v4, &legacy, &v4report);
        engine.ensure_native_creations("out-a");
        assert!(
            engine.retained_creations.is_empty() && engine.native_creation_desires().is_empty(),
            "inventory gap blocks mint (v4={v4})"
        );
        // Negative: incomplete staged screen mints nothing.
        let mut engine = engine_settle_host();
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-9".to_owned()],
        );
        engine.views.insert("out-a".to_owned(), "ws-9".to_owned());
        let (mut legacy, mut v4report) = pair_reports(2, Vec::new(), Vec::new());
        legacy.screens[0].complete = false;
        v4report.screens[0].complete = false;
        legacy.native = vec![native_row("out-a")];
        v4report.native = vec![native_row("out-a")];
        stage_either(&mut engine, v4, &legacy, &v4report);
        engine.ensure_native_creations("out-a");
        assert!(
            engine.retained_creations.is_empty() && engine.native_creation_desires().is_empty(),
            "screen gap blocks mint (v4={v4})"
        );
        // Negative: absent native row mints nothing.
        let mut engine = engine_settle_host();
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-9".to_owned()],
        );
        engine.views.insert("out-a".to_owned(), "ws-9".to_owned());
        let (legacy, v4report) = pair_reports(2, Vec::new(), Vec::new());
        stage_either(&mut engine, v4, &legacy, &v4report);
        engine.ensure_native_creations("out-a");
        assert!(
            engine.retained_creations.is_empty() && engine.native_creation_desires().is_empty(),
            "absent row blocks mint (v4={v4})"
        );
        // Negative: pending and ambiguous creations emit no desire.
        let mut engine = engine_settle_host();
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-9".to_owned()],
        );
        engine.views.insert("out-a".to_owned(), "ws-9".to_owned());
        stage_creation_5a(&mut engine, v4, 2, Vec::new(), 42);
        engine.ensure_native_creations("out-a");
        let pending = creation_row("op-c", "ws-9", "out-a");
        engine.retained_creations[0].pending = Some(pending);
        assert!(
            engine.native_creation_desires().is_empty(),
            "pending emits nothing (v4={v4})"
        );
        engine.retained_creations[0].pending = None;
        engine.retained_creations[0].ambiguous = true;
        assert!(
            engine.native_creation_desires().is_empty(),
            "ambiguous emits nothing (v4={v4})"
        );
        // Shared gate positive: a complete Shared snapshot emits the
        // seeded retained desire; the identical incomplete non-target
        // screen then hides the same retained row.
        let mut engine = shared_host_engine_5a();
        engine.retained_creations.push(retained_creation_5a(10));
        let screens = vec![screen("out-a", 0), screen("out-b", 800)];
        let mut legacy = report_full(
            2,
            2,
            screens.clone(),
            Vec::new(),
            Vec::new(),
            "",
            None,
            None,
        );
        let mut v4report =
            report_v4_simple(2, screens, Vec::new(), Vec::new(), Vec::new(), Vec::new());
        legacy.native = vec![native_row("out-a"), native_row("out-b")];
        v4report.native = vec![native_row("out-a"), native_row("out-b")];
        stage_either(&mut engine, v4, &legacy, &v4report);
        assert_eq!(
            engine.native_creation_desires().len(),
            1,
            "shared complete emits (v4={v4})"
        );
        let mut engine = shared_host_engine_5a();
        engine.retained_creations.push(retained_creation_5a(10));
        let screens = vec![screen("out-a", 0), screen("out-b", 800)];
        let mut legacy = report_full(
            2,
            2,
            screens.clone(),
            Vec::new(),
            Vec::new(),
            "",
            None,
            None,
        );
        let mut v4report =
            report_v4_simple(2, screens, Vec::new(), Vec::new(), Vec::new(), Vec::new());
        v4report.screens[1].complete = false;
        legacy.screens[1].complete = false;
        legacy.native = vec![native_row("out-a"), native_row("out-b")];
        v4report.native = vec![native_row("out-a"), native_row("out-b")];
        stage_either(&mut engine, v4, &legacy, &v4report);
        assert!(
            engine.native_creation_desires().is_empty(),
            "shared screen gap blocks desires (v4={v4})"
        );
    }
}

#[test]
fn staged_native_source_release_defer_and_retry() {
    for v4 in [false, true] {
        // The migrated ws-9 left the out-a ring; its dx pair is dormant.
        let mut engine = engine_settle_host();
        // Fresh mints count from ws-1: skip past the seeded row.
        engine.creation_counter = 100;
        engine
            .mapping
            .insert("out-a".to_owned(), vec!["ws-1".to_owned()]);
        engine
            .native_map
            .insert(("out-a".to_owned(), "d1".to_owned()), "ws-1".to_owned());
        engine
            .native_map
            .insert(("out-a".to_owned(), "dx".to_owned()), "ws-9".to_owned());
        // Incomplete staged list requeues the pair without adopting.
        stage_native_5a(
            &mut engine,
            v4,
            2,
            vec![native_lane_5a("out-a", vec!["d1", "dx"], "d1")],
            Vec::new(),
            false,
        );
        let out = OutputId("out-a".to_owned());
        assert!(
            engine
                .release_migrated_source_backing(&out, "ws-9")
                .is_empty(),
            "gap releases nothing (v4={v4})"
        );
        assert_eq!(
            engine.pending_source_releases,
            vec![("out-a".to_owned(), "ws-9".to_owned())],
            "gap requeues pair (v4={v4})"
        );
        assert_eq!(
            engine
                .native_map
                .get(&("out-a".to_owned(), "dx".to_owned())),
            Some(&"ws-9".to_owned()),
            "gap holds pair (v4={v4})"
        );
        engine.release_migrated_source_backing(&out, "ws-9");
        assert_eq!(
            engine.pending_source_releases.len(),
            1,
            "requeue deduplicates (v4={v4})"
        );
        // Complete recovery drives the retry itself: the complete
        // snapshot converges while the pair is pending, so native intake
        // releases and adopts once with a fresh binding and drained queue.
        stage_native_5a(
            &mut engine,
            v4,
            3,
            vec![native_lane_5a("out-a", vec!["d1", "dx"], "d1")],
            Vec::new(),
            true,
        );
        engine.converge_native_pre();
        assert!(
            engine.pending_source_releases.is_empty(),
            "retry drains queue (v4={v4})"
        );
        let fresh = engine
            .native_map
            .get(&("out-a".to_owned(), "dx".to_owned()))
            .cloned()
            .expect("dx rebound");
        assert_ne!(fresh, "ws-9", "fresh id minted (v4={v4})");
        assert_eq!(
            engine.mapping.get("out-a"),
            Some(&vec!["ws-1".to_owned(), fresh.clone()]),
            "fresh row at order (v4={v4})"
        );
        engine.converge_native_pre();
        assert!(
            engine.pending_source_releases.is_empty(),
            "repeat drains queue (v4={v4})"
        );
        assert_eq!(
            engine
                .native_map
                .get(&("out-a".to_owned(), "dx".to_owned())),
            Some(&fresh),
            "retry binds nothing new (v4={v4})"
        );
        assert_eq!(
            engine.mapping.get("out-a"),
            Some(&vec!["ws-1".to_owned(), fresh.clone()]),
            "repeat stable (v4={v4})"
        );
    }
}

#[test]
fn staged_selection_feedback_first_latch_across_restage() {
    for v4 in [false, true] {
        // Refusal latches at feedback; a later recovery that omits fresh
        // attempts still terminates against the readable preimage.
        let mut engine = engine_settle_host();
        seed_selection_binding(&mut engine, "op-4", false, &["out-a"]);
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-2".to_owned()],
        );
        engine
            .native_map
            .insert(("out-a".to_owned(), "n0".to_owned()), "ws-2".to_owned());
        let refused = ViewAttempt {
            operation_id: "op-4".to_owned(),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: "ws-1".to_owned(),
            setter: FrameSetter::Refused,
            pre: Some(WorkspaceRef::Native {
                output: OutputId("out-a".to_owned()),
                workspace: crate::ids::NativeWorkspaceId::parse("n0").expect("native id"),
            }),
            post: Some(WorkspaceRef::Native {
                output: OutputId("out-a".to_owned()),
                workspace: crate::ids::NativeWorkspaceId::parse("n0").expect("native id"),
            }),
            baseline: None,
            attempt_ms: 7,
        };
        stage_native_5a(
            &mut engine,
            v4,
            2,
            vec![native_lane_5a("out-a", vec!["n0"], "n0")],
            vec![refused],
            true,
        );
        engine.settle_selection_feedback();
        assert_eq!(
            engine
                .retained_selection
                .get(&OutputId("out-a".to_owned()))
                .and_then(|binding| binding.view_raw.get(&OutputId("out-a".to_owned())))
                .map(|attempt| attempt.attempt_ms),
            Some(7),
            "first raw latches (v4={v4})"
        );
        // Fresh attempts gone on recovery: the latched raw still proves.
        stage_native_5a(
            &mut engine,
            v4,
            3,
            vec![native_lane_5a("out-a", vec!["n0"], "n0")],
            Vec::new(),
            true,
        );
        engine.settle_selection_feedback();
        engine.settle_selection_switch();
        assert_eq!(
            engine.switch_settled.len(),
            1,
            "latched refusal terminates (v4={v4})"
        );
        assert_eq!(
            engine.switch_settled.first().map(|event| event.outcome),
            Some(SwitchOutcome::CompletedWithFailures),
            "refused outcome (v4={v4})"
        );
        // Accepted ambiguous original: a later genuinely-proving raw
        // (Accepted post n1 -> ws-1) never upgrades the latch, so the
        // terminal still holds while the current echo stays n0/ws-2.
        let mut engine = engine_settle_host();
        seed_selection_binding(&mut engine, "op-4", false, &["out-a"]);
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-2".to_owned()],
        );
        engine
            .native_map
            .insert(("out-a".to_owned(), "n0".to_owned()), "ws-2".to_owned());
        engine
            .native_map
            .insert(("out-a".to_owned(), "n1".to_owned()), "ws-1".to_owned());
        let mut original = view_row("op-4");
        original.attempt_ms = 7;
        stage_native_5a(
            &mut engine,
            v4,
            2,
            vec![native_lane_5a("out-a", vec!["n0"], "n0")],
            vec![original],
            true,
        );
        engine.settle_selection_feedback();
        engine.settle_selection_switch();
        assert!(
            engine.switch_settled.is_empty(),
            "ambiguous original holds (v4={v4})"
        );
        let mut contradict = view_row("op-4");
        contradict.attempt_ms = 8;
        contradict.post = Some(WorkspaceRef::Native {
            output: OutputId("out-a".to_owned()),
            workspace: crate::ids::NativeWorkspaceId::parse("n1").expect("native id"),
        });
        stage_native_5a(
            &mut engine,
            v4,
            3,
            vec![native_lane_5a("out-a", vec!["n0"], "n0")],
            vec![contradict],
            true,
        );
        engine.settle_selection_feedback();
        engine.settle_selection_switch();
        assert!(
            engine.switch_settled.is_empty(),
            "contradiction never upgrades (v4={v4})"
        );
        assert_eq!(
            engine
                .retained_selection
                .get(&OutputId("out-a".to_owned()))
                .and_then(|binding| binding.view_raw.get(&OutputId("out-a".to_owned())))
                .map(|attempt| attempt.attempt_ms),
            Some(7),
            "first raw kept (v4={v4})"
        );
        // Positive control: the same proving raw without the original
        // latch actually completes, so the hold above proves the
        // first-latch guard rather than an unprovable row.
        let mut control = engine_settle_host();
        seed_selection_binding(&mut control, "op-4", false, &["out-a"]);
        control.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-2".to_owned()],
        );
        control
            .native_map
            .insert(("out-a".to_owned(), "n0".to_owned()), "ws-2".to_owned());
        control
            .native_map
            .insert(("out-a".to_owned(), "n1".to_owned()), "ws-1".to_owned());
        let mut proof = view_row("op-4");
        proof.attempt_ms = 8;
        proof.post = Some(WorkspaceRef::Native {
            output: OutputId("out-a".to_owned()),
            workspace: crate::ids::NativeWorkspaceId::parse("n1").expect("native id"),
        });
        stage_native_5a(
            &mut control,
            v4,
            3,
            vec![native_lane_5a("out-a", vec!["n0"], "n0")],
            vec![proof],
            true,
        );
        control.settle_selection_feedback();
        control.settle_selection_switch();
        assert_eq!(
            control.switch_settled.len(),
            1,
            "proving raw completes without latch (v4={v4})"
        );
        assert_eq!(
            control.switch_settled.first().map(|event| event.outcome),
            Some(SwitchOutcome::Complete),
            "control proven outcome (v4={v4})"
        );
        // ... but the independent fresh current-target echo proves.
        stage_native_5a(
            &mut engine,
            v4,
            4,
            vec![native_lane_5a("out-a", vec!["n0", "n1"], "n1")],
            Vec::new(),
            true,
        );
        engine.settle_selection_feedback();
        engine.settle_selection_switch();
        assert_eq!(
            engine.switch_settled.len(),
            1,
            "fresh echo proves (v4={v4})"
        );
        assert_eq!(
            engine.switch_settled.first().map(|event| event.outcome),
            Some(SwitchOutcome::Complete),
            "proven outcome (v4={v4})"
        );
    }
}

#[test]
fn staged_native_last_views_follow_only_staged_echo_dual_intake() {
    // Helper-level staged probe only (no v4 runtime claim: the v4
    // accepted-run domain assembly is unbuilt, so mappings are seeds and
    // only the staged native/current lanes under test come from the real
    // intakes). The staged native echo alone decides; the legacy logical
    // `views` lane never does, even when it contradicts the staged echo.
    for v4 in [false, true] {
        let mut engine = engine_settle_host();
        engine.mapping.insert(
            "out-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-9".to_owned()],
        );
        engine
            .native_map
            .insert(("out-a".to_owned(), "n0".to_owned()), "ws-1".to_owned());
        engine
            .native_map
            .insert(("out-a".to_owned(), "n1".to_owned()), "ws-9".to_owned());
        stage_native_5a(
            &mut engine,
            v4,
            2,
            vec![native_lane_5a("out-a", vec!["n0", "n1"], "n0")],
            Vec::new(),
            true,
        );
        engine.retain_native_last_views();
        assert_eq!(
            engine.last_views.get(&OutputId("out-a".to_owned())),
            Some(&"ws-1".to_owned()),
            "staged n0 echo retains (v4={v4})"
        );
        // Same staged native move through the same intake, but the legacy
        // logical lane still names the stale workspace: native wins.
        let (mut legacy, mut v4report) = pair_reports(3, Vec::new(), Vec::new());
        let lane = native_lane_5a("out-a", vec!["n0", "n1"], "n1");
        legacy.native = vec![lane.clone()];
        v4report.native = vec![lane];
        legacy.inventory_complete = true;
        v4report.inventory_complete = true;
        legacy.views = vec![ScreenView {
            output: OutputId("out-a".to_owned()),
            view: "ws-1".to_owned(),
        }];
        stage_either(&mut engine, v4, &legacy, &v4report);
        engine.retain_native_last_views();
        assert_eq!(
            engine.last_views.get(&OutputId("out-a".to_owned())),
            Some(&"ws-9".to_owned()),
            "staged n1 echo wins over stale logical lane (v4={v4})"
        );
    }
    // True cross-intake leg: v4 source intake first, then a contradictory
    // legacy intake. Only the current staged echo decides.
    let mut engine = engine_settle_host();
    engine.mapping.insert(
        "out-a".to_owned(),
        vec!["ws-1".to_owned(), "ws-9".to_owned()],
    );
    engine
        .native_map
        .insert(("out-a".to_owned(), "n0".to_owned()), "ws-1".to_owned());
    engine
        .native_map
        .insert(("out-a".to_owned(), "n1".to_owned()), "ws-9".to_owned());
    let (mut legacy0, mut v40) = pair_reports(2, Vec::new(), Vec::new());
    let first = native_lane_5a("out-a", vec!["n0", "n1"], "n0");
    legacy0.native = vec![first.clone()];
    v40.native = vec![first];
    stage_either(&mut engine, true, &legacy0, &v40);
    engine.retain_native_last_views();
    assert_eq!(
        engine.last_views.get(&OutputId("out-a".to_owned())),
        Some(&"ws-1".to_owned()),
        "v4 staged n0 echo retains"
    );
    let (mut legacy1, mut v41) = pair_reports(3, Vec::new(), Vec::new());
    let second = native_lane_5a("out-a", vec!["n0", "n1"], "n1");
    legacy1.native = vec![second.clone()];
    v41.native = vec![second];
    legacy1.views = vec![ScreenView {
        output: OutputId("out-a".to_owned()),
        view: "ws-1".to_owned(),
    }];
    stage_either(&mut engine, false, &legacy1, &v41);
    engine.retain_native_last_views();
    assert_eq!(
        engine.last_views.get(&OutputId("out-a".to_owned())),
        Some(&"ws-9".to_owned()),
        "later legacy staged n1 echo wins over its own stale logical lane"
    );
}
