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
    // Directional migration without carried adjacency has no resolvable
    // target and refuses without mutation, never silently local.
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
            default_tiled: true,
            focused_global: String::new(),
            inventory_complete: true,
            gesture_barrier: false,
            feedback: Vec::new(),
        }
    }
    accepted_reply(&mut engine, &two_output_report(1, 1));
    let mut migrate = two_output_report(2, 2);
    // Explicit migration names adapter-resolved adjacent outputs: the
    // production observation always carries full-output-rect adjacency,
    // which the core uses to derive (and fence) the move direction.
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
    let Some(CommandResult::Float { window, rect }) = reply.command_result.clone() else {
        panic!("expected float result, got {:?}", reply.command_result);
    };
    assert_eq!(window.0, "win-a");
    assert!(rect.is_some(), "explicit float placement carried");
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
