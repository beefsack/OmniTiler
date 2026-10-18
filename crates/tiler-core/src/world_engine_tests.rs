//! World transition tests (D1-D5): handle_report ordering, transfers,
//! tiling, migration, and policy convergence through full typed reports.
//!
//! Split from [`crate::world_engine`]: this submodule keeps its full
//! access to the transition internals while keeping the production
//! module modest.

use super::*;
use crate::directional::WorkspaceId;
use crate::geometry::Rect;
use crate::session::OutputDomain;
use crate::workspace_scope::WorkspaceMode;
use crate::world_report::{
    AttemptFeedback, AttemptKind, AttemptOutcome, DomainReport, HoldReason, OutputTopology,
    ScopeAuthority, TransferKind, WorkspaceBinding, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn domain(output: &str, workspace: &str) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: rect(0, 0, 300, 100),
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}

fn win(id: &str, output: &str, workspace: &str, x: i32) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: rect(x, 0, 40, 90),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}

fn domain_report(
    output: &str,
    workspace: &str,
    focused: &str,
    windows: Vec<EngineWindow>,
) -> DomainReport {
    DomainReport {
        domain: domain(output, workspace),
        outer_gap: 0,
        focused: WindowId(focused.to_owned()),
        windows,
        closed: Vec::new(),
        readable: true,
        hold: None,
    }
}

/// Two-domain working world: win-a focused in ws1, win-b focused in ws2.
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
            native_order: vec!["ws1".to_owned(), "ws2".to_owned()],
            current_native: "ws1".to_owned(),
            complete: true,
            geometry: None,
            primary: false,
        }],
        bindings: vec![
            WorkspaceBinding {
                logical_id: "desk-1".to_owned(),
                native_id: "ws1".to_owned(),
                output: OutputId("out".to_owned()),
                pinned: false,
                displaced: false,
                managed: true,
            },
            WorkspaceBinding {
                logical_id: "desk-2".to_owned(),
                native_id: "ws2".to_owned(),
                output: OutputId("out".to_owned()),
                pinned: false,
                displaced: false,
                managed: true,
            },
        ],
        domains: vec![
            domain_report("out", "ws1", "win-a", vec![win("win-a", "out", "ws1", 0)]),
            domain_report("out", "ws2", "win-b", vec![win("win-b", "out", "ws2", 0)]),
        ],
        minimized: Vec::new(),
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: String::new(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: Vec::new(),
    }
}

fn accepted_reply(engine: &mut WorldEngine, report: &WorldReport) -> WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn homing(reply: &WorldReply, window: &str) -> Option<String> {
    reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == window)
        .map(|w| format!("{}/{}", w.output.0, w.workspace))
}

/// Paired single-domain fixture: two tiled windows sharing one domain,
/// so group/drag routes resolve against a real parent group.
fn paired_report(request_seq: u64, observation_seq: u64) -> WorldReport {
    let mut report = base_report(request_seq, observation_seq);
    report.domains = vec![domain_report(
        "out",
        "ws1",
        "win-a",
        vec![
            win("win-a", "out", "ws1", 0),
            win("win-b", "out", "ws1", 150),
        ],
    )];
    report.topology = vec![OutputTopology {
        output: OutputId("out".to_owned()),
        native_order: vec!["ws1".to_owned()],
        current_native: "ws1".to_owned(),
        complete: true,
        geometry: None,
        primary: false,
    }];
    report
}

#[test]
fn bootstrap_is_not_ready_and_generation_mismatch_rejects() {
    let engine = WorldEngine::new("gen-1").expect("valid generation");
    assert!(WorldEngine::new("GEN BAD").is_none());
    let bootstrap = engine.bootstrap_reply("adapter-1", "req-0");
    assert!(!bootstrap.ready);
    assert!(bootstrap.windows.is_empty());
    let mut engine = engine;
    let mut report = base_report(1, 1);
    report.core_generation = "gen-2".to_owned();
    match engine.handle_report(&report) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "generation-mismatch"),
        other => panic!("expected rejection, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), 0);
}

#[test]
fn reconcile_projects_all_domains_and_revision_advances() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = accepted_reply(&mut engine, &base_report(1, 1));
    assert_eq!(engine.state_revision(), 1);
    assert_eq!(homing(&reply, "win-a"), Some("out/ws1".to_owned()));
    assert_eq!(homing(&reply, "win-b"), Some("out/ws2".to_owned()));
    assert!(reply.windows.iter().all(|w| w.allocation.is_some()));
    // Idempotent desired state under a fresh report with no changes.
    let reply2 = accepted_reply(&mut engine, &base_report(2, 1));
    assert_eq!(engine.state_revision(), 2);
    assert_eq!(reply.windows, reply2.windows);
}

#[test]
fn all_three_modes_accept_with_shared_trailing_policy() {
    use crate::workspace_scope::{mode_token, parse_workspace_mode};
    for token in ["per-output-local", "global-unique", "shared"] {
        let mut engine = WorldEngine::new("gen-1").expect("valid");
        let mut report = base_report(1, 1);
        report.workspace_mode = parse_workspace_mode(token);
        assert_eq!(mode_token(report.workspace_mode), token);
        let reply = accepted_reply(&mut engine, &report);
        assert_eq!(homing(&reply, "win-a"), Some("out/ws1".to_owned()));
        assert_eq!(homing(&reply, "win-b"), Some("out/ws2".to_owned()));
    }
}

#[test]
fn identical_duplicate_returns_cached_without_revision() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let report = base_report(1, 1);
    let first = accepted_reply(&mut engine, &report);
    match engine.handle_report(&report) {
        WorldOutcome::Duplicate(cached) => assert_eq!(cached, first),
        other => panic!("expected duplicate, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), 1);
}

#[test]
fn reused_sequence_with_changed_input_is_invalid() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    let mut changed = base_report(1, 1);
    changed.domains[0]
        .windows
        .push(win("win-c", "out", "ws1", 80));
    match engine.handle_report(&changed) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "request-reused"),
        other => panic!("expected reused rejection, got {other:?}"),
    }
    let mut older = base_report(0, 0);
    older.request_id = "req-0".to_owned();
    match engine.handle_report(&older) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "request-superseded"),
        other => panic!("expected superseded, got {other:?}"),
    }
}

#[test]
fn stale_observation_is_rejected() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 9));
    match engine.handle_report(&base_report(2, 5)) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "observation-stale"),
        other => panic!("expected stale, got {other:?}"),
    }
}

#[test]
fn transfer_retained_across_lag_and_confirmed_by_observation() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    // Send win-a to ws2 (win-a is focused in ws1).
    let mut send = base_report(2, 2);
    send.command_seq = Some(5);
    send.command = Some(WorldCommand::Send {
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws2".to_owned(),
        follow: true,
    });
    let reply = accepted_reply(&mut engine, &send);
    assert_eq!(homing(&reply, "win-a"), Some("out/ws2".to_owned()));
    assert_eq!(reply.transfers.len(), 1);
    assert_eq!(reply.transfers[0].operation_id, "op-5");
    // Actuation lag: identical observation. Without shielding the
    // committed link would flap back to ws1 here.
    let lagged = accepted_reply(&mut engine, &base_report(3, 2));
    assert_eq!(homing(&lagged, "win-a"), Some("out/ws2".to_owned()));
    assert_eq!(lagged.transfers.len(), 1);
    // Confirmed: observation shows win-a in ws2.
    let mut confirmed = base_report(4, 3);
    confirmed.domains[0].windows.clear();
    confirmed.domains[0].focused = WindowId(String::new());
    confirmed.domains[1].windows = vec![
        win("win-b", "out", "ws2", 0),
        win("win-a", "out", "ws2", 50),
    ];
    let settled = accepted_reply(&mut engine, &confirmed);
    assert!(settled.transfers.is_empty());
    assert_eq!(homing(&settled, "win-a"), Some("out/ws2".to_owned()));
}

#[test]
fn consumed_command_reconciles_without_reexecution() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    let mut send = base_report(2, 2);
    send.command_seq = Some(5);
    send.command = Some(WorldCommand::Send {
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws2".to_owned(),
        follow: true,
    });
    accepted_reply(&mut engine, &send);
    // Same command sequence on a fresh request: consumed, never rerun.
    let mut replay = base_report(3, 2);
    replay.command_seq = Some(5);
    replay.command = send.command.clone();
    match engine.handle_report(&replay) {
        WorldOutcome::CommandConsumed(reply) => {
            assert!(reply.command_consumed);
            assert_eq!(reply.transfers.len(), 1);
        }
        other => panic!("expected consumed, got {other:?}"),
    }
    assert_eq!(engine.retained_transfers().len(), 1);
}

#[test]
fn proven_refusal_settles_to_observed_with_narrow_hold() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    let mut send = base_report(2, 2);
    send.command_seq = Some(5);
    send.command = Some(WorldCommand::Send {
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws2".to_owned(),
        follow: true,
    });
    accepted_reply(&mut engine, &send);
    // Adapter proves the native move refused; observation still shows
    // win-a in ws1.
    let mut refused = base_report(3, 2);
    refused.feedback = vec![AttemptFeedback {
        operation_id: "op-5".to_owned(),
        kind: AttemptKind::Transfer,
        outcome: AttemptOutcome::RefusedProven,
        hold: Some(HoldReason::RefusedTarget),
    }];
    let reply = accepted_reply(&mut engine, &refused);
    assert!(reply.transfers.is_empty());
    assert_eq!(homing(&reply, "win-a"), Some("out/ws1".to_owned()));
    // win-b in the untouched scope is unaffected.
    assert_eq!(homing(&reply, "win-b"), Some("out/ws2".to_owned()));
}

#[test]
fn possibly_executed_holds_without_repeat() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    let mut send = base_report(2, 2);
    send.command_seq = Some(5);
    send.command = Some(WorldCommand::Send {
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws2".to_owned(),
        follow: true,
    });
    accepted_reply(&mut engine, &send);
    let mut ambiguous = base_report(3, 2);
    ambiguous.feedback = vec![AttemptFeedback {
        operation_id: "op-5".to_owned(),
        kind: AttemptKind::Transfer,
        outcome: AttemptOutcome::PossiblyExecuted,
        hold: None,
    }];
    let reply = accepted_reply(&mut engine, &ambiguous);
    assert_eq!(reply.transfers.len(), 1);
    assert!(reply.transfers[0].held);
    assert_eq!(homing(&reply, "win-a"), Some("out/ws2".to_owned()));
}

#[test]
fn external_move_supersedes_outstanding_transfer() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    let mut send = base_report(2, 2);
    send.command_seq = Some(5);
    send.command = Some(WorldCommand::Send {
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws2".to_owned(),
        follow: true,
    });
    accepted_reply(&mut engine, &send);
    // User moved win-a to ws3 outside the transfer: newer input wins.
    let mut moved = base_report(3, 3);
    moved.topology[0].native_order.push("ws3".to_owned());
    moved.domains[0].windows.clear();
    moved.domains[0].focused = WindowId(String::new());
    moved.domains.push(domain_report(
        "out",
        "ws3",
        "win-a",
        vec![win("win-a", "out", "ws3", 0)],
    ));
    let reply = accepted_reply(&mut engine, &moved);
    assert!(reply.transfers.is_empty());
    assert_eq!(homing(&reply, "win-a"), Some("out/ws3".to_owned()));
}

#[test]
fn view_switch_retained_then_confirmed() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    let mut switch = base_report(2, 2);
    switch.command_seq = Some(3);
    switch.command = Some(WorldCommand::SwitchView {
        output: OutputId("out".to_owned()),
        workspace: "ws2".to_owned(),
    });
    let reply = accepted_reply(&mut engine, &switch);
    assert_eq!(reply.outputs[0].logical_view.as_deref(), Some("desk-2"));
    assert!(matches!(
        reply.transfers.as_slice(),
        [t] if matches!(t.kind, TransferKind::View)
    ));
    // View lag: current still ws1, target retained.
    let lagged = accepted_reply(&mut engine, &base_report(3, 2));
    assert_eq!(lagged.outputs[0].logical_view.as_deref(), Some("desk-2"));
    assert_eq!(lagged.transfers.len(), 1);
    // Confirmed by observation.
    let mut shown = base_report(4, 3);
    shown.topology[0].current_native = "ws2".to_owned();
    let settled = accepted_reply(&mut engine, &shown);
    assert!(settled.transfers.is_empty());
    assert_eq!(settled.outputs[0].logical_view.as_deref(), Some("desk-2"));
}

#[test]
fn focus_effect_emitted_once_then_spent() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    let mut focus = base_report(2, 2);
    focus.command_seq = Some(7);
    focus.command = Some(WorldCommand::Focus {
        window: WindowId("win-b".to_owned()),
        direction: "left".to_owned(),
    });
    let reply = accepted_reply(&mut engine, &focus);
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].id, "fx-7");
    assert_eq!(reply.effects[0].issued_revision, reply.state_revision);
    // The next unrelated report carries no effect: no refocus.
    let next = accepted_reply(&mut engine, &base_report(3, 2));
    assert!(next.effects.is_empty());
}

#[test]
fn unreadable_scope_holds_narrowly() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    let mut report = base_report(2, 2);
    report.domains[1].readable = false;
    report.domains[1].hold = Some(HoldReason::Unreadable);
    report.domains[1].windows.clear();
    let reply = accepted_reply(&mut engine, &report);
    let held = reply
        .scopes
        .iter()
        .find(|s| s.scope == "out/ws2")
        .expect("ws2 scope");
    assert_eq!(held.authority.hold_reason(), Some(HoldReason::Unreadable));
    // Held lifetimes are explicit hold rows, never silent absence: win-b
    // stays visible with no write authority, and win-a stays homed.
    let row = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-b")
        .expect("held win-b row");
    assert_eq!(
        row.frame_authority.hold_reason(),
        Some(HoldReason::Unreadable)
    );
    assert_eq!(
        row.membership_authority.hold_reason(),
        Some(HoldReason::Unreadable)
    );
    assert!(
        reply
            .windows
            .iter()
            .all(|w| { w.workspace != "ws2" || w.frame_authority.hold_reason().is_some() })
    );
    assert_eq!(homing(&reply, "win-a"), Some("out/ws1".to_owned()));
}

#[test]
fn gesture_barrier_holds_frames_only() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let mut report = base_report(1, 1);
    report.gesture_barrier = true;
    let reply = accepted_reply(&mut engine, &report);
    assert!(!reply.windows.is_empty());
    for window in &reply.windows {
        assert_eq!(
            window.frame_authority.hold_reason(),
            Some(HoldReason::GestureBarrier)
        );
        assert_eq!(window.membership_authority, ScopeAuthority::Controlled);
    }
}

#[test]
fn trailing_creation_is_stable_until_bound() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let mut report = base_report(1, 1);
    // Only ws1 exists and it is occupied: lifecycle wants a spare.
    report.topology[0].native_order = vec!["ws1".to_owned()];
    report.domains.remove(1);
    report.bindings.remove(1);
    let reply = accepted_reply(&mut engine, &report);
    let pending: Vec<_> = reply
        .workspaces
        .iter()
        .filter(|w| w.native_binding.is_none())
        .collect();
    assert_eq!(pending.len(), 1);
    let token = pending[0].id.clone();
    // Identical next report reuses the token; never appends blindly.
    let mut second = report.clone();
    second.request_seq = 2;
    second.request_id = "req-2".to_owned();
    second.observation_seq = 2;
    let reply2 = accepted_reply(&mut engine, &second);
    let pending2: Vec<_> = reply2
        .workspaces
        .iter()
        .filter(|w| w.native_binding.is_none())
        .collect();
    assert_eq!(pending2.len(), 1);
    assert_eq!(pending2[0].id, token);
    // Binding resolves the token through the reported bindings.
    let mut bound = report.clone();
    bound.request_seq = 3;
    bound.request_id = "req-3".to_owned();
    bound.observation_seq = 3;
    bound.topology[0].native_order = vec!["ws1".to_owned(), "ws9".to_owned()];
    bound.bindings.push(WorkspaceBinding {
        logical_id: token.clone(),
        native_id: "ws9".to_owned(),
        output: OutputId("out".to_owned()),
        pinned: false,
        displaced: false,
        managed: true,
    });
    let reply3 = accepted_reply(&mut engine, &bound);
    assert!(
        reply3
            .workspaces
            .iter()
            .filter(|w| w.native_binding.is_none())
            .count()
            == 0
    );
    assert!(reply3.workspaces.iter().any(|w| w.id == "ws9"
        && w.native_binding.as_deref() == Some("ws9")
        && w.logical_id.as_deref() == Some(token.as_str())));
}

#[test]
fn empty_middle_is_eligible_and_incomplete_holds() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let mut report = base_report(1, 1);
    report.topology[0].native_order =
        vec!["ws1".to_owned(), "ws-empty".to_owned(), "ws2".to_owned()];
    report.topology[0].current_native = "ws1".to_owned();
    let reply = accepted_reply(&mut engine, &report);
    let middle = reply
        .workspaces
        .iter()
        .find(|w| w.id == "ws-empty")
        .expect("middle row");
    assert!(!middle.existence);
    // Incomplete enumeration: everything keeps, all rows held.
    let mut engine2 = WorldEngine::new("gen-1").expect("valid");
    let mut vague = report.clone();
    vague.topology[0].complete = false;
    vague.inventory_complete = false;
    let held = accepted_reply(&mut engine2, &vague);
    assert!(
        held.workspaces
            .iter()
            .filter(|w| w.native_binding.is_some())
            .all(|w| w.existence && w.hold == Some(HoldReason::IncompleteInventory))
    );
}

#[test]
fn explicit_close_accepted_with_incomplete_inventory() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let mut first = base_report(1, 1);
    first.domains[0]
        .windows
        .push(win("win-c", "out", "ws1", 80));
    accepted_reply(&mut engine, &first);
    let mut close = base_report(2, 2);
    close.domains[0].windows = vec![win("win-a", "out", "ws1", 0)];
    close.domains[0].closed = vec![WindowId("win-c".to_owned())];
    close.topology[0].complete = false;
    close.inventory_complete = false;
    let reply = accepted_reply(&mut engine, &close);
    assert!(reply.windows.iter().all(|w| w.lifetime.0 != "win-c"));
    assert_eq!(homing(&reply, "win-a"), Some("out/ws1".to_owned()));
}

#[test]
fn minimized_means_explicit_hold_row_and_restore_returns() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let mut first = base_report(1, 1);
    first.domains[0]
        .windows
        .push(win("win-c", "out", "ws1", 80));
    accepted_reply(&mut engine, &first);
    // win-c minimized: absent without an explicit close.
    let mut hidden = base_report(2, 2);
    hidden.domains[0].windows = vec![win("win-a", "out", "ws1", 0)];
    let reply = accepted_reply(&mut engine, &hidden);
    // Explicit minimized row: no managed geometry, never an inferred
    // close, membership retained.
    let row = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-c")
        .expect("minimized win-c row");
    assert_eq!(row.output.0, "out");
    assert_eq!(row.workspace, "ws1");
    assert_eq!(row.allocation, None);
    assert_eq!(row.effective_frame, None);
    assert_eq!(
        row.frame_authority.hold_reason(),
        Some(HoldReason::Minimized)
    );
    assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    let key = DomainKey {
        output: OutputId("out".to_owned()),
        workspace: crate::directional::WorkspaceId("ws1".to_owned()),
    };
    assert!(
        engine
            .engine()
            .session(&key)
            .expect("session")
            .minimized_slot(&WindowId("win-c".to_owned()))
            .is_some()
    );
    // Native restore returns it to the old neighborhood.
    let mut restored_report = first.clone();
    restored_report.request_seq = 3;
    restored_report.request_id = "req-3".to_owned();
    restored_report.observation_seq = 3;
    let restored = accepted_reply(&mut engine, &restored_report);
    assert!(restored.windows.iter().any(|w| w.lifetime.0 == "win-c"));
}

#[test]
fn second_send_while_in_flight_rejects() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    let mut send = base_report(2, 2);
    send.command_seq = Some(5);
    send.command = Some(WorldCommand::Send {
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws2".to_owned(),
        follow: true,
    });
    accepted_reply(&mut engine, &send);
    let mut again = base_report(3, 2);
    again.command_seq = Some(6);
    again.command = send.command.clone();
    match engine.handle_report(&again) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "transfer-in-flight"),
        other => panic!("expected in-flight rejection, got {other:?}"),
    }
}

#[test]
fn unattempted_feedback_retains_transfer() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    let mut send = base_report(2, 2);
    send.command_seq = Some(5);
    send.command = Some(WorldCommand::Send {
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws2".to_owned(),
        follow: true,
    });
    accepted_reply(&mut engine, &send);
    // Adapter reports it never attempted (e.g. held by a barrier):
    // the target stays desired, nothing re-executes.
    let mut untried = base_report(3, 2);
    untried.feedback = vec![AttemptFeedback {
        operation_id: "op-5".to_owned(),
        kind: AttemptKind::Transfer,
        outcome: AttemptOutcome::Unattempted,
        hold: None,
    }];
    let reply = accepted_reply(&mut engine, &untried);
    assert_eq!(reply.transfers.len(), 1);
    assert!(!reply.transfers[0].held);
    assert_eq!(homing(&reply, "win-a"), Some("out/ws2".to_owned()));
}

fn with_command(
    report: &mut WorldReport,
    request_seq: u64,
    command_seq: u64,
    command: WorldCommand,
) {
    report.request_seq = request_seq;
    report.request_id = format!("req-{request_seq}");
    report.command_seq = Some(command_seq);
    report.command = Some(command);
}

fn focus_command(window: &str) -> WorldCommand {
    WorldCommand::Focus {
        window: WindowId(window.to_owned()),
        direction: "left".to_owned(),
    }
}

#[test]
fn oversized_request_rejected_without_mutation() {
    use crate::world_report::WORLD_MAX_REQUEST_BYTES;
    use crate::world_report::report_estimate;
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let first = accepted_reply(&mut engine, &base_report(1, 1));
    // A max-caps report exceeds the 4 MiB request budget.
    let mut huge = base_report(2, 2);
    let long = "q".repeat(crate::bounds::MAX_OPAQUE_ID_LEN);
    let mut n = 0usize;
    for domain in huge.domains.iter_mut() {
        domain.windows = (0..5_000)
            .map(|_| {
                n += 1;
                let mut row = win(&format!("win-{n:06}"), "out", "ws1", 0);
                let mut id = format!("{long}-{n:06}");
                id.truncate(crate::bounds::MAX_OPAQUE_ID_LEN);
                row.window = WindowId(id);
                row.output = OutputId(format!("out-{n:06}"));
                row
            })
            .collect();
    }
    huge.topology = (0..16)
        .map(|o| OutputTopology {
            output: OutputId(format!("out-{o:02}")),
            native_order: (0..256)
                .map(|w| format!("{long}-{o:02}-{w:03}")[..128].to_owned())
                .collect(),
            current_native: format!("{long}-cur-{o:02}")[..128].to_owned(),
            complete: true,
            geometry: None,
            primary: false,
        })
        .collect();
    huge.bindings = (0..4096)
        .map(|b| WorkspaceBinding {
            logical_id: format!("{long}-l-{b:04}")[..128].to_owned(),
            native_id: format!("{long}-n-{b:04}")[..128].to_owned(),
            output: OutputId(format!("out-{:02}", b % 16)),
            pinned: false,
            displaced: false,
            managed: true,
        })
        .collect();
    huge.feedback = (0..1024)
        .map(|f| AttemptFeedback {
            operation_id: format!("op-{f:04}"),
            kind: AttemptKind::Transfer,
            outcome: AttemptOutcome::Unattempted,
            hold: None,
        })
        .collect();
    assert!(report_estimate(&huge) > WORLD_MAX_REQUEST_BYTES);
    assert!(report_estimate(&huge) > WORLD_MAX_REQUEST_BYTES);
    match engine.handle_report(&huge) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "oversized"),
        other => panic!("expected oversized rejection, got {other:?}"),
    }
    // Sessions, ordering, and caches are untouched: the next report
    // still accepts and an identical repeat of the first duplicates.
    assert_eq!(engine.state_revision(), 1);
    assert!(engine.retained_transfers().is_empty());
    let reply = accepted_reply(&mut engine, &base_report(2, 1));
    assert_eq!(reply.windows, first.windows);
    assert_eq!(engine.state_revision(), 2);
}

#[test]
fn highwater_never_resurrects_evicted_sequences() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    // 300 consumed focus commands: past the old 256-entry eviction
    // window, every one must stay consumed.
    for seq in 1..=300u64 {
        let mut report = base_report(1 + seq, 1 + seq);
        with_command(&mut report, 1 + seq, seq, focus_command("win-a"));
        accepted_reply(&mut engine, &report);
    }
    assert_eq!(engine.state_revision(), 301);
    let mut seq = 302u64;
    for old in [1u64, 150, 300] {
        let mut replay = base_report(seq, seq);
        with_command(&mut replay, seq, old, focus_command("win-a"));
        match engine.handle_report(&replay) {
            WorldOutcome::CommandConsumed(reply) => {
                assert!(reply.command_consumed);
                assert!(reply.effects.is_empty());
            }
            other => panic!("seq {old}: expected consumed, got {other:?}"),
        }
        seq += 1;
    }
    // Commandless reports never move the high-watermark and still
    // reconcile.
    let plain = accepted_reply(&mut engine, &base_report(seq, seq - 1));
    assert!(!plain.command_consumed);
    seq += 1;
    let mut fresh = base_report(seq, seq - 1);
    with_command(&mut fresh, seq, 301, focus_command("win-a"));
    let reply = accepted_reply(&mut engine, &fresh);
    assert_eq!(reply.effects.len(), 1);
}

#[test]
fn retained_missing_domain_holds_explicitly_never_closed() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    // ws2 vanishes from the observation with an incomplete inventory:
    // the retained session becomes an explicit held scope, win-b is
    // omitted (no geometry), never closed.
    let mut gap = base_report(2, 2);
    gap.domains.remove(1);
    gap.inventory_complete = false;
    let reply = accepted_reply(&mut engine, &gap);
    let missing = reply
        .scopes
        .iter()
        .find(|s| s.scope == "out/ws2")
        .expect("retained missing scope");
    assert_eq!(
        missing.authority.hold_reason(),
        Some(HoldReason::IncompleteInventory)
    );
    assert!(reply.windows.iter().all(|w| w.lifetime.0 != "win-b"));
    assert_eq!(homing(&reply, "win-a"), Some("out/ws1".to_owned()));
    // The next complete observation restores win-b: absence was never
    // a close.
    let restored = accepted_reply(&mut engine, &base_report(3, 3));
    assert_eq!(homing(&restored, "win-b"), Some("out/ws2".to_owned()));
    assert!(
        restored
            .scopes
            .iter()
            .find(|s| s.scope == "out/ws2")
            .expect("ws2 scope")
            .authority
            == ScopeAuthority::Controlled
    );
}

#[test]
fn workspace_rows_carry_scope_order_mode_and_outputs_hold() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = accepted_reply(&mut engine, &base_report(1, 1));
    let ws1 = reply
        .workspaces
        .iter()
        .find(|w| w.id == "ws1")
        .expect("ws1 row");
    assert_eq!(ws1.scope, "out");
    assert_eq!(ws1.order, 0);
    assert_eq!(ws1.tiling_mode, crate::world_report::TilingMode::Tiled);
    assert_eq!(ws1.native_binding.as_deref(), Some("ws1"));
    assert_eq!(reply.outputs[0].hold, None);
    // Incomplete enumeration holds outputs and workspace rows.
    let mut vague = base_report(2, 2);
    vague.topology[0].complete = false;
    vague.inventory_complete = false;
    let held = accepted_reply(&mut engine, &vague);
    assert_eq!(held.outputs[0].hold, Some(HoldReason::IncompleteInventory));
    assert!(
        held.workspaces
            .iter()
            .all(|w| w.hold == Some(HoldReason::IncompleteInventory))
    );
}

#[test]
fn planner_selection_commands_resolve_through_rings() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    // Numbered selection of the second workspace retains its view.
    let mut numbered = base_report(2, 2);
    with_command(
        &mut numbered,
        2,
        11,
        WorldCommand::SelectNumbered {
            output: OutputId("out".to_owned()),
            index: 2,
        },
    );
    let reply = accepted_reply(&mut engine, &numbered);
    assert_eq!(reply.outputs[0].logical_view.as_deref(), Some("desk-2"));
    // Relative step from ws1 reaches ws2 as well (fresh observation).
    let mut relative = base_report(3, 2);
    with_command(
        &mut relative,
        3,
        12,
        WorldCommand::SelectRelative {
            output: OutputId("out".to_owned()),
            delta: 1,
        },
    );
    let reply = accepted_reply(&mut engine, &relative);
    assert_eq!(reply.outputs[0].logical_view.as_deref(), Some("desk-2"));
    // A verified observed change to ws2 records history; previous then
    // toggles back to ws1.
    let mut shown = base_report(4, 3);
    shown.topology[0].current_native = "ws2".to_owned();
    accepted_reply(&mut engine, &shown);
    // Still showing ws2: previous toggles back to the recorded ws1.
    let mut previous = base_report(5, 3);
    previous.topology[0].current_native = "ws2".to_owned();
    with_command(
        &mut previous,
        5,
        13,
        WorldCommand::SelectPrevious {
            output: OutputId("out".to_owned()),
        },
    );
    let reply = accepted_reply(&mut engine, &previous);
    assert_eq!(reply.outputs[0].logical_view.as_deref(), Some("desk-1"));
    // Out-of-ring numbered selection refuses without mutation.
    let mut bad = base_report(6, 3);
    with_command(
        &mut bad,
        6,
        14,
        WorldCommand::SelectNumbered {
            output: OutputId("out".to_owned()),
            index: 9,
        },
    );
    match engine.handle_report(&bad) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "selection-unknown"),
        other => panic!("expected selection refusal, got {other:?}"),
    }
}

#[test]
fn send_relative_resolves_ring_once() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    let mut send = base_report(2, 2);
    with_command(
        &mut send,
        2,
        21,
        WorldCommand::SendRelative {
            window: WindowId("win-a".to_owned()),
            output: OutputId("out".to_owned()),
            delta: 1,
            follow: true,
        },
    );
    let reply = accepted_reply(&mut engine, &send);
    assert_eq!(homing(&reply, "win-a"), Some("out/ws2".to_owned()));
    assert_eq!(reply.transfers.len(), 1);
}

#[test]
fn operation_send_retains_and_foreign_routes_refuse() {
    use crate::boundary::CoreCommand;
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    // Read-only operation reconciles with no effect.
    let mut read = base_report(2, 2);
    with_command(
        &mut read,
        2,
        31,
        WorldCommand::Operation(CoreCommand::Reconcile),
    );
    let reply = accepted_reply(&mut engine, &read);
    assert!(reply.effects.is_empty());
    assert_eq!(homing(&reply, "win-a"), Some("out/ws1".to_owned()));
    // Operation send retains exactly like an explicit send.
    let mut send = base_report(3, 2);
    with_command(
        &mut send,
        3,
        32,
        WorldCommand::Operation(CoreCommand::SendToOutput {
            window: "win-a".to_owned(),
            target_output: "out".to_owned(),
            target_workspace: "ws2".to_owned(),
            follow: true,
        }),
    );
    let reply = accepted_reply(&mut engine, &send);
    assert_eq!(homing(&reply, "win-a"), Some("out/ws2".to_owned()));
    assert_eq!(reply.transfers.len(), 1);
    // Directional migration with no retained FULL topology has no
    // resolvable target and refuses without mutation, never silently
    // local. (Single output here, and no FULL rectangle retained.)
    let mut refused = base_report(4, 2);
    with_command(
        &mut refused,
        4,
        33,
        WorldCommand::Operation(CoreCommand::MigrateWorkspace {
            direction: "right".to_owned(),
        }),
    );
    match engine.handle_report(&refused) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "operation-refused"),
        other => panic!("expected operation refusal, got {other:?}"),
    }
    // Domain release drops the retained session and consumes the
    // sequence: the released scope carries no retained rows.
    let mut release = base_report(5, 2);
    with_command(
        &mut release,
        5,
        34,
        WorldCommand::Operation(CoreCommand::ReleaseDomain),
    );
    let reply = accepted_reply(&mut engine, &release);
    assert!(reply.effects.is_empty());
}

fn float_report(request_seq: u64, observation_seq: u64, floating: bool) -> WorldReport {
    let mut report = base_report(request_seq, observation_seq);
    let mut floater = win("win-f", "out", "ws1", 200);
    floater.floating = floating;
    report.domains[0].windows.push(floater);
    // The deferred float send moves the focused window: focus the
    // floater so the send is the focused mover.
    report.domains[0].focused = WindowId("win-f".to_owned());
    report
}

#[test]
fn float_send_authorizes_membership_without_frame() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &float_report(1, 1, true));
    let mut send = float_report(2, 2, true);
    with_command(
        &mut send,
        2,
        41,
        WorldCommand::Send {
            window: WindowId("win-f".to_owned()),
            target_output: OutputId("out".to_owned()),
            target_workspace: "ws2".to_owned(),
            follow: true,
        },
    );
    let reply = accepted_reply(&mut engine, &send);
    assert_eq!(reply.transfers.len(), 1);
    let row = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-f")
        .expect("float row");
    assert_eq!(format!("{}/{}", row.output.0, row.workspace), "out/ws2");
    assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    assert!(row.allocation.is_none());
}

#[test]
fn excluded_and_sticky_sends_have_no_automatic_permission() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let mut report = base_report(1, 1);
    let mut game = win("win-g", "out", "ws1", 200);
    game.fit_excluded = true;
    let mut pin = win("win-s", "out", "ws1", 240);
    pin.sticky = true;
    report.domains[0].windows.push(game);
    report.domains[0].windows.push(pin);
    accepted_reply(&mut engine, &report);
    for window in ["win-g", "win-s"] {
        let mut send = base_report(2, 2);
        // Rebuild the same observation so the sweep stays complete.
        let mut game = win("win-g", "out", "ws1", 200);
        game.fit_excluded = true;
        let mut pin = win("win-s", "out", "ws1", 240);
        pin.sticky = true;
        send.domains[0].windows.push(game);
        send.domains[0].windows.push(pin);
        with_command(
            &mut send,
            2,
            42,
            WorldCommand::Send {
                window: WindowId(window.to_owned()),
                target_output: OutputId("out".to_owned()),
                target_workspace: "ws2".to_owned(),
                follow: true,
            },
        );
        match engine.handle_report(&send) {
            WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-refused"),
            other => panic!("{window}: expected send-refused, got {other:?}"),
        }
    }
    // Refused commands consume nothing: a fresh send still works.
    assert!(engine.retained_transfers().is_empty());
    let mut send = base_report(2, 2);
    with_command(
        &mut send,
        2,
        43,
        WorldCommand::Send {
            window: WindowId("win-a".to_owned()),
            target_output: OutputId("out".to_owned()),
            target_workspace: "ws2".to_owned(),
            follow: true,
        },
    );
    accepted_reply(&mut engine, &send);
    assert_eq!(engine.retained_transfers().len(), 1);
}

#[test]
fn held_feedback_holds_only_the_reported_window() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    let mut send = base_report(2, 2);
    with_command(
        &mut send,
        2,
        51,
        WorldCommand::Send {
            window: WindowId("win-a".to_owned()),
            target_output: OutputId("out".to_owned()),
            target_workspace: "ws2".to_owned(),
            follow: true,
        },
    );
    accepted_reply(&mut engine, &send);
    // Ambiguous outcome with an explicit hold: win-a holds narrowly,
    // win-b stays fully controlled.
    let mut ambiguous = base_report(3, 2);
    ambiguous.feedback = vec![AttemptFeedback {
        operation_id: "op-51".to_owned(),
        kind: AttemptKind::Transfer,
        outcome: AttemptOutcome::PossiblyExecuted,
        hold: Some(HoldReason::RefusedTarget),
    }];
    let reply = accepted_reply(&mut engine, &ambiguous);
    let held = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .expect("win-a row");
    assert_eq!(
        held.membership_authority.hold_reason(),
        Some(HoldReason::RefusedTarget)
    );
    let other = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-b")
        .expect("win-b row");
    assert_eq!(other.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(other.frame_authority, ScopeAuthority::Controlled);
}

#[test]
fn deterministic_sequence_replay_matches_exactly() {
    fn script() -> Vec<WorldReport> {
        let mut reports = vec![base_report(1, 1)];
        let mut send = base_report(2, 2);
        send.command_seq = Some(5);
        send.command = Some(WorldCommand::Send {
            window: WindowId("win-a".to_owned()),
            target_output: OutputId("out".to_owned()),
            target_workspace: "ws2".to_owned(),
            follow: true,
        });
        reports.push(send);
        reports.push(base_report(3, 2));
        let mut confirmed = base_report(4, 3);
        confirmed.domains[0].windows.clear();
        confirmed.domains[0].focused = WindowId(String::new());
        confirmed.domains[1].windows = vec![
            win("win-b", "out", "ws2", 0),
            win("win-a", "out", "ws2", 50),
        ];
        reports.push(confirmed);
        let mut focus = base_report(5, 3);
        focus.command_seq = Some(7);
        focus.command = Some(focus_command("win-b"));
        reports.push(focus);
        reports
    }
    let run = |reports: &[WorldReport]| {
        let mut engine = WorldEngine::new("gen-1").expect("valid");
        let mut replies = Vec::new();
        for report in reports {
            match engine.handle_report(report) {
                WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => {
                    replies.push(reply);
                }
                WorldOutcome::Duplicate(reply) => replies.push(reply),
                WorldOutcome::Rejected { kind, message } => {
                    panic!("replay rejected: {kind} {message}")
                }
            }
        }
        (replies, engine.state_revision())
    };
    let reports = script();
    let (first, first_rev) = run(&reports);
    let (second, second_rev) = run(&reports);
    assert_eq!(first_rev, second_rev);
    assert_eq!(first, second);
}

#[test]
fn migration_retained_then_confirmed_by_topology() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    fn two_output_report(request_seq: u64, observation_seq: u64) -> WorldReport {
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
            topology: vec![
                OutputTopology {
                    output: OutputId("out-a".to_owned()),
                    native_order: vec!["wa1".to_owned()],
                    current_native: "wa1".to_owned(),
                    complete: true,
                    geometry: Some(rect(0, 0, 800, 600)),
                    primary: false,
                },
                OutputTopology {
                    output: OutputId("out-b".to_owned()),
                    native_order: vec!["wb1".to_owned()],
                    current_native: "wb1".to_owned(),
                    complete: true,
                    geometry: Some(rect(800, 0, 800, 600)),
                    primary: false,
                },
            ],
            bindings: vec![
                WorkspaceBinding {
                    logical_id: "la".to_owned(),
                    native_id: "wa1".to_owned(),
                    output: OutputId("out-a".to_owned()),
                    pinned: false,
                    displaced: false,
                    managed: true,
                },
                WorkspaceBinding {
                    logical_id: "lb".to_owned(),
                    native_id: "wb1".to_owned(),
                    output: OutputId("out-b".to_owned()),
                    pinned: false,
                    displaced: false,
                    managed: true,
                },
            ],
            domains: vec![
                domain_report(
                    "out-a",
                    "wa1",
                    "win-a",
                    vec![win("win-a", "out-a", "wa1", 0)],
                ),
                domain_report(
                    "out-b",
                    "wb1",
                    "win-b",
                    vec![win("win-b", "out-b", "wb1", 0)],
                ),
            ],
            minimized: Vec::new(),
            fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
            migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
            anchor_fallback: crate::admission::AnchorFallback::default(),
            no_space_result: crate::admission::NoSpaceResult::default(),
            default_tiled: true,
            focused_global: String::new(),
            inventory_complete: true,
            gesture_barrier: false,
            feedback: Vec::new(),
        }
    }
    accepted_reply(&mut engine, &two_output_report(1, 1));
    let mut migrate = two_output_report(2, 2);
    // Explicit migration names its endpoints: the core derives the move
    // direction from the retained FULL topology (strict unique-reciprocal),
    // ignoring the carried adapter adjacency below entirely.
    for domain in &mut migrate.domains {
        if domain.domain.id.0 == "out-a" {
            domain.domain.adjacent.insert(
                crate::directional::Direction::Right,
                OutputId("out-b".to_owned()),
            );
        } else if domain.domain.id.0 == "out-b" {
            domain.domain.adjacent.insert(
                crate::directional::Direction::Left,
                OutputId("out-a".to_owned()),
            );
        }
    }
    with_command(
        &mut migrate,
        2,
        61,
        WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: "wa1".to_owned(),
        },
    );
    let reply = accepted_reply(&mut engine, &migrate);
    assert_eq!(reply.transfers.len(), 1);
    assert!(matches!(
        reply.transfers.as_slice(),
        [t] if matches!(t.kind, TransferKind::Migration)
    ));
    // Topology confirms the move: wa1 lists under out-b only.
    let mut confirmed = two_output_report(3, 3);
    confirmed.topology[0].native_order.clear();
    confirmed.topology[0].current_native = "wa1".to_owned();
    confirmed.topology[1].native_order = vec!["wb1".to_owned(), "wa1".to_owned()];
    let settled = accepted_reply(&mut engine, &confirmed);
    assert!(settled.transfers.is_empty());
    // Shared mode refuses migration with a reason before any write.
    let mut shared = two_output_report(4, 4);
    shared.workspace_mode = WorkspaceMode::Shared;
    with_command(
        &mut shared,
        4,
        62,
        WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: "wa1".to_owned(),
        },
    );
    match engine.handle_report(&shared) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "migration-refused"),
        other => panic!("expected migration refusal, got {other:?}"),
    }
}

#[test]
fn migration_refill_supersedes_older_pending_view_for_source() {
    use crate::output_selection::MigrationSourceRefill;
    // Latest-per-output supersession: an older pending select view on the
    // source output never coexists with the migration refill. Only the new
    // refill view wins; conflicting view targets are not left behind.
    fn report(request_seq: u64, current_a: &str) -> WorldReport {
        WorldReport {
            contract_version: WORLD_CONTRACT_VERSION,
            owner: "owner-a".to_owned(),
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            observation_seq: request_seq,
            request_seq,
            request_id: format!("req-{request_seq}"),
            command_seq: None,
            command: None,
            workspace_mode: WorkspaceMode::PerOutputLocal,
            topology: vec![
                OutputTopology {
                    output: OutputId("out-a".to_owned()),
                    native_order: vec!["wa1".to_owned(), "wa2".to_owned()],
                    current_native: current_a.to_owned(),
                    complete: true,
                    geometry: Some(rect(0, 0, 800, 600)),
                    primary: true,
                },
                OutputTopology {
                    output: OutputId("out-b".to_owned()),
                    native_order: vec!["wb1".to_owned()],
                    current_native: "wb1".to_owned(),
                    complete: true,
                    geometry: Some(rect(800, 0, 800, 600)),
                    primary: false,
                },
            ],
            bindings: ["wa1", "wa2"]
                .iter()
                .map(|id| WorkspaceBinding {
                    logical_id: (*id).to_owned(),
                    native_id: (*id).to_owned(),
                    output: OutputId("out-a".to_owned()),
                    pinned: false,
                    displaced: false,
                    managed: true,
                })
                .chain([WorkspaceBinding {
                    logical_id: "wb1".to_owned(),
                    native_id: "wb1".to_owned(),
                    output: OutputId("out-b".to_owned()),
                    pinned: false,
                    displaced: false,
                    managed: true,
                }])
                .collect(),
            domains: ["wa1", "wa2"]
                .iter()
                .map(|id| {
                    domain_report(
                        "out-a",
                        id,
                        &format!("win-{id}"),
                        vec![win(&format!("win-{id}"), "out-a", id, 0)],
                    )
                })
                .chain([domain_report(
                    "out-b",
                    "wb1",
                    "win-b",
                    vec![win("win-b", "out-b", "wb1", 0)],
                )])
                .collect(),
            minimized: Vec::new(),
            fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
            migration_source_refill: MigrationSourceRefill::LastRemaining,
            anchor_fallback: crate::admission::AnchorFallback::default(),
            no_space_result: crate::admission::NoSpaceResult::default(),
            default_tiled: true,
            focused_global: String::new(),
            inventory_complete: true,
            gesture_barrier: false,
            feedback: Vec::new(),
        }
    }
    fn adjacent(report: &mut WorldReport) {
        for domain in &mut report.domains {
            if domain.domain.id.0 == "out-a" {
                domain.domain.adjacent.insert(
                    crate::directional::Direction::Right,
                    OutputId("out-b".to_owned()),
                );
            } else if domain.domain.id.0 == "out-b" {
                domain.domain.adjacent.insert(
                    crate::directional::Direction::Left,
                    OutputId("out-a".to_owned()),
                );
            }
        }
    }
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &report(1, "wa1"));
    // Older pending select view on out-a.
    let mut select = report(2, "wa1");
    adjacent(&mut select);
    with_command(
        &mut select,
        2,
        71,
        WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: "wa2".to_owned(),
        },
    );
    let selected = accepted_reply(&mut engine, &select);
    assert!(
        selected
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::View) && t.target_output.0 == "out-a"),
        "select retains its view"
    );
    // Migration evicts it: only the new refill view wins for out-a.
    let mut migrate = report(3, "wa1");
    adjacent(&mut migrate);
    with_command(
        &mut migrate,
        3,
        72,
        WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: "wa1".to_owned(),
        },
    );
    let reply = accepted_reply(&mut engine, &migrate);
    let views: Vec<_> = reply
        .transfers
        .iter()
        .filter(|t| matches!(t.kind, TransferKind::View) && t.target_output.0 == "out-a")
        .collect();
    assert_eq!(views.len(), 1, "no conflicting view targets remain");
    assert_eq!(views[0].operation_id, "op-72-refill");
    assert_eq!(views[0].target_workspace, "wa2");
    let view = reply
        .outputs
        .iter()
        .find(|o| o.output.0 == "out-a")
        .and_then(|o| o.logical_view.clone());
    assert_eq!(view.as_deref(), Some("wa2"));
}

#[test]
fn fresh_maximized_seeds_ready_controlled_tile() {
    // R-MAX-06 carrier premise: a fresh maximized birth on a tiled,
    // controlled scope publishes as a controlled tile row (allocation
    // present) in the ready reply, so the adapter clears natively once
    // and then applies the approved allocation. A floating scope keeps
    // the overlay instead (no controlled tile to prove).
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let mut born = base_report(1, 1);
    born.domains[0].windows[0].maximized = true;
    let reply = accepted_reply(&mut engine, &born);
    assert!(reply.ready);
    let row = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .expect("birth row");
    assert_eq!(
        row.frame_authority,
        crate::world_report::ScopeAuthority::Controlled
    );
    assert!(
        row.allocation.is_some(),
        "approved allocation rides the reply"
    );
    let scope = reply
        .scopes
        .iter()
        .find(|s| s.scope == "out/ws1")
        .expect("birth scope");
    assert_eq!(
        scope.authority,
        crate::world_report::ScopeAuthority::Controlled
    );
}

#[test]
fn migration_source_refill_report_value_selects_source_refill() {
    use crate::output_selection::MigrationSourceRefill;
    fn refill_report(
        request_seq: u64,
        observation_seq: u64,
        current_a: &str,
        refill: MigrationSourceRefill,
    ) -> WorldReport {
        let ids = ["wa1", "wa2", "wa3", "wa4"];
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
            topology: vec![
                OutputTopology {
                    output: OutputId("out-a".to_owned()),
                    native_order: ids.iter().map(|id| id.to_string()).collect(),
                    current_native: current_a.to_owned(),
                    complete: true,
                    geometry: Some(rect(0, 0, 800, 600)),
                    primary: true,
                },
                OutputTopology {
                    output: OutputId("out-b".to_owned()),
                    native_order: vec!["wb1".to_owned()],
                    current_native: "wb1".to_owned(),
                    complete: true,
                    geometry: Some(rect(800, 0, 800, 600)),
                    primary: false,
                },
            ],
            bindings: ids
                .iter()
                .map(|id| WorkspaceBinding {
                    logical_id: (*id).to_owned(),
                    native_id: (*id).to_owned(),
                    output: OutputId("out-a".to_owned()),
                    pinned: false,
                    displaced: false,
                    managed: true,
                })
                .chain([WorkspaceBinding {
                    logical_id: "wb1".to_owned(),
                    native_id: "wb1".to_owned(),
                    output: OutputId("out-b".to_owned()),
                    pinned: false,
                    displaced: false,
                    managed: true,
                }])
                .collect(),
            domains: ids
                .iter()
                .map(|id| {
                    domain_report(
                        "out-a",
                        id,
                        &format!("win-{id}"),
                        vec![win(&format!("win-{id}"), "out-a", id, 0)],
                    )
                })
                .chain([domain_report(
                    "out-b",
                    "wb1",
                    "win-b",
                    vec![win("win-b", "out-b", "wb1", 0)],
                )])
                .collect(),
            minimized: Vec::new(),
            fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
            migration_source_refill: refill,
            anchor_fallback: crate::admission::AnchorFallback::default(),
            no_space_result: crate::admission::NoSpaceResult::default(),
            default_tiled: true,
            focused_global: String::new(),
            inventory_complete: true,
            gesture_barrier: false,
            feedback: Vec::new(),
        }
    }
    fn adjacent(report: &mut WorldReport) {
        for domain in &mut report.domains {
            if domain.domain.id.0 == "out-a" {
                domain.domain.adjacent.insert(
                    crate::directional::Direction::Right,
                    OutputId("out-b".to_owned()),
                );
            } else if domain.domain.id.0 == "out-b" {
                domain.domain.adjacent.insert(
                    crate::directional::Direction::Left,
                    OutputId("out-a".to_owned()),
                );
            }
        }
    }
    fn source_view(reply: &WorldReply, out: &str) -> Option<String> {
        reply
            .outputs
            .iter()
            .find(|o| o.output.0 == out)
            .and_then(|o| o.logical_view.clone())
    }
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(
        &mut engine,
        &refill_report(1, 1, "wa1", MigrationSourceRefill::LastRemaining),
    );
    // Verified view wa1 -> wa2 records previous wa1 for out-a.
    let mut shown = refill_report(2, 2, "wa2", MigrationSourceRefill::LastRemaining);
    adjacent(&mut shown);
    accepted_reply(&mut engine, &shown);
    // Migrate the shown wa2 under the default: remaining [wa1, wa3, wa4]
    // with the current gone refills the last remaining id.
    let mut migrate = refill_report(3, 3, "wa2", MigrationSourceRefill::LastRemaining);
    adjacent(&mut migrate);
    with_command(
        &mut migrate,
        3,
        71,
        WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: "wa2".to_owned(),
        },
    );
    let reply = accepted_reply(&mut engine, &migrate);
    assert_eq!(source_view(&reply, "out-a").as_deref(), Some("wa4"));
    // The setting change never resets committed state: wa2 stays migrated
    // and the next migration alone follows the new mode. Visit wa1 then
    // wa3 so previous names wa1 while wa4 stays last remaining.
    let mut visit = refill_report(4, 4, "wa1", MigrationSourceRefill::MostRecentlyUsed);
    adjacent(&mut visit);
    accepted_reply(&mut engine, &visit);
    let mut visit = refill_report(5, 5, "wa3", MigrationSourceRefill::MostRecentlyUsed);
    adjacent(&mut visit);
    accepted_reply(&mut engine, &visit);
    let mut migrate = refill_report(6, 6, "wa3", MigrationSourceRefill::MostRecentlyUsed);
    adjacent(&mut migrate);
    with_command(
        &mut migrate,
        6,
        72,
        WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: "wa3".to_owned(),
        },
    );
    let reply = accepted_reply(&mut engine, &migrate);
    // MRU refills the remembered wa1 instead of the last remaining wa4.
    assert_eq!(source_view(&reply, "out-a").as_deref(), Some("wa1"));
}

#[test]
fn command_result_drag_preview_carries_core_rect_and_hover() {
    use crate::boundary::CoreCommand;
    use crate::world_report::CommandResult;
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &paired_report(1, 1));
    // Read-only preview from the live pointer: the rect and hover come
    // from the core plan (never the oracle), with exact domain/revision
    // provenance, coexisting with the unchanged snapshot ordering.
    let mut preview = paired_report(2, 2);
    with_command(
        &mut preview,
        2,
        81,
        WorldCommand::Operation(CoreCommand::DragPreview {
            window: "win-a".to_owned(),
            x: 155,
            y: 50,
            hover_prior: None,
            source: None,
        }),
    );
    let reply = accepted_reply(&mut engine, &preview);
    let Some(CommandResult::DragPreview {
        rect,
        hover,
        domain,
        source,
        target,
        revision,
    }) = reply.command_result.clone()
    else {
        panic!(
            "expected drag-preview result, got {:?}",
            reply.command_result
        );
    };
    assert!(rect.w > 0 && rect.h > 0, "core-derived proposed rect");
    assert_eq!(domain.output.0, "out");
    assert_eq!(domain.workspace.0, "ws1");
    assert_eq!(source, domain, "same-domain capture");
    // Insertion-neighbor identity (never the mover): the overlay orders
    // above this window, so the result must name it exactly.
    assert_eq!(target.0, "win-b", "neighbor target, not the mover");
    let hover = hover.expect("core hover prior always carried");
    assert_eq!(hover.prior_domain, domain);
    assert_eq!(hover.revision, revision);
    // Read-only: the snapshot still homes both windows, no transfer, no
    // effect, revision advanced exactly once for the accepted report.
    assert_eq!(homing(&reply, "win-a"), Some("out/ws1".to_owned()));
    assert_eq!(homing(&reply, "win-b"), Some("out/ws1".to_owned()));
    assert!(reply.transfers.is_empty());
    assert!(reply.effects.is_empty());
    // The exact bytes carry the result for the adapter renderer.
    let encoded = encode_reply_canonical(&reply);
    assert!(encoded.contains("\"op\":\"drag-preview\""));
    assert!(encoded.contains("\"domainOut\":\"out\""));
    assert!(encoded.contains("\"win\":\"win-b\""), "{encoded}");
    assert!(reply_estimate(&reply) >= encoded.len());
}

#[test]
fn command_result_toggle_float_names_window_and_rect() {
    use crate::boundary::CoreCommand;
    use crate::world_report::CommandResult;
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &paired_report(1, 1));
    let mut float = paired_report(2, 2);
    with_command(
        &mut float,
        2,
        82,
        WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: "win-a".to_owned(),
            float_rect: None,
        }),
    );
    let reply = accepted_reply(&mut engine, &float);
    let Some(CommandResult::Float {
        window,
        rect,
        no_space,
    }) = reply.command_result.clone()
    else {
        panic!("expected float result, got {:?}", reply.command_result);
    };
    assert_eq!(window.0, "win-a");
    assert!(rect.is_some(), "explicit float placement carried");
    assert!(!no_space, "intentional float is never a no-space retry");
    // The exception row retains the desired effective frame with no
    // write authority: the adapter owns floating geometry and places
    // exactly once from the result, while unrelated later replies keep
    // carrying the retained property without rewriting it.
    let row = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .expect("float row present");
    assert_eq!(row.effective_frame, rect);
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    // A later unrelated reply preserves the float membership with no
    // result repeat and no native float frame.
    let mut floated = paired_report(3, 3);
    for domain in &mut floated.domains {
        // Session focus followed the tiled survivor at float commit.
        domain.focused = crate::directional::WindowId("win-b".to_owned());
        for window in &mut domain.windows {
            if window.window.0 == "win-a" {
                window.floating = true;
            }
        }
    }
    let later = accepted_reply(&mut engine, &floated);
    assert!(later.command_result.is_none());
    let row = later
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .expect("float membership preserved");
    assert_eq!(row.effective_frame, rect);
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
}

#[test]
fn command_result_active_group_found_and_no_group() {
    use crate::boundary::CoreCommand;
    use crate::world_report::CommandResult;
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &paired_report(1, 1));
    let mut query = paired_report(2, 2);
    with_command(
        &mut query,
        2,
        83,
        WorldCommand::Operation(CoreCommand::ActiveGroup),
    );
    let reply = accepted_reply(&mut engine, &query);
    let Some(CommandResult::ActiveGroup {
        group,
        focused_window,
        members,
        bounds,
        ..
    }) = reply.command_result.clone()
    else {
        panic!(
            "expected active-group result, got {:?}",
            reply.command_result
        );
    };
    assert_eq!(focused_window.0, "win-a");
    assert_eq!(members.len(), 2);
    assert!(bounds.w > 0 && bounds.h > 0);
    assert!(!group.0.is_empty());
    let encoded = encode_reply_canonical(&reply);
    assert!(encoded.contains("\"op\":\"active-group\""));
    // A lone root leaf has no parent group: explicit no-group with a
    // reason (the renderer clears), never an empty silent result.
    let mut solo = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut solo, &base_report(1, 1));
    let mut query = base_report(2, 2);
    with_command(
        &mut query,
        2,
        84,
        WorldCommand::Operation(CoreCommand::ActiveGroup),
    );
    let reply = accepted_reply(&mut solo, &query);
    assert!(matches!(
        reply.command_result,
        Some(CommandResult::NoGroup { .. })
    ));
}

#[test]
fn command_result_move_and_resize_ride_the_snapshot() {
    use crate::boundary::CoreCommand;
    use crate::directional::SameAxisMove;
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &paired_report(1, 1));
    // A local move changes the snapshot geometry with no separate
    // result: the snapshot IS the result (never silently discarded).
    let mut step = paired_report(2, 2);
    with_command(
        &mut step,
        2,
        85,
        WorldCommand::Operation(CoreCommand::Move {
            window: "win-a".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: false,
            same_axis_move: SameAxisMove::GroupWithNeighbor,
        }),
    );
    let reply = accepted_reply(&mut engine, &step);
    assert!(reply.command_result.is_none());
    assert!(reply.windows.iter().all(|w| w.allocation.is_some()));
}

#[test]
fn command_result_drag_drop_pair_resolves_pointer_target() {
    use crate::boundary::CoreCommand;
    use crate::directional::WorkspaceId;
    // Two side-by-side outputs: the mover still homes the source
    // (native reassignment has not happened) while the pointer names
    // the destination. The world op must resolve the pointer domain,
    // never the mover homing.
    fn output_report(request_seq: u64, observation_seq: u64) -> WorldReport {
        let mut report = base_report(request_seq, observation_seq);
        let mut domain_a = domain_report(
            "out-a",
            "wa1",
            "win-a",
            vec![win("win-a", "out-a", "wa1", 0)],
        );
        domain_a.domain.bounds = Rect {
            x: 0,
            y: 0,
            w: 300,
            h: 100,
        };
        let mut domain_b = domain_report(
            "out-b",
            "wb1",
            "win-b",
            vec![win("win-b", "out-b", "wb1", 850)],
        );
        domain_b.domain.bounds = Rect {
            x: 800,
            y: 0,
            w: 300,
            h: 100,
        };
        report.domains = vec![domain_a, domain_b];
        report.topology = vec![
            OutputTopology {
                output: OutputId("out-a".to_owned()),
                native_order: vec!["wa1".to_owned()],
                current_native: "wa1".to_owned(),
                complete: true,
                geometry: None,
                primary: false,
            },
            OutputTopology {
                output: OutputId("out-b".to_owned()),
                native_order: vec!["wb1".to_owned()],
                current_native: "wb1".to_owned(),
                complete: true,
                geometry: None,
                primary: false,
            },
        ];
        report
    }
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &output_report(1, 1));
    let mut drop = output_report(2, 2);
    with_command(
        &mut drop,
        2,
        87,
        WorldCommand::Operation(CoreCommand::DragDrop {
            window: "win-a".to_owned(),
            x: 810,
            y: 50,
            hover_prior: None,
            source: Some(crate::session::DomainKey {
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId("wa1".to_owned()),
            }),
        }),
    );
    let reply = accepted_reply(&mut engine, &drop);
    // Pair transaction committed: the mover arrives in the pointer
    // domain with a retained window transfer for native actuation.
    // The drop geometry rides the snapshot, so no separate result.
    assert!(reply.command_result.is_none());
    assert!(matches!(
        reply.transfers.as_slice(),
        [t] if matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a")
            && t.target_output.0 == "out-b"
            && t.operation_id == "op-87"
    ));
    assert_eq!(homing(&reply, "win-a"), Some("out-b/wb1".to_owned()));
    assert_eq!(
        reply
            .windows
            .iter()
            .filter(|w| w.lifetime.0 == "win-a")
            .count(),
        1
    );
}

#[test]
fn command_result_migration_rekeys_engine_with_members() {
    use crate::world_report::CommandResult;
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    fn two_output_report(request_seq: u64, observation_seq: u64) -> WorldReport {
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
            topology: vec![
                OutputTopology {
                    output: OutputId("out-a".to_owned()),
                    native_order: vec!["wa1".to_owned()],
                    current_native: "wa1".to_owned(),
                    complete: true,
                    geometry: Some(rect(0, 0, 800, 600)),
                    primary: false,
                },
                OutputTopology {
                    output: OutputId("out-b".to_owned()),
                    native_order: vec!["wb1".to_owned()],
                    current_native: "wb1".to_owned(),
                    complete: true,
                    geometry: Some(rect(800, 0, 800, 600)),
                    primary: false,
                },
            ],
            bindings: vec![
                WorkspaceBinding {
                    logical_id: "la".to_owned(),
                    native_id: "wa1".to_owned(),
                    output: OutputId("out-a".to_owned()),
                    pinned: false,
                    displaced: false,
                    managed: true,
                },
                WorkspaceBinding {
                    logical_id: "lb".to_owned(),
                    native_id: "wb1".to_owned(),
                    output: OutputId("out-b".to_owned()),
                    pinned: false,
                    displaced: false,
                    managed: true,
                },
            ],
            domains: vec![
                domain_report(
                    "out-a",
                    "wa1",
                    "win-a",
                    vec![win("win-a", "out-a", "wa1", 0)],
                ),
                domain_report(
                    "out-b",
                    "wb1",
                    "win-b",
                    vec![win("win-b", "out-b", "wb1", 0)],
                ),
            ],
            minimized: Vec::new(),
            fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
            migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
            anchor_fallback: crate::admission::AnchorFallback::default(),
            no_space_result: crate::admission::NoSpaceResult::default(),
            default_tiled: true,
            focused_global: String::new(),
            inventory_complete: true,
            gesture_barrier: false,
            feedback: Vec::new(),
        }
    }
    accepted_reply(&mut engine, &two_output_report(1, 1));
    let mut migrate = two_output_report(2, 2);
    for domain in &mut migrate.domains {
        if domain.domain.id.0 == "out-a" {
            domain.domain.adjacent.insert(
                crate::directional::Direction::Right,
                OutputId("out-b".to_owned()),
            );
        } else if domain.domain.id.0 == "out-b" {
            domain.domain.adjacent.insert(
                crate::directional::Direction::Left,
                OutputId("out-a".to_owned()),
            );
        }
    }
    with_command(
        &mut migrate,
        2,
        86,
        WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: "wa1".to_owned(),
        },
    );
    let reply = accepted_reply(&mut engine, &migrate);
    // The retained Engine tree rekeyed: source slot gone, target slot
    // holds the relocated member with order/gaps preserved.
    let keys = engine.engine().domain_keys();
    assert!(
        !keys
            .iter()
            .any(|k| k.output.0 == "out-a" && k.workspace.0 == "wa1")
    );
    assert!(
        keys.iter()
            .any(|k| k.output.0 == "out-b" && k.workspace.0 == "wa1")
    );
    // Full member set for exactly-once adapter actuation, no duplicate
    // rows, stable transfer identity for truthful partial feedback.
    let Some(CommandResult::Migration {
        source,
        target,
        members,
        floats,
        ..
    }) = reply.command_result.clone()
    else {
        panic!("expected migration result, got {:?}", reply.command_result);
    };
    assert_eq!(source.output.0, "out-a");
    assert_eq!(target.output.0, "out-b");
    assert_eq!(
        members.iter().map(|w| w.0.clone()).collect::<Vec<_>>(),
        vec!["win-a".to_owned()]
    );
    assert_eq!(floats, 0);
    assert!(matches!(
        reply.transfers.as_slice(),
        [t] if matches!(t.kind, TransferKind::Migration) && t.operation_id == "op-86"
    ));
    assert_eq!(
        reply
            .windows
            .iter()
            .filter(|w| w.lifetime.0 == "win-a")
            .count(),
        1
    );
    assert_eq!(homing(&reply, "win-a"), Some("out-b/wa1".to_owned()));
    let encoded = encode_reply_canonical(&reply);
    assert!(encoded.contains("\"op\":\"migrate-workspace\""));
    assert!(reply_estimate(&reply) >= encoded.len());
}

#[test]
fn trailing_selection_creates_stable_token() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    // Both workspaces occupied: trailing creates one stable token.
    let mut trailing = base_report(2, 2);
    with_command(
        &mut trailing,
        2,
        71,
        WorldCommand::SelectTrailing {
            output: OutputId("out".to_owned()),
        },
    );
    let reply = accepted_reply(&mut engine, &trailing);
    assert_eq!(reply.transfers.len(), 1);
    let token = reply.transfers[0].target_workspace.clone();
    assert!(token.starts_with("wl-"));
    let mut again = base_report(3, 2);
    with_command(
        &mut again,
        3,
        72,
        WorldCommand::SelectTrailing {
            output: OutputId("out".to_owned()),
        },
    );
    let reply2 = accepted_reply(&mut engine, &again);
    assert_eq!(reply2.transfers[0].target_workspace, token);
}

#[test]
fn budgets_hold_across_varied_script() {
    use crate::world_report::{
        WORLD_MAX_REPLY_BYTES, encode_reply_canonical, reply_estimate, reply_upper_bound,
    };
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let mut reports = vec![float_report(1, 1, true)];
    let mut send = float_report(2, 2, true);
    send.domains[0].focused = WindowId("win-a".to_owned());
    with_command(
        &mut send,
        2,
        81,
        WorldCommand::Send {
            window: WindowId("win-a".to_owned()),
            target_output: OutputId("out".to_owned()),
            target_workspace: "ws2".to_owned(),
            follow: true,
        },
    );
    reports.push(send);
    let mut unreadable = base_report(3, 3);
    unreadable.domains[1].readable = false;
    unreadable.domains[1].hold = Some(HoldReason::Unreadable);
    unreadable.domains[1].windows.clear();
    unreadable.inventory_complete = false;
    reports.push(unreadable);
    let mut barrier = base_report(4, 4);
    barrier.gesture_barrier = true;
    barrier.command_seq = Some(82);
    barrier.command = Some(focus_command("win-b"));
    reports.push(barrier);
    reports.push(base_report(5, 5));
    for report in &reports {
        // Conservative pre-mutation bound covers the eventual reply.
        let upper = reply_upper_bound(
            report,
            engine.retained.len(),
            engine.engine.domain_keys().len(),
            engine.pending_creations.len(),
        );
        assert!(upper <= WORLD_MAX_REPLY_BYTES);
        match engine.handle_report(report) {
            WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => {
                let exact = encode_reply_canonical(&reply).len();
                assert!(reply_estimate(&reply) >= exact);
                assert!(upper >= exact);
                assert!(exact <= WORLD_MAX_REPLY_BYTES);
            }
            other => panic!("expected accepted, got {other:?}"),
        }
    }
}

#[test]
fn revision_exhaustion_refuses_without_wrap() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    engine.state_revision = u64::MAX;
    match engine.handle_report(&base_report(2, 2)) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "revision-exhausted"),
        other => panic!("expected exhaustion refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision, u64::MAX);
    // Recovery is a fresh incarnation under a new generation, never a
    // wrapped counter.
    let mut fresh = WorldEngine::new("gen-2").expect("valid");
    let mut report = base_report(1, 1);
    report.core_generation = "gen-2".to_owned();
    accepted_reply(&mut fresh, &report);
    assert_eq!(fresh.state_revision(), 1);
}

#[test]
fn creation_exhaustion_refuses_without_reuse_or_alias() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    // Saturate with no live pending token for the output so the next
    // mint must refuse rather than reuse the early-return path.
    engine.pending_creations.clear();
    engine.creation_counter = u64::MAX;
    // Trailing selection refuses: no wrapped token, no alias.
    let mut trailing = base_report(2, 2);
    with_command(
        &mut trailing,
        2,
        51,
        WorldCommand::SelectTrailing {
            output: OutputId("out".to_owned()),
        },
    );
    match engine.handle_report(&trailing) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "creation-exhausted"),
        other => panic!("expected creation exhaustion, got {other:?}"),
    }
    assert!(engine.pending_creations.is_empty());
    assert_eq!(engine.creation_counter, u64::MAX);
    // Lifecycle likewise appends nothing while exhausted.
    let mut sparse = base_report(2, 2);
    sparse.topology[0].native_order = vec!["ws1".to_owned()];
    sparse.domains.remove(1);
    sparse.bindings.remove(1);
    let reply = accepted_reply(&mut engine, &sparse);
    assert!(reply.workspaces.iter().all(|w| w.native_binding.is_some()));
}

#[test]
fn world_route_carries_linux_policy_opt_ins() {
    // The world route is the Linux planner route: fixed-size admission
    // and the maximized focus fence stay on, while direct `Engine`
    // carriers keep exact historical defaults.
    let engine = WorldEngine::new("gen-1").expect("valid");
    assert!(engine.engine().fixed_size_admission());
    assert!(engine.engine().maximized_focus_fence());
    assert!(!Engine::new().fixed_size_admission());
}

#[test]
fn outer_gap_insets_all_edges_with_canonical_inner_gap() {
    // Regression: the world sweep must inset the raw client area by the
    // carried outer gap (legacy planner-protocol parity). 800x600 with
    // inner 8 / outer 12 tiles two windows at the inset edges with one
    // canonical inner gap, and a settings-reload repeat never double
    // insets or resets history.
    fn gap_report(request_seq: u64, observation_seq: u64) -> WorldReport {
        let mut report = base_report(request_seq, observation_seq);
        report.domains = vec![DomainReport {
            domain: OutputDomain {
                id: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws1".to_owned()),
                bounds: rect(0, 0, 800, 600),
                gap: 8,
                adjacent: BTreeMap::new(),
            },
            outer_gap: 12,
            focused: WindowId("win-a".to_owned()),
            windows: vec![
                EngineWindow {
                    window: WindowId("win-a".to_owned()),
                    output: OutputId("out".to_owned()),
                    workspace: WorkspaceId("ws1".to_owned()),
                    rect: rect(0, 0, 400, 600),
                    floating: false,
                    fit_excluded: false,
                    fullscreen: false,
                    maximized: false,
                    sticky: false,
                    fixed_auto: false,
                    fixed_suppress: false,
                    hints: crate::size_hints::WindowSizeHints::none(),
                },
                EngineWindow {
                    window: WindowId("win-b".to_owned()),
                    output: OutputId("out".to_owned()),
                    workspace: WorkspaceId("ws1".to_owned()),
                    rect: rect(400, 0, 400, 600),
                    floating: false,
                    fit_excluded: false,
                    fullscreen: false,
                    maximized: false,
                    sticky: false,
                    fixed_auto: false,
                    fixed_suppress: false,
                    hints: crate::size_hints::WindowSizeHints::none(),
                },
            ],
            closed: Vec::new(),
            readable: true,
            hold: None,
        }];
        report.topology = vec![OutputTopology {
            output: OutputId("out".to_owned()),
            native_order: vec!["ws1".to_owned()],
            current_native: "ws1".to_owned(),
            complete: true,
            geometry: None,
            primary: false,
        }];
        report.bindings = vec![WorkspaceBinding {
            logical_id: "desk-1".to_owned(),
            native_id: "ws1".to_owned(),
            output: OutputId("out".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        }];
        report
    }

    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = accepted_reply(&mut engine, &gap_report(1, 1));
    let frame = |id: &str| {
        reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == id)
            .and_then(|w| w.effective_frame)
            .expect("tiled frame")
    };
    let left = frame("win-a");
    let right = frame("win-b");
    // Inset 800x600 by 12 on every side: 776x576 at (12,12); two leaves
    // split 776 with one inner gap of 8. Seed order is spatial with
    // focus last, so either window may take either leaf: assert the set.
    let mut frames = [left, right];
    frames.sort_by_key(|r| r.x);
    assert_eq!(frames[0], rect(12, 12, 384, 576), "left tile at inset edge");
    assert_eq!(
        frames[1],
        rect(404, 12, 384, 576),
        "right tile at inset edge"
    );
    assert_eq!(
        frames[1].x - (frames[0].x + frames[0].w),
        8,
        "canonical inner gap between tiles"
    );
    for edge in frames {
        assert!(edge.x >= 12 && edge.y >= 12, "inset from top-left {edge:?}");
        assert!(
            edge.x + edge.w <= 788 && edge.y + edge.h <= 588,
            "inset from bottom-right {edge:?}"
        );
    }
    // Settings-reload repeat: identical observation reconciles to the
    // exact same frames with no double inset, no hold, and no history
    // reset (revision advances exactly once per accepted report).
    let reply2 = accepted_reply(&mut engine, &gap_report(2, 2));
    assert_eq!(reply2.windows, reply.windows, "no double inset on reload");
    assert!(
        reply2
            .scopes
            .iter()
            .all(|s| s.authority == ScopeAuthority::Controlled),
        "no hold on identical reload"
    );
    assert_eq!(engine.state_revision(), 2);
    // Subsequent command on the same generation still routes (focus
    // effect proves the retained inset topology survived the reload).
    let mut focus = gap_report(3, 3);
    with_command(&mut focus, 3, 7, focus_command("win-b"));
    let reply3 = accepted_reply(&mut engine, &focus);
    assert_eq!(reply3.effects.len(), 1);
    // Generation rebind reconstructs the same inset geometry from a
    // fresh engine (no history carried, same projection).
    let mut fresh = WorldEngine::new("gen-2").expect("valid");
    let mut rebind = gap_report(1, 1);
    rebind.core_generation = "gen-2".to_owned();
    let reply4 = accepted_reply(&mut fresh, &rebind);
    assert_eq!(reply4.windows, reply.windows, "rebind projects same inset");
}

#[test]
fn outer_gap_value_changes_adopt_without_double_inset() {
    use crate::boundary::CoreCommand;
    use crate::directional::SameAxisMove;
    // Gap VALUE changes (not just repeats): inner 8/outer 12 -> inner
    // 16/outer 20 -> repeat -> back to 8/12 -> zero gaps. Every step
    // adopts through the topology-preserving update-gaps path: leaf
    // identities survive, edges track the new inset exactly, and no step
    // double-insets or holds.
    fn gap_report_with(
        request_seq: u64,
        observation_seq: u64,
        gap: i32,
        outer_gap: i32,
        bounds: Rect,
    ) -> WorldReport {
        let mut report = base_report(request_seq, observation_seq);
        let windows = vec![
            EngineWindow {
                window: WindowId("win-a".to_owned()),
                output: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws1".to_owned()),
                rect: rect(0, 0, 400, 600),
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            },
            EngineWindow {
                window: WindowId("win-b".to_owned()),
                output: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws1".to_owned()),
                rect: rect(400, 0, 400, 600),
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            },
        ];
        report.domains = vec![DomainReport {
            domain: OutputDomain {
                id: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws1".to_owned()),
                bounds,
                gap,
                adjacent: BTreeMap::new(),
            },
            outer_gap,
            focused: WindowId("win-a".to_owned()),
            windows,
            closed: Vec::new(),
            readable: true,
            hold: None,
        }];
        report.topology = vec![OutputTopology {
            output: OutputId("out".to_owned()),
            native_order: vec!["ws1".to_owned()],
            current_native: "ws1".to_owned(),
            complete: true,
            geometry: None,
            primary: false,
        }];
        report.bindings = vec![WorkspaceBinding {
            logical_id: "desk-1".to_owned(),
            native_id: "ws1".to_owned(),
            output: OutputId("out".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        }];
        report
    }

    fn sorted_frames(reply: &WorldReply) -> Vec<Rect> {
        let mut frames: Vec<Rect> = reply
            .windows
            .iter()
            .filter_map(|w| w.effective_frame)
            .collect();
        frames.sort_by_key(|r| (r.x, r.y, r.w, r.h));
        frames
    }

    fn leaves(engine: &WorldEngine, key: &DomainKey) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = engine
            .engine()
            .session(key)
            .expect("retained session")
            .snapshot()
            .windows
            .iter()
            .map(|l| (l.window.0.clone(), l.leaf.0.clone()))
            .collect();
        out.sort();
        out
    }

    fn assert_controlled(reply: &WorldReply) {
        assert!(
            reply
                .scopes
                .iter()
                .all(|s| s.authority == ScopeAuthority::Controlled),
            "no hold, got {:?}",
            reply.scopes,
        );
    }

    let key = DomainKey {
        output: OutputId("out".to_owned()),
        workspace: crate::directional::WorkspaceId("ws1".to_owned()),
    };
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    // Step 1: inner 8 / outer 12 on 800x600 -> 776x576 at (12,12),
    // two 384-wide leaves with one 8px inner gap.
    let reply = accepted_reply(
        &mut engine,
        &gap_report_with(1, 1, 8, 12, rect(0, 0, 800, 600)),
    );
    assert_controlled(&reply);
    assert_eq!(
        sorted_frames(&reply),
        vec![rect(12, 12, 384, 576), rect(404, 12, 384, 576)],
    );
    let first_leaves = leaves(&engine, &key);
    assert_eq!(engine.state_revision(), 1);
    // Step 2: inner 16 / outer 20 -> 760x560 at (20,20), two 372-wide
    // leaves with one 16px inner gap; leaf identities survive the
    // update-gaps adoption.
    let reply = accepted_reply(
        &mut engine,
        &gap_report_with(2, 2, 16, 20, rect(0, 0, 800, 600)),
    );
    assert_controlled(&reply);
    assert_eq!(
        sorted_frames(&reply),
        vec![rect(20, 20, 372, 560), rect(408, 20, 372, 560)],
    );
    assert_eq!(
        sorted_frames(&reply)[1].x - (sorted_frames(&reply)[0].x + sorted_frames(&reply)[0].w),
        16,
        "inner gap tracks the new value on all of left/top/right/bottom",
    );
    assert_eq!(leaves(&engine, &key), first_leaves, "topology stable");
    assert_eq!(engine.state_revision(), 2);
    // Step 3: identical repeat -> byte-identical windows, no double
    // inset, no hold.
    let reply = accepted_reply(
        &mut engine,
        &gap_report_with(3, 3, 16, 20, rect(0, 0, 800, 600)),
    );
    assert_controlled(&reply);
    assert_eq!(
        sorted_frames(&reply),
        vec![rect(20, 20, 372, 560), rect(408, 20, 372, 560)],
        "repeat never double-insets",
    );
    assert_eq!(leaves(&engine, &key), first_leaves);
    assert_eq!(engine.state_revision(), 3);
    // Step 4: back to inner 8 / outer 12 -> exact original frames and
    // the original leaf mapping (no update-gaps state corruption).
    let reply = accepted_reply(
        &mut engine,
        &gap_report_with(4, 4, 8, 12, rect(0, 0, 800, 600)),
    );
    assert_controlled(&reply);
    assert_eq!(
        sorted_frames(&reply),
        vec![rect(12, 12, 384, 576), rect(404, 12, 384, 576)],
    );
    assert_eq!(leaves(&engine, &key), first_leaves);
    // Step 5: zero gaps -> identity inset, edge-to-edge tiles with no
    // inner gap.
    let reply = accepted_reply(
        &mut engine,
        &gap_report_with(5, 5, 0, 0, rect(0, 0, 800, 600)),
    );
    assert_controlled(&reply);
    assert_eq!(
        sorted_frames(&reply),
        vec![rect(0, 0, 400, 600), rect(400, 0, 400, 600)],
    );
    assert_eq!(
        leaves(&engine, &key),
        first_leaves,
        "zero gaps keep topology"
    );
    // Steps 6-7: a Move and a Resize after the gap changes still route
    // on the retained inset topology. They run on a clone so the exact
    // share assertions below stay pristine; shares legitimately change
    // under commands, while edges and the inner gap must keep tracking
    // the carried values.
    let mut fork = engine.clone();
    let mut step = gap_report_with(6, 6, 16, 20, rect(0, 0, 800, 600));
    with_command(
        &mut step,
        6,
        71,
        WorldCommand::Operation(CoreCommand::Move {
            window: "win-a".to_owned(),
            direction: "left".to_owned(),
            cross_output_transfer: false,
            same_axis_move: SameAxisMove::GroupWithNeighbor,
        }),
    );
    let reply = accepted_reply(&mut fork, &step);
    assert!(reply.command_result.is_none());
    assert_controlled(&reply);
    let mut step = gap_report_with(7, 7, 16, 20, rect(0, 0, 800, 600));
    with_command(
        &mut step,
        7,
        72,
        WorldCommand::Operation(CoreCommand::Resize {
            window: "win-a".to_owned(),
            direction: "right".to_owned(),
            mode: "outwards".to_owned(),
            press_index: 0,
        }),
    );
    let reply = accepted_reply(&mut fork, &step);
    assert!(reply.command_result.is_none());
    assert_controlled(&reply);
    let current = sorted_frames(&reply);
    assert_eq!(current[0].x, 20, "post-command left edge tracks inset");
    assert_eq!(
        current[1].x + current[1].w,
        780,
        "post-command right edge tracks inset"
    );
    assert_eq!(
        current[1].x - (current[0].x + current[0].w),
        16,
        "post-command inner gap tracks value"
    );
    // Adapter rebind on the same (pristine-share) engine reproduces the
    // zero-gap projection exactly; a fresh generation bootstraps the new
    // inset identically from observation.
    let mut rebind = gap_report_with(8, 8, 0, 0, rect(0, 0, 800, 600));
    rebind.adapter_incarnation = "adapter-2".to_owned();
    let reply = accepted_reply(&mut engine, &rebind);
    assert_controlled(&reply);
    assert_eq!(
        sorted_frames(&reply),
        vec![rect(0, 0, 400, 600), rect(400, 0, 400, 600)],
        "adapter rebind keeps the current inset projection",
    );
    let mut fresh = WorldEngine::new("gen-2").expect("valid");
    let mut rebind = gap_report_with(1, 1, 16, 20, rect(0, 0, 800, 600));
    rebind.core_generation = "gen-2".to_owned();
    let reply = accepted_reply(&mut fresh, &rebind);
    assert_controlled(&reply);
    assert_eq!(
        sorted_frames(&reply),
        vec![rect(20, 20, 372, 560), rect(408, 20, 372, 560)],
        "generation bootstrap uses the new inset",
    );
}

#[test]
fn outer_gap_float_overlay_and_exhaustion_hold() {
    // A floating overlay member never takes frame authority (no
    // unintended writes), and an outer gap that exhausts the bounds
    // holds the whole scope as constraint-clamp with explicit held rows
    // until a fitting value recovers it.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let mut report = base_report(1, 1);
    report.domains = vec![DomainReport {
        domain: OutputDomain {
            id: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws1".to_owned()),
            bounds: rect(0, 0, 800, 600),
            gap: 8,
            adjacent: BTreeMap::new(),
        },
        outer_gap: 12,
        focused: WindowId("win-a".to_owned()),
        windows: vec![
            EngineWindow {
                window: WindowId("win-a".to_owned()),
                output: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws1".to_owned()),
                rect: rect(0, 0, 400, 600),
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            },
            EngineWindow {
                window: WindowId("win-float".to_owned()),
                output: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws1".to_owned()),
                rect: rect(100, 100, 200, 200),
                floating: true,
                fit_excluded: true,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            },
        ],
        closed: Vec::new(),
        readable: true,
        hold: None,
    }];
    report.topology = vec![OutputTopology {
        output: OutputId("out".to_owned()),
        native_order: vec!["ws1".to_owned()],
        current_native: "ws1".to_owned(),
        complete: true,
        geometry: None,
        primary: false,
    }];
    report.bindings = vec![WorkspaceBinding {
        logical_id: "desk-1".to_owned(),
        native_id: "ws1".to_owned(),
        output: OutputId("out".to_owned()),
        pinned: false,
        displaced: false,
        managed: true,
    }];
    let reply = accepted_reply(&mut engine, &report);
    let float_row = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-float")
        .expect("float row");
    assert_eq!(
        float_row.frame_authority,
        ScopeAuthority::Untouched,
        "held overlay takes no frame writes"
    );
    assert_eq!(
        float_row.membership_authority,
        ScopeAuthority::Untouched,
        "ordinary float takes no membership writes"
    );
    // Exhaustion: outer 64 on a 100x100 work area cannot inset (100 -
    // 128 < 0), so the scope holds as constraint-clamp with explicit
    // held rows instead of projecting or dropping inventory.
    let mut exhausted = report.clone();
    exhausted.request_seq = 2;
    exhausted.request_id = "req-2".to_owned();
    exhausted.observation_seq = 2;
    exhausted.domains[0].domain.bounds = rect(0, 0, 100, 100);
    exhausted.domains[0].outer_gap = 64;
    let reply = accepted_reply(&mut engine, &exhausted);
    let scope = reply
        .scopes
        .iter()
        .find(|s| s.scope == "out/ws1")
        .expect("held scope");
    assert_eq!(
        scope.authority.hold_reason(),
        Some(HoldReason::ConstraintClamp),
    );
    assert!(
        !reply.windows.is_empty(),
        "held inventory stays explicit, never silent absence"
    );
    assert!(
        reply
            .windows
            .iter()
            .all(|w| w.frame_authority.hold_reason().is_some()),
        "every row held while exhausted"
    );
    // Recovery: a fitting value returns the scope to controlled with
    // exact inset frames.
    let mut recovered = report.clone();
    recovered.request_seq = 3;
    recovered.request_id = "req-3".to_owned();
    recovered.observation_seq = 3;
    let reply = accepted_reply(&mut engine, &recovered);
    assert!(
        reply
            .scopes
            .iter()
            .all(|s| s.authority == ScopeAuthority::Controlled),
        "recovery clears the hold"
    );
    let tile = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .and_then(|w| w.effective_frame)
        .expect("recovered tile frame");
    assert_eq!(tile, rect(12, 12, 776, 576), "sole tile fills the inset");
}

const WINDOWS_CONFIG: WorldEngineConfig = WorldEngineConfig {
    fixed_send_admission: true,
    maximized_focus_fence: false,
};

#[test]
fn default_config_matches_new_exactly() {
    // `Default` is exactly the historical constructor: invalid generations
    // refuse on both, flags match, and direct `Engine` carriers keep exact
    // historical defaults.
    assert!(WorldEngine::new("GEN BAD").is_none());
    assert!(WorldEngine::new_with_config("GEN BAD", WorldEngineConfig::default()).is_none());
    let historical = WorldEngine::new("gen-1").expect("valid");
    let configured =
        WorldEngine::new_with_config("gen-1", WorldEngineConfig::default()).expect("valid");
    assert_eq!(
        WorldEngineConfig::default(),
        WorldEngineConfig {
            fixed_send_admission: false,
            maximized_focus_fence: true,
        }
    );
    for engine in [&historical, &configured] {
        assert!(engine.engine().fixed_size_admission());
        assert!(!engine.engine().fixed_send_admission());
        assert!(engine.engine().maximized_focus_fence());
    }
    let windows =
        WorldEngine::new_with_config("gen-1", WINDOWS_CONFIG).expect("valid windows config");
    assert!(windows.engine().fixed_size_admission());
    assert!(windows.engine().fixed_send_admission());
    assert!(!windows.engine().maximized_focus_fence());
    let bare = Engine::new();
    assert!(!bare.fixed_size_admission());
    assert!(!bare.fixed_send_admission());
    assert!(!bare.maximized_focus_fence());
}

#[test]
fn default_config_replies_match_new_exactly() {
    // Same script through both constructors: every reply and the final
    // revision are identical, so existing call sites keep exact behavior.
    fn script() -> Vec<WorldReport> {
        let mut reports = vec![base_report(1, 1)];
        let mut send = base_report(2, 2);
        send.command_seq = Some(5);
        send.command = Some(WorldCommand::Send {
            window: WindowId("win-a".to_owned()),
            target_output: OutputId("out".to_owned()),
            target_workspace: "ws2".to_owned(),
            follow: true,
        });
        reports.push(send);
        reports.push(base_report(3, 2));
        let mut confirmed = base_report(4, 3);
        confirmed.domains[0].windows.clear();
        confirmed.domains[0].focused = WindowId(String::new());
        confirmed.domains[1].windows = vec![
            win("win-b", "out", "ws2", 0),
            win("win-a", "out", "ws2", 50),
        ];
        reports.push(confirmed);
        let mut focus = base_report(5, 3);
        // Confirmed observation above moved win-a; focus the co-located
        // survivor so the effect resolves against retained topology.
        focus.command_seq = Some(7);
        focus.command = Some(focus_command("win-b"));
        reports.push(focus);
        reports
    }
    fn run(engine: &mut WorldEngine, reports: &[WorldReport]) -> (Vec<WorldReply>, u64) {
        let mut replies = Vec::new();
        for report in reports {
            match engine.handle_report(report) {
                WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => {
                    replies.push(reply);
                }
                WorldOutcome::Duplicate(reply) => replies.push(reply),
                WorldOutcome::Rejected { kind, message } => {
                    panic!("replay rejected: {kind} {message}")
                }
            }
        }
        (replies, engine.state_revision())
    }
    let reports = script();
    let mut historical = WorldEngine::new("gen-1").expect("valid");
    let mut configured =
        WorldEngine::new_with_config("gen-1", WorldEngineConfig::default()).expect("valid");
    let (first, first_rev) = run(&mut historical, &reports);
    let (second, second_rev) = run(&mut configured, &reports);
    assert_eq!(first_rev, second_rev);
    assert_eq!(first, second);
}

fn automatic_float_send_report(request_seq: u64, observation_seq: u64) -> WorldReport {
    // Intentional float observed with automatic origin: refuses the
    // slotless send by default, carries under the Windows opt-in.
    let mut report = float_report(request_seq, observation_seq, true);
    report.domains[0]
        .windows
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("floater observed")
        .fixed_auto = true;
    report.command_seq = Some(9);
    report.command = Some(WorldCommand::Send {
        window: WindowId("win-f".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws2".to_owned(),
        follow: true,
    });
    report
}

#[test]
fn automatic_fixed_float_send_refuses_by_default_and_carries_for_windows() {
    let mut default_engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut default_engine, &float_report(1, 1, true));
    // The engine gates first: a focused mover with no tiled link only
    // proceeds when the deferred path deems it eligible, which the
    // automatic origin refuses without the opt-in.
    match default_engine.handle_report(&automatic_float_send_report(2, 2)) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "focus-mismatch"),
        other => panic!("expected automatic send refusal, got {other:?}"),
    }
    assert!(default_engine.retained_transfers().is_empty());

    let mut windows_engine =
        WorldEngine::new_with_config("gen-1", WINDOWS_CONFIG).expect("valid windows config");
    accepted_reply(&mut windows_engine, &float_report(1, 1, true));
    let reply = accepted_reply(&mut windows_engine, &automatic_float_send_report(2, 2));
    assert_eq!(reply.transfers.len(), 1);
    assert_eq!(reply.transfers[0].operation_id, "op-9");
    assert_eq!(homing(&reply, "win-f"), Some("out/ws2".to_owned()));

    // The explicit tile win still refuses under the Windows opt-in.
    let mut suppressed = automatic_float_send_report(3, 2);
    suppressed.command_seq = Some(10);
    suppressed.command = Some(WorldCommand::Send {
        window: WindowId("win-f".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws2".to_owned(),
        follow: false,
    });
    suppressed.domains[0]
        .windows
        .iter_mut()
        .find(|w| w.window.0 == "win-f")
        .expect("floater observed")
        .fixed_suppress = true;
    match windows_engine.handle_report(&suppressed) {
        WorldOutcome::Rejected { .. } => {}
        other => panic!("expected explicit tile-win refusal, got {other:?}"),
    }
}

#[test]
fn windows_config_persists_across_reports_and_incarnation_reset() {
    let mut engine =
        WorldEngine::new_with_config("gen-1", WINDOWS_CONFIG).expect("valid windows config");
    accepted_reply(&mut engine, &float_report(1, 1, true));
    assert!(engine.engine().fixed_send_admission());
    assert!(!engine.engine().maximized_focus_fence());
    accepted_reply(&mut engine, &automatic_float_send_report(2, 2));
    // New adapter incarnation: retained world state resets, construction
    // policy survives, and the automatic send still carries.
    let mut rebound = float_report(1, 1, true);
    rebound.adapter_incarnation = "adapter-2".to_owned();
    accepted_reply(&mut engine, &rebound);
    assert!(engine.engine().fixed_send_admission());
    assert!(!engine.engine().maximized_focus_fence());
    assert!(engine.engine().fixed_size_admission());
    let mut send = automatic_float_send_report(2, 2);
    send.adapter_incarnation = "adapter-2".to_owned();
    let reply = accepted_reply(&mut engine, &send);
    assert_eq!(reply.transfers.len(), 1);
    assert_eq!(homing(&reply, "win-f"), Some("out/ws2".to_owned()));
}

fn maximized_focus_operation(window: &str) -> crate::boundary::CoreCommand {
    // Focus-last seeding places win-a rightmost, so the neighbor lies left.
    crate::boundary::CoreCommand::Focus {
        window: window.to_owned(),
        direction: "left".to_owned(),
        cross_output_transfer: false,
        float_subject: false,
    }
}

#[test]
fn maximized_focus_fence_holds_by_default_and_releases_for_windows() {
    // Paired tiles with the focused subject maximized: the Linux default
    // fences directional focus as `unchanged`, while the Windows carrier
    // (fence off) moves focus to the neighbor.
    fn maximized_report(request_seq: u64, observation_seq: u64) -> WorldReport {
        let mut report = paired_report(request_seq, observation_seq);
        report.domains[0]
            .windows
            .iter_mut()
            .find(|w| w.window.0 == "win-a")
            .expect("subject observed")
            .maximized = true;
        report
    }
    let mut default_engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut default_engine, &maximized_report(1, 1));
    let mut fenced = maximized_report(2, 2);
    with_command(
        &mut fenced,
        2,
        15,
        WorldCommand::Operation(maximized_focus_operation("win-a")),
    );
    match default_engine.handle_report(&fenced) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "unchanged"),
        other => panic!("expected maximized focus fence, got {other:?}"),
    }

    let mut windows_engine =
        WorldEngine::new_with_config("gen-1", WINDOWS_CONFIG).expect("valid windows config");
    accepted_reply(&mut windows_engine, &maximized_report(1, 1));
    let mut released = maximized_report(2, 2);
    with_command(
        &mut released,
        2,
        15,
        WorldCommand::Operation(maximized_focus_operation("win-a")),
    );
    let reply = accepted_reply(&mut windows_engine, &released);
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].id, "fx-15");
    assert_eq!(reply.effects[0].target.0, "win-b");
}

#[test]
fn native_gesture_defers_capacity_repair_until_clear() {
    // D7: geometric capacity repair defers during a native titlebar
    // gesture (`gesture_barrier` without an explicit Session drag). A
    // bounds/gap shrink mid-gesture must not evict: the scope holds as
    // gesture-barrier with topology/order retained, a repeated report
    // repairs nothing further, and the first report after the gesture
    // clears repairs once, newest-first, with live frames.
    fn gesture_report(
        request_seq: u64,
        observation_seq: u64,
        bounds: Rect,
        gap: i32,
        gesture: bool,
    ) -> WorldReport {
        let mut report = base_report(request_seq, observation_seq);
        report.domains = vec![DomainReport {
            domain: OutputDomain {
                id: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws1".to_owned()),
                bounds,
                gap,
                adjacent: BTreeMap::new(),
            },
            outer_gap: 0,
            focused: WindowId("win-a".to_owned()),
            windows: vec![
                win("win-a", "out", "ws1", 0),
                win("win-b", "out", "ws1", 150),
            ],
            closed: Vec::new(),
            readable: true,
            hold: None,
        }];
        report.topology = vec![OutputTopology {
            output: OutputId("out".to_owned()),
            native_order: vec!["ws1".to_owned()],
            current_native: "ws1".to_owned(),
            complete: true,
            geometry: None,
            primary: false,
        }];
        report.bindings = vec![WorkspaceBinding {
            logical_id: "desk-1".to_owned(),
            native_id: "ws1".to_owned(),
            output: OutputId("out".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        }];
        report.gesture_barrier = gesture;
        report
    }

    let key = DomainKey {
        output: OutputId("out".to_owned()),
        workspace: crate::directional::WorkspaceId("ws1".to_owned()),
    };
    let session = |engine: &WorldEngine| {
        engine
            .engine()
            .session(&key)
            .expect("retained session")
            .clone()
    };
    let scope_of = |reply: &WorldReply| {
        reply
            .scopes
            .iter()
            .find(|s| s.scope == "out/ws1")
            .expect("ws1 scope")
            .authority
    };
    fn row_of<'a>(reply: &'a WorldReply, id: &str) -> &'a crate::world_report::WindowDesire {
        reply
            .windows
            .iter()
            .find(|w| w.lifetime.0 == id)
            .expect("window row")
    }

    let roomy = rect(0, 0, 300, 100);
    let shrunk = rect(0, 0, 1, 100);
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seed = accepted_reply(&mut engine, &gesture_report(1, 1, roomy, 0, false));
    assert_eq!(scope_of(&seed), ScopeAuthority::Controlled);
    assert!(!row_of(&seed, "win-a").floating && !row_of(&seed, "win-b").floating);
    assert_eq!(
        engine
            .engine()
            .session(&key)
            .expect("session")
            .capacity_float_count(),
        0
    );
    let seq_a = session(&engine).admission_sequence(&WindowId("win-a".to_owned()));
    let seq_b = session(&engine).admission_sequence(&WindowId("win-b".to_owned()));
    // Deterministic newest-first victim: higher sequence first, untracked
    // oldest, identity descending for ties.
    let expected_victim = match (seq_a, seq_b) {
        (Some(a), Some(b)) if a > b => "win-a",
        (Some(a), Some(b)) if a < b => "win-b",
        (Some(_), None) => "win-a",
        _ => "win-b",
    };
    let live_frame = |id: &str| {
        gesture_report(2, 2, shrunk, 4, true).domains[0]
            .windows
            .iter()
            .find(|w| w.window.0 == id)
            .expect("observed window")
            .rect
    };

    // Shrink mid-gesture: no eviction, no float intent, no focus effects,
    // bounds/gap not settled.
    let held = accepted_reply(&mut engine, &gesture_report(2, 2, shrunk, 4, true));
    assert_eq!(
        scope_of(&held).hold_reason(),
        Some(HoldReason::GestureBarrier),
        "shrunk scope holds as gesture, never repairs"
    );
    assert!(held.effects.is_empty(), "no focus effects mid-gesture");
    assert!(
        held.windows.iter().all(|w| w.frame_operation.is_none()),
        "no float intent minted mid-gesture"
    );
    for id in ["win-a", "win-b"] {
        let row = row_of(&held, id);
        assert!(!row.floating, "{id} stays tiled mid-gesture");
        assert_eq!(
            row.allocation, None,
            "{id} carries no allocation while held"
        );
        assert_eq!(
            row.frame_authority.hold_reason(),
            Some(HoldReason::GestureBarrier),
            "{id} frame held as gesture"
        );
        assert_eq!(
            row.membership_authority.hold_reason(),
            Some(HoldReason::GestureBarrier),
            "{id} membership held as gesture"
        );
    }
    let mid = session(&engine);
    assert_eq!(
        mid.capacity_float_count(),
        0,
        "no capacity floats mid-gesture"
    );
    assert_eq!(mid.admission_sequence(&WindowId("win-a".to_owned())), seq_a);
    assert_eq!(mid.admission_sequence(&WindowId("win-b".to_owned())), seq_b);
    assert!(engine.retained_floats.is_empty());
    let retained = mid
        .domains()
        .iter()
        .find(|d| d.key() == key)
        .expect("domain");
    assert_eq!(
        (retained.bounds, retained.gap),
        (roomy, 0),
        "bounds/gap unsettled mid-gesture"
    );

    // Repeated report under the same gesture: still nothing evicted,
    // order untouched, rows stable.
    let again = accepted_reply(&mut engine, &gesture_report(3, 3, shrunk, 4, true));
    assert_eq!(
        scope_of(&again).hold_reason(),
        Some(HoldReason::GestureBarrier)
    );
    assert_eq!(
        again.windows, held.windows,
        "repeated gesture report refreshes nothing"
    );
    assert!(again.effects.is_empty());
    let repeated = session(&engine);
    assert_eq!(repeated.capacity_float_count(), 0);
    assert_eq!(
        repeated.admission_sequence(&WindowId("win-a".to_owned())),
        seq_a
    );
    assert_eq!(
        repeated.admission_sequence(&WindowId("win-b".to_owned())),
        seq_b
    );
    assert!(engine.retained_floats.is_empty());

    // Gesture clears with the same shrunk geometry: exactly one
    // deterministic newest-first repair with the live frame.
    let repaired = accepted_reply(&mut engine, &gesture_report(4, 4, shrunk, 4, false));
    assert_eq!(scope_of(&repaired), ScopeAuthority::Controlled);
    assert!(
        repaired.effects.is_empty(),
        "repair issues no native focus setter"
    );
    let victim = row_of(&repaired, expected_victim);
    assert!(victim.floating, "newest victim floats after clear");
    assert_eq!(
        victim.float_origin,
        Some(crate::world_report::FloatOrigin::Capacity),
        "victim carries diagnostic capacity origin"
    );
    assert_eq!(
        victim.effective_frame,
        Some(live_frame(expected_victim)),
        "victim keeps its live frame"
    );
    let survivor_id = if expected_victim == "win-a" {
        "win-b"
    } else {
        "win-a"
    };
    let survivor = row_of(&repaired, survivor_id);
    assert!(!survivor.floating, "survivor stays tiled");
    assert!(
        survivor.allocation.is_some(),
        "survivor keeps live geometry"
    );
    assert_eq!(repaired.windows.iter().filter(|w| w.floating).count(), 1);
    let settled = session(&engine);
    assert_eq!(
        settled.capacity_float_count(),
        1,
        "exactly one repair eviction"
    );
    let retained = settled
        .domains()
        .iter()
        .find(|d| d.key() == key)
        .expect("domain");
    assert_eq!(
        (retained.bounds, retained.gap),
        (shrunk, 4),
        "bounds/gap settle after repair"
    );
}

#[test]
fn migration_accepts_qt_unbounded_maximum_with_real_minimum() {
    // Live T05 producer shape on the production DescribeWorld route: an
    // ordinary resizeable client reports a real minimum (176x150) with
    // Qt's unconstrained maximum (i32::MAX x i32::MAX, "no maximum").
    // Hints are advisory-only: the sentinel sanitizes to absent at every
    // use site, never fails the report, and never classifies the client
    // as an automatic float. Bounded fixed-size pins and malformed
    // (negative) hints keep their exact semantics beside it.
    use crate::size_hints::{WindowSizeHints, is_fixed_size};
    let t05 = WindowSizeHints {
        min_w: Some(176),
        min_h: Some(150),
        max_w: Some(i32::MAX),
        max_h: Some(i32::MAX),
    };
    assert_eq!(t05.meaningful_min_w(), Some(176));
    assert_eq!(t05.meaningful_min_h(), Some(150));
    assert_eq!(
        t05.meaningful_max_w(),
        None,
        "unbounded sentinel sanitizes to absent"
    );
    assert_eq!(
        t05.meaningful_max_h(),
        None,
        "unbounded sentinel sanitizes to absent"
    );
    assert!(!is_fixed_size(t05), "unbounded max never pins fixed-size");
    assert!(
        is_fixed_size(WindowSizeHints {
            min_w: Some(400),
            min_h: Some(300),
            max_w: Some(400),
            max_h: Some(300),
        }),
        "bounded equal pins stay fixed"
    );
    assert!(
        !is_fixed_size(WindowSizeHints {
            min_w: Some(-5),
            min_h: Some(100),
            max_w: None,
            max_h: None,
        }),
        "negative hints never count"
    );
    fn t05_report(request_seq: u64, observation_seq: u64, hints: WindowSizeHints) -> WorldReport {
        let mut mover = win("win-t05", "out-a", "wa1", 0);
        mover.hints = hints;
        let mut report = WorldReport {
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
            topology: vec![
                OutputTopology {
                    output: OutputId("out-a".to_owned()),
                    native_order: vec!["wa1".to_owned()],
                    current_native: "wa1".to_owned(),
                    complete: true,
                    geometry: Some(rect(0, 0, 800, 600)),
                    primary: false,
                },
                OutputTopology {
                    output: OutputId("out-b".to_owned()),
                    native_order: vec!["wb1".to_owned()],
                    current_native: "wb1".to_owned(),
                    complete: true,
                    geometry: Some(rect(800, 0, 800, 600)),
                    primary: false,
                },
            ],
            bindings: vec![
                WorkspaceBinding {
                    logical_id: "la".to_owned(),
                    native_id: "wa1".to_owned(),
                    output: OutputId("out-a".to_owned()),
                    pinned: false,
                    displaced: false,
                    managed: true,
                },
                WorkspaceBinding {
                    logical_id: "lb".to_owned(),
                    native_id: "wb1".to_owned(),
                    output: OutputId("out-b".to_owned()),
                    pinned: false,
                    displaced: false,
                    managed: true,
                },
            ],
            domains: vec![
                domain_report(
                    "out-a",
                    "wa1",
                    "win-t05",
                    vec![mover, win("win-plain", "out-a", "wa1", 150)],
                ),
                domain_report(
                    "out-b",
                    "wb1",
                    "win-t",
                    vec![win("win-t", "out-b", "wb1", 0)],
                ),
            ],
            minimized: Vec::new(),
            fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
            migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
            anchor_fallback: crate::admission::AnchorFallback::default(),
            no_space_result: crate::admission::NoSpaceResult::default(),
            default_tiled: true,
            focused_global: String::new(),
            inventory_complete: true,
            gesture_barrier: false,
            feedback: Vec::new(),
        };
        for domain in &mut report.domains {
            if domain.domain.id.0 == "out-a" {
                domain.domain.bounds = rect(0, 0, 800, 600);
            }
        }
        report
    }
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seed = t05_report(1, 1, t05);
    assert!(crate::world_report::validate_report(&seed));
    assert_eq!(crate::world_report::validate_report_reason(&seed), None);
    accepted_reply(&mut engine, &seed);
    let mut migrate = t05_report(2, 2, t05);
    for domain in &mut migrate.domains {
        if domain.domain.id.0 == "out-a" {
            domain.domain.adjacent.insert(
                crate::directional::Direction::Right,
                OutputId("out-b".to_owned()),
            );
        } else if domain.domain.id.0 == "out-b" {
            domain.domain.adjacent.insert(
                crate::directional::Direction::Left,
                OutputId("out-a".to_owned()),
            );
        }
    }
    with_command(
        &mut migrate,
        2,
        61,
        WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: "wa1".to_owned(),
        },
    );
    assert!(crate::world_report::validate_report(&migrate));
    let reply = accepted_reply(&mut engine, &migrate);
    assert_eq!(reply.transfers.len(), 1);
    assert!(matches!(
        reply.transfers.as_slice(),
        [t] if matches!(t.kind, TransferKind::Migration)
    ));
}

// ---- core-owned output topology (2026-10-18) ----
//
// Directional world commands resolve against the retained FULL topology
// through the strict unique-reciprocal helper, never the carried adapter
// adjacency. These tests pin: geometry authority, empty/wrong adjacency
// ignored, partial/missing/invalid/duplicate fencing, same-report updates,
// all scope modes, stale/duplicate/rejected/reset ordering, and
// hotplug disconnect/return.

/// Two-output world with caller-controlled FULL topology: out-a/wa1 left
/// of out-b/wb1 when both rectangles are `Some`. Carried adjacency stays
/// empty so every resolution below must come from geometry.
fn topo_two_output(
    request_seq: u64,
    observation_seq: u64,
    mode: WorkspaceMode,
    geometry_a: Option<Rect>,
    geometry_b: Option<Rect>,
    complete_b: bool,
    inventory_complete: bool,
) -> WorldReport {
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
        workspace_mode: mode,
        topology: vec![
            OutputTopology {
                output: OutputId("out-a".to_owned()),
                native_order: vec!["wa1".to_owned()],
                current_native: "wa1".to_owned(),
                complete: true,
                geometry: geometry_a,
                primary: true,
            },
            OutputTopology {
                output: OutputId("out-b".to_owned()),
                native_order: vec!["wb1".to_owned()],
                current_native: "wb1".to_owned(),
                complete: complete_b,
                geometry: geometry_b,
                primary: false,
            },
        ],
        bindings: vec![
            WorkspaceBinding {
                logical_id: "la".to_owned(),
                native_id: "wa1".to_owned(),
                output: OutputId("out-a".to_owned()),
                pinned: false,
                displaced: false,
                managed: true,
            },
            WorkspaceBinding {
                logical_id: "lb".to_owned(),
                native_id: "wb1".to_owned(),
                output: OutputId("out-b".to_owned()),
                pinned: false,
                displaced: false,
                managed: true,
            },
        ],
        domains: vec![
            domain_report(
                "out-a",
                "wa1",
                "win-a",
                vec![win("win-a", "out-a", "wa1", 0)],
            ),
            domain_report(
                "out-b",
                "wb1",
                "win-b",
                vec![win("win-b", "out-b", "wb1", 0)],
            ),
        ],
        minimized: Vec::new(),
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: String::new(),
        inventory_complete,
        gesture_barrier: false,
        feedback: Vec::new(),
    }
}

fn migrate_a_to_b() -> WorldCommand {
    WorldCommand::MigrateWorkspace {
        source_output: OutputId("out-a".to_owned()),
        target_output: OutputId("out-b".to_owned()),
        workspace: "wa1".to_owned(),
    }
}

fn expect_migration_refused(engine: &mut WorldEngine, report: &WorldReport) {
    match engine.handle_report(report) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "migration-refused"),
        other => panic!("expected migration refusal, got {other:?}"),
    }
}

#[test]
fn core_topology_explicit_migration_ignores_carried_adjacency() {
    use crate::directional::Direction;
    let rect_a = rect(0, 0, 800, 600);
    let rect_b = rect(800, 0, 800, 600);
    for (label, mut seed) in [
        (
            "empty",
            topo_two_output(
                1,
                1,
                WorkspaceMode::PerOutputLocal,
                Some(rect_a),
                Some(rect_b),
                true,
                true,
            ),
        ),
        (
            "contradictory",
            topo_two_output(
                1,
                1,
                WorkspaceMode::PerOutputLocal,
                Some(rect_a),
                Some(rect_b),
                true,
                true,
            ),
        ),
    ] {
        if label == "contradictory" {
            // Carried adjacency claims the neighbor on the wrong side in
            // both directions: the core must still derive right from FULL
            // rects and ignore this entirely.
            for domain in &mut seed.domains {
                if domain.domain.id.0 == "out-a" {
                    domain
                        .domain
                        .adjacent
                        .insert(Direction::Left, OutputId("out-b".to_owned()));
                } else {
                    domain
                        .domain
                        .adjacent
                        .insert(Direction::Right, OutputId("out-a".to_owned()));
                }
            }
        }
        let mut engine = WorldEngine::new("gen-1").expect("valid");
        accepted_reply(&mut engine, &seed);
        let mut migrate = topo_two_output(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            Some(rect_a),
            Some(rect_b),
            true,
            true,
        );
        if label == "contradictory" {
            for domain in &mut migrate.domains {
                if domain.domain.id.0 == "out-a" {
                    domain
                        .domain
                        .adjacent
                        .insert(Direction::Left, OutputId("out-b".to_owned()));
                } else {
                    domain
                        .domain
                        .adjacent
                        .insert(Direction::Right, OutputId("out-a".to_owned()));
                }
            }
        }
        with_command(&mut migrate, 2, 11, migrate_a_to_b());
        let reply = accepted_reply(&mut engine, &migrate);
        assert!(
            reply
                .transfers
                .iter()
                .any(|t| matches!(t.kind, TransferKind::Migration)
                    && t.target_output.0 == "out-b"
                    && t.target_workspace == "wa1"),
            "{label}: migration retained despite carried adjacency"
        );
        let Some(CommandResult::Migration { direction, .. }) = reply.command_result else {
            panic!(
                "{label}: expected migration result, got {:?}",
                reply.command_result
            );
        };
        assert_eq!(
            direction,
            Direction::Right,
            "{label}: geometry-derived direction"
        );
        assert_eq!(homing(&reply, "win-a"), Some("out-b/wa1".to_owned()));
    }
}

#[test]
fn core_topology_directional_focus_ignores_carried_adjacency() {
    use crate::boundary::CoreCommand;
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(
        &mut engine,
        &topo_two_output(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            Some(rect(0, 0, 800, 600)),
            Some(rect(800, 0, 800, 600)),
            true,
            true,
        ),
    );
    // No carried adjacency anywhere: the pair still resolves from FULL
    // rects and the focus lands on the cross-output target.
    let mut focus = topo_two_output(
        2,
        2,
        WorkspaceMode::PerOutputLocal,
        Some(rect(0, 0, 800, 600)),
        Some(rect(800, 0, 800, 600)),
        true,
        true,
    );
    with_command(
        &mut focus,
        2,
        12,
        WorldCommand::Operation(CoreCommand::Focus {
            window: "win-a".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: true,
            float_subject: false,
        }),
    );
    match engine.handle_report(&focus) {
        WorldOutcome::Accepted(reply) => {
            assert_eq!(reply.effects.len(), 1);
            assert_eq!(reply.effects[0].target.0, "win-b");
        }
        other => panic!("expected cross-output focus effect, got {other:?}"),
    }
}

#[test]
fn core_topology_partial_and_invalid_fence_narrowly() {
    let rect_a = rect(0, 0, 800, 600);
    let rect_b = rect(800, 0, 800, 600);
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(
        &mut engine,
        &topo_two_output(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            Some(rect_a),
            Some(rect_b),
            true,
            true,
        ),
    );
    // Missing FULL rect fences migration without touching unrelated ops:
    // an explicit focus on the same partial report still emits.
    let mut partial = topo_two_output(
        2,
        2,
        WorkspaceMode::PerOutputLocal,
        Some(rect_a),
        None,
        true,
        true,
    );
    with_command(&mut partial, 2, 21, migrate_a_to_b());
    expect_migration_refused(&mut engine, &partial);
    let mut focus = topo_two_output(
        3,
        3,
        WorkspaceMode::PerOutputLocal,
        Some(rect_a),
        None,
        true,
        true,
    );
    with_command(&mut focus, 3, 22, focus_command("win-a"));
    let reply = accepted_reply(&mut engine, &focus);
    assert_eq!(reply.effects.len(), 1, "named focus needs no topology");
    // Invalid rect (zero width) fences exactly like a missing one.
    let mut invalid = topo_two_output(
        4,
        4,
        WorkspaceMode::PerOutputLocal,
        Some(rect_a),
        Some(rect(800, 0, 0, 600)),
        true,
        true,
    );
    with_command(&mut invalid, 4, 23, migrate_a_to_b());
    expect_migration_refused(&mut engine, &invalid);
    // Incomplete topology entry fences even with both rects present.
    let mut incomplete = topo_two_output(
        5,
        5,
        WorkspaceMode::PerOutputLocal,
        Some(rect_a),
        Some(rect_b),
        false,
        true,
    );
    with_command(&mut incomplete, 5, 24, migrate_a_to_b());
    expect_migration_refused(&mut engine, &incomplete);
    // Duplicate output ids fence: never derive from the readable subset.
    let mut duplicate = topo_two_output(
        6,
        6,
        WorkspaceMode::PerOutputLocal,
        Some(rect_a),
        Some(rect_b),
        true,
        true,
    );
    duplicate.topology[1].output = OutputId("out-a".to_owned());
    with_command(&mut duplicate, 6, 25, migrate_a_to_b());
    expect_migration_refused(&mut engine, &duplicate);
    // The next complete report recovers immediately.
    let mut recovered = topo_two_output(
        7,
        7,
        WorkspaceMode::PerOutputLocal,
        Some(rect_a),
        Some(rect_b),
        true,
        true,
    );
    with_command(&mut recovered, 7, 26, migrate_a_to_b());
    let reply = accepted_reply(&mut engine, &recovered);
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "complete report recovers"
    );
}

#[test]
fn core_topology_incomplete_inventory_fences_and_recovers() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(
        &mut engine,
        &topo_two_output(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            Some(rect(0, 0, 800, 600)),
            Some(rect(800, 0, 800, 600)),
            true,
            true,
        ),
    );
    // Complete entries but a partial inventory: no stale-edge actuation.
    let mut partial = topo_two_output(
        2,
        2,
        WorkspaceMode::PerOutputLocal,
        Some(rect(0, 0, 800, 600)),
        Some(rect(800, 0, 800, 600)),
        true,
        false,
    );
    with_command(&mut partial, 2, 31, migrate_a_to_b());
    expect_migration_refused(&mut engine, &partial);
    let mut recovered = topo_two_output(
        3,
        3,
        WorkspaceMode::PerOutputLocal,
        Some(rect(0, 0, 800, 600)),
        Some(rect(800, 0, 800, 600)),
        true,
        true,
    );
    with_command(&mut recovered, 3, 32, migrate_a_to_b());
    accepted_reply(&mut engine, &recovered);
}

#[test]
fn core_topology_split_edge_selects_but_gaps_and_corners_refuse() {
    fn three_output_report(request_seq: u64, rects: [Rect; 3]) -> WorldReport {
        let mut report = topo_two_output(
            request_seq,
            request_seq,
            WorkspaceMode::PerOutputLocal,
            Some(rects[0]),
            Some(rects[1]),
            true,
            true,
        );
        report.topology.push(OutputTopology {
            output: OutputId("out-c".to_owned()),
            native_order: vec!["wc1".to_owned()],
            current_native: "wc1".to_owned(),
            complete: true,
            geometry: Some(rects[2]),
            primary: false,
        });
        report.bindings.push(WorkspaceBinding {
            logical_id: "lc".to_owned(),
            native_id: "wc1".to_owned(),
            output: OutputId("out-c".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        });
        report.domains.push(domain_report(
            "out-c",
            "wc1",
            "win-c",
            vec![win("win-c", "out-c", "wc1", 0)],
        ));
        report
    }
    // Split edge: out-a spans both outputs above. Migration ranks by
    // largest shared edge, then left/top: the 800-vs-800 tie selects
    // out-b (left/top), never refusing as ambiguous. Window position
    // plays no role.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(
        &mut engine,
        &three_output_report(
            1,
            [
                rect(0, 600, 1600, 600),
                rect(0, 0, 800, 600),
                rect(800, 0, 800, 600),
            ],
        ),
    );
    let mut split = three_output_report(
        2,
        [
            rect(0, 600, 1600, 600),
            rect(0, 0, 800, 600),
            rect(800, 0, 800, 600),
        ],
    );
    with_command(&mut split, 2, 41, migrate_a_to_b());
    let reply = accepted_reply(&mut engine, &split);
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)
                && t.target_output.0 == "out-b"
                && t.target_workspace == "wa1"),
        "split-edge migration tie breaks left/top to out-b"
    );
    // Gap: out-b sits 100px off the edge, so nothing touches.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(
        &mut engine,
        &three_output_report(
            1,
            [
                rect(0, 0, 800, 600),
                rect(900, 0, 800, 600),
                rect(1800, 0, 800, 600),
            ],
        ),
    );
    let mut gap = three_output_report(
        2,
        [
            rect(0, 0, 800, 600),
            rect(900, 0, 800, 600),
            rect(1800, 0, 800, 600),
        ],
    );
    with_command(&mut gap, 2, 42, migrate_a_to_b());
    expect_migration_refused(&mut engine, &gap);
    // Corner-only contact overlaps zero: no candidate either way.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(
        &mut engine,
        &three_output_report(
            1,
            [
                rect(0, 0, 800, 600),
                rect(800, 600, 800, 600),
                rect(1600, 600, 800, 600),
            ],
        ),
    );
    let mut corner = three_output_report(
        2,
        [
            rect(0, 0, 800, 600),
            rect(800, 600, 800, 600),
            rect(1600, 600, 800, 600),
        ],
    );
    with_command(&mut corner, 2, 43, migrate_a_to_b());
    expect_migration_refused(&mut engine, &corner);
}

#[test]
fn core_topology_same_report_geometry_update_applies() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    // Seed with a gap: migration refuses on the retained map.
    accepted_reply(
        &mut engine,
        &topo_two_output(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            Some(rect(0, 0, 800, 600)),
            Some(rect(2000, 0, 800, 600)),
            true,
            true,
        ),
    );
    let mut gapped = topo_two_output(
        2,
        2,
        WorkspaceMode::PerOutputLocal,
        Some(rect(0, 0, 800, 600)),
        Some(rect(2000, 0, 800, 600)),
        true,
        true,
    );
    with_command(&mut gapped, 2, 51, migrate_a_to_b());
    expect_migration_refused(&mut engine, &gapped);
    // The rearranged report carries its own command: the retained map is
    // already the moved geometry when the command plans.
    let mut moved = topo_two_output(
        3,
        3,
        WorkspaceMode::PerOutputLocal,
        Some(rect(0, 0, 800, 600)),
        Some(rect(800, 0, 800, 600)),
        true,
        true,
    );
    with_command(&mut moved, 3, 52, migrate_a_to_b());
    let reply = accepted_reply(&mut engine, &moved);
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "same-report geometry update plans the migration"
    );
}

#[test]
fn core_topology_shared_mode_resolves_then_scope_refuses() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(
        &mut engine,
        &topo_two_output(
            1,
            1,
            WorkspaceMode::Shared,
            Some(rect(0, 0, 800, 600)),
            Some(rect(800, 0, 800, 600)),
            true,
            true,
        ),
    );
    // Complete FULL topology in shared mode resolves the target, so the
    // directional migration reaches the scope policy (which refuses shared
    // migration): the kind proves topology resolved.
    let mut migrate = topo_two_output(
        2,
        2,
        WorkspaceMode::Shared,
        Some(rect(0, 0, 800, 600)),
        Some(rect(800, 0, 800, 600)),
        true,
        true,
    );
    with_command(
        &mut migrate,
        2,
        61,
        WorldCommand::Operation(crate::boundary::CoreCommand::MigrateWorkspace {
            direction: "right".to_owned(),
        }),
    );
    match engine.handle_report(&migrate) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "migration-refused"),
        other => panic!("expected scope-policy refusal, got {other:?}"),
    }
    // Without FULL geometry the same route fences at the topology, never
    // reaching the scope policy.
    let mut fenced = topo_two_output(
        3,
        3,
        WorkspaceMode::Shared,
        Some(rect(0, 0, 800, 600)),
        None,
        true,
        true,
    );
    with_command(
        &mut fenced,
        3,
        62,
        WorldCommand::Operation(crate::boundary::CoreCommand::MigrateWorkspace {
            direction: "right".to_owned(),
        }),
    );
    match engine.handle_report(&fenced) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "operation-refused"),
        other => panic!("expected topology fence, got {other:?}"),
    }
}

#[test]
fn core_topology_named_send_needs_no_geometry() {
    // Explicit named sends keep their command eligibility: missing FULL
    // topology never blocks them.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(&mut engine, &base_report(1, 1));
    let mut send = base_report(2, 2);
    with_command(
        &mut send,
        2,
        71,
        WorldCommand::Send {
            window: WindowId("win-a".to_owned()),
            target_output: OutputId("out".to_owned()),
            target_workspace: "ws2".to_owned(),
            follow: true,
        },
    );
    let reply = accepted_reply(&mut engine, &send);
    assert_eq!(homing(&reply, "win-a"), Some("out/ws2".to_owned()));
    assert_eq!(reply.transfers.len(), 1);
}

#[test]
fn core_topology_hotplug_disconnect_fences_return_recovers() {
    let rect_a = rect(0, 0, 800, 600);
    let rect_b = rect(800, 0, 800, 600);
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(
        &mut engine,
        &topo_two_output(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            Some(rect_a),
            Some(rect_b),
            true,
            true,
        ),
    );
    // Disconnect: out-b leaves the topology, so the target is unresolvable
    // from the retained map (never the disconnect-time facts).
    let mut gone = topo_two_output(
        2,
        2,
        WorkspaceMode::PerOutputLocal,
        Some(rect_a),
        Some(rect_b),
        true,
        true,
    );
    gone.topology.remove(1);
    gone.bindings.remove(1);
    gone.domains.remove(1);
    with_command(&mut gone, 2, 81, migrate_a_to_b());
    expect_migration_refused(&mut engine, &gone);
    // Return: the complete report restores the map and migration succeeds.
    let mut back = topo_two_output(
        3,
        3,
        WorkspaceMode::PerOutputLocal,
        Some(rect_a),
        Some(rect_b),
        true,
        true,
    );
    with_command(&mut back, 3, 82, migrate_a_to_b());
    let reply = accepted_reply(&mut engine, &back);
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "returned output migrates again"
    );
}

#[test]
fn core_topology_ordering_fences_survive_replay_and_reset() {
    let rect_a = rect(0, 0, 800, 600);
    let rect_b = rect(800, 0, 800, 600);
    let complete = || {
        topo_two_output(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            Some(rect_a),
            Some(rect_b),
            true,
            true,
        )
    };
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let first = accepted_reply(&mut engine, &complete());
    // Identical duplicate returns the cached reply with no new revision.
    match engine.handle_report(&complete()) {
        WorldOutcome::Duplicate(cached) => assert_eq!(cached, first),
        other => panic!("expected duplicate, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), 1);
    // Reused sequence with different input rejects without touching the
    // retained topology: the next fresh migration still succeeds.
    let mut changed = complete();
    changed.focused_global = "win-a".to_owned();
    match engine.handle_report(&changed) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "request-reused"),
        other => panic!("expected reused rejection, got {other:?}"),
    }
    let mut migrate = complete();
    migrate.request_seq = 2;
    migrate.request_id = "req-2".to_owned();
    migrate.observation_seq = 2;
    with_command(&mut migrate, 2, 91, migrate_a_to_b());
    accepted_reply(&mut engine, &migrate);
    // A new adapter incarnation resets retained topology: its partial
    // report fences until a complete report re-retains.
    let mut partial = complete();
    partial.adapter_incarnation = "adapter-2".to_owned();
    partial.request_seq = 1;
    partial.request_id = "req-1".to_owned();
    partial.observation_seq = 3;
    partial.topology[1].geometry = None;
    with_command(&mut partial, 1, 92, migrate_a_to_b());
    expect_migration_refused(&mut engine, &partial);
    let mut re_complete = complete();
    re_complete.adapter_incarnation = "adapter-2".to_owned();
    re_complete.request_seq = 2;
    re_complete.request_id = "req-2".to_owned();
    re_complete.observation_seq = 4;
    with_command(&mut re_complete, 2, 93, migrate_a_to_b());
    let reply = accepted_reply(&mut engine, &re_complete);
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "complete report under the new incarnation re-retains"
    );
}

#[test]
fn unequal_height_occupied_return_r4_recovers_vacated_source() {
    use crate::directional::SameAxisMove;
    // Producer for the KWin `world-r4-unequal-engine` return-leg
    // `operation-refused`: genuinely unequal FULL heights (left 2048x1152,
    // right 1920x1080 at y72, unequal work areas), both outputs occupied,
    // mover shuttles right then immediately left with the global focus
    // riding it. The vacated source keeps reporting the absent mover as
    // its per-domain focus (exactly what the adapter reports for a
    // current-marked domain while the active window lives elsewhere), so
    // the retained Reconcile must converge the focusless source with focus
    // none instead of wedging on `focus-mismatch` and fencing the return
    // pair as `operation-refused`.
    fn r4win(id: &str, out: &str, ws: &str, r: Rect) -> EngineWindow {
        EngineWindow {
            window: WindowId(id.to_owned()),
            output: OutputId(out.to_owned()),
            workspace: WorkspaceId(ws.to_owned()),
            rect: r,
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        }
    }
    fn r4domain(out: &str, ws: &str, bounds: Rect, windows: Vec<EngineWindow>) -> DomainReport {
        DomainReport {
            domain: OutputDomain {
                id: OutputId(out.to_owned()),
                workspace: WorkspaceId(ws.to_owned()),
                bounds,
                gap: 8,
                adjacent: BTreeMap::new(),
            },
            outer_gap: 8,
            // Honest adapter shape: every current-marked domain names the
            // global active window, even the domain the mover just left.
            focused: WindowId("w-move".to_owned()),
            windows,
            closed: Vec::new(),
            readable: true,
            hold: None,
        }
    }
    fn r4report(seq: u64, left: Vec<EngineWindow>, right: Vec<EngineWindow>) -> WorldReport {
        WorldReport {
            contract_version: WORLD_CONTRACT_VERSION,
            owner: "owner-a".to_owned(),
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            observation_seq: seq,
            request_seq: seq,
            request_id: format!("req-{seq}"),
            command_seq: None,
            command: None,
            workspace_mode: WorkspaceMode::PerOutputLocal,
            topology: vec![
                OutputTopology {
                    output: OutputId("out-left".to_owned()),
                    native_order: vec!["ws-a".to_owned()],
                    current_native: "ws-a".to_owned(),
                    complete: true,
                    geometry: Some(Rect {
                        x: 0,
                        y: 0,
                        w: 2048,
                        h: 1152,
                    }),
                    primary: true,
                },
                OutputTopology {
                    output: OutputId("out-right".to_owned()),
                    native_order: vec!["ws-b".to_owned()],
                    current_native: "ws-b".to_owned(),
                    complete: true,
                    geometry: Some(Rect {
                        x: 2048,
                        y: 72,
                        w: 1920,
                        h: 1080,
                    }),
                    primary: false,
                },
            ],
            bindings: vec![
                WorkspaceBinding {
                    logical_id: "ws-a".to_owned(),
                    native_id: "ws-a".to_owned(),
                    output: OutputId("out-left".to_owned()),
                    pinned: false,
                    displaced: false,
                    managed: true,
                },
                WorkspaceBinding {
                    logical_id: "ws-b".to_owned(),
                    native_id: "ws-b".to_owned(),
                    output: OutputId("out-right".to_owned()),
                    pinned: false,
                    displaced: false,
                    managed: true,
                },
            ],
            domains: vec![
                r4domain("out-left", "ws-a", rect(0, 0, 2048, 1100), left),
                r4domain("out-right", "ws-b", rect(2048, 72, 1920, 1000), right),
            ],
            minimized: Vec::new(),
            fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
            migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
            anchor_fallback: crate::admission::AnchorFallback::default(),
            no_space_result: crate::admission::NoSpaceResult::default(),
            default_tiled: true,
            focused_global: "w-move".to_owned(),
            inventory_complete: true,
            gesture_barrier: false,
            feedback: Vec::new(),
        }
    }
    let left_before = vec![
        r4win("w-l1", "out-left", "ws-a", rect(8, 8, 1012, 1084)),
        r4win("w-move", "out-left", "ws-a", rect(1028, 8, 1012, 1084)),
    ];
    let right_before = vec![r4win(
        "w-r1",
        "out-right",
        "ws-b",
        rect(2056, 80, 1904, 984),
    )];
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted_reply(
        &mut engine,
        &r4report(1, left_before.clone(), right_before.clone()),
    );
    // Leg right: mover crosses to the occupied right output.
    let mut go = r4report(2, left_before, right_before);
    with_command(
        &mut go,
        2,
        11,
        WorldCommand::MoveDirectional {
            window: WindowId("w-move".to_owned()),
            direction: "right".to_owned(),
            same_axis: SameAxisMove::GroupWithNeighbor,
        },
    );
    let reply = accepted_reply(&mut engine, &go);
    assert_eq!(homing(&reply, "w-move"), Some("out-right/ws-b".to_owned()));
    assert!(reply.transfers.iter().any(|t| t.operation_id == "op-11"
        && matches!(&t.kind, TransferKind::Window(w) if w.0 == "w-move")
        && t.target_output.0 == "out-right"));
    // Arrival observation: membership actuated, global focus still on the
    // mover, vacated left current still names the absent mover.
    let left_after = vec![r4win("w-l1", "out-left", "ws-a", rect(8, 8, 2032, 1084))];
    let right_after = vec![
        r4win("w-move", "out-right", "ws-b", rect(2056, 80, 948, 984)),
        r4win("w-r1", "out-right", "ws-b", rect(3012, 80, 948, 984)),
    ];
    let settled = accepted_reply(
        &mut engine,
        &r4report(3, left_after.clone(), right_after.clone()),
    );
    assert!(settled.transfers.is_empty());
    // Leg left: the immediate return must plan R4 instead of refusing as
    // `operation-refused` over the held vacated source.
    let mut back = r4report(4, left_after, right_after);
    with_command(
        &mut back,
        4,
        12,
        WorldCommand::MoveDirectional {
            window: WindowId("w-move".to_owned()),
            direction: "left".to_owned(),
            same_axis: SameAxisMove::GroupWithNeighbor,
        },
    );
    match engine.handle_report(&back) {
        WorldOutcome::Accepted(reply) => {
            assert_eq!(homing(&reply, "w-move"), Some("out-left/ws-a".to_owned()));
            assert!(reply.transfers.iter().any(|t| t.operation_id == "op-12"
                && matches!(&t.kind, TransferKind::Window(w) if w.0 == "w-move")
                && t.target_output.0 == "out-left"));
        }
        WorldOutcome::Rejected { kind, message } => {
            panic!("return R4 refused: {kind} {message}")
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
}
