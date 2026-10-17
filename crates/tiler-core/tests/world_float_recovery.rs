//! Explicit retained float-placement recovery: a `ToggleFloat` command
//! retains its exact target under the stable command operation identity
//! (`op-<cmdSeq>`) and publishes it as an explicit row token until
//! observation confirms it, proven feedback settles it, or a newer intent
//! replaces it. A lost one-shot result reply therefore never loses the
//! placement: the next unrelated snapshot still carries the token.
//!
//! Typed-only and portable like the other world contract repairs.

use tiler_core::boundary::CoreCommand;
use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    AttemptFeedback, AttemptKind, AttemptOutcome, DomainReport, OutputTopology,
    WORLD_CONTRACT_VERSION, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn win(id: &str, x: i32) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId("out".to_owned()),
        workspace: WorkspaceId("ws1".to_owned()),
        rect: rect(x, 0, 200, 200),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn base_report(request_seq: u64, observation_seq: u64) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq: None,
        command: None,
        workspace_mode: WorkspaceMode::PerOutputLocal,
        topology: vec![OutputTopology {
            output: OutputId("out".to_owned()),
            native_order: vec!["ws1".to_owned()],
            current_native: "ws1".to_owned(),
            complete: true,
            geometry: None,
            primary: false,
        }],
        bindings: Vec::new(),
        domains: vec![DomainReport {
            domain: OutputDomain {
                id: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws1".to_owned()),
                bounds: rect(0, 0, 800, 600),
                gap: 8,
                adjacent: Default::default(),
            },
            outer_gap: 4,
            focused: WindowId("win-a".to_owned()),
            windows: vec![win("win-a", 0), win("win-b", 210)],
            closed: Vec::new(),
            readable: true,
            hold: None,
        }],
        minimized: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        default_tiled: true,
        focused_global: String::new(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: Vec::new(),
    }
}

fn with_toggle_float(report: &mut WorldReport, request_seq: u64, command_seq: u64, target: Rect) {
    report.request_seq = request_seq;
    report.request_id = format!("req-{request_seq}");
    report.command_seq = Some(command_seq);
    report.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: "win-a".to_owned(),
        float_rect: Some(target),
    }));
}

fn accepted(
    engine: &mut WorldEngine,
    report: &WorldReport,
) -> tiler_core::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn float_row(
    reply: &tiler_core::world_report::WorldReply,
) -> &tiler_core::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .expect("float row for win-a")
}

const TARGET: Rect = Rect {
    x: 160,
    y: 120,
    w: 480,
    h: 360,
};

#[test]
fn toggle_float_retains_explicit_token_with_exact_target() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &base_report(1, 1));
    let mut command = base_report(2, 2);
    with_toggle_float(&mut command, 2, 1, TARGET);
    let reply = accepted(&mut engine, &command);
    // One-shot provenance plus the explicit retained row token under the
    // stable command identity.
    match &reply.command_result {
        Some(tiler_core::world_report::CommandResult::Float { window, rect }) => {
            assert_eq!(window.0, "win-a");
            assert_eq!(*rect, Some(TARGET));
        }
        other => panic!("expected float result, got {other:?}"),
    }
    let row = float_row(&reply);
    assert_eq!(row.effective_frame, Some(TARGET));
    assert_eq!(
        row.frame_authority,
        tiler_core::world_report::ScopeAuthority::Untouched
    );
    assert_eq!(row.frame_operation.as_deref(), Some("op-1"));
    // The ordinary float next to it carries no token and takes no writes.
    let plain = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-b")
        .expect("tiled row for win-b");
    assert_eq!(plain.frame_operation, None);
}

#[test]
fn lost_result_reply_recovers_through_retained_token() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &base_report(1, 1));
    let mut command = base_report(2, 2);
    with_toggle_float(&mut command, 2, 1, TARGET);
    let first = accepted(&mut engine, &command);
    assert!(first.command_result.is_some());
    // The reply carrying the one-shot result is dropped before any native
    // dispatch. The next unrelated observation-only snapshot still shows
    // the old frame with no feedback.
    let mut next = base_report(3, 3);
    next.request_seq = 3;
    next.request_id = "req-3".to_owned();
    let reply = accepted(&mut engine, &next);
    assert_eq!(reply.command_result, None);
    let row = float_row(&reply);
    assert_eq!(row.effective_frame, Some(TARGET));
    assert_eq!(row.frame_operation.as_deref(), Some("op-1"));
}

#[test]
fn stale_sequences_never_resurrect_or_duplicate_float_intent() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &base_report(1, 1));
    let mut command = base_report(2, 2);
    with_toggle_float(&mut command, 2, 1, TARGET);
    accepted(&mut engine, &command);
    // Identical latest input duplicates with no new revision.
    match engine.handle_report(&command) {
        WorldOutcome::Duplicate(reply) => {
            assert_eq!(float_row(&reply).frame_operation.as_deref(), Some("op-1"));
        }
        other => panic!("expected duplicate, got {other:?}"),
    }
    // Older request sequences are boundedly superseded, never executed.
    let mut stale = base_report(1, 9);
    stale.request_seq = 1;
    stale.request_id = "req-stale".to_owned();
    match engine.handle_report(&stale) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "request-superseded"),
        other => panic!("expected superseded, got {other:?}"),
    }
    // Repeating the consumed command sequence reconciles only: the
    // token is retained from before, never re-executed.
    let mut replay = base_report(3, 3);
    with_toggle_float(&mut replay, 3, 1, TARGET);
    match engine.handle_report(&replay) {
        WorldOutcome::CommandConsumed(reply) => {
            assert_eq!(reply.command_result, None);
            assert_eq!(float_row(&reply).frame_operation.as_deref(), Some("op-1"));
        }
        other => panic!("expected consumed, got {other:?}"),
    }
}

#[test]
fn possibly_executed_holds_target_without_reactuation() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &base_report(1, 1));
    let mut command = base_report(2, 2);
    with_toggle_float(&mut command, 2, 1, TARGET);
    accepted(&mut engine, &command);
    // Ambiguous attempt (e.g. the setter threw): the target holds without
    // any new actuation state, and the next snapshot still carries it.
    let mut ambiguous = base_report(3, 3);
    ambiguous.feedback = vec![AttemptFeedback {
        operation_id: "op-1".to_owned(),
        kind: AttemptKind::Property,
        outcome: AttemptOutcome::PossiblyExecuted,
        hold: None,
    }];
    let reply = accepted(&mut engine, &ambiguous);
    let row = float_row(&reply);
    assert_eq!(row.frame_operation.as_deref(), Some("op-1"));
    assert_eq!(row.effective_frame, Some(TARGET));
}

#[test]
fn observation_confirmation_settles_token() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &base_report(1, 1));
    let mut command = base_report(2, 2);
    with_toggle_float(&mut command, 2, 1, TARGET);
    accepted(&mut engine, &command);
    // The adapter applied the exact target: the next observation proves it
    // (with the applied read-back feedback riding along).
    let mut applied = base_report(3, 3);
    applied.domains[0].windows[0].rect = TARGET;
    applied.feedback = vec![AttemptFeedback {
        operation_id: "op-1".to_owned(),
        kind: AttemptKind::Property,
        outcome: AttemptOutcome::Applied,
        hold: None,
    }];
    let reply = accepted(&mut engine, &applied);
    assert_eq!(float_row(&reply).frame_operation, None);
    // Repeated settled replies never re-place: no token, no result.
    let settled = accepted(&mut engine, &base_report(4, 4));
    assert_eq!(float_row(&settled).frame_operation, None);
    assert_eq!(settled.command_result, None);
}

#[test]
fn manual_float_move_supersedes_stale_target() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &base_report(1, 1));
    let mut command = base_report(2, 2);
    with_toggle_float(&mut command, 2, 1, TARGET);
    accepted(&mut engine, &command);
    // Native input moved the frame somewhere that is neither the
    // command-time source nor the retained target: newer input wins and
    // the stale target drops rather than fighting it.
    let mut moved = base_report(3, 3);
    moved.domains[0].windows[0].rect = rect(300, 300, 200, 150);
    let reply = accepted(&mut engine, &moved);
    assert_eq!(float_row(&reply).frame_operation, None);
}

#[test]
fn proven_refusal_settles_token() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &base_report(1, 1));
    let mut command = base_report(2, 2);
    with_toggle_float(&mut command, 2, 1, TARGET);
    accepted(&mut engine, &command);
    // Explicit native refusal with the frame provably unchanged.
    let mut refused = base_report(3, 3);
    refused.feedback = vec![AttemptFeedback {
        operation_id: "op-1".to_owned(),
        kind: AttemptKind::Property,
        outcome: AttemptOutcome::RefusedProven,
        hold: Some(tiler_core::world_report::HoldReason::ConstraintClamp),
    }];
    assert_eq!(
        float_row(&accepted(&mut engine, &refused)).frame_operation,
        None
    );
    // A repeated settled operation never re-places.
    let settled = accepted(&mut engine, &base_report(4, 4));
    assert!(
        settled.windows.iter().all(|w| w.frame_operation.is_none()),
        "settled refusal stays settled"
    );
}

#[test]
fn explicit_close_settles_pending_target() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &base_report(1, 1));
    let mut command = base_report(2, 2);
    with_toggle_float(&mut command, 2, 1, TARGET);
    accepted(&mut engine, &command);
    // Explicit close clears the pending target.
    let mut closed = base_report(3, 3);
    closed.domains[0].windows.retain(|w| w.window.0 != "win-a");
    closed.domains[0].closed = vec![WindowId("win-a".to_owned())];
    let reply = accepted(&mut engine, &closed);
    assert!(
        reply.windows.iter().all(|w| w.frame_operation.is_none()),
        "no float token survives the close"
    );
}

#[test]
fn unfloat_withdraws_token_and_newer_intent_replaces() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &base_report(1, 1));
    let mut command = base_report(2, 2);
    with_toggle_float(&mut command, 2, 1, TARGET);
    accepted(&mut engine, &command);
    // Toggling the floating window unfloats (toggle semantics): the
    // outstanding target withdraws and tiled placement rides the snapshot.
    let mut unfloat = base_report(3, 3);
    unfloat.request_seq = 3;
    unfloat.request_id = "req-3".to_owned();
    unfloat.command_seq = Some(2);
    unfloat.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: "win-a".to_owned(),
        float_rect: None,
    }));
    let reply = accepted(&mut engine, &unfloat);
    match &reply.command_result {
        Some(tiler_core::world_report::CommandResult::Float { window, rect }) => {
            assert_eq!(window.0, "win-a");
            assert_eq!(*rect, None);
        }
        other => panic!("expected unfloat result, got {other:?}"),
    }
    assert!(
        reply.windows.iter().all(|w| w.frame_operation.is_none()),
        "unfloat carries no placement token"
    );
    // A newer float intent re-retains under its own stable identity.
    let other = rect(10, 10, 100, 100);
    let mut newer = base_report(4, 4);
    with_toggle_float(&mut newer, 4, 3, other);
    let reply = accepted(&mut engine, &newer);
    let row = float_row(&reply);
    assert_eq!(row.frame_operation.as_deref(), Some("op-3"));
    assert_eq!(row.effective_frame, Some(other));
}

#[test]
fn incarnation_and_generation_reset_clear_float_targets() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &base_report(1, 1));
    let mut command = base_report(2, 2);
    with_toggle_float(&mut command, 2, 1, TARGET);
    accepted(&mut engine, &command);
    // A new adapter incarnation cancels retained world state, never native
    // windows: the token is gone on the next accepted report.
    let mut migrated = base_report(1, 3);
    migrated.adapter_incarnation = "adapter-2".to_owned();
    migrated.request_seq = 1;
    migrated.request_id = "req-1".to_owned();
    let reply = accepted(&mut engine, &migrated);
    assert!(
        reply.windows.iter().all(|w| w.frame_operation.is_none()),
        "incarnation reset clears float targets"
    );
    // A delayed reply from another generation can never reset ordering.
    let mut foreign = base_report(9, 9);
    foreign.core_generation = "gen-2".to_owned();
    match engine.handle_report(&foreign) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "generation-mismatch"),
        other => panic!("expected generation mismatch, got {other:?}"),
    }
}

#[test]
fn third_frame_with_possibly_executed_holds_target() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &base_report(1, 1));
    let mut command = base_report(2, 2);
    with_toggle_float(&mut command, 2, 1, TARGET);
    accepted(&mut engine, &command);
    // A clamp/partial setter leaves a frame matching neither the
    // command-time source nor the target while the attempt is ambiguous:
    // the target holds (no newer-input inference) without re-actuation,
    // and the next snapshot still carries the exact token.
    let mut clamped = base_report(3, 3);
    clamped.domains[0].windows[0].rect = rect(300, 300, 200, 150);
    clamped.feedback = vec![AttemptFeedback {
        operation_id: "op-1".to_owned(),
        kind: AttemptKind::Property,
        outcome: AttemptOutcome::PossiblyExecuted,
        hold: None,
    }];
    let reply = accepted(&mut engine, &clamped);
    let row = float_row(&reply);
    assert_eq!(row.frame_operation.as_deref(), Some("op-1"));
    assert_eq!(row.effective_frame, Some(TARGET));
    // A repeated ambiguous snapshot keeps the same token exactly once:
    // retention is the hold, never a second placement.
    let mut again = base_report(4, 3);
    again.domains[0].windows[0].rect = rect(300, 300, 200, 150);
    again.feedback = vec![AttemptFeedback {
        operation_id: "op-1".to_owned(),
        kind: AttemptKind::Property,
        outcome: AttemptOutcome::PossiblyExecuted,
        hold: None,
    }];
    let held = accepted(&mut engine, &again);
    let row = float_row(&held);
    assert_eq!(row.frame_operation.as_deref(), Some("op-1"));
    assert_eq!(row.effective_frame, Some(TARGET));
    assert_eq!(held.command_result, None, "no repeated result while held");
    // Eventual arrival at the exact target still confirms and settles.
    let mut applied = base_report(5, 4);
    applied.domains[0].windows[0].rect = TARGET;
    let settled = accepted(&mut engine, &applied);
    assert_eq!(float_row(&settled).frame_operation, None);
}

#[test]
fn third_frame_with_explicit_external_settles() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &base_report(1, 1));
    let mut command = base_report(2, 2);
    with_toggle_float(&mut command, 2, 1, TARGET);
    accepted(&mut engine, &command);
    // Same third frame, but the adapter proves newer external input took
    // over: the stale target settles instead of holding.
    let mut moved = base_report(3, 3);
    moved.domains[0].windows[0].rect = rect(300, 300, 200, 150);
    moved.feedback = vec![AttemptFeedback {
        operation_id: "op-1".to_owned(),
        kind: AttemptKind::Property,
        outcome: AttemptOutcome::SupersededExternal,
        hold: None,
    }];
    let reply = accepted(&mut engine, &moved);
    assert_eq!(float_row(&reply).frame_operation, None);
}
