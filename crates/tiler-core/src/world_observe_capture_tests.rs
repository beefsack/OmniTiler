//! Raw observation capture regressions (unit 6 sub-increment 4a): both
//! intakes stage the same verbatim observation boundary before any
//! classification, intent ingestion consumes the staged capture before the
//! classifier runs, and capture never mutates retained routing state.

use crate::connection::HostCapabilities;
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::gesture::{GestureIngress, GestureKind, GesturePhase};
use crate::intent_data::{IntentData, encode_intent_data};
use crate::world_directional::tests::{accepted, fact, rect, report_full, screen};
use crate::world_engine::{StagedObservation, WorldEngine};
use crate::world_report::{
    ArrivalBracket, AttemptLaneV4, AuthorityScope, ClearAttempt, FocusFeedback, FrameAttempt,
    FrameSetter, HeldAttemptRow, HeldNativeRow, HeldScreenRow, HeldWindowRow, IntentRecord,
    MemberAttempt, NativeCreationAttempt, NativeObservation, NativeOccupancy,
    PresentationIngressV4, PresentationRowV4, RenderTarget, RendererInput, RowDiagnostic,
    RowDiagnosticKind, RowDiagnosticLane, StateAttempt, StateProperty, ViewAttempt,
    VisibilityReason, VisibilitySetter, WORLD_CONTRACT_VERSION_V4, WindowFact, WorkspaceRef,
    WorldReport, WorldReportV4,
};

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
    // Restages below reuse classification-only `stage_facts` and must
    // never ingest again.
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
