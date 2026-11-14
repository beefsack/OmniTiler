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
    WindowFact, WorldCommand, WorldReportV4,
};
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
    engine.stage_facts_v4(&report_v4(base + 2, screens, windows, focused, ingress));
    // Domain assembly from the staged v4 facts (shared intake helper,
    // report-independent): publication reads staged domains, and the
    // v4 converge path that would own this call is later work.
    engine.build_staged_domains();
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
    let reply = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
    assert_eq!(engine.latest_published_presentation_rev_v4, 7);
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
    engine.retained.push(retained_window(
        "op-7",
        Some(7),
        "win-t1",
        "out-a",
        "out-b",
        "ws-x",
    ));
    // Commandless publication keeps the originating sequence.
    let first = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
    assert_eq!(first.transfers.len(), 1);
    assert_eq!(first.transfers[0].operation_id, "op-7");
    assert_eq!(first.transfers[0].command_seq, Some(7));
    assert!(!first.transfers[0].held);
    // An unrelated command alongside never rewrites the retained sequence.
    let unrelated = WorldCommand::Focus {
        window: WindowId("win-a".to_owned()),
        direction: "left".to_owned(),
    };
    let second = engine.publish_v4(
        envelope(8),
        Some(&unrelated),
        Vec::new(),
        None,
        &BTreeSet::new(),
    );
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
    engine.retained.push(retained_window(
        "op-999",
        Some(7),
        "win-t1",
        "out-a",
        "out-b",
        "ws-x",
    ));
    // Opaque id spelling "op-12" with no retained sequence stays null.
    engine.retained.push(retained_window(
        "op-12", None, "win-t2", "out-b", "out-b", "ws-y",
    ));
    let reply = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
    engine.staged.diagnostics = staged.clone();
    let reply = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
    engine.gesture_terminal = Some(GestureSettled {
        id: 9,
        committed: true,
    });
    engine.operation_settled.insert(
        (WindowId("win-a".to_owned()), OperationKind::Float),
        OperationEvent {
            window: WindowId("win-a".to_owned()),
            kind: OperationKind::Float,
            operation_id: "op-1".to_owned(),
            outcome: OperationOutcome::FloatFrameProven,
            cause: None,
        },
    );
    engine.switch_settled.insert(
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
    let first = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
    assert_eq!(
        first.gesture_settled,
        Some(GestureSettled {
            id: 9,
            committed: true
        })
    );
    assert_eq!(first.operation_settled.len(), 1);
    assert_eq!(first.switch_settled.len(), 1);
    let second = engine.publish_v4(envelope(8), None, Vec::new(), None, &BTreeSet::new());
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
    assert_eq!(engine.latest_published_presentation_rev_v4, 2);
    let before = engine.staged.presentation_v4.clone();
    assert!(matches!(before, Some(PresentationRowV4::Valid(_))));
    let first = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
    assert_eq!(first.presentation.rev, 7);
    assert_eq!(first.state_revision, 7);
    assert_eq!(engine.latest_published_presentation_rev_v4, 7);
    // The staged producer row is never rewritten by publication.
    assert_eq!(engine.staged.presentation_v4, before);
    let second = engine.publish_v4(envelope(9), None, Vec::new(), None, &BTreeSet::new());
    assert_eq!(second.presentation.rev, 9);
    assert_eq!(engine.latest_published_presentation_rev_v4, 9);
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
            .presentation_live_v4
            .contains_key(&WindowId("win-b".to_owned()))
    );
    let order_before = engine.presentation_order_v4.clone();
    let live_before = engine.presentation_live_v4.clone();
    let focused_before = engine.presentation_focused_v4.clone();
    let _ = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
    assert_eq!(engine.presentation_order_v4, order_before);
    assert_eq!(engine.presentation_live_v4, live_before);
    assert_eq!(engine.presentation_focused_v4, focused_before);
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
        screens,
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let (held_output, _) = engine
        .presentation_style_hold_v4()
        .expect("past-cap style holds");
    assert_eq!(held_output, OutputId("out-a".to_owned()));
    engine.retained.push(retained_window(
        "op-t1",
        Some(3),
        "win-t1",
        "out-a",
        "out-b",
        "ws-x",
    ));
    engine.retained.push(retained_window(
        "op-t2", None, "win-t2", "out-b", "out-b", "ws-y",
    ));
    let reply = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
        .mapping
        .insert("out-a".to_owned(), vec!["ws-9".to_owned()]);
    engine
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
    engine.stage_facts_v4(&staged_report);
    engine.build_staged_domains();
    // Backing transfers keep both creations past the prune lane.
    engine.retained.push(retained_window(
        "op-back-a",
        None,
        "win-x1",
        "out-a",
        "out-a",
        "ws-9",
    ));
    engine.retained.push(retained_window(
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
        engine.retained_creations.push(RetainedNativeCreation {
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
        .native_creation_desires()
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
    let reply = engine.publish_v4(
        PublishEnvelopeV4 {
            adapter_incarnation: "adapter-1".to_owned(),
            request_id: "req-5".to_owned(),
            observation_seq: 1,
            command_consumed: false,
            issuing_revision: 5,
        },
        None,
        Vec::new(),
        None,
        &BTreeSet::new(),
    );
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
    assert_eq!(engine.retained_creations.len(), 2);
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
        .staged_homing(&WindowId("win-a".to_owned()))
        .expect("win-a home");
    let home_c = engine
        .staged_homing(&WindowId("win-c".to_owned()))
        .expect("win-c home");
    assert_eq!(home_a.output, OutputId("out-a".to_owned()));
    assert_eq!(home_c.output, OutputId("out-b".to_owned()));
    engine
        .retained_born
        .push(crate::world_engine::RetainedBornClear {
            operation_id: "op-born-a".to_owned(),
            window: WindowId("win-a".to_owned()),
            home: home_a,
            pending: None,
            ambiguous: false,
        });
    engine
        .retained_born
        .push(crate::world_engine::RetainedBornClear {
            operation_id: "op-born-c".to_owned(),
            window: WindowId("win-c".to_owned()),
            home: home_c,
            pending: None,
            ambiguous: false,
        });
    let reply = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
    let reply = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
    engine.retained.push(retained_window(
        "op-t1",
        Some(3),
        "win-t",
        "out-a",
        "out-b",
        "ws-x",
    ));
    assert_eq!(engine.staged_homing(&WindowId("win-t".to_owned())), None);
    let reply = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
    engine.retained.push(retained_window(
        "op-t2",
        Some(4),
        "win-u",
        "out-b",
        "out-a",
        "ws-ghost",
    ));
    assert_eq!(engine.staged_homing(&WindowId("win-u".to_owned())), None);
    let reply = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
        .staged_homing(&WindowId("win-a".to_owned()))
        .expect("win-a home");
    let home_c = engine
        .staged_homing(&WindowId("win-c".to_owned()))
        .expect("win-c home");
    let before_a = engine.canonical_memory(&home_a);
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
    engine.note_focus_decision(
        &WindowId("win-c".to_owned()),
        crate::world_report::FocusRequestCause::AutomaticAdmission,
        None,
        7,
        "win-a",
    );
    let reply = engine.publish_v4(envelope(7), None, effects, None, &BTreeSet::new());
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {:?}",
        reply.effects
    );
    // No native effect kind remains: every carrier strips, so the held
    // output keeps nothing and the clean output carries nothing either.
    assert_eq!(
        engine.canonical_memory(&home_c),
        Some(WindowId("win-c".to_owned()))
    );
    assert_eq!(engine.canonical_memory(&home_a), before_a);
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
    assert!(clean.presentation_style_hold_v4().is_none());
    let reply = clean.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
        held.presentation_style_hold_v4().map(|(output, _)| output),
        Some(OutputId("out-a".to_owned()))
    );
    let reply = held.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
    engine.staged.presentation_v4 = Some(PresentationRowV4::Held(
        crate::world_report::HeldPresentationRow {
            fields: Vec::new(),
            kind: RowDiagnosticKind::InvalidFact,
            diagnostic: None,
        },
    ));
    assert!(engine.presentation_style_hold_v4().is_none());
    let reply = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
    let ring_b = engine.mapping.get("out-a").cloned().expect("out-a ring");
    let homed_ws: std::collections::BTreeSet<String> = engine
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
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: target_ws.clone(),
        follow: false,
    };
    let applied = engine.execute_command(
        &publish_test_issue(),
        11,
        &cmd,
        &BTreeSet::new(),
        None,
        None,
    );
    match applied {
        crate::world_commands::CommandApplied::Effect(_)
        | crate::world_commands::CommandApplied::NoEffect => {}
        crate::world_commands::CommandApplied::Rejected { kind, message } => {
            panic!("genuine send rejected {kind} {message}")
        }
    }
    let retained = engine
        .retained
        .iter()
        .find(|retained| matches!(&retained.target.kind, TransferKind::Window(w) if w.0 == "win-a"))
        .expect("retained send");
    assert_eq!(retained.command_seq, Some(11));
    let reply = engine.publish_v4(envelope(12), None, Vec::new(), None, &BTreeSet::new());
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
    let ring = engine.mapping.get("out-b").cloned().expect("out-b ring");
    let other = ring
        .iter()
        .find(|id| Some(*id) != engine.views.get("out-b"))
        .cloned()
        .unwrap_or_else(|| ring[0].clone());
    let cmd = WorldCommand::SelectWorkspace {
        output: OutputId("out-b".to_owned()),
        workspace: other.clone(),
    };
    let _ = engine.execute_command(
        &publish_test_issue(),
        13,
        &cmd,
        &BTreeSet::new(),
        None,
        None,
    );
    let retained = engine
        .retained
        .iter()
        .find(|retained| {
            matches!(&retained.target.kind, TransferKind::View)
                && retained.target.target_output == OutputId("out-b".to_owned())
        })
        .expect("retained view");
    assert_eq!(retained.command_seq, Some(13));
    let reply = engine.publish_v4(envelope(14), None, Vec::new(), None, &BTreeSet::new());
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
        workspace: "ws-1".to_owned(),
    };
    match engine.execute_command(
        &publish_test_issue(),
        15,
        &cmd,
        &BTreeSet::new(),
        None,
        None,
    ) {
        crate::world_commands::CommandApplied::Effect(_)
        | crate::world_commands::CommandApplied::NoEffect => {}
        crate::world_commands::CommandApplied::Rejected { kind, message } => {
            panic!("genuine migrate rejected {kind} {message}")
        }
    }
    let retained = engine
        .retained
        .iter()
        .find(|retained| {
            matches!(&retained.target.kind, TransferKind::Migration)
                && retained.target.target_workspace == "ws-1"
        })
        .expect("retained migration parent");
    assert_eq!(retained.command_seq, Some(15));
    let reply = engine.publish_v4(envelope(16), None, Vec::new(), None, &BTreeSet::new());
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
        .mapping
        .get("out-b")
        .and_then(|ring| ring.first().cloned())
        .expect("out-b ring");
    let (transfer, _) = engine
        .execute_output_send(
            &publish_test_issue(),
            "gesture-win-a-99",
            &WindowId("win-a".to_owned()),
            &OutputId("out-b".to_owned()),
            &target_ws,
            true,
            &by_id,
            &BTreeSet::new(),
        )
        .expect("gesture send");
    assert_eq!(transfer.command_seq, None);
    engine.retained.push(transfer);
    let reply = engine.publish_v4(envelope(15), None, Vec::new(), None, &BTreeSet::new());
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
    assert_eq!(engine.presentation_for_publish_v4().rev, 0);
    // Genuine v4 seed admissions stamp the latest rev to base+1 (see above).
    assert_eq!(engine.latest_published_presentation_rev_v4, 2);
    let _ = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
    assert_eq!(engine.latest_published_presentation_rev_v4, 7);
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
        .staged_homing(&WindowId("win-a".to_owned()))
        .expect("win-a home");
    // Genuine publication helper only: legacy seeding above, no v4
    // accepted-run claim. Mark the staged home floating after seeding.
    engine.floating_scopes.insert(home.clone());
    engine.retained.push(retained_window(
        "op-float-hold",
        Some(3),
        "win-a",
        "out-b",
        &home.output.0,
        &home.workspace.0,
    ));
    let reply = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
    assert!(engine.presentation_style_hold_v4().is_none());
    let home = engine
        .staged_homing(&WindowId("win-a".to_owned()))
        .expect("win-a home");
    engine.floating_scopes.insert(home.clone());
    engine.retained.push(retained_window(
        "op-float-clean",
        Some(3),
        "win-a",
        "out-b",
        &home.output.0,
        &home.workspace.0,
    ));
    let reply = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
        .staged_homing(&WindowId("win-m".to_owned()))
        .expect("win-m home");
    assert_eq!(source.output, OutputId("out-a".to_owned()));
    let clean_ws = engine
        .staged_homing(&WindowId("win-c".to_owned()))
        .expect("win-c home")
        .workspace
        .0
        .clone();
    engine.retained.push(retained_window(
        "op-min-move",
        Some(5),
        "win-m",
        "out-a",
        "out-b",
        &clean_ws,
    ));
    let reply = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
            .staged_homing(&WindowId("win-m".to_owned()))
            .is_some()
    );
    engine.retained.push(retained_window(
        "op-min-ghost",
        Some(6),
        "win-m",
        "out-a",
        "out-b",
        "ws-ghost",
    ));
    let reply = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
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
    assert_eq!(engine.latest_published_presentation_rev_v4, 2);
    let mut clone = engine.clone();
    let reply = clone.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
    assert_eq!(reply.state_revision, 7);
    assert_eq!(clone.latest_published_presentation_rev_v4, 7);
    assert_eq!(
        engine.latest_published_presentation_rev_v4, 2,
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
    engine.emit_operation(
        "op-1".to_owned(),
        WindowId("win-a".to_owned()),
        OperationOutcome::ClearContinuedFloat,
        Some("clear-proof"),
    );
    engine.emit_operation(
        "op-1".to_owned(),
        WindowId("win-a".to_owned()),
        OperationOutcome::FloatFrameProven,
        None,
    );
    let r1 = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
    assert_eq!(r1.operation_settled.len(), 2);
    assert_eq!(r1.operation_settled[0].kind, OperationKind::Clear);
    assert_eq!(r1.operation_settled[1].kind, OperationKind::Float);
    assert_eq!(r1.operation_settled[0].operation_id, "op-1");
    assert_eq!(r1.operation_settled[1].operation_id, "op-1");

    // Q1 A: pending clear/float on win-a does NOT clear existing outcomes.
    engine
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
    let r2 = engine.publish_v4(envelope(8), None, Vec::new(), None, &BTreeSet::new());
    assert_eq!(r2.operation_settled.len(), 2);

    // Q2 A: explicit close drops owner BEFORE publish_v4 projects and emits trace.
    engine
        .staged
        .observation
        .closed
        .push(WindowId("win-a".to_owned()));
    let lines = crate::world_engine::with_focus_trace_capture(|| {
        let r3 = engine.publish_v4(envelope(9), None, Vec::new(), None, &BTreeSet::new());
        assert!(r3.operation_settled.is_empty());
    });
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("ev=operation-settled-dropped"));
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
    engine.switch_settled.insert(
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
    let r1 = engine.publish_v4(envelope(7), None, Vec::new(), None, &BTreeSet::new());
    assert_eq!(r1.switch_settled.len(), 1);
    assert_eq!(r1.switch_settled[0].output, OutputId("out-b".to_owned()));

    // Remove out-b from staged screens under complete inventory -> pruned + traced.
    engine.staged.observation.inventory_complete = true;
    engine
        .staged
        .screens
        .retain(|screen| screen.output != OutputId("out-b".to_owned()));
    let lines = crate::world_engine::with_focus_trace_capture(|| {
        let r2 = engine.publish_v4(envelope(8), None, Vec::new(), None, &BTreeSet::new());
        assert!(r2.switch_settled.is_empty());
    });
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("ev=switch-settled-dropped"));
    assert!(lines[0].contains("out=out-b"));
    assert!(lines[0].contains("outcome=completed-with-failures"));
    assert!(lines[0].contains("fallback=false"));
    assert!(lines[0].contains("failed=win-c"));
}

#[test]
fn gesture_terminal_guards_stale_replays_and_preserves_first_unbound() {
    let mut engine = engine_with(Vec::new());
    // Genuine first unbound terminal records.
    engine.record_gesture_terminal(5, true);
    assert_eq!(
        engine.gesture_terminal,
        Some(GestureSettled {
            id: 5,
            committed: true,
        })
    );
    // Same-id or older-id replay (e.g. Esc) cannot overwrite committed=true or regress id.
    engine.record_gesture_terminal(5, false);
    engine.record_gesture_terminal(4, false);
    engine.record_gesture_terminal(0, false);
    assert_eq!(
        engine.gesture_terminal,
        Some(GestureSettled {
            id: 5,
            committed: true,
        })
    );
    // Newer terminal replaces.
    engine.record_gesture_terminal(6, false);
    assert_eq!(
        engine.gesture_terminal,
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
        engine.emit_operation(
            format!("op-c-{i}"),
            win.clone(),
            OperationOutcome::ClearContinuedTiled,
            None,
        );
        engine.emit_operation(
            format!("op-f-{i}"),
            win,
            OperationOutcome::FloatFrameProven,
            None,
        );
    }
    assert_eq!(
        engine.operation_settle_slot_count(None, &[]),
        crate::world_report::MAX_REPLY_OPERATIONS
    );
    // Replacing an existing slot does not exceed 1024.
    let existing_cmd = WorldCommand::Operation(crate::boundary::CoreCommand::ToggleFloat {
        window: "w-0".to_owned(),
        float_rect: None,
    });
    assert_eq!(
        engine.operation_settle_slot_count(Some(&existing_cmd), &[]),
        crate::world_report::MAX_REPLY_OPERATIONS
    );
    // Adding a new window exceeds 1024.
    let new_cmd = WorldCommand::Operation(crate::boundary::CoreCommand::ToggleFloat {
        window: "w-new".to_owned(),
        float_rect: None,
    });
    assert_eq!(
        engine.operation_settle_slot_count(Some(&new_cmd), &[]),
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
        engine.emit_operation(
            format!("op-c-{i}"),
            win.clone(),
            OperationOutcome::ClearContinuedTiled,
            None,
        );
        engine.emit_operation(
            format!("op-f-{i}"),
            win,
            OperationOutcome::FloatFrameProven,
            None,
        );
    }
    assert_eq!(
        engine.operation_settle_slot_count(None, &[]),
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
                window: "w-new".to_owned(),
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
    assert_eq!(engine.retained_born.len(), 1, "genuine born-clear retained");

    // Fill the remaining 1023 operation slots: 511 * 2 + 1 = 1023.
    for i in 0..511 {
        let win = WindowId(format!("w-{i}"));
        engine.emit_operation(
            format!("op-c-{i}"),
            win.clone(),
            OperationOutcome::ClearContinuedTiled,
            None,
        );
        engine.emit_operation(
            format!("op-f-{i}"),
            win,
            OperationOutcome::FloatFrameProven,
            None,
        );
    }
    engine.emit_operation(
        "op-c-extra".to_owned(),
        WindowId("w-extra".to_owned()),
        OperationOutcome::ClearContinuedTiled,
        None,
    );
    assert_eq!(
        engine.operation_settle_slot_count(None, &[]),
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
                window: "w-new".to_owned(),
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
        engine.emit_operation(
            format!("op-c-{i}"),
            win.clone(),
            OperationOutcome::ClearContinuedTiled,
            None,
        );
        engine.emit_operation(
            format!("op-f-{i}"),
            win,
            OperationOutcome::FloatFrameProven,
            None,
        );
    }
    let rev_before = engine.state_revision;

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
            fields: Vec::new(),
            diagnostic: None,
            kind: RowDiagnosticKind::InvalidFact,
        })
        .collect();

    // Precondition / nonvacuity: with 1 free slot (1023 occupied), the same
    // report is accepted, preserves all 512 owners, and mints RetainedBornClear.
    let mut under_cap = engine.clone();
    let removed = under_cap.operation_settled.pop_last();
    assert!(removed.is_some());
    assert_eq!(
        under_cap.operation_settle_slot_count(None, &[]),
        crate::world_report::MAX_REPLY_OPERATIONS - 1
    );
    let (under_outcome, _) = under_cap.handle_report(&over_cap, &[]);
    assert!(
        matches!(under_outcome, V4RunOutcome::Accepted(_)),
        "expected acceptance at 1023 slots when born-clear mints 1024th slot, got {under_outcome:?}"
    );
    assert_eq!(under_cap.retained_born.len(), 1);
    assert_eq!(
        under_cap.retained_born[0].window,
        WindowId("born-over-cap".to_owned())
    );
    assert_eq!(
        under_cap.operation_settle_slot_count(None, &[]),
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
    assert!(engine.retained_born.is_empty(), "candidate discarded");
    assert_eq!(
        engine.operation_settle_slot_count(None, &[]),
        crate::world_report::MAX_REPLY_OPERATIONS
    );
    assert_eq!(engine.state_revision, rev_before, "revision unchanged");

    // Subsequent empty report still succeeds (engine is not wedged).
    let followup = report_full_v4(2, 2, scr, Vec::new(), Vec::new(), "", None, None);
    let _ = accepted_v4(&mut engine, &followup);
}

#[test]
fn staged_pending_operation_future_budget_rejects_over_4mib_transactionally() {
    use crate::bounds::is_opaque_id;
    use crate::directional::WorkspaceId;
    use crate::session::DomainKey;
    use crate::world_engine::RetainedBornClear;
    use crate::world_report::{
        MAX_INTENT_DESIRED, MAX_REPLY_DIAGNOSTICS, MAX_REPLY_NATIVE_CREATIONS,
        MAX_REPLY_OPERATIONS, MAX_REPLY_OUTPUTS, MAX_REPLY_SCOPES, MAX_REPLY_SWITCH_EVENTS,
        MAX_REPLY_TRANSFERS, MAX_REPLY_WINDOWS, MAX_REPLY_WORKSPACES, MAX_STATE_TARGETS_PER_WINDOW,
        StateProperty, StateTarget, WORLD_MAX_REPLY_BYTES, WindowDesire,
        validate_focus_request_reason, validate_intent_desired_reason,
    };
    use crate::world_report_v4_checks::report_digest_v4;
    use crate::world_run_v4::V4RunOutcome;
    use crate::world_sizing::operation_settled_estimate;
    use crate::world_sizing_v4::{reply_estimate_v4, reply_exact_bytes_v4};

    let mut engine = engine_with(Vec::new());
    let scr = vec![screen("DP-1", 0)];
    let startup = report_full_v4(1, 1, scr.clone(), Vec::new(), Vec::new(), "", None, None);
    let base_reply = accepted_v4(&mut engine, &startup);
    let rev_before = engine.state_revision;

    // Build a cap-valid reply just under 4 MiB (paired with the protocol
    // `encode_reply_v4` test in `crates/tiler-protocol/tests/reply_v4_decode.rs`),
    // with one existing settled operation slot (covering same-key future
    // replacement headroom). Without new born obligations it finalizes
    // cleanly; adding 30 cap-valid RetainedBornClear obligations pushes both
    // estimated and exact sizes + future reserve over WORLD_MAX_REPLY_BYTES.
    let mut staged_without_born = engine.clone();
    staged_without_born.state_revision += 1;
    staged_without_born.emit_operation(
        "op-0".to_owned(),
        WindowId("w-0".to_owned()),
        OperationOutcome::ClearContinuedTiled,
        None,
    );

    let mut reply = base_reply.clone();
    reply.operation_settled = staged_without_born
        .operation_settled
        .values()
        .cloned()
        .collect();
    // Pad `windows` in O(N) using cap-valid WindowDesire rows (opaque IDs
    // <= 128 bytes, 4 state targets <= MAX_STATE_TARGETS_PER_WINDOW, total
    // rows <= MAX_REPLY_WINDOWS = 10_000).
    let pad_win = |i: usize| WindowDesire {
        lifetime: WindowId(format!("w-{i:05}-{}", "x".repeat(120))),
        output: OutputId(format!("o-{}", "y".repeat(126))),
        workspace: format!("ws-{}", "z".repeat(125)),
        allocation: Some(Rect {
            x: 100,
            y: 100,
            w: 1000,
            h: 1000,
        }),
        effective_frame: Some(Rect {
            x: 100,
            y: 100,
            w: 1000,
            h: 1000,
        }),
        frame_authority: ScopeAuthority::Controlled,
        membership_authority: ScopeAuthority::Controlled,
        floating: false,
        float_origin: None,
        frame_operation: Some(format!("op-f-{i:05}-{}", "f".repeat(115))),
        clear_operation: Some(format!("op-c-{i:05}-{}", "c".repeat(115))),
        command_seq: None,
        visibility: None,
        visibility_authority: ScopeAuthority::Untouched,
        state: vec![
            StateTarget {
                property: StateProperty::Fullscreen,
                value: false,
                authority: ScopeAuthority::Untouched,
                command_seq: None,
            },
            StateTarget {
                property: StateProperty::Maximized,
                value: false,
                authority: ScopeAuthority::Untouched,
                command_seq: None,
            },
            StateTarget {
                property: StateProperty::Sticky,
                value: false,
                authority: ScopeAuthority::Untouched,
                command_seq: None,
            },
            StateTarget {
                property: StateProperty::Above,
                value: false,
                authority: ScopeAuthority::Untouched,
                command_seq: None,
            },
        ],
    };
    reply.windows.push(pad_win(0));
    let after_one = reply_estimate_v4(&reply).max(reply_exact_bytes_v4(&reply));
    reply.windows.push(pad_win(1));
    let after_two = reply_estimate_v4(&reply).max(reply_exact_bytes_v4(&reply));
    let row_step = after_two - after_one;
    let target_bytes = WORLD_MAX_REPLY_BYTES - 500;
    let extra = target_bytes.saturating_sub(after_two) / row_step;
    reply.windows.extend((2..2 + extra).map(pad_win));

    assert!(reply.scopes.len() <= MAX_REPLY_SCOPES);
    assert!(reply.workspaces.len() <= MAX_REPLY_WORKSPACES);
    assert!(reply.outputs.len() <= MAX_REPLY_OUTPUTS);
    assert!(reply.windows.len() <= MAX_REPLY_WINDOWS);
    assert!(reply.transfers.len() <= MAX_REPLY_TRANSFERS);
    assert!(reply.effects.is_empty());
    assert!(reply.operation_settled.len() <= MAX_REPLY_OPERATIONS);
    assert!(reply.native_creations.len() <= MAX_REPLY_NATIVE_CREATIONS);
    assert!(reply.intent_desired.len() <= MAX_INTENT_DESIRED);
    assert!(reply.switch_settled.len() <= MAX_REPLY_SWITCH_EVENTS);
    assert!(reply.diagnostics.len() <= MAX_REPLY_DIAGNOSTICS);
    assert!(reply.windows.iter().all(|w| {
        is_opaque_id(&w.lifetime.0)
            && is_opaque_id(&w.output.0)
            && is_opaque_id(&w.workspace)
            && w.frame_operation.as_deref().is_none_or(is_opaque_id)
            && w.clear_operation.as_deref().is_none_or(is_opaque_id)
            && w.state.len() <= MAX_STATE_TARGETS_PER_WINDOW
    }));
    assert!(validate_intent_desired_reason(&reply.intent_desired).is_none());
    assert!(validate_focus_request_reason(&reply.focus_request).is_none());
    assert!(reply_estimate_v4(&reply) <= WORLD_MAX_REPLY_BYTES);
    assert!(reply_exact_bytes_v4(&reply) <= WORLD_MAX_REPLY_BYTES);

    // Nonvacuity: without RetainedBornClear, the same-key operation reserve fits
    // under 4 MiB and finalization accepts.
    let no_born_delta = staged_without_born
        .operation_settle_reserve(None, &[])
        .saturating_sub(operation_settled_estimate(&reply.operation_settled));
    assert!(
        reply_estimate_v4(&reply).saturating_add(no_born_delta) <= WORLD_MAX_REPLY_BYTES
            && reply_exact_bytes_v4(&reply).saturating_add(no_born_delta) <= WORLD_MAX_REPLY_BYTES,
        "without born obligations, reply + same-key reserve must fit under 4 MiB"
    );
    let next_rep = report_full_v4(2, 2, scr, Vec::new(), Vec::new(), "", None, None);
    let digest = report_digest_v4(&next_rep);
    let mut ok_engine = engine.clone();
    let ok_outcome = ok_engine.finalize_v4_candidate(
        staged_without_born.clone(),
        &next_rep,
        digest,
        reply.clone(),
    );
    assert!(
        matches!(ok_outcome, V4RunOutcome::Accepted(_)),
        "expected acceptance without born obligations, got {ok_outcome:?}"
    );

    // Adding 30 cap-valid RetainedBornClear obligations pushes future reserve
    // past 4 MiB and rejects transactionally.
    let mut staged_with_born = staged_without_born;
    staged_with_born
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
    assert!(staged_with_born.operation_settle_slot_count(None, &[]) <= MAX_REPLY_OPERATIONS);
    let future_op_delta = staged_with_born
        .operation_settle_reserve(None, &[])
        .saturating_sub(operation_settled_estimate(&reply.operation_settled));
    assert!(future_op_delta > no_born_delta);
    assert!(
        reply_estimate_v4(&reply).saturating_add(future_op_delta) > WORLD_MAX_REPLY_BYTES
            && reply_exact_bytes_v4(&reply).saturating_add(future_op_delta) > WORLD_MAX_REPLY_BYTES,
        "future_op_delta={future_op_delta} must push both estimate ({}) and exact ({}) over 4 MiB",
        reply_estimate_v4(&reply),
        reply_exact_bytes_v4(&reply),
    );

    let outcome = engine.finalize_v4_candidate(staged_with_born, &next_rep, digest, reply);
    assert!(
        matches!(
            outcome,
            V4RunOutcome::Rejected {
                message: crate::world_engine::MSG_REPLY_SIZE,
                ..
            }
        ),
        "expected reply-too-large when future born-clear reserve exceeds 4 MiB, got {outcome:?}"
    );
    assert_eq!(engine.state_revision, rev_before, "revision unchanged");
    assert!(engine.retained_born.is_empty(), "staged not adopted");
}

#[test]
fn toggle_maximize_and_toggle_sticky_reserve_float_slots_explicit_and_implicit() {
    let mut engine = engine_with(Vec::new());
    engine.emit_operation(
        "op-existing".to_owned(),
        WindowId("w-existing".to_owned()),
        OperationOutcome::FloatFrameProven,
        None,
    );
    assert_eq!(engine.operation_settle_slot_count(None, &[]), 1);

    // Explicit ToggleMaximize / ToggleSticky on an existing Float slot do not add a slot.
    let max_existing = WorldCommand::ToggleMaximize {
        window: WindowId("w-existing".to_owned()),
    };
    let sticky_existing = WorldCommand::ToggleSticky {
        window: WindowId("w-existing".to_owned()),
    };
    assert_eq!(
        engine.operation_settle_slot_count(Some(&max_existing), &[]),
        1
    );
    assert_eq!(
        engine.operation_settle_slot_count(Some(&sticky_existing), &[]),
        1
    );

    // Explicit ToggleMaximize / ToggleSticky on new windows reserve 1 Float slot each.
    let max_new = WorldCommand::ToggleMaximize {
        window: WindowId("w-max".to_owned()),
    };
    let sticky_new = WorldCommand::ToggleSticky {
        window: WindowId("w-sticky".to_owned()),
    };
    assert_eq!(
        engine.operation_settle_slot_count(Some(&max_new), &[(1, sticky_new)]),
        3
    );

    // Implicit variants conservatively reserve 1 slot each.
    let imp_max = WorldCommand::Implicit(Box::new(max_new));
    let imp_sticky = WorldCommand::Implicit(Box::new(WorldCommand::ToggleSticky {
        window: WindowId("w-sticky".to_owned()),
    }));
    assert_eq!(
        engine.operation_settle_slot_count(Some(&imp_max), &[(1, imp_sticky)]),
        3
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
        engine.emit_operation(
            format!("op-c-{i}"),
            win.clone(),
            OperationOutcome::ClearContinuedTiled,
            None,
        );
        engine.emit_operation(
            format!("op-f-{i}"),
            win,
            OperationOutcome::FloatFrameProven,
            None,
        );
    }
    assert_eq!(
        engine.operation_settle_slot_count(None, &[]),
        crate::world_report::MAX_REPLY_OPERATIONS
    );

    for (seq, cmd) in [
        (
            2,
            WorldCommand::ToggleMaximize {
                window: WindowId("w-new".to_owned()),
            },
        ),
        (
            3,
            WorldCommand::ToggleSticky {
                window: WindowId("w-new".to_owned()),
            },
        ),
        (
            4,
            WorldCommand::Implicit(Box::new(WorldCommand::ToggleMaximize {
                window: WindowId("w-new".to_owned()),
            })),
        ),
        (
            5,
            WorldCommand::Implicit(Box::new(WorldCommand::ToggleSticky {
                window: WindowId("w-new".to_owned()),
            })),
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
                window: WindowId("w-new".to_owned()),
            },
        ),
        (
            3,
            WorldCommand::ToggleSticky {
                window: WindowId("w-new".to_owned()),
            },
        ),
    ] {
        let mut case_engine = engine.clone();
        assert_eq!(
            case_engine.operation_settle_slot_count(None, &[]),
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
