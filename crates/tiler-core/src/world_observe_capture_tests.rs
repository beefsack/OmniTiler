//! Raw observation capture regressions (unit 6 sub-increment 4a): both
//! intakes stage the same verbatim observation boundary before any
//! classification, intent ingestion consumes the staged capture before the
//! classifier runs, and capture never mutates retained routing state.

use std::collections::{BTreeMap, BTreeSet};

use crate::connection::{HostCapabilities, RawSettingValue};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::gesture::{GestureIngress, GestureKind, GesturePhase};
use crate::intent_data::{IntentData, encode_intent_data};
use crate::seed::EngineWindow;
use crate::session::DomainKey;
use crate::world_directional::tests::{accepted, fact, rect, report_full, screen};
use crate::world_engine::{ExcludedIntent, StagedDomain, StagedObservation, WorldEngine};
use crate::world_focus::{FocusVerdict, RetainedFocusGrant};
use crate::world_members::MemberState;
use crate::world_report::{
    ArrivalBracket, ArrivalLocation, AttemptLaneV4, AttemptOutcome, AuthorityScope, ClearAttempt,
    EffectCause, FocusFeedback, FrameAttempt, FrameSetter, HeldAttemptRow, HeldNativeRow,
    HeldScreenRow, HeldWindowRow, IntentRecord, IntentWrite, MemberAttempt, NativeCreationAttempt,
    NativeObservation, NativeOccupancy, OneShotEffect, PresentationIngressV4, PresentationRowV4,
    RenderTarget, RendererInput, RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane,
    ScopeAuthority, ScreenReport, StateAttempt, StateProperty, ViewAttempt, VisibilityFact,
    VisibilityReason, VisibilitySample, VisibilitySetter, WORLD_CONTRACT_VERSION_V4, WindowDesire,
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
