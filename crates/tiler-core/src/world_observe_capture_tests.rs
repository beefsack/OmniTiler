//! Raw observation capture regressions (unit 6 sub-increment 4a): both
//! intakes stage the same verbatim observation boundary before any
//! classification, intent ingestion consumes the staged capture before the
//! classifier runs, and capture never mutates retained routing state.

use std::collections::BTreeMap;

use crate::connection::HostCapabilities;
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::gesture::{GestureIngress, GestureKind, GesturePhase};
use crate::intent_data::{IntentData, encode_intent_data};
use crate::session::DomainKey;
use crate::world_directional::tests::{accepted, fact, rect, report_full, screen};
use crate::world_engine::{ExcludedIntent, StagedDomain, StagedObservation, WorldEngine};
use crate::world_focus::{FocusVerdict, RetainedFocusGrant};
use crate::world_members::MemberState;
use crate::world_report::{
    ArrivalBracket, AttemptLaneV4, AttemptOutcome, AuthorityScope, ClearAttempt, FocusFeedback,
    FrameAttempt, FrameSetter, HeldAttemptRow, HeldNativeRow, HeldScreenRow, HeldWindowRow,
    IntentRecord, IntentWrite, MemberAttempt, NativeCreationAttempt, NativeObservation,
    NativeOccupancy, OneShotEffect, PresentationIngressV4, PresentationRowV4, RenderTarget,
    RendererInput, RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane, ScreenReport, StateAttempt,
    StateProperty, ViewAttempt, VisibilityReason, VisibilitySetter, WORLD_CONTRACT_VERSION_V4,
    WindowFact, WorkspaceRef, WorldReport, WorldReportV4,
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
