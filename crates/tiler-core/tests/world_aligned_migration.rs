//! Aligned whole-workspace migration regressions (platform opt-in):
//! moves land in the destination trailing empty, preserving logical
//! identity, tree/shares, remembered focus and tiling mode while only the
//! native binding changes. Covers delayed/partial/unreadable arrival,
//! closes in both directions, external supersession, feedback settlement,
//! no-replay, the trailing-create pend/drain path, and refusal fencing.
//! Disjoint/global/shared behavior is unchanged (one contrast test).

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldEngineConfig, WorldOutcome};
use tiler_core::world_report::{
    AttemptFeedback, AttemptKind, AttemptOutcome, CommandResult, DomainReport, HoldReason,
    OutputTopology, TransferKind, WORLD_CONTRACT_VERSION, WorkspaceBinding, WorldCommand,
    WorldReport,
};

fn aligned_config() -> WorldEngineConfig {
    WorldEngineConfig {
        fixed_send_admission: false,
        maximized_focus_fence: true,
        aligned_per_output_workspaces: true,
    }
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn win(id: &str, out: &str, ws: &str) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        rect: rect(0, 0, 200, 200),
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

fn domain_with_focus(
    out: &str,
    ws: &str,
    focused: &str,
    windows: Vec<EngineWindow>,
) -> DomainReport {
    DomainReport {
        domain: OutputDomain {
            id: OutputId(out.to_owned()),
            workspace: WorkspaceId(ws.to_owned()),
            bounds: rect(0, 0, 800, 600),
            gap: 8,
            adjacent: BTreeMap::new(),
        },
        outer_gap: 4,
        focused: WindowId(focused.to_owned()),
        windows,
        closed: Vec::new(),
        readable: true,
        hold: None,
    }
}

fn readable_domain(out: &str, ws: &str, windows: Vec<EngineWindow>) -> DomainReport {
    let focused = windows
        .first()
        .map(|w| w.window.0.clone())
        .unwrap_or_default();
    domain_with_focus(out, ws, &focused, windows)
}

fn topo(out: &str, order: &[&str], current: &str, primary: bool) -> OutputTopology {
    OutputTopology {
        output: OutputId(out.to_owned()),
        native_order: order.iter().map(|s| s.to_string()).collect(),
        current_native: current.to_owned(),
        complete: true,
        geometry: Some(if out == "out-2" {
            rect(800, 0, 800, 600)
        } else {
            rect(0, 0, 800, 600)
        }),
        primary,
    }
}

const NATIVES: [&str; 4] = ["ws-1", "ws-2", "ws-3", "ws-4"];

/// Base grid: out-1 shows ws-1 (win-a), out-2 shows ws-2 (win-b).
/// Per-output trailings: out-1 -> ws-2, out-2 -> ws-3.
fn base_report(seq: u64) -> WorldReport {
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
            topo("out-1", &NATIVES, "ws-1", true),
            topo("out-2", &NATIVES, "ws-2", false),
        ],
        bindings: Vec::new(),
        domains: vec![
            readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
            readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        ],
        minimized: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: String::new(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: Vec::new(),
    }
}

fn with_command(report: &mut WorldReport, seq: u64, command_seq: u64, command: WorldCommand) {
    report.observation_seq = seq;
    report.request_seq = seq;
    report.request_id = format!("req-{seq}");
    report.command_seq = Some(command_seq);
    report.command = Some(command);
}

fn migrate_command() -> WorldCommand {
    WorldCommand::MigrateWorkspace {
        source_output: OutputId("out-1".to_owned()),
        target_output: OutputId("out-2".to_owned()),
        workspace: "ws-1".to_owned(),
    }
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

fn logical_of(
    reply: &tiler_core::world_report::WorldReply,
    out: &str,
    native: &str,
) -> Option<String> {
    reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == out && row.native_binding.as_deref() == Some(native))
        .and_then(|row| row.logical_id.clone())
}

fn natives_for(reply: &tiler_core::world_report::WorldReply, out: &str) -> Vec<String> {
    let mut rows: Vec<_> = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == out && row.native_binding.is_some())
        .collect();
    rows.sort_by_key(|row| row.order);
    rows.into_iter()
        .map(|row| row.native_binding.clone().expect("bound"))
        .collect()
}

fn homings(reply: &tiler_core::world_report::WorldReply) -> BTreeMap<String, String> {
    reply
        .windows
        .iter()
        .map(|w| {
            (
                w.lifetime.0.clone(),
                format!("{}/{}", w.output.0, w.workspace),
            )
        })
        .collect()
}

#[test]
fn migrate_lands_in_destination_trailing_preserving_logical() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    let first = accepted(&mut engine, &base_report(1));
    let source_logical = logical_of(&first, "out-1", "ws-1").expect("source logical");
    let dest_placeholder = logical_of(&first, "out-2", "ws-3").expect("dest placeholder");
    assert_ne!(source_logical, dest_placeholder);

    let mut migrate = base_report(2);
    with_command(&mut migrate, 2, 61, migrate_command());
    let reply = accepted(&mut engine, &migrate);

    // Provenance: one member moves from (out-1, ws-1) to the out-2
    // trailing native ws-3; only the native changes.
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
    assert_eq!(source.output.0, "out-1");
    assert_eq!(source.workspace.0, "ws-1");
    assert_eq!(target.output.0, "out-2");
    assert_eq!(target.workspace.0, "ws-3");
    assert_eq!(
        members.iter().map(|w| w.0.clone()).collect::<Vec<_>>(),
        vec!["win-a".to_owned()]
    );
    assert_eq!(floats, 0);
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)
                && t.operation_id == "op-61"
                && t.target_output.0 == "out-2"
                && t.target_workspace == "ws-3"),
        "one migration transfer: {:?}",
        reply.transfers
    );
    // D5 refill (reading B, logical identity): the moved logical is gone
    // and the fresh replacement at (out-1, ws-1) is excluded, so the
    // source refills to the last remaining native different from the
    // migrated one (ws-4) under an explicit core view transfer.
    let refill = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("core refill view");
    assert_eq!(refill.operation_id, "op-61-refill");
    assert_eq!(refill.target_output.0, "out-1");
    assert_eq!(refill.target_workspace, "ws-4");
    // Logical identity rides to the destination; the emptied source cell
    // mints fresh; the displaced empty placeholder is replaced, not moved.
    assert_eq!(
        logical_of(&reply, "out-2", "ws-3"),
        Some(source_logical.clone())
    );
    let source_fresh = logical_of(&reply, "out-1", "ws-1").expect("source cell");
    assert_ne!(source_fresh, source_logical);
    assert_ne!(source_fresh, dest_placeholder);
    // No ring insertion, no renumber: full native-position rings stay.
    let full: Vec<String> = NATIVES.iter().map(|s| s.to_string()).collect();
    assert_eq!(natives_for(&reply, "out-1"), full);
    assert_eq!(natives_for(&reply, "out-2"), full);
    // Engine rekeyed across natives.
    assert!(
        engine
            .engine()
            .domain_keys()
            .iter()
            .any(|k| k.output.0 == "out-2" && k.workspace.0 == "ws-3"),
        "target session: {:?}",
        engine.engine().domain_keys()
    );
    assert!(
        engine
            .engine()
            .domain_keys()
            .iter()
            .all(|k| !(k.output.0 == "out-1" && k.workspace.0 == "ws-1")),
        "source session gone: {:?}",
        engine.engine().domain_keys()
    );
}

#[test]
fn migrate_preserves_tree_shares_focus_and_tiling() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // Two members seed a non-trivial tree on the source cell.
    let mut seeded = base_report(1);
    seeded.domains[0] = domain_with_focus(
        "out-1",
        "ws-1",
        "win-a2",
        vec![
            win("win-a", "out-1", "ws-1"),
            win("win-a2", "out-1", "ws-1"),
        ],
    );
    let first = accepted(&mut engine, &seeded);
    let source_key = tiler_core::session::DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    let target_key = tiler_core::session::DomainKey {
        output: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws-3".to_owned()),
    };
    let pre_tree = engine
        .engine()
        .session(&source_key)
        .expect("source session")
        .tree_for(&source_key)
        .cloned();
    assert!(pre_tree.is_some(), "seeded tree exists");
    let pre_focus = engine
        .engine()
        .session(&source_key)
        .expect("source session")
        .focus();
    let pre_tiling = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-1" && row.native_binding.as_deref() == Some("ws-1"))
        .expect("source row")
        .tiling_mode;

    let mut migrate = seeded.clone();
    with_command(&mut migrate, 2, 61, migrate_command());
    let reply = accepted(&mut engine, &migrate);
    let Some(CommandResult::Migration { members, .. }) = reply.command_result.clone() else {
        panic!("expected migration result");
    };
    assert_eq!(members.len(), 2);

    let moved = engine
        .engine()
        .session(&target_key)
        .expect("target session");
    assert_eq!(moved.tree_for(&target_key), pre_tree.as_ref());
    assert_eq!(moved.focus().1, pre_focus.1, "remembered leaf focus rides");
    assert_eq!(moved.focus().0, Some(target_key.clone()));
    // Tiling mode rides with the moved unit; the emptied source cell
    // reseeds from the saved default on the next report.
    let target_row = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-2" && row.native_binding.as_deref() == Some("ws-3"))
        .expect("target row");
    assert_eq!(target_row.tiling_mode, pre_tiling);
}

#[test]
fn migrate_delayed_arrival_retains_without_premature_settlement() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine, &base_report(1));
    let mut migrate = base_report(2);
    with_command(&mut migrate, 2, 61, migrate_command());
    accepted(&mut engine, &migrate);
    // Actuation lag: identical observation, no command. Both transfers
    // stay outstanding; the reply never duplicates rows or transfers.
    let lagged = accepted(&mut engine, &base_report(3));
    assert!(lagged.command_result.is_none());
    assert_eq!(
        lagged
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "migration transfer retained: {:?}",
        lagged.transfers
    );
    assert_eq!(
        lagged
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::View))
            .count(),
        1,
        "refill view retained until the source shows it: {:?}",
        lagged.transfers
    );
    let wins: Vec<_> = lagged
        .windows
        .iter()
        .filter(|w| w.lifetime.0 == "win-a")
        .collect();
    assert_eq!(wins.len(), 1, "single homing for win-a");
}

#[test]
fn migrate_partial_arrival_settles_only_when_complete() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    let mut seeded = base_report(1);
    seeded.domains[0] = readable_domain(
        "out-1",
        "ws-1",
        vec![
            win("win-a", "out-1", "ws-1"),
            win("win-a2", "out-1", "ws-1"),
        ],
    );
    accepted(&mut engine, &seeded);
    let mut migrate = seeded.clone();
    with_command(&mut migrate, 2, 61, migrate_command());
    accepted(&mut engine, &migrate);

    // Partial arrival: win-a homed at the target, win-a2 still at source.
    let mut partial = base_report(3);
    partial.topology[1].current_native = "ws-3".to_owned();
    partial.domains = vec![
        readable_domain("out-1", "ws-1", vec![win("win-a2", "out-1", "ws-1")]),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        domain_with_focus(
            "out-2",
            "ws-3",
            "win-a",
            vec![win("win-a", "out-2", "ws-3")],
        ),
    ];
    let held = accepted(&mut engine, &partial);
    assert_eq!(
        held.transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "partial arrival keeps the transfer: {:?}",
        held.transfers
    );

    // Full arrival: both members at the target, source cell observed empty,
    // target output showing the trailing ws-3 and source output showing the
    // core refill ws-4 (the adapter actuates both views from the retained
    // transfers; D6 settles only with members plus both views proven).
    let mut arrived = base_report(4);
    arrived.topology[0].current_native = "ws-4".to_owned();
    arrived.topology[1].current_native = "ws-3".to_owned();
    arrived.domains = vec![
        domain_with_focus("out-1", "ws-1", "", Vec::new()),
        domain_with_focus("out-1", "ws-4", "", Vec::new()),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        domain_with_focus(
            "out-2",
            "ws-3",
            "win-a",
            vec![
                win("win-a", "out-2", "ws-3"),
                win("win-a2", "out-2", "ws-3"),
            ],
        ),
    ];
    let settled = accepted(&mut engine, &arrived);
    assert!(
        settled.transfers.is_empty(),
        "full arrival settles: {:?}",
        settled.transfers
    );
    let homing = homings(&settled);
    assert_eq!(homing["win-a"], "out-2/ws-3");
    assert_eq!(homing["win-a2"], "out-2/ws-3");
    assert!(
        settled.effects.is_empty(),
        "arrived members take no admission focus: {:?}",
        settled.effects
    );
}

#[test]
fn migrate_unreadable_destination_holds_then_settles() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine, &base_report(1));
    let mut migrate = base_report(2);
    with_command(&mut migrate, 2, 61, migrate_command());
    accepted(&mut engine, &migrate);

    // The destination reports unreadable mid-flight: members are absent
    // everywhere without a close, so the transfer stays outstanding and
    // the destination rows hold instead of vanishing.
    let mut dark = base_report(3);
    dark.domains[1].readable = false;
    dark.domains[1].hold = Some(HoldReason::Unreadable);
    dark.domains[1].windows.clear();
    let held = accepted(&mut engine, &dark);
    assert_eq!(
        held.transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "unreadable arrival keeps the transfer: {:?}",
        held.transfers
    );

    // Readable arrival settles: members at the target, target showing
    // ws-3, source showing the core refill ws-4.
    let mut arrived = base_report(4);
    arrived.topology[0].current_native = "ws-4".to_owned();
    arrived.topology[1].current_native = "ws-3".to_owned();
    arrived.domains = vec![
        domain_with_focus("out-1", "ws-1", "", Vec::new()),
        domain_with_focus("out-1", "ws-4", "", Vec::new()),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        domain_with_focus(
            "out-2",
            "ws-3",
            "win-a",
            vec![win("win-a", "out-2", "ws-3")],
        ),
    ];
    let settled = accepted(&mut engine, &arrived);
    assert!(
        settled.transfers.is_empty(),
        "readable arrival settles: {:?}",
        settled.transfers
    );
}

#[test]
fn migrate_close_at_source_settles_member() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    let mut seeded = base_report(1);
    seeded.domains[0] = readable_domain(
        "out-1",
        "ws-1",
        vec![
            win("win-a", "out-1", "ws-1"),
            win("win-a2", "out-1", "ws-1"),
        ],
    );
    accepted(&mut engine, &seeded);
    let mut migrate = seeded.clone();
    with_command(&mut migrate, 2, 61, migrate_command());
    accepted(&mut engine, &migrate);

    // win-a closes at the source mid-flight while win-a2 arrives: the
    // close settles its member, the arrival settles the other. The source
    // output shows the core refill ws-4, the target ws-3.
    let mut closed = base_report(3);
    closed.topology[0].current_native = "ws-4".to_owned();
    closed.topology[1].current_native = "ws-3".to_owned();
    closed.domains = vec![
        domain_with_focus("out-1", "ws-1", "", Vec::new()),
        domain_with_focus("out-1", "ws-4", "", Vec::new()),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        domain_with_focus(
            "out-2",
            "ws-3",
            "win-a2",
            vec![win("win-a2", "out-2", "ws-3")],
        ),
    ];
    closed.domains[0].closed = vec![WindowId("win-a".to_owned())];
    let settled = accepted(&mut engine, &closed);
    assert!(
        settled.transfers.is_empty(),
        "source close plus arrival settles: {:?}",
        settled.transfers
    );
}

#[test]
fn migrate_close_at_target_settles_member() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    let mut seeded = base_report(1);
    seeded.domains[0] = readable_domain(
        "out-1",
        "ws-1",
        vec![
            win("win-a", "out-1", "ws-1"),
            win("win-a2", "out-1", "ws-1"),
        ],
    );
    accepted(&mut engine, &seeded);
    let mut migrate = seeded.clone();
    with_command(&mut migrate, 2, 61, migrate_command());
    accepted(&mut engine, &migrate);

    // win-a arrives then closes at the target while win-a2 is still
    // delayed: the transfer stays for the outstanding member only.
    let mut partial = base_report(3);
    partial.topology[1].current_native = "ws-3".to_owned();
    partial.domains = vec![
        readable_domain("out-1", "ws-1", vec![win("win-a2", "out-1", "ws-1")]),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        domain_with_focus("out-2", "ws-3", "", Vec::new()),
    ];
    partial.domains[2].closed = vec![WindowId("win-a".to_owned())];
    let held = accepted(&mut engine, &partial);
    assert_eq!(
        held.transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "target close keeps the transfer for win-a2: {:?}",
        held.transfers
    );

    // win-a2 arrives with both views actuated: everything outstanding
    // is proven, both transfers drop.
    let mut arrived = base_report(4);
    arrived.topology[0].current_native = "ws-4".to_owned();
    arrived.topology[1].current_native = "ws-3".to_owned();
    arrived.domains = vec![
        domain_with_focus("out-1", "ws-1", "", Vec::new()),
        domain_with_focus("out-1", "ws-4", "", Vec::new()),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        domain_with_focus(
            "out-2",
            "ws-3",
            "win-a2",
            vec![win("win-a2", "out-2", "ws-3")],
        ),
    ];
    let settled = accepted(&mut engine, &arrived);
    assert!(
        settled.transfers.is_empty(),
        "late arrival settles: {:?}",
        settled.transfers
    );
}

#[test]
fn migrate_external_move_supersedes_member() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine, &base_report(1));
    let mut migrate = base_report(2);
    with_command(&mut migrate, 2, 61, migrate_command());
    accepted(&mut engine, &migrate);

    // Newer external input moves win-a to a third homing mid-flight:
    // newer input wins and the transfer settles instead of fighting it.
    let mut moved = base_report(3);
    moved.domains = vec![
        domain_with_focus("out-1", "ws-1", "", Vec::new()),
        domain_with_focus(
            "out-2",
            "ws-2",
            "win-a",
            vec![win("win-b", "out-2", "ws-2"), win("win-a", "out-2", "ws-2")],
        ),
    ];
    let reply = accepted(&mut engine, &moved);
    assert!(
        reply
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "external move settles the migration: {:?}",
        reply.transfers
    );
    // The refill view is untouched by the external member move and still
    // waits for the source to show it.
    assert_eq!(
        reply
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::View))
            .count(),
        1,
        "refill outstanding: {:?}",
        reply.transfers
    );
    assert_eq!(homings(&reply)["win-a"], "out-2/ws-2");
}

#[test]
fn migrate_no_replay_on_consumed_sequence() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine, &base_report(1));
    let mut migrate = base_report(2);
    with_command(&mut migrate, 2, 61, migrate_command());
    accepted(&mut engine, &migrate);
    // Same command sequence on a fresh request: consumed, never rerun.
    let mut replay = base_report(3);
    with_command(&mut replay, 3, 61, migrate_command());
    match engine.handle_report(&replay) {
        WorldOutcome::CommandConsumed(reply) => {
            assert!(reply.command_consumed);
            assert!(reply.command_result.is_none());
            assert_eq!(
                reply
                    .transfers
                    .iter()
                    .filter(|t| matches!(t.kind, TransferKind::Migration))
                    .count(),
                1,
                "exactly one retained transfer, no duplicate: {:?}",
                reply.transfers
            );
            assert_eq!(
                reply
                    .transfers
                    .iter()
                    .filter(|t| matches!(t.kind, TransferKind::View))
                    .count(),
                1,
                "paired refill retained once, no duplicate: {:?}",
                reply.transfers
            );
        }
        other => panic!("expected consumed, got {other:?}"),
    }
}

#[test]
fn migrate_refused_proven_settles_to_observed() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine, &base_report(1));
    let mut migrate = base_report(2);
    with_command(&mut migrate, 2, 61, migrate_command());
    accepted(&mut engine, &migrate);
    // The adapter proves the native move refused while observation still
    // shows win-a at the source: settle to observed with a narrow hold.
    let mut refused = base_report(3);
    refused.feedback = vec![AttemptFeedback {
        operation_id: "op-61".to_owned(),
        kind: AttemptKind::Transfer,
        outcome: AttemptOutcome::RefusedProven,
        hold: Some(HoldReason::RefusedTarget),
    }];
    let reply = accepted(&mut engine, &refused);
    assert!(
        reply
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "proven refusal settles the migration: {:?}",
        reply.transfers
    );
    // The paired refill view has no refusal feedback and its view is
    // unproven (source still shows ws-1), so it stays outstanding.
    assert_eq!(
        reply
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::View))
            .count(),
        1,
        "refill waits for its own proof: {:?}",
        reply.transfers
    );
    assert_eq!(homings(&reply)["win-a"], "out-1/ws-1");
}

#[test]
fn migrate_possibly_executed_holds_without_reactuation() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine, &base_report(1));
    let mut migrate = base_report(2);
    with_command(&mut migrate, 2, 61, migrate_command());
    accepted(&mut engine, &migrate);
    let mut ambiguous = base_report(3);
    ambiguous.feedback = vec![AttemptFeedback {
        operation_id: "op-61".to_owned(),
        kind: AttemptKind::Transfer,
        outcome: AttemptOutcome::PossiblyExecuted,
        hold: None,
    }];
    let reply = accepted(&mut engine, &ambiguous);
    let held: Vec<_> = reply
        .transfers
        .iter()
        .filter(|t| matches!(t.kind, TransferKind::Migration))
        .collect();
    assert_eq!(held.len(), 1, "transfer held: {:?}", reply.transfers);
    assert!(held[0].held, "held flag without re-actuation");
    assert_eq!(
        reply
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::View))
            .count(),
        1,
        "refill retained alongside the held migration: {:?}",
        reply.transfers
    );
}

#[test]
fn migrate_empty_workspace_commits_and_settles() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // An empty but observed source cell migrates its (empty) unit onto
    // the destination trailing and settles on the next report.
    let mut empty = base_report(1);
    empty
        .domains
        .push(domain_with_focus("out-1", "ws-2", "", Vec::new()));
    let first = accepted(&mut engine, &empty);
    let source_logical = logical_of(&first, "out-1", "ws-2").expect("source logical");
    let mut migrate = empty.clone();
    with_command(
        &mut migrate,
        2,
        61,
        WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-1".to_owned()),
            target_output: OutputId("out-2".to_owned()),
            workspace: "ws-2".to_owned(),
        },
    );
    let reply = accepted(&mut engine, &migrate);
    let Some(CommandResult::Migration {
        members, target, ..
    }) = reply.command_result.clone()
    else {
        panic!("expected migration result");
    };
    assert!(members.is_empty());
    assert_eq!(target.workspace.0, "ws-3");
    assert_eq!(logical_of(&reply, "out-2", "ws-3"), Some(source_logical));
    // The empty move still refills explicitly, but the source current
    // (ws-1) is unrelated to the migrated ws-2 and still valid, so
    // reading B preserves it as the refill.
    let refill = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("core refill view");
    assert_eq!(refill.operation_id, "op-61-refill");
    assert_eq!(refill.target_output.0, "out-1");
    assert_eq!(refill.target_workspace, "ws-1");
    assert_eq!(
        reply
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1
    );
    let mut settle = empty.clone();
    settle.observation_seq = 3;
    settle.request_seq = 3;
    settle.request_id = "req-3".to_owned();
    // The adapter actuates the target view ws-3; the source already
    // shows the preserved refill ws-1, so both transfers settle.
    settle.topology[1].current_native = "ws-3".to_owned();
    let settled = accepted(&mut engine, &settle);
    assert!(
        settled.transfers.is_empty(),
        "empty migration settles next report: {:?}",
        settled.transfers
    );
}

#[test]
fn migrate_second_flight_while_outstanding_refuses() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine, &base_report(1));
    let mut migrate = base_report(2);
    with_command(&mut migrate, 2, 61, migrate_command());
    accepted(&mut engine, &migrate);
    // A second commit for the same source cell while the first flight is
    // still outstanding refuses instead of moving the fresh empty cell
    // and rebinding logicals underneath the flight.
    let mut again = base_report(3);
    with_command(&mut again, 3, 62, migrate_command());
    match engine.handle_report(&again) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "migration-refused"),
        other => panic!("expected refusal, got {other:?}"),
    }
    let clean = accepted(&mut engine, &base_report(4));
    assert_eq!(
        clean
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "first flight undisturbed: {:?}",
        clean.transfers
    );
    assert_eq!(
        clean
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::View))
            .count(),
        1,
        "paired refill undisturbed: {:?}",
        clean.transfers
    );
}

#[test]
fn migrate_double_pend_to_full_destination_refuses() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // out-2 occupies every live native through the end: no trailing
    // empty, so the move pends on a stable creation token.
    let mut full = base_report(1);
    full.domains = vec![
        readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
        readable_domain("out-2", "ws-1", vec![win("win-c", "out-2", "ws-1")]),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        readable_domain("out-2", "ws-3", vec![win("win-d", "out-2", "ws-3")]),
        readable_domain("out-2", "ws-4", vec![win("win-e", "out-2", "ws-4")]),
    ];
    accepted(&mut engine, &full);
    let mut migrate = full.clone();
    with_command(&mut migrate, 2, 61, migrate_command());
    let pended = accepted(&mut engine, &migrate);
    assert!(
        pended
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "no migration transfer before the binding: {:?}",
        pended.transfers
    );
    let token = pended
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-2" && row.native_binding.is_none())
        .and_then(|row| row.logical_id.clone())
        .expect("pending token row on out-2");
    assert!(token.starts_with("wl-"), "stable creation token");

    // The adapter creates the native desktop: the binding resolves the
    // token, the pending move commits onto ws-5, never onto the token.
    let natives5 = ["ws-1", "ws-2", "ws-3", "ws-4", "ws-5"];
    let mut bound = full.clone();
    bound.observation_seq = 3;
    bound.request_seq = 3;
    bound.request_id = "req-3".to_owned();
    bound.topology = vec![
        topo("out-1", &natives5, "ws-1", true),
        topo("out-2", &natives5, "ws-2", false),
    ];
    bound.bindings = vec![WorkspaceBinding {
        logical_id: token.clone(),
        native_id: "ws-5".to_owned(),
        output: OutputId("out-2".to_owned()),
        pinned: false,
        displaced: false,
        managed: true,
    }];
    let drained = accepted(&mut engine, &bound);
    let migration = drained
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("drained migration transfer");
    assert_eq!(migration.operation_id, "op-61");
    assert_eq!(migration.target_output.0, "out-2");
    assert_eq!(migration.target_workspace, "ws-5");
    // Drained commits retain the explicit refill too: the migrated ws-1
    // is excluded, so the refill is the last remaining ws-5 cell on out-1.
    let refill = drained
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("drained refill view");
    assert_eq!(refill.operation_id, "op-61-refill");
    assert_eq!(refill.target_output.0, "out-1");
    assert_eq!(refill.target_workspace, "ws-5");
    for key in engine.engine().domain_keys() {
        assert!(
            !key.workspace.0.starts_with("wl-"),
            "token never a native: {key:?}"
        );
    }
    assert!(
        engine
            .engine()
            .domain_keys()
            .iter()
            .any(|k| k.output.0 == "out-2" && k.workspace.0 == "ws-5"),
        "target session: {:?}",
        engine.engine().domain_keys()
    );
    // A second pend for the same full destination refuses instead of
    // queueing two moves onto one created native.
    let mut engine2 = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine2, &full);
    let mut first_pend = full.clone();
    with_command(&mut first_pend, 2, 61, migrate_command());
    accepted(&mut engine2, &first_pend);
    let mut second_pend = full.clone();
    with_command(&mut second_pend, 3, 62, migrate_command());
    match engine2.handle_report(&second_pend) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "migration-refused"),
        other => panic!("expected refusal, got {other:?}"),
    }
}

#[test]
fn migrate_same_native_across_outputs() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // out-1 occupies ws-1 and ws-2 (win-a on ws-2); out-2 occupies ws-1
    // only, so its trailing is ws-2: the move keeps the native name but
    // crosses outputs into distinct domain keys.
    let mut grid = base_report(1);
    grid.domains = vec![
        readable_domain("out-1", "ws-1", vec![win("win-z", "out-1", "ws-1")]),
        readable_domain("out-1", "ws-2", vec![win("win-a", "out-1", "ws-2")]),
        readable_domain("out-2", "ws-1", vec![win("win-b", "out-2", "ws-1")]),
    ];
    let first = accepted(&mut engine, &grid);
    let source_logical = logical_of(&first, "out-1", "ws-2").expect("source logical");
    let mut migrate = grid.clone();
    with_command(
        &mut migrate,
        2,
        61,
        WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-1".to_owned()),
            target_output: OutputId("out-2".to_owned()),
            workspace: "ws-2".to_owned(),
        },
    );
    let reply = accepted(&mut engine, &migrate);
    let Some(CommandResult::Migration {
        source,
        target,
        members,
        ..
    }) = reply.command_result.clone()
    else {
        panic!("expected migration result");
    };
    assert_eq!(
        (source.output.0.as_str(), source.workspace.0.as_str()),
        ("out-1", "ws-2")
    );
    assert_eq!(
        (target.output.0.as_str(), target.workspace.0.as_str()),
        ("out-2", "ws-2")
    );
    assert_eq!(members.len(), 1);
    assert_eq!(logical_of(&reply, "out-2", "ws-2"), Some(source_logical));
    // The source current (ws-1) is unrelated to the migrated ws-2 and
    // still valid, so reading B preserves it as the refill.
    let refill = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("core refill view");
    assert_eq!(refill.target_output.0, "out-1");
    assert_eq!(refill.target_workspace, "ws-1");
}

#[test]
fn migrate_directional_routes_in_aligned_mode() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine, &base_report(1));
    let mut migrate = base_report(2);
    with_command(
        &mut migrate,
        2,
        61,
        WorldCommand::MigrateDirectional {
            source_output: OutputId("out-1".to_owned()),
            workspace: "ws-1".to_owned(),
            direction: "right".to_owned(),
        },
    );
    let reply = accepted(&mut engine, &migrate);
    let Some(CommandResult::Migration { source, target, .. }) = reply.command_result.clone() else {
        panic!("expected migration result, got {:?}", reply.command_result);
    };
    assert_eq!(source.workspace.0, "ws-1");
    assert_eq!(target.output.0, "out-2");
    assert_eq!(target.workspace.0, "ws-3");
}

#[test]
fn migrate_refusals_fence_narrowly() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine, &base_report(1));
    let refused =
        |engine: &mut WorldEngine, report: &WorldReport| match engine.handle_report(report) {
            WorldOutcome::Rejected { kind, .. } => kind,
            other => panic!("expected refusal, got {other:?}"),
        };
    // Unknown workspace (not live).
    let mut unknown = base_report(2);
    with_command(
        &mut unknown,
        2,
        61,
        WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-1".to_owned()),
            target_output: OutputId("out-2".to_owned()),
            workspace: "ws-9".to_owned(),
        },
    );
    assert_eq!(refused(&mut engine, &unknown), "migration-refused");
    // Unknown target output.
    let mut no_output = base_report(3);
    with_command(
        &mut no_output,
        3,
        62,
        WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-1".to_owned()),
            target_output: OutputId("out-9".to_owned()),
            workspace: "ws-1".to_owned(),
        },
    );
    assert_eq!(refused(&mut engine, &no_output), "migration-refused");
    // Partial inventory fences even with complete topology.
    let mut partial = base_report(4);
    partial.inventory_complete = false;
    with_command(&mut partial, 4, 63, migrate_command());
    assert_eq!(refused(&mut engine, &partial), "migration-refused");
    // Unreadable source scope cannot prove its members: refuse with no
    // mutation (unreadable scopes never enter the held set, so the
    // readable-domain gate fences here).
    let mut dark = base_report(5);
    dark.domains[0].readable = false;
    dark.domains[0].hold = Some(HoldReason::Unreadable);
    dark.domains[0].windows.clear();
    with_command(&mut dark, 5, 64, migrate_command());
    assert_eq!(refused(&mut engine, &dark), "migration-refused");
    // A refused command mutates nothing observable: the next clean
    // observation carries no transfers and stable logicals.
    let clean = accepted(&mut engine, &base_report(6));
    assert!(clean.transfers.is_empty());
}

#[test]
fn migrate_floating_source_carries_without_engine_tree() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine, &base_report(1));
    // Floating scopes are adapter-owned with no retained session: there
    // is no tree to relocate, so the report's workspace-bound contents
    // carry by sessionless membership remap (no refusal on the absent
    // tree, no fabricated tree or focus).
    let mut float = base_report(2);
    with_command(
        &mut float,
        2,
        61,
        WorldCommand::SetWorkspaceTiling {
            output: OutputId("out-1".to_owned()),
            workspace: "ws-1".to_owned(),
            tiled: false,
        },
    );
    accepted(&mut engine, &float);
    let mut migrate = base_report(3);
    with_command(&mut migrate, 3, 62, migrate_command());
    let reply = accepted(&mut engine, &migrate);
    let Some(CommandResult::Migration {
        source,
        target,
        members,
        ..
    }) = reply.command_result.clone()
    else {
        panic!("expected migration result, got {:?}", reply.command_result);
    };
    assert_eq!(
        (source.output.0.as_str(), source.workspace.0.as_str()),
        ("out-1", "ws-1")
    );
    assert_eq!(
        (target.output.0.as_str(), target.workspace.0.as_str()),
        ("out-2", "ws-3")
    );
    assert_eq!(
        members.iter().map(|w| w.0.clone()).collect::<Vec<_>>(),
        vec!["win-a".to_owned()]
    );
    // The floating mode rides with the moved unit.
    let target_row = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-2" && row.native_binding.as_deref() == Some("ws-3"))
        .expect("target row");
    assert_eq!(
        target_row.tiling_mode,
        tiler_core::world_report::TilingMode::Floating
    );
    let migration = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("migration transfer");
    assert_eq!(migration.operation_id, "op-62");
    let refill = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("core refill view");
    assert_eq!(refill.operation_id, "op-62-refill");
    assert_eq!(refill.target_output.0, "out-1");
    assert_eq!(refill.target_workspace, "ws-4");
    // Arrival with both views actuated settles the sessionless carry
    // through the same member-homing settlement.
    let mut arrived = base_report(4);
    arrived.topology[0].current_native = "ws-4".to_owned();
    arrived.topology[1].current_native = "ws-3".to_owned();
    arrived.domains = vec![
        domain_with_focus("out-1", "ws-1", "", Vec::new()),
        domain_with_focus("out-1", "ws-4", "", Vec::new()),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        domain_with_focus(
            "out-2",
            "ws-3",
            "win-a",
            vec![win("win-a", "out-2", "ws-3")],
        ),
    ];
    let settled = accepted(&mut engine, &arrived);
    assert!(
        settled.transfers.is_empty(),
        "floating arrival settles: {:?}",
        settled.transfers
    );
}

#[test]
fn disjoint_migration_keeps_native_identity() {
    // Contrast lock: without the opt-in, the same command moves the
    // native id with the workspace (insert-after plan), it never
    // rebinds onto the destination trailing.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &base_report(1));
    let mut migrate = base_report(2);
    with_command(&mut migrate, 2, 61, migrate_command());
    let reply = accepted(&mut engine, &migrate);
    let Some(CommandResult::Migration { source, target, .. }) = reply.command_result.clone() else {
        panic!("expected migration result");
    };
    assert_eq!(source.workspace.0, "ws-1");
    assert_eq!(target.workspace.0, "ws-1");
    assert_eq!(target.output.0, "out-2");
}

#[test]
fn migrate_refill_prefers_mru_previous_over_last_remaining() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // Prime per-output history: out-1 shows ws-2, then ws-1.
    let mut first = base_report(1);
    first.topology[0].current_native = "ws-2".to_owned();
    first.domains = vec![
        readable_domain("out-1", "ws-2", vec![win("win-z", "out-1", "ws-2")]),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
    ];
    accepted(&mut engine, &first);
    let mut second = base_report(2);
    second.domains = vec![
        readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
        readable_domain("out-1", "ws-2", vec![win("win-z", "out-1", "ws-2")]),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
    ];
    accepted(&mut engine, &second);

    // MRU refill: the previous live source native ws-2 wins over the last
    // remaining ws-4. A LastRemaining control on the same state picks ws-4.
    let mut migrate = second.clone();
    migrate.migration_source_refill =
        tiler_core::output_selection::MigrationSourceRefill::MostRecentlyUsed;
    with_command(&mut migrate, 3, 61, migrate_command());
    let reply = accepted(&mut engine, &migrate);
    assert!(
        reply.command_result.is_some(),
        "migration commits: {:?}",
        reply.command_result
    );
    let refill = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("core refill view");
    assert_eq!(refill.operation_id, "op-61-refill");
    assert_eq!(refill.target_output.0, "out-1");
    assert_eq!(refill.target_workspace, "ws-2");

    let mut engine2 = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine2, &first);
    accepted(&mut engine2, &second);
    let mut migrate2 = second.clone();
    with_command(&mut migrate2, 3, 61, migrate_command());
    let reply2 = accepted(&mut engine2, &migrate2);
    let refill2 = reply2
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("core refill view");
    assert_eq!(refill2.target_workspace, "ws-4");
}

#[test]
fn migrate_carries_minimized_and_leaves_sticky() {
    use tiler_core::seed::EngineWindow as EW;
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    let mut seeded = base_report(1);
    let sticky_win = EW {
        sticky: true,
        ..win("win-s", "out-1", "ws-1")
    };
    seeded.domains[0] = readable_domain(
        "out-1",
        "ws-1",
        vec![win("win-a", "out-1", "ws-1"), sticky_win],
    );
    seeded.minimized = vec![tiler_core::world_report::MinimizedEntry {
        window: WindowId("win-m".to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        sticky: false,
    }];
    accepted(&mut engine, &seeded);

    let mut migrate = seeded.clone();
    with_command(&mut migrate, 2, 61, migrate_command());
    let reply = accepted(&mut engine, &migrate);
    let Some(CommandResult::Migration { members, .. }) = reply.command_result.clone() else {
        panic!("expected migration result, got {:?}", reply.command_result);
    };
    // Minimized rides; sticky stays on the source (D7).
    assert_eq!(
        members.iter().map(|w| w.0.clone()).collect::<Vec<_>>(),
        vec!["win-a".to_owned(), "win-m".to_owned()]
    );
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("migration transfer");
    assert_eq!(transfer.operation_id, "op-61");

    // Arrival: tiled member and minimized both homed at the target, sticky
    // still homed at the source, both views actuated.
    let mut arrived = base_report(3);
    arrived.topology[0].current_native = "ws-4".to_owned();
    arrived.topology[1].current_native = "ws-3".to_owned();
    arrived.domains = vec![
        domain_with_focus(
            "out-1",
            "ws-1",
            "win-s",
            vec![win("win-s", "out-1", "ws-1")],
        ),
        domain_with_focus("out-1", "ws-4", "", Vec::new()),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        domain_with_focus(
            "out-2",
            "ws-3",
            "win-a",
            vec![win("win-a", "out-2", "ws-3")],
        ),
    ];
    arrived.domains[3].windows[0].sticky = false;
    arrived.domains[0].windows[0].sticky = true;
    arrived.minimized = vec![tiler_core::world_report::MinimizedEntry {
        window: WindowId("win-m".to_owned()),
        output: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws-3".to_owned()),
        sticky: false,
    }];
    let settled = accepted(&mut engine, &arrived);
    assert!(
        settled.transfers.is_empty(),
        "arrival with sticky left behind settles: {:?}",
        settled.transfers
    );
    let homing = homings(&settled);
    assert_eq!(homing["win-s"], "out-1/ws-1");
}

#[test]
fn migrate_second_source_to_reserved_trailing_refuses() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // out-1 occupies ws-1 (win-a) and ws-2 (win-x); out-2 occupies ws-2
    // (win-b), so the destination trailing is ws-3 for both sources.
    let mut grid = base_report(1);
    grid.domains = vec![
        readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
        readable_domain("out-1", "ws-2", vec![win("win-x", "out-1", "ws-2")]),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
    ];
    accepted(&mut engine, &grid);
    let mut first = grid.clone();
    with_command(&mut first, 2, 61, migrate_command());
    let committed = accepted(&mut engine, &first);
    assert_eq!(
        committed
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "first migration owns the ws-3 trailing: {:?}",
        committed.transfers
    );
    // A second source migrating toward the same destination trailing
    // while the first flight is still pending refuses instead of
    // double-booking ws-3 before arrival.
    let mut second = grid.clone();
    with_command(
        &mut second,
        3,
        62,
        WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-1".to_owned()),
            target_output: OutputId("out-2".to_owned()),
            workspace: "ws-2".to_owned(),
        },
    );
    match engine.handle_report(&second) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "migration-refused"),
        other => panic!("expected refusal, got {other:?}"),
    }
    // A window send onto the reserved trailing refuses the same way.
    let mut send = grid.clone();
    with_command(
        &mut send,
        4,
        63,
        WorldCommand::Send {
            window: WindowId("win-x".to_owned()),
            target_output: OutputId("out-2".to_owned()),
            target_workspace: "ws-3".to_owned(),
            follow: false,
        },
    );
    match engine.handle_report(&send) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-target-unknown"),
        other => panic!("expected refusal, got {other:?}"),
    }
    let mut clean = base_report(5);
    clean.domains = grid.domains.clone();
    let clean = accepted(&mut engine, &clean);
    assert_eq!(
        clean
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "first flight undisturbed: {:?}",
        clean.transfers
    );
}

#[test]
fn migrate_members_arrived_but_views_old_holds_both_directions() {
    // Members at the destination with either view still old keep both
    // transfers: no premature settlement in either direction.
    let cases = vec![
        // out-1/ws-1 (win-a) -> out-2/ws-3, refill out-1/ws-4.
        (
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
            ],
            migrate_command(),
            vec![
                domain_with_focus("out-1", "ws-1", "", Vec::new()),
                domain_with_focus("out-1", "ws-4", "", Vec::new()),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
                domain_with_focus(
                    "out-2",
                    "ws-3",
                    "win-a",
                    vec![win("win-a", "out-2", "ws-3")],
                ),
            ],
            "ws-4",
            "ws-3",
            "out-1",
        ),
        // out-2/ws-2 (win-b) -> out-1/ws-3, refill out-2/ws-4. out-1
        // occupies ws-1 and ws-2, so its trailing is ws-3.
        (
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-1", "ws-2", vec![win("win-x", "out-1", "ws-2")]),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
            ],
            WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-2".to_owned()),
                target_output: OutputId("out-1".to_owned()),
                workspace: "ws-2".to_owned(),
            },
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-1", "ws-2", vec![win("win-x", "out-1", "ws-2")]),
                domain_with_focus(
                    "out-1",
                    "ws-3",
                    "win-b",
                    vec![win("win-b", "out-1", "ws-3")],
                ),
                domain_with_focus("out-2", "ws-2", "", Vec::new()),
                domain_with_focus("out-2", "ws-4", "", Vec::new()),
            ],
            "ws-4",
            "ws-3",
            "out-2",
        ),
    ];
    for (seed_domains, command, arrived_domains, refill_ws, target_ws, source_out) in cases {
        let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
        let mut seed = base_report(1);
        seed.domains = seed_domains;
        accepted(&mut engine, &seed);
        let mut migrate = seed.clone();
        with_command(&mut migrate, 2, 61, command);
        let committed = accepted(&mut engine, &migrate);
        let target_out = if source_out == "out-1" {
            "out-2"
        } else {
            "out-1"
        };
        let migration = committed
            .transfers
            .iter()
            .find(|t| matches!(t.kind, TransferKind::Migration))
            .expect("migration retained");
        assert_eq!(migration.target_workspace, target_ws);
        let refill = committed
            .transfers
            .iter()
            .find(|t| matches!(t.kind, TransferKind::View))
            .expect("refill retained");
        assert_eq!(refill.target_output.0, source_out);
        assert_eq!(refill.target_workspace, refill_ws);
        // Members arrived but both currents still old: everything stays.
        let mut lagged = base_report(3);
        lagged.domains = arrived_domains;
        let held = accepted(&mut engine, &lagged);
        assert_eq!(
            held.transfers
                .iter()
                .filter(|t| matches!(t.kind, TransferKind::Migration))
                .count(),
            1,
            "members arrived but views old keeps the migration ({source_out})"
        );
        assert_eq!(
            held.transfers
                .iter()
                .filter(|t| matches!(t.kind, TransferKind::View))
                .count(),
            1,
            "refill view still outstanding ({source_out})"
        );
        // Both views actuated (target shows the trailing, source shows
        // the refill): members plus proofs settle everything.
        let mut proved = base_report(4);
        proved.domains = lagged.domains.clone();
        for topology in &mut proved.topology {
            if topology.output.0 == target_out {
                topology.current_native = target_ws.to_owned();
            } else if topology.output.0 == source_out {
                topology.current_native = refill_ws.to_owned();
            }
        }
        let settled = accepted(&mut engine, &proved);
        assert!(
            settled.transfers.is_empty(),
            "members plus both views settle ({source_out}): {:?}",
            settled.transfers
        );
    }
}

#[test]
#[allow(clippy::type_complexity)]
fn migrate_delayed_preserves_topology_both_directions() {
    // Delayed native observations (identical pre-migration grid, fresh seq,
    // no feedback) must preserve the relocated tree/order/shares, focus,
    // tiling mode and logical identity in both directions with unequal
    // nontrivial membership. No replay: follow-up replies carry no result.
    let cases: Vec<(Vec<DomainReport>, WorldCommand, &str, &str, Vec<&str>)> = vec![
        // out-1/ws-1 (3 members, focus win-a2) -> out-2/ws-3.
        (
            vec![
                domain_with_focus(
                    "out-1",
                    "ws-1",
                    "win-a2",
                    vec![
                        win("win-a", "out-1", "ws-1"),
                        win("win-a2", "out-1", "ws-1"),
                        win("win-a3", "out-1", "ws-1"),
                    ],
                ),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
            ],
            migrate_command(),
            "out-1",
            "out-2",
            vec!["win-a", "win-a2", "win-a3"],
        ),
        // out-2/ws-2 (2 members, focus win-b2) -> out-1/ws-3. out-1 occupies
        // ws-1 and ws-2, so its trailing is ws-3.
        (
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-1", "ws-2", vec![win("win-x", "out-1", "ws-2")]),
                domain_with_focus(
                    "out-2",
                    "ws-2",
                    "win-b2",
                    vec![
                        win("win-b", "out-2", "ws-2"),
                        win("win-b2", "out-2", "ws-2"),
                    ],
                ),
            ],
            WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-2".to_owned()),
                target_output: OutputId("out-1".to_owned()),
                workspace: "ws-2".to_owned(),
            },
            "out-2",
            "out-1",
            vec!["win-b", "win-b2"],
        ),
    ];
    for (seed_domains, command, source_out, target_out, members) in cases {
        let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
        let mut seed = base_report(1);
        seed.domains = seed_domains;
        let first = accepted(&mut engine, &seed);
        let source_native = if source_out == "out-1" {
            "ws-1"
        } else {
            "ws-2"
        };
        let source_logical = logical_of(&first, source_out, source_native).expect("source logical");
        let source_key = tiler_core::session::DomainKey {
            output: OutputId(source_out.to_owned()),
            workspace: WorkspaceId(source_native.to_owned()),
        };
        let pre_tree = engine
            .engine()
            .session(&source_key)
            .expect("source session")
            .tree_for(&source_key)
            .cloned()
            .expect("seeded tree");
        let pre_focus = engine
            .engine()
            .session(&source_key)
            .expect("source session")
            .focus();
        let pre_tiling = first
            .workspaces
            .iter()
            .find(|row| {
                row.output.0 == source_out && row.native_binding.as_deref() == Some(source_native)
            })
            .expect("source row")
            .tiling_mode;

        let mut migrate = seed.clone();
        with_command(&mut migrate, 2, 61, command);
        let committed = accepted(&mut engine, &migrate);
        let migration = committed
            .transfers
            .iter()
            .find(|t| matches!(t.kind, TransferKind::Migration))
            .expect("migration retained");
        let target_ws = migration.target_workspace.clone();
        let target_key = tiler_core::session::DomainKey {
            output: OutputId(target_out.to_owned()),
            workspace: WorkspaceId(target_ws.clone()),
        };

        // Delayed: identical observation, fresh seq, no command, no feedback.
        let mut lagged = seed.clone();
        lagged.observation_seq = 3;
        lagged.request_seq = 3;
        lagged.request_id = "req-3".to_owned();
        let held = accepted(&mut engine, &lagged);
        assert!(held.command_result.is_none(), "no replay ({source_out})");
        assert_eq!(
            held.transfers
                .iter()
                .filter(|t| matches!(t.kind, TransferKind::Migration))
                .count(),
            1,
            "delayed keeps migration ({source_out})"
        );
        // Exact logical/native identity: moved logical rides to the target
        // trailing; source cell mints fresh; rings unchanged.
        assert_eq!(
            logical_of(&held, target_out, &target_ws),
            Some(source_logical.clone()),
            "logical rides ({source_out})"
        );
        let full: Vec<String> = NATIVES.iter().map(|s| s.to_string()).collect();
        assert_eq!(natives_for(&held, "out-1"), full);
        assert_eq!(natives_for(&held, "out-2"), full);
        // Retained topology: target session keeps the relocated tree, focus
        // and mode; source session never rebuilds with members.
        let moved = engine
            .engine()
            .session(&target_key)
            .expect("target session survives delay");
        assert_eq!(
            moved.tree_for(&target_key),
            Some(&pre_tree),
            "tree/shares/order survive delay ({source_out})"
        );
        assert_eq!(moved.focus().1, pre_focus.1, "focus rides ({source_out})");
        let target_row = held
            .workspaces
            .iter()
            .find(|row| {
                row.output.0 == target_out && row.native_binding.as_deref() == Some(&target_ws)
            })
            .expect("target row");
        assert_eq!(target_row.tiling_mode, pre_tiling, "mode rides");
        // Every member keeps exactly one homing at the target (delayed
        // members ride the transfer, never duplicate at the source).
        let homing = homings(&held);
        for member in &members {
            assert_eq!(
                homing.get(*member).map(String::as_str),
                Some(format!("{target_out}/{target_ws}").as_str()),
                "member homing at target ({source_out}/{member})"
            );
        }
        let wins: Vec<_> = held
            .windows
            .iter()
            .filter(|w| members.iter().any(|m| w.lifetime.0 == *m))
            .collect();
        assert_eq!(
            wins.len(),
            members.len(),
            "no duplication ({source_out}): {:?}",
            held.windows
        );
    }
}

#[test]
fn migrate_partial_preserves_topology_and_arrived_evidence() {
    // Partial arrival in both directions: arrived members plus outstanding
    // members stay in the retained set (arrived evidence never dropped),
    // the relocated tree survives, and every member homes once at the
    // target. Fresh seq, no feedback, no replay.
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    let mut seeded = base_report(1);
    seeded.domains[0] = domain_with_focus(
        "out-1",
        "ws-1",
        "win-a2",
        vec![
            win("win-a", "out-1", "ws-1"),
            win("win-a2", "out-1", "ws-1"),
            win("win-a3", "out-1", "ws-1"),
        ],
    );
    accepted(&mut engine, &seeded);
    let source_key = tiler_core::session::DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    let pre_tree = engine
        .engine()
        .session(&source_key)
        .expect("source")
        .tree_for(&source_key)
        .cloned()
        .expect("tree");
    let mut migrate = seeded.clone();
    with_command(&mut migrate, 2, 61, migrate_command());
    accepted(&mut engine, &migrate);
    let target_key = tiler_core::session::DomainKey {
        output: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws-3".to_owned()),
    };

    // Partial: win-a arrived, win-a2/win-a3 still at source. Target shows
    // ws-3 (follow actuated), source still old.
    let mut partial = base_report(3);
    partial.topology[1].current_native = "ws-3".to_owned();
    partial.domains = vec![
        readable_domain(
            "out-1",
            "ws-1",
            vec![
                win("win-a2", "out-1", "ws-1"),
                win("win-a3", "out-1", "ws-1"),
            ],
        ),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        domain_with_focus(
            "out-2",
            "ws-3",
            "win-a",
            vec![win("win-a", "out-2", "ws-3")],
        ),
    ];
    let held = accepted(&mut engine, &partial);
    assert!(held.command_result.is_none(), "no replay");
    let migration = held
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("partial keeps migration");
    assert_eq!(migration.target_workspace, "ws-3");
    // Target session keeps the full relocated tree (arrived plus shielded
    // delayed members), never shrinks to the arrived subset.
    let moved = engine
        .engine()
        .session(&target_key)
        .expect("target survives partial");
    assert_eq!(
        moved.tree_for(&target_key),
        Some(&pre_tree),
        "partial keeps full tree"
    );
    // Every member homes once at the target (delayed ride the transfer).
    let homing = homings(&held);
    for member in ["win-a", "win-a2", "win-a3"] {
        assert_eq!(
            homing.get(member).map(String::as_str),
            Some("out-2/ws-3"),
            "partial homing {member}"
        );
    }

    // Second partial round with one more arrival (win-a2 arrives, win-a3
    // still delayed): arrived evidence for win-a survives (still at
    // target, never reverts to source), tree still full.
    let mut partial2 = base_report(4);
    partial2.topology[1].current_native = "ws-3".to_owned();
    partial2.domains = vec![
        readable_domain("out-1", "ws-1", vec![win("win-a3", "out-1", "ws-1")]),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        domain_with_focus(
            "out-2",
            "ws-3",
            "win-a",
            vec![
                win("win-a", "out-2", "ws-3"),
                win("win-a2", "out-2", "ws-3"),
            ],
        ),
    ];
    let held2 = accepted(&mut engine, &partial2);
    assert_eq!(
        held2
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "second partial keeps migration"
    );
    let moved2 = engine
        .engine()
        .session(&target_key)
        .expect("target survives second partial");
    assert_eq!(
        moved2.tree_for(&target_key),
        Some(&pre_tree),
        "second partial keeps full tree"
    );
    let homing2 = homings(&held2);
    for member in ["win-a", "win-a2", "win-a3"] {
        assert_eq!(
            homing2.get(member).map(String::as_str),
            Some("out-2/ws-3"),
            "second partial homing {member}"
        );
    }
}

#[test]
fn migrate_unreadable_rounds_preserve_topology() {
    // Unreadable source then unreadable target mid-flight: the transfer
    // stays with identities preserved and the relocated tree survives;
    // readable arrival with both views settles. Fresh seq, no feedback.
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    let mut seeded = base_report(1);
    seeded.domains[0] = domain_with_focus(
        "out-1",
        "ws-1",
        "win-a2",
        vec![
            win("win-a", "out-1", "ws-1"),
            win("win-a2", "out-1", "ws-1"),
        ],
    );
    accepted(&mut engine, &seeded);
    let source_key = tiler_core::session::DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    let pre_tree = engine
        .engine()
        .session(&source_key)
        .expect("source")
        .tree_for(&source_key)
        .cloned()
        .expect("tree");
    let mut migrate = seeded.clone();
    with_command(&mut migrate, 2, 61, migrate_command());
    accepted(&mut engine, &migrate);
    let target_key = tiler_core::session::DomainKey {
        output: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws-3".to_owned()),
    };

    // Unreadable source mid-flight (members absent without close).
    let mut dark_source = base_report(3);
    dark_source.domains[0].readable = false;
    dark_source.domains[0].hold = Some(HoldReason::Unreadable);
    dark_source.domains[0].windows.clear();
    let held = accepted(&mut engine, &dark_source);
    assert_eq!(
        held.transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "unreadable source holds"
    );
    assert!(
        engine.engine().session(&target_key).is_some(),
        "target survives unreadable source"
    );

    // Unreadable target mid-flight.
    let mut dark_target = base_report(4);
    dark_target.domains[1].readable = false;
    dark_target.domains[1].hold = Some(HoldReason::Unreadable);
    dark_target.domains[1].windows.clear();
    let held2 = accepted(&mut engine, &dark_target);
    assert_eq!(
        held2
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "unreadable target holds"
    );

    // Readable full arrival with both views settles with the full tree.
    let mut arrived = base_report(5);
    arrived.topology[0].current_native = "ws-4".to_owned();
    arrived.topology[1].current_native = "ws-3".to_owned();
    arrived.domains = vec![
        domain_with_focus("out-1", "ws-1", "", Vec::new()),
        domain_with_focus("out-1", "ws-4", "", Vec::new()),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        domain_with_focus(
            "out-2",
            "ws-3",
            "win-a",
            vec![
                win("win-a", "out-2", "ws-3"),
                win("win-a2", "out-2", "ws-3"),
            ],
        ),
    ];
    let settled = accepted(&mut engine, &arrived);
    assert!(settled.transfers.is_empty(), "arrival settles");
    let moved = engine
        .engine()
        .session(&target_key)
        .expect("target after settle");
    assert_eq!(
        moved.tree_for(&target_key),
        Some(&pre_tree),
        "unreadable rounds keep tree through settle"
    );
}

#[test]
fn migrate_empty_requires_both_views_and_survives_refill_child() {
    // Originally-empty migrations still perform/verify target follow plus
    // the explicit source refill (D1/D5/D6). The expected refill rides the
    // parent, so refill-child settlement before the parent never loses the
    // proof. Fresh seq, no feedback, no replay. Empty ws-1 (current) migrates
    // so the refill (ws-4, last remaining) differs from the old current.
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    let mut empty = base_report(1);
    empty.domains = vec![
        domain_with_focus("out-1", "ws-1", "", Vec::new()),
        readable_domain("out-1", "ws-2", vec![win("win-x", "out-1", "ws-2")]),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
    ];
    accepted(&mut engine, &empty);
    let mut migrate = empty.clone();
    with_command(
        &mut migrate,
        2,
        61,
        WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-1".to_owned()),
            target_output: OutputId("out-2".to_owned()),
            workspace: "ws-1".to_owned(),
        },
    );
    let committed = accepted(&mut engine, &migrate);
    let refill = committed
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("refill retained");
    assert_eq!(refill.target_workspace, "ws-4");

    // Target follow only (source still shows the migrated ws-1, not the
    // refill): both transfers stay. Isolated engine so later source-only
    // views never read as external supersession here.
    let mut engine_target = engine.clone();
    let mut target_only = empty.clone();
    target_only.observation_seq = 3;
    target_only.request_seq = 3;
    target_only.request_id = "req-3".to_owned();
    target_only.topology[1].current_native = "ws-3".to_owned();
    let held = accepted(&mut engine_target, &target_only);
    assert_eq!(
        held.transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "empty waits for source refill"
    );
    assert_eq!(
        held.transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::View))
            .count(),
        1,
        "refill waits for source proof"
    );

    // Source refill only (target still old): refill child confirms and
    // drops, but the empty parent still waits for target follow (stored
    // refill proof survives the child). Continues on the post-commit engine
    // (target never actuated, so no external-supersession read).
    let mut source_only = empty.clone();
    source_only.observation_seq = 3;
    source_only.request_seq = 3;
    source_only.request_id = "req-3".to_owned();
    source_only.topology[0].current_native = "ws-4".to_owned();
    let settled_refill = accepted(&mut engine, &source_only);
    assert_eq!(
        settled_refill
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::View))
            .count(),
        0,
        "refill child confirms on source proof"
    );
    assert_eq!(
        settled_refill
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "empty parent survives refill-child settlement until target follow"
    );

    // Both views: everything settles, never reactivates.
    let mut both = empty.clone();
    both.observation_seq = 4;
    both.request_seq = 4;
    both.request_id = "req-4".to_owned();
    both.topology[0].current_native = "ws-4".to_owned();
    both.topology[1].current_native = "ws-3".to_owned();
    let settled = accepted(&mut engine, &both);
    assert!(settled.transfers.is_empty(), "both views settle empty");
    let mut again = empty.clone();
    again.observation_seq = 5;
    again.request_seq = 5;
    again.request_id = "req-5".to_owned();
    again.topology[0].current_native = "ws-4".to_owned();
    again.topology[1].current_native = "ws-3".to_owned();
    let clean = accepted(&mut engine, &again);
    assert!(clean.transfers.is_empty(), "no reactivation");
}

#[test]
fn migrate_external_view_supersession_settles_without_fight() {
    // Newer native view input wins: when the target view moves externally
    // away from the destination (or the source refill view moves away),
    // the parent settles instead of fighting, and never reactivates.
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine, &base_report(1));
    let mut migrate = base_report(2);
    with_command(&mut migrate, 2, 61, migrate_command());
    accepted(&mut engine, &migrate);

    // Members arrive but the target view moves externally to ws-2 (still
    // showing the old current, not the ws-3 follow): newer input wins.
    let mut external = base_report(3);
    external.topology[1].current_native = "ws-4".to_owned();
    external.domains = vec![
        domain_with_focus("out-1", "ws-1", "", Vec::new()),
        domain_with_focus("out-1", "ws-4", "", Vec::new()),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        domain_with_focus(
            "out-2",
            "ws-3",
            "win-a",
            vec![win("win-a", "out-2", "ws-3")],
        ),
    ];
    let settled = accepted(&mut engine, &external);
    assert!(
        settled
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "external target view settles migration: {:?}",
        settled.transfers
    );
    // No reactivation on the next identical external report.
    let mut again = external.clone();
    again.observation_seq = 4;
    again.request_seq = 4;
    again.request_id = "req-4".to_owned();
    let clean = accepted(&mut engine, &again);
    assert!(
        clean
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "no reactivation after external view"
    );
}

#[test]
fn migrate_producer_closures_both_directions() {
    // Producer-shaped explicit closes (closed list plus absence, fresh seq,
    // no feedback) settle members in both directions without inferring
    // closure from absence.
    let cases: Vec<(Vec<DomainReport>, WorldCommand, Vec<DomainReport>)> = vec![
        // out-1 -> out-2: win-a closes at source, win-a2 arrives.
        (
            vec![
                readable_domain(
                    "out-1",
                    "ws-1",
                    vec![
                        win("win-a", "out-1", "ws-1"),
                        win("win-a2", "out-1", "ws-1"),
                    ],
                ),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
            ],
            migrate_command(),
            vec![
                domain_with_focus("out-1", "ws-1", "", Vec::new()),
                domain_with_focus("out-1", "ws-4", "", Vec::new()),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
                domain_with_focus(
                    "out-2",
                    "ws-3",
                    "win-a2",
                    vec![win("win-a2", "out-2", "ws-3")],
                ),
            ],
        ),
        // out-2 -> out-1: win-b closes at source, win-b2 arrives.
        (
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-1", "ws-2", vec![win("win-x", "out-1", "ws-2")]),
                readable_domain(
                    "out-2",
                    "ws-2",
                    vec![
                        win("win-b", "out-2", "ws-2"),
                        win("win-b2", "out-2", "ws-2"),
                    ],
                ),
            ],
            WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-2".to_owned()),
                target_output: OutputId("out-1".to_owned()),
                workspace: "ws-2".to_owned(),
            },
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-1", "ws-2", vec![win("win-x", "out-1", "ws-2")]),
                domain_with_focus(
                    "out-1",
                    "ws-3",
                    "win-b2",
                    vec![win("win-b2", "out-1", "ws-3")],
                ),
                domain_with_focus("out-2", "ws-2", "", Vec::new()),
                domain_with_focus("out-2", "ws-4", "", Vec::new()),
            ],
        ),
    ];
    for (index, (seed_domains, command, arrived_domains)) in cases.into_iter().enumerate() {
        let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
        let mut seed = base_report(1);
        seed.domains = seed_domains;
        accepted(&mut engine, &seed);
        let mut migrate = seed.clone();
        with_command(&mut migrate, 2, 61, command);
        accepted(&mut engine, &migrate);
        let mut closed = base_report(3);
        closed.domains = arrived_domains;
        // Producer close evidence: explicit close list on the emptied source
        // cell plus absence everywhere else.
        if index == 0 {
            closed.topology[0].current_native = "ws-4".to_owned();
            closed.topology[1].current_native = "ws-3".to_owned();
            closed.domains[0].closed = vec![WindowId("win-a".to_owned())];
        } else {
            closed.topology[0].current_native = "ws-3".to_owned();
            closed.topology[1].current_native = "ws-4".to_owned();
            closed.domains[3].closed = vec![WindowId("win-b".to_owned())];
        }
        let settled = accepted(&mut engine, &closed);
        assert!(
            settled.transfers.is_empty(),
            "producer close plus arrival settles both directions ({index}): {:?}",
            settled.transfers
        );
    }
}

#[test]
fn migrate_floating_carry_delayed_partial_unreadable_e2e() {
    // Sessionless floating carry end to end: delayed, partial-gap and
    // unreadable rounds keep the transfer with exact target homing, then
    // full arrival with both views settles. Fresh seq, no feedback.
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine, &base_report(1));
    let mut float = base_report(2);
    with_command(
        &mut float,
        2,
        61,
        WorldCommand::SetWorkspaceTiling {
            output: OutputId("out-1".to_owned()),
            workspace: "ws-1".to_owned(),
            tiled: false,
        },
    );
    accepted(&mut engine, &float);
    let mut migrate = base_report(3);
    with_command(&mut migrate, 3, 62, migrate_command());
    let committed = accepted(&mut engine, &migrate);
    assert!(
        committed
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "floating carry commits"
    );

    // Delayed: identical observation still homes once at the target.
    let lagged = base_report(4);
    let held = accepted(&mut engine, &lagged);
    assert_eq!(
        held.transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "floating delayed holds"
    );
    assert_eq!(
        homings(&held).get("win-a").map(String::as_str),
        Some("out-2/ws-3"),
        "floating delayed homing"
    );

    // Unreadable destination holds without vanishing.
    let mut dark = base_report(5);
    dark.domains[1].readable = false;
    dark.domains[1].hold = Some(HoldReason::Unreadable);
    dark.domains[1].windows.clear();
    let held2 = accepted(&mut engine, &dark);
    assert_eq!(
        held2
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "floating unreadable holds"
    );

    // Full arrival with both views settles.
    let mut arrived = base_report(6);
    arrived.topology[0].current_native = "ws-4".to_owned();
    arrived.topology[1].current_native = "ws-3".to_owned();
    arrived.domains = vec![
        domain_with_focus("out-1", "ws-1", "", Vec::new()),
        domain_with_focus("out-1", "ws-4", "", Vec::new()),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        domain_with_focus(
            "out-2",
            "ws-3",
            "win-a",
            vec![win("win-a", "out-2", "ws-3")],
        ),
    ];
    let settled = accepted(&mut engine, &arrived);
    assert!(
        settled.transfers.is_empty(),
        "floating arrival settles: {:?}",
        settled.transfers
    );
    assert_eq!(
        homings(&settled).get("win-a").map(String::as_str),
        Some("out-2/ws-3")
    );
}

#[test]
fn migrate_shown_source_rebind_forgets_stale_previous_without_reselecting_fresh() {
    // D9/1.3/1.5: history stores natives while rows carry per-(output,
    // native) logicals. Migrating the shown ws-1 rebinds (out-1, ws-1) to a
    // fresh logical; the stale delayed view must not re-record ws-1 as
    // previous, and SelectPrevious after the refill must never select the
    // fresh replacement at the old native.
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // Prime out-1 history: ws-2 then ws-1, so previous=ws-2, baseline=ws-1.
    let mut first = base_report(1);
    first.topology[0].current_native = "ws-2".to_owned();
    first.domains = vec![
        readable_domain("out-1", "ws-2", vec![win("win-z", "out-1", "ws-2")]),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
    ];
    accepted(&mut engine, &first);
    let mut second = base_report(2);
    second.domains = vec![
        readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
        readable_domain("out-1", "ws-2", vec![win("win-z", "out-1", "ws-2")]),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
    ];
    let shown = accepted(&mut engine, &second);
    let source_logical = logical_of(&shown, "out-1", "ws-1").expect("source logical");

    // Migrate the shown ws-1 onto out-2/ws-3 (LastRemaining refill ws-4).
    let mut migrate = second.clone();
    with_command(&mut migrate, 3, 61, migrate_command());
    let committed = accepted(&mut engine, &migrate);
    assert!(
        committed.command_result.is_some(),
        "migration commits: {:?}",
        committed.command_result
    );
    let refill = committed
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("refill");
    assert_eq!(refill.target_workspace, "ws-4");
    let moved = logical_of(&committed, "out-2", "ws-3").expect("moved logical");
    assert_eq!(moved, source_logical, "moved logical rides");
    let fresh = logical_of(&committed, "out-1", "ws-1").expect("fresh source");
    assert_ne!(fresh, source_logical, "source cell mints fresh");

    // Delayed actuation lag: identical stale views, no command. The flight
    // stays and logicals stay stable in both directions.
    let mut lagged = second.clone();
    lagged.observation_seq = 4;
    lagged.request_seq = 4;
    lagged.request_id = "req-4".to_owned();
    let held = accepted(&mut engine, &lagged);
    assert_eq!(
        held.transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1
    );
    assert_eq!(
        logical_of(&held, "out-2", "ws-3"),
        Some(source_logical.clone())
    );
    assert_eq!(logical_of(&held, "out-1", "ws-1"), Some(fresh.clone()));

    // Final arrival: source shows refill ws-4, target shows ws-3.
    let mut arrived = base_report(5);
    arrived.topology[0].current_native = "ws-4".to_owned();
    arrived.topology[1].current_native = "ws-3".to_owned();
    arrived.domains = vec![
        domain_with_focus("out-1", "ws-1", "", Vec::new()),
        domain_with_focus("out-1", "ws-4", "", Vec::new()),
        readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        domain_with_focus(
            "out-2",
            "ws-3",
            "win-a",
            vec![win("win-a", "out-2", "ws-3")],
        ),
    ];
    let settled = accepted(&mut engine, &arrived);
    assert!(
        settled.transfers.is_empty(),
        "arrival settles: {:?}",
        settled.transfers
    );
    assert_eq!(
        logical_of(&settled, "out-2", "ws-3"),
        Some(source_logical.clone())
    );
    assert_eq!(logical_of(&settled, "out-1", "ws-1"), Some(fresh.clone()));

    // SelectPrevious on out-1 must select the preserved unrelated ws-2,
    // never the fresh replacement at the old native ws-1.
    let mut previous = arrived.clone();
    with_command(
        &mut previous,
        6,
        62,
        WorldCommand::SelectPrevious {
            output: OutputId("out-1".to_owned()),
        },
    );
    let reply = accepted(&mut engine, &previous);
    let view = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View) && t.target_output.0 == "out-1")
        .expect("previous view retained");
    assert_eq!(
        view.target_workspace, "ws-2",
        "preserved unrelated previous wins: {:?}",
        reply.transfers
    );
    assert_ne!(
        view.target_workspace, "ws-1",
        "never reselects the fresh replacement at the old native"
    );
}

#[test]
fn migrate_target_placeholder_previous_clears_while_unrelated_survives() {
    // Target history naming the rebound trailing ws-3 clears; an unrelated
    // target previous survives the same rebind.
    for (prime_current, prime_previous, expect_select, expect_none) in [
        ("ws-3", "ws-3", None, true),
        ("ws-4", "ws-4", Some("ws-4"), false),
    ] {
        let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
        // Prime out-2 history: show prime_current, then ws-2.
        let mut first = base_report(1);
        first.topology[1].current_native = prime_current.to_owned();
        first.domains = vec![
            readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
            readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
            domain_with_focus("out-2", prime_current, "", Vec::new()),
        ];
        accepted(&mut engine, &first);
        let mut second = base_report(2);
        second.domains = vec![
            readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
            readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        ];
        accepted(&mut engine, &second);

        // Sanity: the primed previous is recorded before migration.
        let mut migrate = second.clone();
        with_command(&mut migrate, 3, 61, migrate_command());
        let committed = accepted(&mut engine, &migrate);
        assert!(
            committed.command_result.is_some(),
            "migration commits for primed {prime_previous}"
        );

        // Delayed views (both outputs still old): ask SelectPrevious on the
        // target. Placeholder ws-3 must be gone; unrelated ws-4 survives.
        let mut previous = second.clone();
        with_command(
            &mut previous,
            4,
            62,
            WorldCommand::SelectPrevious {
                output: OutputId("out-2".to_owned()),
            },
        );
        let reply = accepted(&mut engine, &previous);
        let view = reply
            .transfers
            .iter()
            .find(|t| matches!(t.kind, TransferKind::View) && t.target_output.0 == "out-2");
        if expect_none {
            assert!(
                view.is_none(),
                "rebound placeholder {prime_previous} clears: {:?}",
                reply.transfers
            );
        } else {
            assert_eq!(
                view.map(|v| v.target_workspace.as_str()),
                expect_select,
                "unrelated previous survives: {:?}",
                reply.transfers
            );
        }
    }
}
