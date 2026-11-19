//! Direct staged-v4 publication and admission/finalization regressions:
//! genuine parallel `publish_v4` assembly from staged/retained state
//! (seeding admissions labelled; synthetic retained seeds name their own
//! correlation explicitly, never parsed from operation ids), plus staged-v4
//! admission, `finalize_v4_candidate` adoption/transactional-rejection, and
//! consumed-command reconciliation regressions for operation/switch slot caps
//! and future reply-budget reserves.

use std::collections::{BTreeMap, BTreeSet};

use crate::connection::{HostCapabilities, RawSettingValue};
use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::gesture::GestureSettled;
use crate::world_engine::{RetainedNativeCreation, RetainedTransfer, WorldEngine};
use crate::world_publish_v4::PublishEnvelopeV4;
use crate::world_report::{
    AuthorityScope, HoldReason, NativeObservation, NativeOccupancy, OperationEvent, OperationKind,
    OperationOutcome, PresentationIngressV4, PresentationPagerFrameV4, PresentationRowV4,
    RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane, ScopeAuthority, ScreenReport, SwitchEvent,
    SwitchFocus, SwitchOutcome, TransferKind, TransferTarget, WORLD_CONTRACT_VERSION_V4,
    WindowFact, WorldCommand, WorldReplyV4, WorldReportV4,
};
use crate::world_run_v4::V4RunOutcome;
use crate::world_v4_support_tests::{
    accepted as accepted_v4, fact, report_full as report_full_v4, screen,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn engine_with(settings: Vec<(String, RawSettingValue)>) -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            maximize: true,
            fullscreen: true,
            native_sticky: true,
            above: true,
            window_suppression: true,
            ..HostCapabilities::default()
        },
        settings,
    )
    .expect("valid generation")
}

fn engine_host() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
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
        vec![
            ("borderWidth".to_owned(), RawSettingValue::Number(32.0)),
            ("borderGap".to_owned(), RawSettingValue::Number(0.0)),
            ("borderRadius".to_owned(), RawSettingValue::Number(0.0)),
        ],
    )
    .expect("generation")
}

fn witness(fg: &str, frame: Rect) -> crate::world_report::FrameWitness {
    crate::world_report::FrameWitness {
        win: WindowId(fg.to_owned()),
        obs: 3,
        frame,
        foreground: WindowId(fg.to_owned()),
    }
}

fn ingress_v4(fg: &str) -> PresentationIngressV4 {
    let frame = if fg.is_empty() {
        None
    } else {
        Some(rect(0, 0, 40, 90))
    };
    PresentationIngressV4 {
        foreground: fg.to_owned(),
        foreground_minimized: Some(false),
        foreground_maximized: Some(false),
        foreground_fullscreen: Some(false),
        foreground_host_hidden: Some(false),
        foreground_hidden_workspace: Some(false),
        foreground_frame: frame,
        chord_held: false,
        move_window: String::new(),
        pager_order: Vec::new(),
        pager_frames: Vec::new(),
        witness: frame.map(|_| witness(fg, rect(0, 0, 40, 90))),
        foreground_out: if fg.is_empty() {
            None
        } else {
            Some(OutputId("out-a".to_owned()))
        },
    }
}

fn report_v4(
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    ingress: PresentationIngressV4,
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
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: Vec::new(),
        native: Vec::new(),
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        foreground_known: !focused.is_empty(),
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
        visibility_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(ingress),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn screens_pair() -> Vec<ScreenReport> {
    vec![screen("out-a", 0), screen("out-b", 800)]
}

fn facts_pair() -> Vec<WindowFact> {
    vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-b", 800),
    ]
}

/// Genuine v4 admissions plus a staged v4 report, seeding only: builds live
/// sessions/mapping/views, then replaces staged state with the v4 report.
/// Never retains pager presentation (the no-side-effect test stages pager
/// rows that must stay unretained).
fn seeded(
    engine: &mut WorldEngine,
    base: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    ingress: PresentationIngressV4,
) {
    accepted_v4(
        engine,
        &report_full_v4(
            base,
            base,
            screens.clone(),
            windows.clone(),
            Vec::new(),
            focused,
            None,
            None,
        ),
    );
    accepted_v4(
        engine,
        &report_full_v4(
            base + 1,
            base + 1,
            screens.clone(),
            windows.clone(),
            Vec::new(),
            focused,
            None,
            None,
        ),
    );
    engine.tx(|tx| tx.stage_facts_v4(&report_v4(base + 2, screens, windows, focused, ingress)));
    // Domain assembly from the staged v4 facts (shared intake helper,
    // report-independent): publication reads staged domains, and the
    // v4 converge path that would own this call is later work.
    engine.tx(|tx| tx.build_staged_domains());
}

fn envelope(rev: u64) -> PublishEnvelopeV4 {
    PublishEnvelopeV4 {
        adapter_incarnation: "adapter-1".to_owned(),
        request_id: format!("req-{rev}"),
        observation_seq: rev,
        command_consumed: false,
        issuing_revision: rev,
    }
}

fn publish_and_finish_v4(
    engine: &mut WorldEngine,
    rev: u64,
    report: &WorldReportV4,
) -> WorldReplyV4 {
    let outcome = engine.tx(|tx| {
        let reply = tx.publish_v4(envelope(rev), None, Vec::new(), &BTreeSet::new());
        tx.finish_v4(report, rev, reply)
    });
    match outcome {
        V4RunOutcome::Accepted(reply) | V4RunOutcome::CommandConsumed(reply) => reply,
        other => panic!("unexpected v4 finish outcome: {other:?}"),
    }
}

/// Synthetic retained window-transfer seed: the correlation is explicit,
/// never parsed from the operation id at projection time.
fn retained_window(
    op: &str,
    seq: Option<u64>,
    window: &str,
    source: &str,
    target: &str,
    workspace: &str,
) -> RetainedTransfer {
    RetainedTransfer {
        target: TransferTarget {
            operation_id: op.to_owned(),
            kind: TransferKind::Window(WindowId(window.to_owned())),
            target_output: OutputId(target.to_owned()),
            target_workspace: workspace.to_owned(),
            follow: false,
            held: false,
        },
        command_seq: seq,
        policy: false,
        source_output: OutputId(source.to_owned()),
        source_workspace: "ws-src".to_owned(),
        members: Vec::new(),
        migration_refill: None,
        migration_initial_empty: false,
        migration_source_index: None,
        migration_source_view: None,
        migration_target_view: None,
        migration_terminal: None,
        migration_selection_rev: None,
        pending_member: None,
        member_predecessors: Vec::new(),
        pending_members: BTreeMap::new(),
        pending_view: None,
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

#[test]
fn clean_publish_authorizes_without_holds() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let reply = publish_and_finish_v4(
        &mut engine,
        7,
        &report_v4(
            3,
            screens_pair(),
            facts_pair(),
            "win-a",
            ingress_v4("win-a"),
        ),
    );
    assert_eq!(reply.contract_version, WORLD_CONTRACT_VERSION_V4);
    assert_eq!(reply.adapter_incarnation, "adapter-1");
    assert_eq!(reply.request_id, "req-7");
    assert_eq!(reply.based_on_observation_seq, 7);
    assert_eq!(reply.state_revision, 7);
    assert!(reply.ready);
    assert!(!reply.command_consumed);
    assert_eq!(reply.outputs.len(), 2);
    for row in &reply.outputs {
        assert_eq!(row.hold, None, "clean output holds nothing");
    }
    for row in &reply.workspaces {
        assert_eq!(row.hold, None, "clean workspace holds nothing: {row:?}");
    }
    let win_a = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("win-a row");
    assert_eq!(win_a.membership_authority, ScopeAuthority::Controlled);
    assert!(reply.transfers.is_empty());
    assert!(reply.diagnostics.is_empty());
    assert!(reply.effects.is_empty());
    assert_eq!(reply.gesture_settled, None);
    assert!(reply.operation_settled.is_empty());
    assert!(reply.switch_settled.is_empty());
    assert!(reply.intent_desired.is_empty());
    assert_eq!(reply.cursor_warp, None);
    assert_eq!(reply.presentation.foreground, "win-a");
    assert_eq!(reply.presentation.rev, 7);
    assert_eq!(engine.world.presentation.latest_published_rev_v4, 7);
}

#[test]
fn transfer_seq_survives_commandless_and_unrelated() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    engine.world.settlement.retained.push(retained_window(
        "op-7",
        Some(7),
        "win-t1",
        "out-a",
        "out-b",
        "ws-x",
    ));
    // Commandless publication keeps the originating sequence.
    let first = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    assert_eq!(first.transfers.len(), 1);
    assert_eq!(first.transfers[0].operation_id, "op-7");
    assert_eq!(first.transfers[0].command_seq, Some(7));
    assert!(!first.transfers[0].held);
    // An unrelated command alongside never rewrites the retained sequence.
    let unrelated = WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
        direction: Some(crate::directional::Direction::Left),
    };
    let second =
        engine.tx(|tx| tx.publish_v4(envelope(8), Some(&unrelated), Vec::new(), &BTreeSet::new()));
    assert_eq!(second.transfers.len(), 1);
    assert_eq!(second.transfers[0].command_seq, Some(7));
}

#[test]
fn transfer_seq_never_parsed_from_operation_id() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    // Opaque id spelling "op-999" still projects the retained 7, never 999.
    engine.world.settlement.retained.push(retained_window(
        "op-999",
        Some(7),
        "win-t1",
        "out-a",
        "out-b",
        "ws-x",
    ));
    // Opaque id spelling "op-12" with no retained sequence stays null.
    engine.world.settlement.retained.push(retained_window(
        "op-12", None, "win-t2", "out-b", "out-b", "ws-y",
    ));
    let reply = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    assert_eq!(reply.transfers.len(), 2);
    let explicit = reply
        .transfers
        .iter()
        .find(|row| row.operation_id == "op-999")
        .expect("op-999 row");
    assert_eq!(explicit.command_seq, Some(7));
    let bare = reply
        .transfers
        .iter()
        .find(|row| row.operation_id == "op-12")
        .expect("op-12 row");
    assert_eq!(bare.command_seq, None);
}

#[test]
fn diagnostics_preserve_staged_order() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let staged = vec![
        RowDiagnostic {
            scope: AuthorityScope::Output(OutputId("out-a".to_owned())),
            row: RowDiagnosticLane::Window,
            id: Some("win-a".to_owned()),
            kind: RowDiagnosticKind::InvalidFact,
        },
        RowDiagnostic {
            scope: AuthorityScope::Output(OutputId("out-b".to_owned())),
            row: RowDiagnosticLane::Screen,
            id: Some("out-b".to_owned()),
            kind: RowDiagnosticKind::Unreadable,
        },
        RowDiagnostic {
            scope: AuthorityScope::Output(OutputId("out-a".to_owned())),
            row: RowDiagnosticLane::Attempt,
            id: Some("win-a".to_owned()),
            kind: RowDiagnosticKind::ConflictingFact,
        },
    ];
    engine.world.staged.diagnostics = staged.clone();
    let reply = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    assert_eq!(reply.diagnostics, staged);
}

#[test]
fn terminals_retain_across_publications() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    engine.world.gesture.terminal = Some(GestureSettled {
        id: 9,
        committed: true,
    });
    engine.world.settlement.operation_settled.insert(
        (WindowId("win-a".to_owned()), OperationKind::Float),
        OperationEvent {
            window: WindowId("win-a".to_owned()),
            kind: OperationKind::Float,
            operation_id: "op-1".to_owned(),
            outcome: OperationOutcome::FloatFrameProven,
            cause: None,
        },
    );
    engine.world.settlement.switch_settled.insert(
        OutputId("out-a".to_owned()),
        SwitchEvent {
            output: OutputId("out-a".to_owned()),
            operation_id: "op-s".to_owned(),
            outcome: SwitchOutcome::Complete,
            failed: Vec::new(),
            focus: SwitchFocus::Unproven,
            fallback: false,
        },
    );
    let first = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    assert_eq!(
        first.gesture_settled,
        Some(GestureSettled {
            id: 9,
            committed: true
        })
    );
    assert_eq!(first.operation_settled.len(), 1);
    assert_eq!(first.switch_settled.len(), 1);
    let second = engine.tx(|tx| tx.publish_v4(envelope(8), None, Vec::new(), &BTreeSet::new()));
    assert_eq!(second.gesture_settled, first.gesture_settled);
    assert_eq!(second.operation_settled, first.operation_settled);
    assert_eq!(second.switch_settled, first.switch_settled);
}

#[test]
fn presentation_stamp_and_published_rev() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    // Genuine v4 admissions publish internally: the second seed (seq
    // base+1) stamps the latest rev, unlike the retired legacy seeding
    // which never touched the v4 publication state.
    assert_eq!(engine.world.presentation.latest_published_rev_v4, 2);
    let before = engine.world.staged.presentation_v4.clone();
    assert!(matches!(before, Some(PresentationRowV4::Valid(_))));
    let report = report_v4(
        3,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let first = publish_and_finish_v4(&mut engine, 7, &report);
    assert_eq!(first.presentation.rev, 7);
    assert_eq!(first.state_revision, 7);
    assert_eq!(engine.world.presentation.latest_published_rev_v4, 7);
    // The staged producer row is never rewritten by publication.
    assert_eq!(engine.world.staged.presentation_v4, before);
    let second = publish_and_finish_v4(&mut engine, 9, &report);
    assert_eq!(second.presentation.rev, 9);
    assert_eq!(engine.world.presentation.latest_published_rev_v4, 9);
}

#[test]
fn publish_leaves_pager_retention_untouched() {
    let mut engine = engine_with(Vec::new());
    let mut ingress = ingress_v4("win-a");
    ingress.pager_order = vec![WindowId("win-b".to_owned())];
    ingress.pager_frames = vec![PresentationPagerFrameV4 {
        window: WindowId("win-b".to_owned()),
        live: Some(rect(100, 0, 40, 90)),
    }];
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress,
    );
    // Guard: the staged pager frame would change live retention if it ran:
    // genuine v4 seed admissions already retain (roster fallback covers
    // the order), but no admission has seen this staged frame yet.
    assert!(
        !engine
            .world
            .presentation
            .live_v4
            .contains_key(&WindowId("win-b".to_owned()))
    );
    let order_before = engine.world.presentation.order_v4.clone();
    let live_before = engine.world.presentation.live_v4.clone();
    let focused_before = engine.world.presentation.focused_v4.clone();
    let _ = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    assert_eq!(engine.world.presentation.order_v4, order_before);
    assert_eq!(engine.world.presentation.live_v4, live_before);
    assert_eq!(engine.world.presentation.focused_v4, focused_before);
}

#[test]
fn style_hold_suppresses_held_output_writes() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(32.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(0.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(0.0)),
    ]);
    // Past the style cap on out-a only: 32 * 33 = 1056 > 1024.
    let mut screens = screens_pair();
    screens[0].scale = 33.0;
    seeded(
        &mut engine,
        1,
        screens.clone(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let (held_output, _) = engine
        .tx(|tx| tx.presentation_style_hold_v4())
        .expect("past-cap style holds");
    assert_eq!(held_output, OutputId("out-a".to_owned()));
    engine.world.settlement.retained.push(retained_window(
        "op-t1",
        Some(3),
        "win-t1",
        "out-a",
        "out-b",
        "ws-x",
    ));
    engine.world.settlement.retained.push(retained_window(
        "op-t2", None, "win-t2", "out-b", "out-b", "ws-y",
    ));
    let reply = publish_and_finish_v4(
        &mut engine,
        7,
        &report_v4(3, screens, facts_pair(), "win-a", ingress_v4("win-a")),
    );
    let out_a = reply
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .expect("out-a row");
    assert_eq!(out_a.hold, Some(HoldReason::Unreadable));
    let out_b = reply
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-b")
        .expect("out-b row");
    assert_eq!(out_b.hold, None);
    for row in reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
    {
        assert_eq!(
            row.hold,
            Some(HoldReason::Unreadable),
            "held output row: {row:?}"
        );
    }
    for row in reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-b")
    {
        assert_eq!(row.hold, None, "clean output row: {row:?}");
    }
    // Held-output windows carry the hold on both lanes, with no write.
    for id in ["win-a", "win-b"] {
        let row = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == id)
            .expect("held window row");
        assert_eq!(row.output.0, "out-a");
        assert_eq!(
            row.frame_authority,
            ScopeAuthority::Hold(HoldReason::Unreadable)
        );
        assert_eq!(
            row.membership_authority,
            ScopeAuthority::Hold(HoldReason::Unreadable)
        );
    }
    // The unaffected output keeps full authority: no blanket hold.
    let win_c = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-c")
        .expect("win-c row");
    assert_eq!(win_c.output.0, "out-b");
    assert_eq!(win_c.membership_authority, ScopeAuthority::Controlled);
    // Transfers: style-held source or target projects held; the
    // out-b-only transfer stays live.
    let cross = reply
        .transfers
        .iter()
        .find(|row| row.operation_id == "op-t1")
        .expect("cross row");
    assert!(cross.held);
    assert_eq!(cross.command_seq, Some(3));
    let local = reply
        .transfers
        .iter()
        .find(|row| row.operation_id == "op-t2")
        .expect("local row");
    assert!(!local.held);
    // The hold never leaks into retained state.
    assert!(
        !engine
            .world
            .hold_reasons
            .keys()
            .any(|key| key.output.0 == "out-a"),
        "style hold stays per-reply"
    );
    // Presentation still publishes its foreground truthfully.
    assert_eq!(reply.presentation.foreground, "win-a");
    assert_eq!(reply.presentation.suspended_output, None);
    assert_eq!(reply.presentation.rev, 7);
}

#[test]
fn style_hold_filters_held_output_native_desires() {
    let mut engine = engine_host();
    engine
        .world
        .workspace
        .mapping
        .insert("out-a".to_owned(), vec!["ws-9".to_owned()]);
    engine
        .world
        .workspace
        .mapping
        .insert("out-b".to_owned(), vec!["ws-8".to_owned()]);
    let mut screens = screens_pair();
    screens[0].scale = 33.0;
    let mut staged_report = report_v4(
        1,
        screens,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        ingress_v4("win-a"),
    );
    staged_report.native = vec![native_row("out-a"), native_row("out-b")];
    engine.tx(|tx| tx.stage_facts_v4(&staged_report));
    engine.tx(|tx| tx.build_staged_domains());
    // Backing transfers keep both creations past the prune lane.
    engine.world.settlement.retained.push(retained_window(
        "op-back-a",
        None,
        "win-x1",
        "out-a",
        "out-a",
        "ws-9",
    ));
    engine.world.settlement.retained.push(retained_window(
        "op-back-b",
        None,
        "win-x2",
        "out-b",
        "out-b",
        "ws-8",
    ));
    for (op, output, logical) in [
        ("op-native-a", "out-a", "ws-9"),
        ("op-native-b", "out-b", "ws-8"),
    ] {
        engine
            .world
            .settlement
            .retained_creations
            .push(RetainedNativeCreation {
                operation_id: op.to_owned(),
                logical_workspace: logical.to_owned(),
                target_output: OutputId(output.to_owned()),
                order_before: vec![format!("native-{output}-0")],
                minted_ms: 1,
                pending: None,
                ambiguous: false,
            });
    }
    // Guard: both desires are live before publication.
    let live: Vec<String> = engine
        .tx(|tx| tx.native_creation_desires())
        .iter()
        .map(|desire| desire.operation_id.clone())
        .collect();
    assert!(
        live.contains(&"op-native-a".to_owned()),
        "seeded live: {live:?}"
    );
    assert!(
        live.contains(&"op-native-b".to_owned()),
        "seeded live: {live:?}"
    );
    let reply = engine.tx(|tx| {
        tx.publish_v4(
            PublishEnvelopeV4 {
                adapter_incarnation: "adapter-1".to_owned(),
                request_id: "req-5".to_owned(),
                observation_seq: 1,
                command_consumed: false,
                issuing_revision: 5,
            },
            None,
            Vec::new(),
            &BTreeSet::new(),
        )
    });
    let ops: Vec<&str> = reply
        .native_creations
        .iter()
        .map(|desire| desire.operation_id.as_str())
        .collect();
    assert!(
        !ops.contains(&"op-native-a"),
        "held output filtered: {ops:?}"
    );
    assert!(ops.contains(&"op-native-b"), "clean output kept: {ops:?}");
    // The filter is per-reply: retained authorizations survive.
    assert_eq!(engine.world.settlement.retained_creations.len(), 2);
}

fn birth_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            birth_clear: true,
            maximize: true,
            fullscreen: true,
            native_sticky: true,
            above: true,
            ..HostCapabilities::default()
        },
        vec![
            ("borderWidth".to_owned(), RawSettingValue::Number(32.0)),
            ("borderGap".to_owned(), RawSettingValue::Number(0.0)),
            ("borderRadius".to_owned(), RawSettingValue::Number(0.0)),
        ],
    )
    .expect("generation")
}

fn style_engine() -> WorldEngine {
    engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(32.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(0.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(0.0)),
    ])
}

fn style_screens(scale_a: f64) -> Vec<ScreenReport> {
    let mut screens = screens_pair();
    screens[0].scale = scale_a;
    screens
}

#[test]
fn born_tokens_withhold_on_style_held_output() {
    let mut engine = birth_engine();
    seeded(
        &mut engine,
        1,
        style_screens(33.0),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let home_a = engine
        .tx_ro(|tx| tx.staged_homing(&WindowId("win-a".to_owned())))
        .expect("win-a home");
    let home_c = engine
        .tx_ro(|tx| tx.staged_homing(&WindowId("win-c".to_owned())))
        .expect("win-c home");
    assert_eq!(home_a.output, OutputId("out-a".to_owned()));
    assert_eq!(home_c.output, OutputId("out-b".to_owned()));
    engine
        .world
        .settlement
        .retained_born
        .push(crate::world_engine::RetainedBornClear {
            operation_id: "op-born-a".to_owned(),
            window: WindowId("win-a".to_owned()),
            home: home_a,
            pending: None,
            ambiguous: false,
        });
    engine
        .world
        .settlement
        .retained_born
        .push(crate::world_engine::RetainedBornClear {
            operation_id: "op-born-c".to_owned(),
            window: WindowId("win-c".to_owned()),
            home: home_c,
            pending: None,
            ambiguous: false,
        });
    let reply = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    let row_a = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("win-a row");
    let row_c = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-c")
        .expect("win-c row");
    assert_eq!(row_a.clear_operation, None, "held home withholds born");
    assert_eq!(
        row_c.clear_operation,
        Some("op-born-c".to_owned()),
        "clean home keeps born"
    );
}

#[test]
fn minimized_fact_on_style_held_output_holds_membership() {
    let mut engine = style_engine();
    let mut facts = facts_pair();
    let mut win_m = fact("win-m", "out-a", 200);
    win_m.minimized = Some(true);
    facts.push(win_m);
    seeded(
        &mut engine,
        1,
        style_screens(33.0),
        facts,
        "win-a",
        ingress_v4("win-a"),
    );
    let reply = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-m")
        .expect("win-m row");
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(HoldReason::Minimized)
    );
    assert_eq!(
        row.membership_authority,
        ScopeAuthority::Hold(HoldReason::Unreadable),
        "style-held minimized fact: {row:?}"
    );
}

#[test]
fn source_held_transfer_redirect_holds() {
    let mut engine = style_engine();
    seeded(
        &mut engine,
        1,
        style_screens(33.0),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    engine.world.settlement.retained.push(retained_window(
        "op-t1",
        Some(3),
        "win-t",
        "out-a",
        "out-b",
        "ws-x",
    ));
    assert_eq!(
        engine.tx_ro(|tx| tx.staged_homing(&WindowId("win-t".to_owned()))),
        None
    );
    let reply = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-t")
        .expect("win-t redirect row");
    assert_eq!(row.output, OutputId("out-b".to_owned()));
    assert_eq!(
        row.membership_authority,
        ScopeAuthority::Hold(HoldReason::Unreadable),
        "source-held flight: {row:?}"
    );
}

#[test]
fn missing_domain_target_redirect_holds() {
    let mut engine = style_engine();
    seeded(
        &mut engine,
        1,
        style_screens(33.0),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    engine.world.settlement.retained.push(retained_window(
        "op-t2",
        Some(4),
        "win-u",
        "out-b",
        "out-a",
        "ws-ghost",
    ));
    assert_eq!(
        engine.tx_ro(|tx| tx.staged_homing(&WindowId("win-u".to_owned()))),
        None
    );
    let reply = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-u")
        .expect("win-u redirect row");
    assert_eq!(row.output, OutputId("out-a".to_owned()));
    assert_eq!(
        row.membership_authority,
        ScopeAuthority::Hold(HoldReason::Unreadable),
        "held target absent staged domain: {row:?}"
    );
}

#[test]
fn focus_effects_filter_held_output() {
    let mut engine = style_engine();
    seeded(
        &mut engine,
        1,
        style_screens(33.0),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let home_a = engine
        .tx_ro(|tx| tx.staged_homing(&WindowId("win-a".to_owned())))
        .expect("win-a home");
    let home_c = engine
        .tx_ro(|tx| tx.staged_homing(&WindowId("win-c".to_owned())))
        .expect("win-c home");
    let before_a = engine.tx_ro(|tx| tx.canonical_memory(&home_a));
    // Focus is the only effect kind and it never publishes as a wire
    // effect; the carriers below exercise the held-output publication
    // path and must all strip.
    let effects = vec![
        crate::world_report::OneShotEffect::explicit_focus(7, WindowId("win-a".to_owned()), 7),
        crate::world_report::OneShotEffect::explicit_focus(8, WindowId("win-c".to_owned()), 7),
    ];
    // Pending-authority recording: publication records the retained
    // winner, never the synthetic carrier effects. Seat the clean win-c
    // request so the canonical slot follows it.
    engine.tx(|tx| {
        tx.note_focus_decision(
            &WindowId("win-c".to_owned()),
            crate::world_report::FocusRequestCause::AutomaticAdmission,
            None,
            7,
            "win-a",
        )
    });
    let reply = engine.tx(|tx| tx.publish_v4(envelope(7), None, effects, &BTreeSet::new()));
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {:?}",
        reply.effects
    );
    // No native effect kind remains: every carrier strips, so the held
    // output keeps nothing and the clean output carries nothing either.
    assert_eq!(
        engine.tx_ro(|tx| tx.canonical_memory(&home_c)),
        Some(WindowId("win-c".to_owned()))
    );
    assert_eq!(engine.tx_ro(|tx| tx.canonical_memory(&home_a)), before_a);
}

#[test]
fn style_boundary_1024_holds_only_past_cap() {
    let mut clean = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(32.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(0.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(0.0)),
    ]);
    seeded(
        &mut clean,
        1,
        style_screens(32.0),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    assert!(clean.tx(|tx| tx.presentation_style_hold_v4()).is_none());
    let reply = clean.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    for row in &reply.outputs {
        assert_eq!(row.hold, None, "at-cap output holds nothing");
    }
    let mut held = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(32.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(0.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(0.0)),
    ]);
    seeded(
        &mut held,
        1,
        style_screens(32.015625),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    assert_eq!(
        held.tx(|tx| tx.presentation_style_hold_v4())
            .map(|(output, _)| output),
        Some(OutputId("out-a".to_owned()))
    );
    let reply = held.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    let out_a = reply
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .expect("out-a row");
    assert_eq!(out_a.hold, Some(HoldReason::Unreadable));
}

#[test]
fn held_presentation_publishes_without_style_hold() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    engine.world.staged.presentation_v4 = Some(PresentationRowV4::Held(
        crate::world_report::HeldPresentationRow {
            kind: RowDiagnosticKind::InvalidFact,
            diagnostic: None,
        },
    ));
    assert!(engine.tx(|tx| tx.presentation_style_hold_v4()).is_none());
    let reply = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    for row in &reply.outputs {
        assert_eq!(row.hold, None, "held presentation holds no output");
    }
    let win_a = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("win-a row");
    assert_eq!(win_a.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(reply.presentation.foreground, "");
    assert!(!reply.presentation.border.visible);
}

fn publish_test_issue() -> crate::world_engine::EventIssue {
    crate::world_engine::EventIssue {
        owner: crate::ids::OwnerId::parse("adapter-1").expect("owner"),
        correlation: crate::ids::CorrelationId::parse("req-3").expect("correlation"),
    }
}

#[test]
fn genuine_send_projects_explicit_seq() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let ring_b = engine
        .world
        .workspace
        .mapping
        .get("out-a")
        .cloned()
        .expect("out-a ring");
    let homed_ws: std::collections::BTreeSet<String> = engine
        .world
        .staged
        .homing
        .values()
        .filter(|key| key.output.0 == "out-a")
        .map(|key| key.workspace.0.clone())
        .collect();
    let target_ws = ring_b
        .iter()
        .find(|id| !homed_ws.contains(*id))
        .cloned()
        .unwrap_or_else(|| ring_b[0].clone());
    let cmd = WorldCommand::Send {
        window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: crate::directional::WorkspaceId(target_ws.clone()),
        follow: false,
    };
    let applied = engine.tx(|tx| {
        tx.execute_command(
            &publish_test_issue(),
            11,
            &cmd,
            &BTreeSet::new(),
            None,
            None,
        )
    });
    match applied {
        crate::world_commands::CommandApplied::Effect(_)
        | crate::world_commands::CommandApplied::NoEffect => {}
        crate::world_commands::CommandApplied::Rejected { kind, message } => {
            panic!("genuine send rejected {kind} {message}")
        }
    }
    let retained = engine
        .world
        .settlement
        .retained
        .iter()
        .find(|retained| matches!(&retained.target.kind, TransferKind::Window(w) if w.0 == "win-a"))
        .expect("retained send");
    assert_eq!(retained.command_seq, Some(11));
    let reply = engine.tx(|tx| tx.publish_v4(envelope(12), None, Vec::new(), &BTreeSet::new()));
    let row = reply
        .transfers
        .iter()
        .find(|row| row.target_output == OutputId("out-a".to_owned()))
        .expect("projected send");
    assert_eq!(row.command_seq, Some(11));
}

#[test]
fn genuine_select_workspace_projects_view_seq() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let ring = engine
        .world
        .workspace
        .mapping
        .get("out-b")
        .cloned()
        .expect("out-b ring");
    let other = ring
        .iter()
        .find(|id| Some(*id) != engine.world.workspace.views.get("out-b"))
        .cloned()
        .unwrap_or_else(|| ring[0].clone());
    let cmd = WorldCommand::SelectWorkspace {
        output: OutputId("out-b".to_owned()),
        workspace: crate::directional::WorkspaceId(other.clone()),
    };
    let _ = engine.tx(|tx| {
        tx.execute_command(
            &publish_test_issue(),
            13,
            &cmd,
            &BTreeSet::new(),
            None,
            None,
        )
    });
    let retained = engine
        .world
        .settlement
        .retained
        .iter()
        .find(|retained| {
            matches!(&retained.target.kind, TransferKind::View)
                && retained.target.target_output == OutputId("out-b".to_owned())
        })
        .expect("retained view");
    assert_eq!(retained.command_seq, Some(13));
    let reply = engine.tx(|tx| tx.publish_v4(envelope(14), None, Vec::new(), &BTreeSet::new()));
    assert!(
        reply
            .transfers
            .iter()
            .any(|row| row.command_seq == Some(13)),
        "view seq projects: {:?}",
        reply.transfers
    );
}

#[test]
fn genuine_migrate_workspace_projects_parent_seq() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let cmd = WorldCommand::MigrateWorkspace {
        source_output: OutputId("out-a".to_owned()),
        target_output: OutputId("out-b".to_owned()),
        workspace: crate::directional::WorkspaceId("ws-1".to_owned()),
    };
    match engine.tx(|tx| {
        tx.execute_command(
            &publish_test_issue(),
            15,
            &cmd,
            &BTreeSet::new(),
            None,
            None,
        )
    }) {
        crate::world_commands::CommandApplied::Effect(_)
        | crate::world_commands::CommandApplied::NoEffect => {}
        crate::world_commands::CommandApplied::Rejected { kind, message } => {
            panic!("genuine migrate rejected {kind} {message}")
        }
    }
    let retained = engine
        .world
        .settlement
        .retained
        .iter()
        .find(|retained| {
            matches!(&retained.target.kind, TransferKind::Migration)
                && retained.target.target_workspace == "ws-1"
        })
        .expect("retained migration parent");
    assert_eq!(retained.command_seq, Some(15));
    let reply = engine.tx(|tx| tx.publish_v4(envelope(16), None, Vec::new(), &BTreeSet::new()));
    assert!(
        reply
            .transfers
            .iter()
            .any(|row| row.command_seq == Some(15)),
        "migration seq projects: {:?}",
        reply.transfers
    );
}

#[test]
fn genuine_null_gesture_send_projects_no_seq() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let owned: Vec<crate::seed::EngineWindow> = engine
        .world
        .staged
        .domains
        .iter()
        .flat_map(|domain| domain.windows.iter().cloned())
        .collect();
    let by_id: BTreeMap<&WindowId, &crate::seed::EngineWindow> = owned
        .iter()
        .map(|window| (&window.window, window))
        .collect();
    let target_ws = engine
        .world
        .workspace
        .mapping
        .get("out-b")
        .and_then(|ring| ring.first().cloned())
        .expect("out-b ring");
    let (transfer, _) = engine
        .tx(|tx| {
            tx.execute_output_send(
                &publish_test_issue(),
                "gesture-win-a-99",
                &WindowId("win-a".to_owned()),
                &OutputId("out-b".to_owned()),
                &target_ws,
                true,
                &by_id,
                &BTreeSet::new(),
            )
        })
        .expect("gesture send");
    assert_eq!(transfer.command_seq, None);
    engine.world.settlement.retained.push(transfer);
    let reply = engine.tx(|tx| tx.publish_v4(envelope(15), None, Vec::new(), &BTreeSet::new()));
    let row = reply
        .transfers
        .iter()
        .find(|row| row.operation_id == "gesture-win-a-99")
        .expect("gesture row");
    assert_eq!(row.command_seq, None);
}

#[test]
fn publish_placeholders_rev_zero_and_latest_stamp() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    assert_eq!(engine.tx_ro(|tx| tx.presentation_for_publish_v4()).rev, 0);
    // Genuine v4 seed admissions stamp the latest rev to base+1 (see above).
    assert_eq!(engine.world.presentation.latest_published_rev_v4, 2);
    let _ = publish_and_finish_v4(
        &mut engine,
        7,
        &report_v4(
            3,
            screens_pair(),
            facts_pair(),
            "win-a",
            ingress_v4("win-a"),
        ),
    );
    assert_eq!(engine.world.presentation.latest_published_rev_v4, 7);
}

#[test]
fn floating_boundary_mover_style_hold() {
    let mut engine = style_engine();
    seeded(
        &mut engine,
        1,
        style_screens(33.0),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let home = engine
        .tx_ro(|tx| tx.staged_homing(&WindowId("win-a".to_owned())))
        .expect("win-a home");
    // Genuine publication helper only: legacy seeding above, no v4
    // accepted-run claim. Mark the staged home floating after seeding.
    engine.world.floating_scopes.insert(home.clone());
    engine.world.settlement.retained.push(retained_window(
        "op-float-hold",
        Some(3),
        "win-a",
        "out-b",
        &home.output.0,
        &home.workspace.0,
    ));
    let reply = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("win-a floating row");
    assert_eq!(
        row.membership_authority,
        ScopeAuthority::Hold(HoldReason::Unreadable),
        "style-held floating mover: {row:?}"
    );
}

#[test]
fn floating_boundary_mover_clean_stays_controlled() {
    let mut engine = style_engine();
    seeded(
        &mut engine,
        1,
        style_screens(1.0),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    assert!(engine.tx(|tx| tx.presentation_style_hold_v4()).is_none());
    let home = engine
        .tx_ro(|tx| tx.staged_homing(&WindowId("win-a".to_owned())))
        .expect("win-a home");
    engine.world.floating_scopes.insert(home.clone());
    engine.world.settlement.retained.push(retained_window(
        "op-float-clean",
        Some(3),
        "win-a",
        "out-b",
        &home.output.0,
        &home.workspace.0,
    ));
    let reply = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("win-a floating row");
    assert_eq!(
        row.membership_authority,
        ScopeAuthority::Controlled,
        "clean floating mover: {row:?}"
    );
}

#[test]
fn minimized_source_held_to_clean_target_holds() {
    let mut engine = style_engine();
    let mut facts = facts_pair();
    let mut win_m = fact("win-m", "out-a", 200);
    win_m.minimized = Some(true);
    facts.push(win_m);
    seeded(
        &mut engine,
        1,
        style_screens(33.0),
        facts,
        "win-a",
        ingress_v4("win-a"),
    );
    // Known staged window (homing present), not a phantom lifetime.
    let source = engine
        .tx_ro(|tx| tx.staged_homing(&WindowId("win-m".to_owned())))
        .expect("win-m home");
    assert_eq!(source.output, OutputId("out-a".to_owned()));
    let clean_ws = engine
        .tx_ro(|tx| tx.staged_homing(&WindowId("win-c".to_owned())))
        .expect("win-c home")
        .workspace
        .0
        .clone();
    engine.world.settlement.retained.push(retained_window(
        "op-min-move",
        Some(5),
        "win-m",
        "out-a",
        "out-b",
        &clean_ws,
    ));
    let reply = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-m")
        .expect("win-m row");
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(HoldReason::Minimized)
    );
    assert_eq!(
        row.membership_authority,
        ScopeAuthority::Hold(HoldReason::Unreadable),
        "source-held minimized flight to clean target: {row:?}"
    );
}

#[test]
fn minimized_source_held_to_missing_slot_holds() {
    let mut engine = style_engine();
    let mut facts = facts_pair();
    let mut win_m = fact("win-m", "out-a", 200);
    win_m.minimized = Some(true);
    facts.push(win_m);
    seeded(
        &mut engine,
        1,
        style_screens(33.0),
        facts,
        "win-a",
        ingress_v4("win-a"),
    );
    assert!(
        engine
            .tx_ro(|tx| tx.staged_homing(&WindowId("win-m".to_owned())))
            .is_some()
    );
    engine.world.settlement.retained.push(retained_window(
        "op-min-ghost",
        Some(6),
        "win-m",
        "out-a",
        "out-b",
        "ws-ghost",
    ));
    let reply = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-m")
        .expect("win-m row");
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(HoldReason::Minimized)
    );
    assert_eq!(
        row.membership_authority,
        ScopeAuthority::Hold(HoldReason::Unreadable),
        "source-held minimized flight to missing slot: {row:?}"
    );
}

#[test]
fn publish_on_clone_leaves_original_rev() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    // Genuine v4 seed admissions stamp the latest rev to base+1 (see above).
    assert_eq!(engine.world.presentation.latest_published_rev_v4, 2);
    let mut clone = engine.clone();
    let reply = publish_and_finish_v4(
        &mut clone,
        7,
        &report_v4(
            3,
            screens_pair(),
            facts_pair(),
            "win-a",
            ingress_v4("win-a"),
        ),
    );
    assert_eq!(reply.state_revision, 7);
    assert_eq!(clone.world.presentation.latest_published_rev_v4, 7);
    assert_eq!(
        engine.world.presentation.latest_published_rev_v4, 2,
        "original rev untouched by clone publish"
    );
}

#[test]
fn retained_operation_phase_slots_and_q1_q2_pruning_with_trace() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    // Clear then float under the SAME op-1 keeps both phase slots.
    engine.tx(|tx| {
        tx.emit_operation(
            "op-1".to_owned(),
            WindowId("win-a".to_owned()),
            OperationOutcome::ClearContinuedFloat,
            Some("clear-proof"),
        )
    });
    engine.tx(|tx| {
        tx.emit_operation(
            "op-1".to_owned(),
            WindowId("win-a".to_owned()),
            OperationOutcome::FloatFrameProven,
            None,
        )
    });
    let r1 = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    assert_eq!(r1.operation_settled.len(), 2);
    assert_eq!(r1.operation_settled[0].kind, OperationKind::Clear);
    assert_eq!(r1.operation_settled[1].kind, OperationKind::Float);
    assert_eq!(r1.operation_settled[0].operation_id, "op-1");
    assert_eq!(r1.operation_settled[1].operation_id, "op-1");

    // Q1 A: pending clear/float on win-a does NOT clear existing outcomes.
    engine
        .world
        .settlement
        .retained_floats
        .push(crate::world_engine::RetainedFloat {
            operation_id: "op-2".to_owned(),
            command_seq: 2,
            window: WindowId("win-a".to_owned()),
            target: rect(10, 10, 100, 100),
            source: Some(rect(0, 0, 100, 100)),
            output: OutputId("out-a".to_owned()),
            workspace: "ws-1".to_owned(),
            ambiguous: false,
            pending: None,
        });
    let r2 = engine.tx(|tx| tx.publish_v4(envelope(8), None, Vec::new(), &BTreeSet::new()));
    assert_eq!(r2.operation_settled.len(), 2);

    // Q2 A: explicit close drops owner BEFORE publish_v4 projects and buffers
    // dropped outcomes without premature adopted emission; emission happens at
    // final-publication boundary.
    std::sync::Arc::make_mut(&mut engine.world.staged.observation)
        .closed
        .push(WindowId("win-a".to_owned()));
    let mut r3_opt = None;
    let premature = crate::world_engine::with_focus_trace_capture(|| {
        let r3 = engine.tx(|tx| tx.publish_v4(envelope(9), None, Vec::new(), &BTreeSet::new()));
        assert!(r3.operation_settled.is_empty());
        r3_opt = Some(r3);
    });
    assert!(
        premature.is_empty(),
        "direct publish_v4 must buffer, not emit premature drops: {premature:?}"
    );
    assert_eq!(engine.world.settlement.staged_dropped_operations.len(), 2);
    let r3 = r3_opt.expect("r3");
    let lines = crate::world_engine::with_focus_trace_capture(|| {
        engine
            .world
            .settlement
            .emit_staged_dropped_settlement_trace(&r3);
    });
    assert!(engine.world.settlement.staged_dropped_operations.is_empty());
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("ev=operation-settled-dropped"));
    assert!(lines[0].contains("req=req-9"));
    assert!(lines[0].contains("rev=9"));
    assert!(lines[0].contains("obs=9"));
    assert!(lines[0].contains("win=win-a"));
    assert!(lines[0].contains("outcome=continued-float"));
    assert!(lines[0].contains("cause=clear-proof"));
    assert!(lines[1].contains("outcome=float-frame-proven"));
    assert!(lines[1].contains("cause=-"));
}

#[test]
fn retained_switch_outcome_retains_and_prunes_on_output_departure() {
    let mut engine = engine_with(Vec::new());
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    engine.world.settlement.switch_settled.insert(
        OutputId("out-b".to_owned()),
        SwitchEvent {
            output: OutputId("out-b".to_owned()),
            operation_id: "op-s".to_owned(),
            outcome: SwitchOutcome::CompletedWithFailures,
            failed: vec![WindowId("win-c".to_owned())],
            focus: SwitchFocus::Unproven,
            fallback: false,
        },
    );
    let r1 = engine.tx(|tx| tx.publish_v4(envelope(7), None, Vec::new(), &BTreeSet::new()));
    assert_eq!(r1.switch_settled.len(), 1);
    assert_eq!(r1.switch_settled[0].output, OutputId("out-b".to_owned()));

    // Remove out-b from staged screens under complete inventory -> pruned + buffered,
    // then emitted at final publication.
    std::sync::Arc::make_mut(&mut engine.world.staged.observation).inventory_complete = true;
    engine
        .world
        .staged
        .screens
        .retain(|screen| screen.output != OutputId("out-b".to_owned()));
    let mut r2_opt = None;
    let premature = crate::world_engine::with_focus_trace_capture(|| {
        let r2 = engine.tx(|tx| tx.publish_v4(envelope(8), None, Vec::new(), &BTreeSet::new()));
        assert!(r2.switch_settled.is_empty());
        r2_opt = Some(r2);
    });
    assert!(
        premature.is_empty(),
        "direct publish_v4 must buffer, not emit premature drops: {premature:?}"
    );
    assert_eq!(engine.world.settlement.staged_dropped_switches.len(), 1);
    let r2 = r2_opt.expect("r2");
    let lines = crate::world_engine::with_focus_trace_capture(|| {
        engine
            .world
            .settlement
            .emit_staged_dropped_settlement_trace(&r2);
    });
    assert!(engine.world.settlement.staged_dropped_switches.is_empty());
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("ev=switch-settled-dropped"));
    assert!(lines[0].contains("req=req-8"));
    assert!(lines[0].contains("rev=8"));
    assert!(lines[0].contains("obs=8"));
    assert!(lines[0].contains("out=out-b"));
    assert!(lines[0].contains("outcome=completed-with-failures"));
    assert!(lines[0].contains("fallback=false"));
    assert!(lines[0].contains("failed=win-c"));
}

#[test]
fn gesture_terminal_guards_stale_replays_and_preserves_first_unbound() {
    let mut engine = engine_with(Vec::new());
    // Genuine first unbound terminal records.
    engine.tx(|tx| tx.record_gesture_terminal(5, true));
    assert_eq!(
        engine.world.gesture.terminal,
        Some(GestureSettled {
            id: 5,
            committed: true,
        })
    );
    // Same-id or older-id replay (e.g. Esc) cannot overwrite committed=true or regress id.
    engine.tx(|tx| tx.record_gesture_terminal(5, false));
    engine.tx(|tx| tx.record_gesture_terminal(4, false));
    engine.tx(|tx| tx.record_gesture_terminal(0, false));
    assert_eq!(
        engine.world.gesture.terminal,
        Some(GestureSettled {
            id: 5,
            committed: true,
        })
    );
    // Newer terminal replaces.
    engine.tx(|tx| tx.record_gesture_terminal(6, false));
    assert_eq!(
        engine.world.gesture.terminal,
        Some(GestureSettled {
            id: 6,
            committed: false,
        })
    );
}

#[test]
fn operation_settle_cap_shares_two_phase_slots_at_1024() {
    let mut engine = engine_with(Vec::new());
    // 512 windows * 2 phases = 1024 slots.
    for i in 0..512 {
        let win = WindowId(format!("w-{i}"));
        engine.tx(|tx| {
            tx.emit_operation(
                format!("op-c-{i}"),
                win.clone(),
                OperationOutcome::ClearContinuedTiled,
                None,
            )
        });
        engine.tx(|tx| {
            tx.emit_operation(
                format!("op-f-{i}"),
                win,
                OperationOutcome::FloatFrameProven,
                None,
            )
        });
    }
    assert_eq!(
        engine.world.operation_settle_slot_count(None, &[]),
        crate::world_report::MAX_REPLY_OPERATIONS
    );
    // Replacing an existing slot does not exceed 1024.
    let existing_cmd = WorldCommand::Operation(crate::boundary::CoreCommand::ToggleFloat {
        window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
            "w-0".to_owned(),
        )),
        float_rect: None,
    });
    assert_eq!(
        engine
            .world
            .operation_settle_slot_count(Some(&existing_cmd), &[]),
        crate::world_report::MAX_REPLY_OPERATIONS
    );
    // Adding a new window exceeds 1024.
    let new_cmd = WorldCommand::Operation(crate::boundary::CoreCommand::ToggleFloat {
        window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
            "w-new".to_owned(),
        )),
        float_rect: None,
    });
    assert_eq!(
        engine
            .world
            .operation_settle_slot_count(Some(&new_cmd), &[]),
        crate::world_report::MAX_REPLY_OPERATIONS + 1
    );
}

#[test]
fn consumed_inline_command_at_1024_operation_cap_reconciles_instead_of_rejecting() {
    use crate::world_run_v4::V4RunOutcome;
    let mut engine = engine_with(Vec::new());
    let scr = vec![screen("DP-1", 0)];
    // Advance command_highwater to 1 via an accepted report.
    let init = report_full_v4(
        1,
        1,
        scr.clone(),
        vec![fact("w-0", "DP-1", 0)],
        Vec::new(),
        "w-0",
        Some(1),
        Some(WorldCommand::SelectNumbered {
            output: None,
            index: 1,
        }),
    );
    let _ = accepted_v4(&mut engine, &init);

    // Fill 1024 operation slots (512 windows * 2 phases).
    for i in 0..512 {
        let win = WindowId(format!("w-{i}"));
        engine.tx(|tx| {
            tx.emit_operation(
                format!("op-c-{i}"),
                win.clone(),
                OperationOutcome::ClearContinuedTiled,
                None,
            )
        });
        engine.tx(|tx| {
            tx.emit_operation(
                format!("op-f-{i}"),
                win,
                OperationOutcome::FloatFrameProven,
                None,
            )
        });
    }
    assert_eq!(
        engine.world.operation_settle_slot_count(None, &[]),
        crate::world_report::MAX_REPLY_OPERATIONS
    );

    // A fresh report carrying an already-consumed inline command (cmdSeq=1) for
    // a new window must reconcile as CommandConsumed rather than rejecting over cap.
    let consumed_rep = report_full_v4(
        2,
        2,
        scr,
        vec![fact("w-0", "DP-1", 0)],
        Vec::new(),
        "w-0",
        Some(1),
        Some(WorldCommand::Operation(
            crate::boundary::CoreCommand::ToggleFloat {
                window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
                    "w-new".to_owned(),
                )),
                float_rect: None,
            },
        )),
    );
    let (outcome, _) = engine.handle_report(&consumed_rep, &[]);
    assert!(
        matches!(outcome, V4RunOutcome::CommandConsumed(_)),
        "consumed inline command at 1024 cap must reconcile, got {outcome:?}"
    );
}

#[test]
fn retained_born_clear_reserves_operation_slot_at_1024_cap() {
    use crate::world_run_v4::V4RunOutcome;
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            birth_clear: true,
            maximize: true,
            fullscreen: true,
            native_sticky: true,
            above: true,
            window_suppression: true,
            ..HostCapabilities::default()
        },
        Vec::new(),
    )
    .expect("valid generation");
    let scr = vec![screen("DP-1", 0)];
    // Seed startup so subsequent maximized newcomer mints a genuine RetainedBornClear.
    let startup = report_full_v4(1, 1, scr.clone(), Vec::new(), Vec::new(), "", None, None);
    let _ = accepted_v4(&mut engine, &startup);

    let mut born_fact = fact("born-win", "DP-1", 0);
    born_fact.maximized = Some(true);
    let mint = report_full_v4(
        2,
        2,
        scr.clone(),
        vec![born_fact],
        Vec::new(),
        "born-win",
        None,
        None,
    );
    let _ = accepted_v4(&mut engine, &mint);
    assert_eq!(
        engine.world.settlement.retained_born.len(),
        1,
        "genuine born-clear retained"
    );

    // Fill the remaining 1023 operation slots: 511 * 2 + 1 = 1023.
    for i in 0..511 {
        let win = WindowId(format!("w-{i}"));
        engine.tx(|tx| {
            tx.emit_operation(
                format!("op-c-{i}"),
                win.clone(),
                OperationOutcome::ClearContinuedTiled,
                None,
            )
        });
        engine.tx(|tx| {
            tx.emit_operation(
                format!("op-f-{i}"),
                win,
                OperationOutcome::FloatFrameProven,
                None,
            )
        });
    }
    engine.tx(|tx| {
        tx.emit_operation(
            "op-c-extra".to_owned(),
            WindowId("w-extra".to_owned()),
            OperationOutcome::ClearContinuedTiled,
            None,
        )
    });
    assert_eq!(
        engine.world.operation_settle_slot_count(None, &[]),
        crate::world_report::MAX_REPLY_OPERATIONS
    );

    // A fresh ToggleFloat for a new window must be refused with section-over-cap
    // because retained_born occupies the 1024th slot.
    let over_cap = report_full_v4(
        3,
        3,
        scr,
        Vec::new(),
        Vec::new(),
        "",
        Some(1),
        Some(WorldCommand::Operation(
            crate::boundary::CoreCommand::ToggleFloat {
                window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
                    "w-new".to_owned(),
                )),
                float_rect: None,
            },
        )),
    );
    let (outcome, _) = engine.handle_report(&over_cap, &[]);
    assert!(
        matches!(
            outcome,
            V4RunOutcome::Rejected {
                message: "section-over-cap",
                ..
            }
        ),
        "expected section-over-cap with 1023 settled + 1 retained_born, got {outcome:?}"
    );
}

#[test]
fn staged_automatic_born_clear_at_1024_cap_rejects_transactionally_without_wedging() {
    use crate::world_run_v4::V4RunOutcome;
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            birth_clear: true,
            maximize: true,
            fullscreen: true,
            native_sticky: true,
            above: true,
            window_suppression: true,
            ..HostCapabilities::default()
        },
        Vec::new(),
    )
    .expect("valid generation");
    let scr = vec![screen("DP-1", 0)];
    let startup = report_full_v4(1, 1, scr.clone(), Vec::new(), Vec::new(), "", None, None);
    let _ = accepted_v4(&mut engine, &startup);

    // Fill all 1024 operation slots (512 windows * 2 phases).
    for i in 0..512 {
        let win = WindowId(format!("w-{i}"));
        engine.tx(|tx| {
            tx.emit_operation(
                format!("op-c-{i}"),
                win.clone(),
                OperationOutcome::ClearContinuedTiled,
                None,
            )
        });
        engine.tx(|tx| {
            tx.emit_operation(
                format!("op-f-{i}"),
                win,
                OperationOutcome::FloatFrameProven,
                None,
            )
        });
    }
    let rev_before = engine.state_revision();

    // A commandless report introducing a maximized newcomer mints a staged
    // RetainedBornClear (1025th slot) while preserving all 512 settled owners
    // via held_windows so Q2 does not prune them; stage finalization must
    // reject transactionally before adoption so the engine is not wedged.
    let mut born_fact = fact("born-over-cap", "DP-1", 0);
    born_fact.maximized = Some(true);
    let mut over_cap = report_full_v4(
        2,
        2,
        scr.clone(),
        vec![born_fact],
        Vec::new(),
        "born-over-cap",
        None,
        None,
    );
    over_cap.held_windows = (0..512)
        .map(|i| crate::world_report::HeldWindowRow {
            window: WindowId(format!("w-{i}")),
            ordinal: i,
            output: None,
            has_native_workspace: false,
            diagnostic: None,
            kind: RowDiagnosticKind::InvalidFact,
        })
        .collect();

    // Precondition / nonvacuity: with 1 free slot (1023 occupied), the same
    // report is accepted, preserves all 512 owners, and mints RetainedBornClear.
    let mut under_cap = engine.clone();
    let removed = under_cap.world.settlement.operation_settled.pop_last();
    assert!(removed.is_some());
    assert_eq!(
        under_cap.world.operation_settle_slot_count(None, &[]),
        crate::world_report::MAX_REPLY_OPERATIONS - 1
    );
    let (under_outcome, _) = under_cap.handle_report(&over_cap, &[]);
    assert!(
        matches!(under_outcome, V4RunOutcome::Accepted(_)),
        "expected acceptance at 1023 slots when born-clear mints 1024th slot, got {under_outcome:?}"
    );
    assert_eq!(under_cap.world.settlement.retained_born.len(), 1);
    assert_eq!(
        under_cap.world.settlement.retained_born[0].window,
        WindowId("born-over-cap".to_owned())
    );
    assert_eq!(
        under_cap.world.operation_settle_slot_count(None, &[]),
        crate::world_report::MAX_REPLY_OPERATIONS
    );

    let (outcome, _) = engine.handle_report(&over_cap, &[]);
    assert!(
        matches!(
            outcome,
            V4RunOutcome::Rejected {
                message: "section-over-cap",
                ..
            }
        ),
        "expected section-over-cap for staged born-clear at 1024 cap, got {outcome:?}"
    );
    assert!(
        engine.world.settlement.retained_born.is_empty(),
        "candidate discarded"
    );
    assert_eq!(
        engine.world.operation_settle_slot_count(None, &[]),
        crate::world_report::MAX_REPLY_OPERATIONS
    );
    assert_eq!(engine.state_revision(), rev_before, "revision unchanged");

    // Subsequent empty report still succeeds (engine is not wedged).
    let followup = report_full_v4(2, 2, scr, Vec::new(), Vec::new(), "", None, None);
    let _ = accepted_v4(&mut engine, &followup);
}

#[test]
fn staged_pending_operation_future_gate_refusal_rolls_back_transactionally() {
    use std::cell::{Cell, RefCell};

    use crate::directional::WorkspaceId;
    use crate::session::DomainKey;
    use crate::world_admit_v4::{
        AdmissionGateDecision, CoreAdmissionGate, ExactInnerCheck, ExactInnerDecision,
        FutureReserveGateFacts, PreflightGateFacts,
    };
    use crate::world_engine::RetainedBornClear;
    use crate::world_report::MAX_REPLY_OPERATIONS;
    use crate::world_report_v4_checks::report_digest_v4;
    use crate::world_run_v4::V4RunOutcome;

    struct LocalFutureReserveGate {
        max_operation_slots: usize,
        observed_facts: RefCell<Vec<FutureReserveGateFacts>>,
        observed_emitted_ops: RefCell<Vec<usize>>,
    }

    impl LocalFutureReserveGate {
        fn new(max_operation_slots: usize) -> Self {
            Self {
                max_operation_slots,
                observed_facts: RefCell::new(Vec::new()),
                observed_emitted_ops: RefCell::new(Vec::new()),
            }
        }
    }

    impl CoreAdmissionGate for LocalFutureReserveGate {
        fn check_request_size(&self, _report: &WorldReportV4) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }

        fn check_preflight(
            &self,
            _report: &WorldReportV4,
            _facts: &PreflightGateFacts<'_>,
        ) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }

        fn check_exact_inner(&self, _reply: &WorldReplyV4) -> ExactInnerDecision {
            ExactInnerDecision::Allow(ExactInnerCheck::new(0))
        }

        fn check_future_reserve(
            &self,
            reply: &WorldReplyV4,
            facts: &FutureReserveGateFacts,
            _exact: ExactInnerCheck,
        ) -> AdmissionGateDecision {
            self.observed_facts.borrow_mut().push(*facts);
            self.observed_emitted_ops
                .borrow_mut()
                .push(reply.operation_settled.len());
            if facts.operation_slot_count > self.max_operation_slots {
                AdmissionGateDecision::Refuse
            } else {
                AdmissionGateDecision::Allow
            }
        }
    }

    let mut engine = engine_with(Vec::new());
    let scr = vec![screen("DP-1", 0)];
    let startup = report_full_v4(1, 1, scr.clone(), Vec::new(), Vec::new(), "", None, None);
    let base_reply = accepted_v4(&mut engine, &startup);
    let rev_before = engine.state_revision();
    let replay_before = engine
        .replay
        .as_ref()
        .map(|r| (r.request_seq, r.digest, r.observation_seq));

    // Stage one settled operation slot (same-key dedup keeps slot_count == 1
    // when the reply emits that one settled operation row).
    let mut staged_without_born = engine.clone();
    staged_without_born.world.state_revision += 1;
    staged_without_born.tx(|tx| {
        tx.emit_operation(
            "op-0".to_owned(),
            WindowId("w-0".to_owned()),
            OperationOutcome::ClearContinuedTiled,
            None,
        )
    });

    let mut reply = base_reply.clone();
    reply.operation_settled = staged_without_born
        .world
        .settlement
        .operation_settled
        .values()
        .cloned()
        .collect();
    assert_eq!(reply.operation_settled.len(), 1);
    assert_eq!(
        staged_without_born
            .world
            .operation_settle_slot_count(None, &[]),
        1
    );

    // Nonvacuity: without RetainedBornClear, the future gate observes 1 slot
    // and accepts, invoking prepare.
    let next_rep = report_full_v4(2, 2, scr, Vec::new(), Vec::new(), "", None, None);
    let digest = report_digest_v4(&next_rep);
    let ok_gate = LocalFutureReserveGate::new(1);
    let ok_prepared = Cell::new(false);
    let mut ok_engine = engine.clone();
    let ok_outcome = ok_engine
        .finalize_v4_candidate_with_gate_and_prepare(
            staged_without_born.world.clone(),
            &next_rep,
            digest,
            reply.clone(),
            &[],
            &ok_gate,
            |_| {
                ok_prepared.set(true);
                Ok::<(), std::convert::Infallible>(())
            },
        )
        .unwrap_or_else(|e| match e {});
    assert!(
        matches!(ok_outcome, V4RunOutcome::Accepted(_)),
        "expected acceptance without born obligations, got {ok_outcome:?}"
    );
    assert!(ok_prepared.get(), "accepted candidate must run prepare");
    assert_eq!(
        ok_gate.observed_facts.borrow().as_slice(),
        &[FutureReserveGateFacts {
            operation_slot_count: 1,
            switch_output_count: 0,
            switch_failed_count: 0,
        }]
    );
    assert_eq!(ok_gate.observed_emitted_ops.borrow().as_slice(), &[1]);

    // Adding 30 cap-valid RetainedBornClear obligations stays under the hard
    // 1,024-slot cap (31 <= 1,024), so the future gate observes 31 slots and
    // refuses transactionally with zero prepare/install/replay/revision changes.
    let mut staged_with_born = staged_without_born;
    staged_with_born
        .world
        .settlement
        .retained_born
        .extend((0..30).map(|i| RetainedBornClear {
            operation_id: format!("op-born-{i}"),
            window: WindowId(format!("born-win-{i}")),
            home: DomainKey {
                output: OutputId("DP-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
            },
            pending: None,
            ambiguous: false,
        }));
    let slot_count_with_born = staged_with_born
        .world
        .operation_settle_slot_count(None, &[]);
    assert_eq!(slot_count_with_born, 31);
    assert!(slot_count_with_born <= MAX_REPLY_OPERATIONS);

    let rej_gate = LocalFutureReserveGate::new(1);
    let rej_prepared = Cell::new(false);
    let outcome = engine
        .finalize_v4_candidate_with_gate_and_prepare(
            staged_with_born.world,
            &next_rep,
            digest,
            reply,
            &[],
            &rej_gate,
            |_| {
                rej_prepared.set(true);
                Ok::<(), std::convert::Infallible>(())
            },
        )
        .unwrap_or_else(|e| match e {});
    assert_eq!(
        rej_gate.observed_facts.borrow().as_slice(),
        &[FutureReserveGateFacts {
            operation_slot_count: 31,
            switch_output_count: 0,
            switch_failed_count: 0,
        }]
    );
    assert_eq!(rej_gate.observed_emitted_ops.borrow().as_slice(), &[1]);
    assert!(
        matches!(
            outcome,
            V4RunOutcome::Rejected {
                kind: crate::world_engine::KIND_REPLY_SIZE,
                message: crate::world_engine::MSG_REPLY_SIZE,
            }
        ),
        "expected reply-too-large when future gate refuses born-clear reserve, got {outcome:?}"
    );
    assert!(
        !rej_prepared.get(),
        "refused candidate must not run prepare"
    );
    assert_eq!(engine.state_revision(), rev_before, "revision unchanged");
    assert!(
        engine.world.settlement.retained_born.is_empty(),
        "staged born obligations not installed"
    );
    assert!(
        engine.world.settlement.operation_settled.is_empty(),
        "staged settled operations not installed"
    );
    assert_eq!(
        engine
            .replay
            .as_ref()
            .map(|r| (r.request_seq, r.digest, r.observation_seq)),
        replay_before,
        "replay cache unchanged"
    );
}

#[test]
fn toggle_maximize_and_toggle_sticky_reserve_float_slots_explicit_and_implicit() {
    let mut engine = engine_with(Vec::new());
    engine.tx(|tx| {
        tx.emit_operation(
            "op-existing".to_owned(),
            WindowId("w-existing".to_owned()),
            OperationOutcome::FloatFrameProven,
            None,
        )
    });
    assert_eq!(engine.world.operation_settle_slot_count(None, &[]), 1);

    // Explicit ToggleMaximize / ToggleSticky on an existing Float slot do not add a slot.
    let max_existing = WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::Window(WindowId("w-existing".to_owned())),
    };
    let sticky_existing = WorldCommand::ToggleSticky {
        window: crate::directional::WindowSubject::Window(WindowId("w-existing".to_owned())),
    };
    assert_eq!(
        engine
            .world
            .operation_settle_slot_count(Some(&max_existing), &[]),
        1
    );
    assert_eq!(
        engine
            .world
            .operation_settle_slot_count(Some(&sticky_existing), &[]),
        1
    );

    // Explicit ToggleMaximize / ToggleSticky on new windows reserve 1 Float slot each.
    let max_new = WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::Window(WindowId("w-max".to_owned())),
    };
    let sticky_new = WorldCommand::ToggleSticky {
        window: crate::directional::WindowSubject::Window(WindowId("w-sticky".to_owned())),
    };
    assert_eq!(
        engine
            .world
            .operation_settle_slot_count(Some(&max_new), &[(1, sticky_new)]),
        3
    );

    // Current-focus variants conservatively reserve 1 slot each.
    let imp_max = WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let imp_sticky = WorldCommand::ToggleSticky {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    assert_eq!(
        engine
            .world
            .operation_settle_slot_count(Some(&imp_max), &[(1, imp_sticky)]),
        3
    );

    // Genuine NoFocus FocusDirectional reserves 2 deduplicated slots once
    // across inline + FIFO, distinct from per-command CurrentFocus slots.
    let no_focus = WorldCommand::FocusDirectional {
        window: crate::world_report::FocusDirectionalSubject::NoFocus,
        direction: crate::directional::Direction::Right,
    };
    let cur_focus = WorldCommand::FocusDirectional {
        window: crate::world_report::FocusDirectionalSubject::CurrentFocus,
        direction: crate::directional::Direction::Right,
    };
    assert_eq!(
        engine
            .world
            .operation_settle_slot_count(Some(&no_focus), &[(1, no_focus.clone()), (2, cur_focus)]),
        5
    );
}

#[test]
fn toggle_maximize_and_toggle_sticky_at_1024_cap_reject_or_reconcile_consumed() {
    use crate::world_run_v4::V4RunOutcome;

    let mut engine = engine_with(Vec::new());
    let scr = vec![screen("DP-1", 0)];
    let init = report_full_v4(
        1,
        1,
        scr.clone(),
        vec![fact("w-0", "DP-1", 0)],
        Vec::new(),
        "w-0",
        Some(1),
        Some(WorldCommand::SelectNumbered {
            output: None,
            index: 1,
        }),
    );
    let _ = accepted_v4(&mut engine, &init);

    for i in 0..512 {
        let win = WindowId(format!("w-{i}"));
        engine.tx(|tx| {
            tx.emit_operation(
                format!("op-c-{i}"),
                win.clone(),
                OperationOutcome::ClearContinuedTiled,
                None,
            )
        });
        engine.tx(|tx| {
            tx.emit_operation(
                format!("op-f-{i}"),
                win,
                OperationOutcome::FloatFrameProven,
                None,
            )
        });
    }
    assert_eq!(
        engine.world.operation_settle_slot_count(None, &[]),
        crate::world_report::MAX_REPLY_OPERATIONS
    );

    for (seq, cmd) in [
        (
            2,
            WorldCommand::ToggleMaximize {
                window: crate::directional::WindowSubject::Window(WindowId("w-new".to_owned())),
            },
        ),
        (
            3,
            WorldCommand::ToggleSticky {
                window: crate::directional::WindowSubject::Window(WindowId("w-new".to_owned())),
            },
        ),
        (
            4,
            WorldCommand::ToggleMaximize {
                window: crate::directional::WindowSubject::CurrentFocus,
            },
        ),
        (
            5,
            WorldCommand::ToggleSticky {
                window: crate::directional::WindowSubject::CurrentFocus,
            },
        ),
    ] {
        let rep = report_full_v4(
            seq,
            seq,
            scr.clone(),
            vec![fact("w-0", "DP-1", 0)],
            Vec::new(),
            "w-0",
            Some(seq),
            Some(cmd),
        );
        let (outcome, _) = engine.handle_report(&rep, &[]);
        assert!(
            matches!(
                outcome,
                V4RunOutcome::Rejected {
                    message: "section-over-cap",
                    ..
                }
            ),
            "expected section-over-cap for seq={seq}, got {outcome:?}"
        );
    }

    // Consumed inline ToggleMaximize / ToggleSticky at 1024 cap must still
    // reconcile as CommandConsumed rather than rejecting. Clone per case so
    // Q2 pruning from the first complete report does not drop the second case
    // below the 1024 cap.
    for (obs_seq, cmd) in [
        (
            2,
            WorldCommand::ToggleMaximize {
                window: crate::directional::WindowSubject::Window(WindowId("w-new".to_owned())),
            },
        ),
        (
            3,
            WorldCommand::ToggleSticky {
                window: crate::directional::WindowSubject::Window(WindowId("w-new".to_owned())),
            },
        ),
    ] {
        let mut case_engine = engine.clone();
        assert_eq!(
            case_engine.world.operation_settle_slot_count(None, &[]),
            crate::world_report::MAX_REPLY_OPERATIONS
        );
        let consumed_rep = report_full_v4(
            obs_seq,
            obs_seq,
            scr.clone(),
            vec![fact("w-0", "DP-1", 0)],
            Vec::new(),
            "w-0",
            Some(1),
            Some(cmd),
        );
        let (outcome, _) = case_engine.handle_report(&consumed_rep, &[]);
        assert!(
            matches!(outcome, V4RunOutcome::CommandConsumed(_)),
            "consumed inline command at 1024 cap must reconcile, got {outcome:?}"
        );
    }
}

#[test]
fn published_settlement_trace_operation_q1_q2_duplicate_late_toggle_and_reset() {
    use crate::boundary::CoreCommand;
    use crate::world_report::{FrameAttempt, FrameSetter};
    use crate::world_run_v4::V4RunOutcome;

    let mut engine = engine_with(Vec::new());
    let screens = vec![screen("out-a", 0)];
    let mut win_a = fact("win-a", "out-a", 0);
    win_a.maximized = Some(false);
    let mut win_b = fact("win-b", "out-a", 400);
    win_b.maximized = Some(false);

    // 1. Initial empty summary.
    let init_lines = crate::world_engine::with_settlement_trace_capture(|| {
        let r1 = accepted_v4(
            &mut engine,
            &report_v4(
                1,
                screens.clone(),
                vec![win_a.clone(), win_b.clone()],
                "win-a",
                ingress_v4("win-a"),
            ),
        );
        assert!(r1.operation_settled.is_empty());
    });
    assert_eq!(
        init_lines,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-1 req=req-1 rev=1 obs=1 ready=true dup=false ops=0 switches=0 gesture=none"
        ]
    );

    // 2. Float win-a (op-2) and settle via FrameAttempt at seq 3 -> state=new.
    let mut r2 = report_v4(
        2,
        screens.clone(),
        vec![win_a.clone(), win_b.clone()],
        "win-a",
        ingress_v4("win-a"),
    );
    r2.command_seq = Some(2);
    r2.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
            "win-a".to_owned(),
        )),
        float_rect: None,
    }));
    let reply2 = accepted_v4(&mut engine, &r2);
    let float_target = reply2
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .and_then(|w| w.effective_frame)
        .expect("float target");

    let mut r3 = report_v4(
        3,
        screens.clone(),
        vec![win_a.clone(), win_b.clone()],
        "win-a",
        ingress_v4("win-a"),
    );
    r3.windows[0].rect = Some(float_target);
    r3.frames = vec![FrameAttempt {
        operation_id: Some("op-2".to_owned()),
        window: WindowId("win-a".to_owned()),
        target: float_target,
        setter: FrameSetter::Accepted,
        readback: Some(float_target),
        dpi_before: None,
        dpi_after: None,
        attempt_ms: 7,
        prewrite: None,
    }];
    let settled_lines = crate::world_engine::with_settlement_trace_capture(|| {
        let reply3 = accepted_v4(&mut engine, &r3);
        assert_eq!(reply3.operation_settled.len(), 1);
    });
    assert_eq!(
        settled_lines,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-1 req=req-3 rev=3 obs=3 ready=true dup=false ops=1 switches=0 gesture=none",
            "omnitiler:core:trace ev=settlement-operation gen=gen-1 req=req-3 rev=3 obs=3 key=win-a:float owner=win-a kind=float op=op-2 outcome=float-frame-proven cause=- state=new pending=-",
        ]
    );

    // 3. Cached duplicate of r3 -> dup=true and state=retained (never new).
    let dup_lines = crate::world_engine::with_settlement_trace_capture(|| {
        let (outcome, _) = engine.handle_report(&r3, &[]);
        assert!(matches!(outcome, V4RunOutcome::Duplicate(_)));
    });
    assert_eq!(
        dup_lines,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-1 req=req-3 rev=3 obs=3 ready=true dup=true ops=1 switches=0 gesture=none",
            "omnitiler:core:trace ev=settlement-operation gen=gen-1 req=req-3 rev=3 obs=3 key=win-a:float owner=win-a kind=float op=op-2 outcome=float-frame-proven cause=- state=retained pending=-",
        ]
    );

    // 4. Q1: tile win-a (op-4), then float win-a again (op-5) -> retained op-2 + pending=op-5.
    let mut win_a_floated = win_a.clone();
    win_a_floated.rect = Some(float_target);
    let mut r4 = report_v4(
        4,
        screens.clone(),
        vec![win_a_floated.clone(), win_b.clone()],
        "win-a",
        ingress_v4("win-a"),
    );
    r4.command_seq = Some(4);
    r4.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
            "win-a".to_owned(),
        )),
        float_rect: None,
    }));
    let _ = accepted_v4(&mut engine, &r4);

    let mut r5 = report_v4(
        5,
        screens.clone(),
        vec![win_a.clone(), win_b.clone()],
        "win-a",
        ingress_v4("win-a"),
    );
    r5.command_seq = Some(5);
    r5.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
            "win-a".to_owned(),
        )),
        float_rect: None,
    }));
    let pending_lines = crate::world_engine::with_settlement_trace_capture(|| {
        let reply5 = accepted_v4(&mut engine, &r5);
        assert_eq!(reply5.operation_settled.len(), 1);
    });
    assert_eq!(
        pending_lines,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-1 req=req-5 rev=5 obs=5 ready=true dup=false ops=1 switches=0 gesture=none",
            "omnitiler:core:trace ev=settlement-operation gen=gen-1 req=req-5 rev=5 obs=5 key=win-a:float owner=win-a kind=float op=op-2 outcome=float-frame-proven cause=- state=retained pending=op-5",
        ]
    );

    // 5. Replacement settles op-5 -> state=new pending=-.
    let mut r6 = report_v4(
        6,
        screens.clone(),
        vec![win_a_floated.clone(), win_b.clone()],
        "win-a",
        ingress_v4("win-a"),
    );
    r6.frames = vec![FrameAttempt {
        operation_id: Some("op-5".to_owned()),
        window: WindowId("win-a".to_owned()),
        target: float_target,
        setter: FrameSetter::Accepted,
        readback: Some(float_target),
        dpi_before: None,
        dpi_after: None,
        attempt_ms: 7,
        prewrite: None,
    }];
    let replaced_lines = crate::world_engine::with_settlement_trace_capture(|| {
        let reply6 = accepted_v4(&mut engine, &r6);
        assert_eq!(reply6.operation_settled[0].operation_id, "op-5");
    });
    assert_eq!(
        replaced_lines,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-1 req=req-6 rev=6 obs=6 ready=true dup=false ops=1 switches=0 gesture=none",
            "omnitiler:core:trace ev=settlement-operation gen=gen-1 req=req-6 rev=6 obs=6 key=win-a:float owner=win-a kind=float op=op-5 outcome=float-frame-proven cause=- state=new pending=-",
        ]
    );

    // 6. Late trace toggle: settle win-b (op-7) with trace disabled -> zero lines,
    //    then publish seq 9 with trace enabled -> win-b is state=retained (not new).
    let disabled_lines = crate::world_engine::with_settlement_trace_capture(|| {
        crate::world_engine::with_focus_trace_disabled(|| {
            let mut r7 = report_v4(
                7,
                screens.clone(),
                vec![win_a_floated.clone(), win_b.clone()],
                "win-b",
                ingress_v4("win-b"),
            );
            r7.command_seq = Some(7);
            r7.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
                    "win-b".to_owned(),
                )),
                float_rect: None,
            }));
            let reply7 = accepted_v4(&mut engine, &r7);
            let target_b = reply7
                .windows
                .iter()
                .find(|w| w.lifetime.0 == "win-b")
                .and_then(|w| w.effective_frame)
                .expect("win-b float target");

            let mut r8 = report_v4(
                8,
                screens.clone(),
                vec![win_a_floated.clone(), win_b.clone()],
                "win-b",
                ingress_v4("win-b"),
            );
            r8.windows[1].rect = Some(target_b);
            r8.frames = vec![FrameAttempt {
                operation_id: Some("op-7".to_owned()),
                window: WindowId("win-b".to_owned()),
                target: target_b,
                setter: FrameSetter::Accepted,
                readback: Some(target_b),
                dpi_before: None,
                dpi_after: None,
                attempt_ms: 7,
                prewrite: None,
            }];
            let _ = accepted_v4(&mut engine, &r8);
        });
    });
    assert!(disabled_lines.is_empty());

    // 7. Q2 disappearance: close win-a -> win-a dropped, win-b retained.
    let mut r9 = report_v4(
        9,
        screens.clone(),
        vec![win_b.clone()],
        "win-b",
        ingress_v4("win-b"),
    );
    r9.closed = vec![WindowId("win-a".to_owned())];
    let q2_lines = crate::world_engine::with_settlement_trace_capture(|| {
        let reply9 = accepted_v4(&mut engine, &r9);
        assert_eq!(reply9.operation_settled.len(), 1);
        assert_eq!(reply9.operation_settled[0].window.0, "win-b");
    });
    assert_eq!(
        q2_lines,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-1 req=req-9 rev=9 obs=9 ready=true dup=false ops=1 switches=0 gesture=none",
            "omnitiler:core:trace ev=settlement-operation gen=gen-1 req=req-9 rev=9 obs=9 key=win-b:float owner=win-b kind=float op=op-7 outcome=float-frame-proven cause=- state=retained pending=-",
        ]
    );

    // 8. Generation reset & bootstrap ready=false: fresh unbound-settings engine
    //    emits empty ready=false summary under gen-2.
    let mut boot_engine = WorldEngine::new_test_bound("gen-2", "adapter-1").expect("gen-2");
    // Clear initial settings by constructing via new_with_generation? Wait:
    // new_test_bound sets ready=false until initial settings are accepted!
    let boot_lines = crate::world_engine::with_settlement_trace_capture(|| {
        let mut r_boot = report_v4(
            1,
            screens.clone(),
            vec![win_b.clone()],
            "win-b",
            ingress_v4("win-b"),
        );
        r_boot.core_generation = "gen-2".to_owned();
        r_boot.inventory_complete = false;
        let reply_boot = accepted_v4(&mut boot_engine, &r_boot);
        assert!(!reply_boot.ready);
        assert!(reply_boot.operation_settled.is_empty());
    });
    assert_eq!(
        boot_lines,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-2 req=req-1 rev=0 obs=1 ready=false dup=false ops=0 switches=0 gesture=none"
        ]
    );

    // 9. Genuine clear-phase trace assertions.
    let mut clear_engine = engine_with(Vec::new());
    let clear_screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let mut win_max = fact("win-max", "out-a", 0);
    win_max.maximized = Some(true);
    let win_other = fact("win-other", "out-b", 900);
    let _ = accepted_v4(
        &mut clear_engine,
        &report_v4(
            1,
            clear_screens.clone(),
            vec![win_max.clone(), win_other.clone()],
            "win-max",
            ingress_v4("win-max"),
        ),
    );
    let mut r_move = report_v4(
        2,
        clear_screens.clone(),
        vec![win_max.clone(), win_other.clone()],
        "win-max",
        ingress_v4("win-max"),
    );
    r_move.command_seq = Some(2);
    r_move.command = Some(WorldCommand::MoveDirectional {
        window: crate::directional::WindowSubject::Window(WindowId("win-max".to_owned())),
        direction: crate::directional::Direction::Right,
        same_axis: crate::directional::SameAxisMove::default(),
    });
    let _ = accepted_v4(&mut clear_engine, &r_move);

    let mut r_clear_proof = report_v4(
        3,
        clear_screens,
        vec![
            WindowFact {
                maximized: Some(false),
                ..win_max
            },
            win_other,
        ],
        "win-max",
        ingress_v4("win-max"),
    );
    r_clear_proof.clear_attempts = vec![crate::world_report::ClearAttempt {
        operation_id: "op-2".to_owned(),
        window: WindowId("win-max".to_owned()),
        setter: crate::world_report::FrameSetter::Accepted,
        pre_maximized: Some(true),
        post_maximized: Some(false),
        attempt_ms: 7,
    }];
    let clear_lines = crate::world_engine::with_settlement_trace_capture(|| {
        let reply_clear = accepted_v4(&mut clear_engine, &r_clear_proof);
        assert_eq!(reply_clear.operation_settled.len(), 1);
        assert_eq!(reply_clear.operation_settled[0].kind, OperationKind::Clear);
    });
    assert_eq!(
        clear_lines,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-1 req=req-3 rev=3 obs=3 ready=true dup=false ops=1 switches=0 gesture=none",
            "omnitiler:core:trace ev=settlement-operation gen=gen-1 req=req-3 rev=3 obs=3 key=win-max:clear owner=win-max kind=clear op=op-2 outcome=continued-tiled cause=- state=new pending=-",
        ]
    );
}

#[test]
fn published_settlement_trace_switch_q1_lifecycle_genuine() {
    use crate::world_report::{
        ArrivalBracket, ArrivalLocation, FocusFeedback, LastWritePath, VisibilityAttempt,
        VisibilityFact, VisibilityReason, VisibilitySample, VisibilitySetter,
    };

    let mut engine = engine_with(Vec::new());
    let screens = vec![screen("out-a", 0)];
    let mut w1 = fact("win-a", "out-a", 0);
    w1.suppression_available = Some(true);
    w1.visibility.suppression_owned = Some(false);
    let mut w2 = fact("win-b", "out-a", 400);
    w2.suppression_available = Some(true);
    w2.visibility.suppression_owned = Some(false);

    // 1. Seed two windows on home workspace, then send win-b to away (follow=false)
    //    with win-b focused so the transfer is admitted and away exists.
    let boot = accepted_v4(
        &mut engine,
        &report_v4(
            1,
            screens.clone(),
            vec![w1.clone(), w2.clone()],
            "win-a",
            ingress_v4("win-a"),
        ),
    );
    let mut ring = boot
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone());
    let home = ring.next().expect("home workspace");
    let away = ring.next().expect("away workspace");
    let mut r_send = report_v4(
        2,
        screens.clone(),
        vec![w1.clone(), w2.clone()],
        "win-b",
        ingress_v4("win-b"),
    );
    r_send.command_seq = Some(2);
    r_send.command = Some(WorldCommand::Send {
        window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: crate::directional::WorkspaceId(away.clone()),
        follow: false,
    });
    let _ = accepted_v4(&mut engine, &r_send);

    // 2. Select away on out-a (op-3), then settle with genuine LastWrite + FocusFeedback -> state=new.
    let mut r3 = report_v4(
        3,
        screens.clone(),
        vec![w1.clone(), w2.clone()],
        "win-a",
        ingress_v4("win-a"),
    );
    r3.command_seq = Some(3);
    r3.command = Some(WorldCommand::SelectWorkspace {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(away),
    });
    let grant3 = accepted_v4(&mut engine, &r3);
    let rev3 = grant3.state_revision;
    let eff3 = grant3.focus_request.clone().expect("focus request 3");
    let frame_b = grant3
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-b")
        .and_then(|w| w.effective_frame);

    let vis_attempt = |win: &str, target: bool, rev: u64, obs: u64| VisibilityAttempt {
        window: WindowId(win.to_owned()),
        target,
        revision: rev,
        path: LastWritePath::Staged,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: None,
        post: Some(VisibilitySample {
            obs,
            lifetime_matched: Some(true),
            visibility: VisibilityFact {
                visible: Some(target),
                suppression_owned: Some(false),
            },
            minimized: Some(false),
            host_hidden: Some(false),
        }),
    };
    let mut r4 = report_v4(
        4,
        screens.clone(),
        vec![w1.clone(), w2.clone()],
        "win-b",
        ingress_v4("win-b"),
    );
    r4.visibility_attempts = vec![
        vis_attempt("win-a", false, rev3, 4),
        vis_attempt("win-b", true, rev3, 4),
    ];
    r4.focus_feedback = vec![FocusFeedback {
        id: eff3.id,
        target: WindowId("win-b".to_owned()),
        revision: eff3.issued_revision,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: Some(WindowId("win-a".to_owned())),
        post: Some(WindowId("win-b".to_owned())),
        foreground_proven: true,
        arrival: vec![ArrivalBracket {
            window: WindowId("win-b".to_owned()),
            obs: 4,
            before: VisibilitySample {
                obs: 4,
                lifetime_matched: Some(true),
                visibility: VisibilityFact {
                    visible: Some(true),
                    suppression_owned: Some(false),
                },
                minimized: Some(false),
                host_hidden: Some(false),
            },
            location: ArrivalLocation::Output {
                out: Some(OutputId("out-a".to_owned())),
            },
            frame: frame_b,
            after: VisibilitySample {
                obs: 4,
                lifetime_matched: Some(true),
                visibility: VisibilityFact {
                    visible: Some(true),
                    suppression_owned: Some(false),
                },
                minimized: Some(false),
                host_hidden: Some(false),
            },
        }],
        attempt_ms: 5,
    }];
    let settled_lines = crate::world_engine::with_settlement_trace_capture(|| {
        let reply4 = accepted_v4(&mut engine, &r4);
        assert_eq!(reply4.switch_settled.len(), 1);
    });
    assert_eq!(
        settled_lines,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-1 req=req-4 rev=4 obs=4 ready=true dup=false ops=0 switches=1 gesture=none",
            "omnitiler:core:trace ev=settlement-switch gen=gen-1 req=req-4 rev=4 obs=4 key=out-a owner=out-a kind=switch op=op-3 outcome=complete focus=proven fallback=false state=new pending=- failed=",
        ]
    );

    // 3. Q1: admit successor SelectWorkspace back to home (op-5) -> op-3 retained + pending=op-5.
    let mut r5 = report_v4(
        5,
        screens.clone(),
        vec![w1.clone(), w2.clone()],
        "win-b",
        ingress_v4("win-b"),
    );
    r5.command_seq = Some(5);
    r5.command = Some(WorldCommand::SelectWorkspace {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(home),
    });
    let mut grant5_opt = None;
    let pending_lines = crate::world_engine::with_settlement_trace_capture(|| {
        grant5_opt = Some(accepted_v4(&mut engine, &r5));
    });
    assert_eq!(
        pending_lines,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-1 req=req-5 rev=5 obs=5 ready=true dup=false ops=0 switches=1 gesture=none",
            "omnitiler:core:trace ev=settlement-switch gen=gen-1 req=req-5 rev=5 obs=5 key=out-a owner=out-a kind=switch op=op-3 outcome=complete focus=proven fallback=false state=retained pending=op-5 failed=",
        ]
    );

    // 4. Settle successor op-5 -> replaced with op=op-5 state=new pending=-.
    let grant5 = grant5_opt.expect("grant5");
    let rev5 = grant5.state_revision;
    let eff5 = grant5.focus_request.clone().expect("focus request 5");
    let frame_a = grant5
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .and_then(|w| w.effective_frame);
    let mut r6 = report_v4(6, screens, vec![w1, w2], "win-a", ingress_v4("win-a"));
    r6.visibility_attempts = vec![
        vis_attempt("win-a", true, rev5, 6),
        vis_attempt("win-b", false, rev5, 6),
    ];
    r6.focus_feedback = vec![FocusFeedback {
        id: eff5.id,
        target: WindowId("win-a".to_owned()),
        revision: eff5.issued_revision,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: Some(WindowId("win-b".to_owned())),
        post: Some(WindowId("win-a".to_owned())),
        foreground_proven: true,
        arrival: vec![ArrivalBracket {
            window: WindowId("win-a".to_owned()),
            obs: 6,
            before: VisibilitySample {
                obs: 6,
                lifetime_matched: Some(true),
                visibility: VisibilityFact {
                    visible: Some(true),
                    suppression_owned: Some(false),
                },
                minimized: Some(false),
                host_hidden: Some(false),
            },
            location: ArrivalLocation::Output {
                out: Some(OutputId("out-a".to_owned())),
            },
            frame: frame_a,
            after: VisibilitySample {
                obs: 6,
                lifetime_matched: Some(true),
                visibility: VisibilityFact {
                    visible: Some(true),
                    suppression_owned: Some(false),
                },
                minimized: Some(false),
                host_hidden: Some(false),
            },
        }],
        attempt_ms: 5,
    }];
    let replaced_lines = crate::world_engine::with_settlement_trace_capture(|| {
        let reply6 = accepted_v4(&mut engine, &r6);
        assert_eq!(reply6.switch_settled[0].operation_id, "op-5");
    });
    assert_eq!(
        replaced_lines,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-1 req=req-6 rev=6 obs=6 ready=true dup=false ops=0 switches=1 gesture=none",
            "omnitiler:core:trace ev=settlement-switch gen=gen-1 req=req-6 rev=6 obs=6 key=out-a owner=out-a kind=switch op=op-5 outcome=complete focus=proven fallback=false state=new pending=- failed=",
        ]
    );
}

#[test]
fn published_settlement_trace_bounded_failed_rendering_synthetic() {
    // Synthetic rendering test only: verifies TRACE_FAILED_CAP (8) + failedOmitted=<n>
    // on both settlement-switch and switch-settled-dropped without fake lifecycle claims.
    let mut engine = engine_with(Vec::new());
    let screens = screens_pair();
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)];
    let _ = accepted_v4(
        &mut engine,
        &report_v4(
            1,
            screens.clone(),
            windows.clone(),
            "win-a",
            ingress_v4("win-a"),
        ),
    );

    let failed: Vec<WindowId> = (0..10).map(|i| WindowId(format!("w-{i:02}"))).collect();
    engine.world.settlement.switch_settled.insert(
        OutputId("out-b".to_owned()),
        SwitchEvent {
            output: OutputId("out-b".to_owned()),
            operation_id: "op-s1".to_owned(),
            outcome: SwitchOutcome::CompletedWithFailures,
            failed,
            focus: SwitchFocus::Unproven,
            fallback: false,
        },
    );

    let r2 = report_v4(2, screens, windows.clone(), "win-a", ingress_v4("win-a"));
    let lines_r2 = crate::world_engine::with_settlement_trace_capture(|| {
        let reply2 = accepted_v4(&mut engine, &r2);
        assert_eq!(reply2.switch_settled.len(), 1);
    });
    assert_eq!(
        lines_r2,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-1 req=req-2 rev=2 obs=2 ready=true dup=false ops=0 switches=1 gesture=none",
            "omnitiler:core:trace ev=settlement-switch gen=gen-1 req=req-2 rev=2 obs=2 key=out-b owner=out-b kind=switch op=op-s1 outcome=completed-with-failures focus=unproven fallback=false state=new pending=- failed=w-00,w-01,w-02,w-03,w-04,w-05,w-06,w-07 failedOmitted=2",
        ]
    );

    let r3 = report_v4(
        3,
        vec![screen("out-a", 0)],
        vec![windows[0].clone()],
        "win-a",
        ingress_v4("win-a"),
    );
    let drop_lines = crate::world_engine::with_focus_trace_capture(|| {
        let reply3 = accepted_v4(&mut engine, &r3);
        assert!(reply3.switch_settled.is_empty());
    });
    assert_eq!(
        drop_lines,
        vec![
            "omnitiler:core:trace ev=switch-settled-dropped gen=gen-1 req=req-3 rev=3 obs=3 out=out-b op=op-s1 outcome=completed-with-failures focus=unproven fallback=false failed=w-00,w-01,w-02,w-03,w-04,w-05,w-06,w-07 failedOmitted=2",
        ]
    );
}

#[test]
fn published_settlement_trace_gesture_lifecycle_and_rejected_candidate() {
    use crate::gesture::{GestureIngress, GestureKind, GesturePhase};
    use crate::world_run_v4::V4RunOutcome;

    let mut engine = engine_with(Vec::new());
    let screens = vec![screen("out-a", 0)];
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)];

    // Seed initial state.
    let _ = accepted_v4(
        &mut engine,
        &report_v4(
            1,
            screens.clone(),
            windows.clone(),
            "win-a",
            ingress_v4("win-a"),
        ),
    );

    // Start + release gesture id=10 -> state=new pending=-.
    let mut r2 = report_v4(
        2,
        screens.clone(),
        windows.clone(),
        "win-a",
        ingress_v4("win-a"),
    );
    r2.gesture_barrier = true;
    r2.gesture = Some(GestureIngress {
        window: WindowId("win-a".to_owned()),
        id: 10,
        kind: GestureKind::Move,
        phase: GesturePhase::Started,
        start_frame: rect(0, 0, 40, 90),
        cursor: Some((20, 20)),
        start_cursor: Some((20, 20)),
        esc: false,
        modal_move: false,
    });
    let _ = accepted_v4(&mut engine, &r2);

    let mut r3 = report_v4(
        3,
        screens.clone(),
        windows.clone(),
        "win-a",
        ingress_v4("win-a"),
    );
    r3.gesture = Some(GestureIngress {
        window: WindowId("win-a".to_owned()),
        id: 10,
        kind: GestureKind::Move,
        phase: GesturePhase::Released,
        start_frame: rect(0, 0, 40, 90),
        cursor: Some((450, 20)),
        start_cursor: Some((20, 20)),
        esc: false,
        modal_move: false,
    });
    let g1_lines = crate::world_engine::with_settlement_trace_capture(|| {
        let reply3 = accepted_v4(&mut engine, &r3);
        assert_eq!(
            reply3.gesture_settled,
            Some(GestureSettled {
                id: 10,
                committed: false,
            })
        );
    });
    assert_eq!(
        g1_lines,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-1 req=req-3 rev=3 obs=3 ready=true dup=false ops=0 switches=0 gesture=present",
            "omnitiler:core:trace ev=settlement-gesture gen=gen-1 req=req-3 rev=3 obs=3 id=10 owner=gen-1 kind=gesture outcome=uncommitted state=new pending=-",
        ]
    );

    // Start successor gesture id=11 -> id=10 is state=retained pending=11.
    let mut r4 = report_v4(
        4,
        screens.clone(),
        windows.clone(),
        "win-a",
        ingress_v4("win-a"),
    );
    r4.gesture_barrier = true;
    r4.gesture = Some(GestureIngress {
        window: WindowId("win-a".to_owned()),
        id: 11,
        kind: GestureKind::Move,
        phase: GesturePhase::Started,
        start_frame: rect(0, 0, 40, 90),
        cursor: Some((20, 20)),
        start_cursor: Some((20, 20)),
        esc: false,
        modal_move: false,
    });
    let g2_pending_lines = crate::world_engine::with_settlement_trace_capture(|| {
        let _ = accepted_v4(&mut engine, &r4);
    });
    assert_eq!(
        g2_pending_lines,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-1 req=req-4 rev=4 obs=4 ready=true dup=false ops=0 switches=0 gesture=present",
            "omnitiler:core:trace ev=settlement-gesture gen=gen-1 req=req-4 rev=4 obs=4 id=10 owner=gen-1 kind=gesture outcome=uncommitted state=retained pending=11",
        ]
    );

    // Release successor gesture id=11 -> replaces id=10 with id=11 state=new pending=-.
    let mut r4b = report_v4(
        5,
        screens.clone(),
        windows.clone(),
        "win-a",
        ingress_v4("win-a"),
    );
    r4b.gesture = Some(GestureIngress {
        window: WindowId("win-a".to_owned()),
        id: 11,
        kind: GestureKind::Move,
        phase: GesturePhase::Released,
        start_frame: rect(0, 0, 40, 90),
        cursor: Some((450, 20)),
        start_cursor: Some((20, 20)),
        esc: false,
        modal_move: false,
    });
    let g2_settled_lines = crate::world_engine::with_settlement_trace_capture(|| {
        let reply4b = accepted_v4(&mut engine, &r4b);
        assert_eq!(
            reply4b.gesture_settled,
            Some(GestureSettled {
                id: 11,
                committed: false,
            })
        );
    });
    assert_eq!(
        g2_settled_lines,
        vec![
            "omnitiler:core:trace ev=settlement-published gen=gen-1 req=req-5 rev=5 obs=5 ready=true dup=false ops=0 switches=0 gesture=present",
            "omnitiler:core:trace ev=settlement-gesture gen=gen-1 req=req-5 rev=5 obs=5 id=11 owner=gen-1 kind=gesture outcome=uncommitted state=new pending=-",
        ]
    );

    // Rejected candidate in finalize_v4_candidate emits zero settlement lines.
    let mut forged = engine.last_reply_v4().clone().expect("last reply");
    forged.intent_desired = vec![crate::world_report::IntentDesire {
        win: WindowId("bad id".to_owned()),
        data: None,
    }];
    let staged = engine.clone();
    let r5 = report_v4(6, screens, windows, "win-a", ingress_v4("win-a"));
    let rejected_lines = crate::world_engine::with_settlement_trace_capture(|| {
        let outcome = engine.finalize_v4_candidate(staged.world, &r5, 0, forged);
        assert!(matches!(outcome, V4RunOutcome::Rejected { .. }));
    });
    assert!(rejected_lines.is_empty());
}

#[test]
fn retained_state_and_pending_op_trace_exposes_unspent_ownership_and_pending_totals() {
    use crate::boundary::CoreCommand;

    let caps = crate::connection::HostCapabilities {
        fullscreen: true,
        maximize: true,
        native_sticky: true,
        above: true,
        ..Default::default()
    };
    let mut engine = WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", caps)
        .expect("generation");
    let screens = vec![screen("out-a", 0)];
    let mut win_a = crate::world_v4_support_tests::float_fact("win-a", "out-a", 0, 100);
    win_a.above = Some(false);
    win_a.below = Some(false);

    // 1. Initial floating admission retains `above` binding (spent=false, pending=false).
    let lines1 = crate::world_engine::with_retained_state_trace_capture(|| {
        let _ = accepted_v4(
            &mut engine,
            &report_v4(
                1,
                screens.clone(),
                vec![win_a.clone()],
                "win-a",
                ingress_v4("win-a"),
            ),
        );
    });
    assert!(
        lines1.iter().any(|l| l.starts_with(
            "omnitiler:core:trace ev=settlement-pending gen=gen-1 req=req-1 rev=1 obs=1 ready=true dup=false pendingFloats=0 pendingClears=0 pendingBornClears=0 pendingTransfers=0 pendingCreations=0 retainedStates=1"
        )),
        "expected settlement-pending summary, got {lines1:?}"
    );
    assert!(
        lines1.iter().any(|l| l.starts_with(
            "omnitiler:core:trace ev=retained-state gen=gen-1 req=req-1 rev=1 obs=1 win=win-a prop=above targetRev=1 targetVal=true spent=false pending=false disposition=unattempted latched=0"
        )),
        "expected unspent pending=false retained-state row, got {lines1:?}"
    );

    // 2. Raw above=true without correlated attempt keeps spent=false, pending=false (not settled).
    let mut preowned = win_a.clone();
    preowned.above = Some(true);
    let lines2 = crate::world_engine::with_retained_state_trace_capture(|| {
        let _ = accepted_v4(
            &mut engine,
            &report_v4(
                2,
                screens.clone(),
                vec![preowned],
                "win-a",
                ingress_v4("win-a"),
            ),
        );
    });
    assert!(
        lines2.iter().any(|l| l.contains("ev=retained-state") && l.contains("win=win-a prop=above targetRev=1 targetVal=true spent=false pending=false disposition=unattempted latched=0")),
        "raw above=true alone must still trace spent=false pending=false, got {lines2:?}"
    );

    // 3. Pending ToggleFloat with no terminal settlement still exposes pendingFloats=1.
    let mut win_b = fact("win-b", "out-a", 400);
    win_b.maximized = Some(false);
    let mut r3 = report_v4(3, screens, vec![win_a, win_b], "win-b", ingress_v4("win-b"));
    r3.command_seq = Some(3);
    r3.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
            "win-b".to_owned(),
        )),
        float_rect: None,
    }));
    let lines3 = crate::world_engine::with_retained_state_trace_capture(|| {
        let reply3 = accepted_v4(&mut engine, &r3);
        assert!(reply3.operation_settled.is_empty());
    });
    assert!(
        lines3
            .iter()
            .any(|l| l.contains("ev=settlement-pending") && l.contains("pendingFloats=1")),
        "expected pendingFloats=1 with no terminal settlement, got {lines3:?}"
    );
}

#[test]
fn q2_dropped_trace_rejects_outbound_gate_preserves_same_obs_and_dedups() {
    use crate::boundary::CoreCommand;
    use crate::world_report::{FrameAttempt, FrameSetter};
    use crate::world_run_v4::V4RunOutcome;

    let mut engine = engine_with(Vec::new());
    let screens = vec![screen("out-a", 0)];
    let mut win_a = fact("win-a", "out-a", 0);
    win_a.maximized = Some(false);
    let mut win_b = fact("win-b", "out-a", 400);
    win_b.maximized = Some(false);

    // 1. Seed and settle win-a float (op-2) at obs=3, rev=3.
    let _ = accepted_v4(
        &mut engine,
        &report_v4(
            1,
            screens.clone(),
            vec![win_a.clone(), win_b.clone()],
            "win-a",
            ingress_v4("win-a"),
        ),
    );
    let mut r2 = report_v4(
        2,
        screens.clone(),
        vec![win_a.clone(), win_b.clone()],
        "win-a",
        ingress_v4("win-a"),
    );
    r2.command_seq = Some(2);
    r2.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
            "win-a".to_owned(),
        )),
        float_rect: None,
    }));
    let reply2 = accepted_v4(&mut engine, &r2);
    let float_target = reply2
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .and_then(|w| w.effective_frame)
        .expect("float target");

    let mut r3 = report_v4(
        3,
        screens.clone(),
        vec![win_a.clone(), win_b.clone()],
        "win-a",
        ingress_v4("win-a"),
    );
    r3.windows[0].rect = Some(float_target);
    r3.frames = vec![FrameAttempt {
        operation_id: Some("op-2".to_owned()),
        window: WindowId("win-a".to_owned()),
        target: float_target,
        setter: FrameSetter::Accepted,
        readback: Some(float_target),
        dpi_before: None,
        dpi_after: None,
        attempt_ms: 7,
        prewrite: None,
    }];
    let _ = accepted_v4(&mut engine, &r3);
    assert_eq!(engine.world.settlement.operation_settled.len(), 1);

    // 2. Staged candidate prunes win-a via close, then fails outbound gate ->
    //    NO drop trace emitted, self.operation_settled and drop buffers unchanged.
    let mut staged = engine.clone();
    std::sync::Arc::make_mut(&mut staged.world.staged.observation).observation_seq = 4;
    std::sync::Arc::make_mut(&mut staged.world.staged.observation)
        .closed
        .push(WindowId("win-a".to_owned()));
    let mut r4_rej = report_v4(
        4,
        screens.clone(),
        vec![win_b.clone()],
        "win-b",
        ingress_v4("win-b"),
    );
    r4_rej.closed = vec![WindowId("win-a".to_owned())];
    let rej_drops = crate::world_engine::with_focus_trace_capture(|| {
        let mut forged =
            staged.tx(|tx| tx.publish_v4(envelope(4), None, Vec::new(), &BTreeSet::new()));
        assert!(forged.operation_settled.is_empty());
        assert_eq!(staged.world.settlement.staged_dropped_operations.len(), 1);
        forged.intent_desired = vec![crate::world_report::IntentDesire {
            win: WindowId("bad id".to_owned()),
            data: None,
        }];
        let outcome = engine.finalize_v4_candidate(staged.world, &r4_rej, 0, forged);
        assert!(matches!(outcome, V4RunOutcome::Rejected { .. }));
    });
    assert!(
        rej_drops.is_empty(),
        "rejected candidate must emit no drop trace: {rej_drops:?}"
    );
    assert_eq!(engine.world.settlement.operation_settled.len(), 1);
    assert!(engine.world.settlement.staged_dropped_operations.is_empty());

    // 3. Different req with SAME obs=4 accepted retaining win-a -> no misattributed drop.
    let mut r4_keep = report_v4(
        4,
        screens.clone(),
        vec![win_a.clone(), win_b.clone()],
        "win-a",
        ingress_v4("win-a"),
    );
    r4_keep.request_id = "req-4-keep".to_owned();
    let keep_drops = crate::world_engine::with_focus_trace_capture(|| {
        let reply_keep = accepted_v4(&mut engine, &r4_keep);
        assert_eq!(reply_keep.operation_settled.len(), 1);
    });
    assert!(
        keep_drops
            .iter()
            .all(|l| !l.contains("ev=operation-settled-dropped")),
        "retained owner at same obs must emit no drop: {keep_drops:?}"
    );

    // 4. Genuine later accepted departure (obs=5, req=req-5-drop) emits one
    //    identity-bound drop; subsequent Duplicate emits no removal.
    let mut r5_drop = report_v4(5, screens, vec![win_b], "win-b", ingress_v4("win-b"));
    r5_drop.request_id = "req-5-drop".to_owned();
    r5_drop.closed = vec![WindowId("win-a".to_owned())];
    let dep_drops: Vec<String> = crate::world_engine::with_focus_trace_capture(|| {
        let reply_drop = accepted_v4(&mut engine, &r5_drop);
        assert!(reply_drop.operation_settled.is_empty());
    })
    .into_iter()
    .filter(|l| {
        l.contains("ev=operation-settled-dropped") || l.contains("ev=switch-settled-dropped")
    })
    .collect();
    assert_eq!(
        dep_drops,
        vec![
            "omnitiler:core:trace ev=operation-settled-dropped gen=gen-1 req=req-5-drop rev=5 obs=5 win=win-a kind=float op=op-2 outcome=float-frame-proven cause=-"
        ]
    );
    assert!(engine.world.settlement.staged_dropped_operations.is_empty());

    let dup_drops = crate::world_engine::with_focus_trace_capture(|| {
        let (outcome, _) = engine.handle_report(&r5_drop, &[]);
        assert!(matches!(outcome, V4RunOutcome::Duplicate(_)));
    });
    assert!(
        dup_drops.is_empty(),
        "cached Duplicate must emit no Q2 removal: {dup_drops:?}"
    );
}

#[test]
fn drag_preview_trace_emits_correlated_branch_reasons_and_is_gated() {
    use crate::gesture::{GestureIngress, GestureKind, GesturePhase};
    use crate::world_report::{FrameAttempt, FrameSetter};
    use crate::world_run_v4::V4RunOutcome;

    let mut engine = engine_with(Vec::new());
    let screens = vec![screen("out-a", 0)];
    let mut win_a = fact("win-a", "out-a", 0);
    win_a.rect = Some(rect(0, 0, 400, 560));
    let mut win_b = fact("win-b", "out-a", 400);
    win_b.rect = Some(rect(400, 0, 400, 560));
    let windows = vec![win_a.clone(), win_b.clone()];

    // 1. Bootstrap (incomplete inventory before ready) -> reason=bootstrap.
    let mut boot = report_v4(1, screens.clone(), Vec::new(), "", ingress_v4(""));
    boot.inventory_complete = false;
    boot.screens[0].complete = false;
    let boot_lines = crate::world_engine::with_drag_preview_trace_capture(|| {
        let (outcome, _) = engine.handle_report(&boot, &[]);
        assert!(matches!(outcome, V4RunOutcome::Accepted(ref r) if !r.ready));
    });
    assert_eq!(
        boot_lines,
        vec![
            "omnitiler:core:trace ev=drag-preview gen=gen-1 req=req-1 rev=0 obs=1 ready=false dup=false preview=none id=- domainOut=- suspendedOut=- reason=bootstrap"
        ]
    );

    // 2. Ready without gesture -> reason=no-session.
    let r2 = report_v4(
        2,
        screens.clone(),
        windows.clone(),
        "win-a",
        ingress_v4("win-a"),
    );
    let mut reply2_opt = None;
    let init_lines = crate::world_engine::with_drag_preview_trace_capture(|| {
        let reply = accepted_v4(&mut engine, &r2);
        assert!(reply.drag_preview.is_none());
        reply2_opt = Some(reply);
    });
    assert_eq!(
        init_lines,
        vec![
            "omnitiler:core:trace ev=drag-preview gen=gen-1 req=req-2 rev=1 obs=2 ready=true dup=false preview=none id=- domainOut=- suspendedOut=- reason=no-session"
        ]
    );
    let reply2 = reply2_opt.expect("reply2");
    for w in &reply2.windows {
        if let Some(target_rect) = w.effective_frame {
            if w.lifetime.0 == "win-a" {
                win_a.rect = Some(target_rect);
            } else if w.lifetime.0 == "win-b" {
                win_b.rect = Some(target_rect);
            }
        }
    }
    let windows = vec![win_a.clone(), win_b.clone()];
    let mut r2_settle = report_v4(
        3,
        screens.clone(),
        windows.clone(),
        "win-a",
        ingress_v4("win-a"),
    );
    r2_settle.frames = reply2
        .windows
        .iter()
        .filter_map(|w| {
            w.effective_frame.map(|target| FrameAttempt {
                operation_id: None,
                window: w.lifetime.clone(),
                target,
                setter: FrameSetter::Accepted,
                readback: Some(target),
                dpi_before: None,
                dpi_after: None,
                attempt_ms: 5,
                prewrite: None,
            })
        })
        .collect();
    let _ = accepted_v4(&mut engine, &r2_settle);

    let make_ingress = |id: u64, phase: GesturePhase, cursor: Option<(i32, i32)>| GestureIngress {
        window: WindowId("win-a".to_owned()),
        id,
        kind: GestureKind::Move,
        phase,
        start_frame: rect(0, 0, 396, 600),
        cursor,
        start_cursor: Some((20, 20)),
        esc: false,
        modal_move: false,
    };

    // 3. Explicit Started -> reason=bind-only.
    let mut r3 = report_v4(
        4,
        screens.clone(),
        windows.clone(),
        "win-a",
        ingress_v4("win-a"),
    );
    r3.gesture_barrier = true;
    r3.gesture = Some(make_ingress(7, GesturePhase::Started, Some((20, 20))));
    let bind_lines = crate::world_engine::with_drag_preview_trace_capture(|| {
        let reply = accepted_v4(&mut engine, &r3);
        assert!(reply.drag_preview.is_none());
    });
    assert_eq!(
        bind_lines,
        vec![
            "omnitiler:core:trace ev=drag-preview gen=gen-1 req=req-4 rev=3 obs=4 ready=true dup=false preview=none id=- domainOut=- suspendedOut=- reason=bind-only"
        ]
    );

    // 4. Updated at start cursor -> reason=zero-move.
    let mut r4 = report_v4(
        5,
        screens.clone(),
        windows.clone(),
        "win-a",
        ingress_v4("win-a"),
    );
    r4.gesture_barrier = true;
    r4.gesture = Some(make_ingress(7, GesturePhase::Updated, Some((20, 20))));
    let zero_lines = crate::world_engine::with_drag_preview_trace_capture(|| {
        let reply = accepted_v4(&mut engine, &r4);
        assert!(reply.drag_preview.is_none());
    });
    assert_eq!(
        zero_lines,
        vec![
            "omnitiler:core:trace ev=drag-preview gen=gen-1 req=req-5 rev=4 obs=5 ready=true dup=false preview=none id=- domainOut=- suspendedOut=- reason=zero-move"
        ]
    );

    // 5. Updated over self centre -> reason=local-engine-refused.
    let mut r5 = report_v4(
        6,
        screens.clone(),
        windows.clone(),
        "win-a",
        ingress_v4("win-a"),
    );
    r5.gesture_barrier = true;
    r5.gesture = Some(make_ingress(7, GesturePhase::Updated, Some((600, 300))));
    let zero_or_refused_lines = crate::world_engine::with_drag_preview_trace_capture(|| {
        let reply = accepted_v4(&mut engine, &r5);
        assert!(reply.drag_preview.is_none());
    });
    assert_eq!(
        zero_or_refused_lines,
        vec![
            "omnitiler:core:trace ev=drag-preview gen=gen-1 req=req-6 rev=5 obs=6 ready=true dup=false preview=none id=- domainOut=- suspendedOut=- reason=local-engine-refused"
        ]
    );

    // 6. Updated over win-b edge -> reason=preview-local, preview=present id=7 domainOut=out-a.
    let mut r6 = report_v4(
        7,
        screens.clone(),
        windows.clone(),
        "win-a",
        ingress_v4("win-a"),
    );
    r6.gesture_barrier = true;
    r6.cursor = Some((20, 300));
    r6.gesture = Some(make_ingress(7, GesturePhase::Updated, Some((20, 300))));
    let mut r6_preview = None;
    let preview_lines = crate::world_engine::with_drag_preview_trace_capture(|| {
        let reply = accepted_v4(&mut engine, &r6);
        r6_preview = reply.drag_preview.as_ref().map(|p| p.id);
    });
    assert_eq!(
        preview_lines,
        vec![
            "omnitiler:core:trace ev=drag-preview gen=gen-1 req=req-7 rev=6 obs=7 ready=true dup=false preview=present id=7 domainOut=out-a suspendedOut=- reason=preview-local"
        ]
    );
    assert_eq!(r6_preview, Some(7));

    // 7. Omitted ingress while neighbor valid -> reason=omitted-preserved.
    let r7 = report_v4(
        8,
        screens.clone(),
        windows.clone(),
        "win-a",
        ingress_v4("win-a"),
    );
    let omitted_lines = crate::world_engine::with_drag_preview_trace_capture(|| {
        let reply = accepted_v4(&mut engine, &r7);
        assert_eq!(reply.drag_preview.as_ref().map(|p| p.id), Some(7));
    });
    assert_eq!(
        omitted_lines,
        vec![
            "omnitiler:core:trace ev=drag-preview gen=gen-1 req=req-8 rev=7 obs=8 ready=true dup=false preview=present id=7 domainOut=out-a suspendedOut=- reason=omitted-preserved"
        ]
    );

    // 8. Duplicate replay of r7 -> dup=true reason=duplicate.
    let dup_lines = crate::world_engine::with_drag_preview_trace_capture(|| {
        let (outcome, _) = engine.handle_report(&r7, &[]);
        assert!(matches!(outcome, V4RunOutcome::Duplicate(_)));
    });
    assert_eq!(
        dup_lines,
        vec![
            "omnitiler:core:trace ev=drag-preview gen=gen-1 req=req-8 rev=7 obs=8 ready=true dup=true preview=present id=7 domainOut=out-a suspendedOut=- reason=duplicate"
        ]
    );

    // 9. Omitted ingress with neighbor minimized -> reason=prune-neighbor-invalid.
    let mut win_b_min = win_b.clone();
    win_b_min.minimized = Some(true);
    let r8 = report_v4(
        9,
        screens.clone(),
        vec![win_a.clone(), win_b_min],
        "win-a",
        ingress_v4("win-a"),
    );
    let prune_lines = crate::world_engine::with_drag_preview_trace_capture(|| {
        let reply = accepted_v4(&mut engine, &r8);
        assert!(reply.drag_preview.is_none());
    });
    assert_eq!(
        prune_lines,
        vec![
            "omnitiler:core:trace ev=drag-preview gen=gen-1 req=req-9 rev=8 obs=9 ready=true dup=false preview=none id=- domainOut=- suspendedOut=- reason=prune-neighbor-invalid"
        ]
    );

    // 10. Cancelled -> reason=cancelled.
    let mut r9 = report_v4(
        10,
        screens.clone(),
        windows.clone(),
        "win-a",
        ingress_v4("win-a"),
    );
    r9.gesture = Some(make_ingress(7, GesturePhase::Cancelled, None));
    let cancel_lines = crate::world_engine::with_drag_preview_trace_capture(|| {
        let reply = accepted_v4(&mut engine, &r9);
        assert!(reply.drag_preview.is_none());
    });
    assert_eq!(
        cancel_lines,
        vec![
            "omnitiler:core:trace ev=drag-preview gen=gen-1 req=req-10 rev=9 obs=10 ready=true dup=false preview=none id=- domainOut=- suspendedOut=- reason=cancelled"
        ]
    );

    // 11. Trace disabled -> zero emissions.
    let r10 = report_v4(11, screens, windows, "win-a", ingress_v4("win-a"));
    let disabled_lines = crate::world_engine::with_drag_preview_trace_capture(|| {
        crate::world_engine::with_focus_trace_disabled(|| {
            assert!(!crate::world_engine::core_trace_enabled());
            let _ = accepted_v4(&mut engine, &r10);
        });
    });
    assert!(disabled_lines.is_empty());
}
