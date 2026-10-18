//! Direction-only world intents (core-owned topology): KDE sends
//! win+direction or source+direction only with empty wire adjacency; the
//! core resolves the strict unique-reciprocal target from retained FULL
//! rects. Ordinary two-output, single-output local fallback,
//! ambiguity/partial fencing, follow/stay sends, source-explicit migrate
//! with no active window, and topology retention independent of window
//! inventory.

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    DomainReport, OutputTopology, TransferKind, WORLD_CONTRACT_VERSION, WorkspaceBinding,
    WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn engine_window_at(id: &str, out: &str, ws: &str, x: i32, y: i32) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        rect: rect(x, y, 100, 100),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    }
}

fn engine_window(id: &str, out: &str, ws: &str) -> EngineWindow {
    let x = if id.ends_with("2") || id == "win-b" { 400 } else { 10 };
    engine_window_at(id, out, ws, x, 10)
}

fn domain_report(out: &str, ws: &str, windows: Vec<EngineWindow>) -> DomainReport {
    DomainReport {
        domain: OutputDomain {
            id: OutputId(out.to_owned()),
            workspace: WorkspaceId(ws.to_owned()),
            bounds: rect(0, 0, 800, 600),
            gap: 0,
            // No wire adjacency: core owns selection.
            adjacent: BTreeMap::new(),
        },
        outer_gap: 0,
        focused: WindowId(windows.first().map(|w| w.window.0.clone()).unwrap_or_default()),
        windows,
        closed: Vec::new(),
        readable: true,
        hold: None,
    }
}

/// Distinct-workspace two-output world (wa1 vs wb1): sends and migrates
/// resolve across outputs; focus cross needs same-workspace (see below).
fn two_output_report(seq: u64, geometry_b: Option<Rect>) -> WorldReport {
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
                output: OutputId("out-a".to_owned()),
                native_order: vec!["wa1".to_owned()],
                current_native: "wa1".to_owned(),
                complete: true,
                geometry: Some(rect(0, 0, 800, 600)),
                primary: true,
            },
            OutputTopology {
                output: OutputId("out-b".to_owned()),
                native_order: vec!["wb1".to_owned()],
                current_native: "wb1".to_owned(),
                complete: true,
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
                vec![
                    engine_window("win-a", "out-a", "wa1"),
                    engine_window("win-a2", "out-a", "wa1"),
                ],
            ),
            domain_report("out-b", "wb1", vec![engine_window("win-b", "out-b", "wb1")]),
        ],
        minimized: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: "win-a".to_owned(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: Vec::new(),
    }
}

/// Same-workspace two-output world (ws1 on both): directional focus/move
/// cross through the strict pair (shared counterpart), exactly like the
/// existing `world_contract_repairs` cross-output focus fixture. Single
/// window per domain so cross has no local competitor.
fn same_ws_report(seq: u64) -> WorldReport {
    WorldReport {
        topology: vec![
            OutputTopology {
                output: OutputId("out-a".to_owned()),
                native_order: vec!["ws1".to_owned()],
                current_native: "ws1".to_owned(),
                complete: true,
                geometry: Some(rect(0, 0, 800, 600)),
                primary: true,
            },
            OutputTopology {
                output: OutputId("out-b".to_owned()),
                native_order: vec!["ws1".to_owned()],
                current_native: "ws1".to_owned(),
                complete: true,
                geometry: Some(rect(800, 0, 800, 600)),
                primary: false,
            },
        ],
        bindings: Vec::new(),
        domains: vec![
            domain_report("out-a", "ws1", vec![engine_window("win-a", "out-a", "ws1")]),
            domain_report("out-b", "ws1", vec![engine_window("win-b", "out-b", "ws1")]),
        ],
        focused_global: "win-a".to_owned(),
        ..two_output_report(seq, Some(rect(800, 0, 800, 600)))
    }
}

fn single_output_report(seq: u64) -> WorldReport {
    let mut report = two_output_report(seq, Some(rect(800, 0, 800, 600)));
    report.topology.remove(1);
    report.bindings.remove(1);
    report.domains.remove(1);
    report
}

fn with_command(report: &mut WorldReport, seq: u64, cmd_seq: u64, command: WorldCommand) {
    report.request_seq = seq;
    report.request_id = format!("req-{seq}");
    report.observation_seq = seq;
    report.command_seq = Some(cmd_seq);
    report.command = Some(command);
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> tiler_core::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

#[test]
fn focus_directional_crosses_two_outputs_without_wire_adjacency() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &same_ws_report(1));
    let mut cmd = same_ws_report(2);
    with_command(
        &mut cmd,
        2,
        11,
        WorldCommand::FocusDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            float_subject: false,
        },
    );
    let reply = accepted(&mut engine, &cmd);
    // Cross-output focus emits a one-shot effect for the target window.
    assert!(
        reply.effects.iter().any(|e| e.target.0 == "win-b"),
        "focus right crosses to win-b, effects={:?}",
        reply.effects
    );
}

#[test]
fn move_directional_crosses_and_single_output_falls_back_local() {
    // Two outputs, distinct workspaces, two tiles in the source: move
    // attempts cross, falling back to local. The command must never
    // refuse on topology (planner-noop/unchanged are legitimate local
    // outcomes when the focused tile has no neighbor in that direction).
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &two_output_report(1, Some(rect(800, 0, 800, 600))));
    let mut cross = two_output_report(2, Some(rect(800, 0, 800, 600)));
    with_command(
        &mut cross,
        2,
        12,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    match engine.handle_report(&cross) {
        WorldOutcome::Accepted(_) => {}
        WorldOutcome::Rejected { kind, .. } => {
            assert!(
                matches!(kind, "planner-noop" | "unchanged"),
                "cross move must not refuse on topology, got {kind}"
            );
        }
        other => panic!("unexpected outcome {other:?}"),
    }
    // Single output: no resolvable target, so the core runs the local
    // R1-R3 path instead of refusing all operations.
    let mut solo_engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut solo_engine, &single_output_report(1));
    let mut local = single_output_report(2);
    with_command(
        &mut local,
        2,
        13,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    match solo_engine.handle_report(&local) {
        WorldOutcome::Accepted(_) => {}
        WorldOutcome::Rejected { kind, .. } => {
            assert!(
                matches!(kind, "planner-noop" | "unchanged"),
                "local move must not refuse on topology, got {kind}"
            );
        }
        other => panic!("unexpected outcome {other:?}"),
    }
    // Focus right on a single output is local (never a cross refusal).
    // Fresh engine so the local layout still has win-a left of win-a2.
    let mut focus_engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut focus_engine, &single_output_report(1));
    let mut focus = single_output_report(2);
    with_command(
        &mut focus,
        2,
        14,
        WorldCommand::FocusDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            float_subject: false,
        },
    );
    let reply = accepted(&mut focus_engine, &focus);
    assert!(
        reply.effects.iter().any(|e| e.target.0 == "win-a2"),
        "local focus right reaches win-a2, effects={:?}",
        reply.effects
    );
}

#[test]
fn focus_up_never_crosses_even_with_vertical_topology() {
    // Vertical stack: out-b below out-a. Focus up/down stays local per KDE
    // policy even though the strict map has vertical edges. The out-b
    // domain holds two vertically arranged tiles so the local up has a
    // target: win-b2 (bottom, focused) up to win-b (top).
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let mut stacked = same_ws_report(1);
    stacked.topology[0].geometry = Some(rect(0, 0, 800, 600));
    stacked.topology[1].geometry = Some(rect(0, 600, 800, 600));
    stacked.domains[1] = domain_report(
        "out-b",
        "ws1",
        vec![
            engine_window_at("win-b2", "out-b", "ws1", 10, 400),
            engine_window_at("win-b", "out-b", "ws1", 10, 10),
        ],
    );
    // Focused is win-b2 (first window, bottom); up goes to win-b locally.
    accepted(&mut engine, &stacked);
    let mut cmd = stacked.clone();
    with_command(
        &mut cmd,
        2,
        15,
        WorldCommand::FocusDirectional {
            window: WindowId("win-b2".to_owned()),
            direction: "up".to_owned(),
            float_subject: false,
        },
    );
    let reply = accepted(&mut engine, &cmd);
    // Local fallback: effect targets the local upper tile, never win-a.
    assert!(
        reply.effects.iter().any(|e| e.target.0 == "win-b"),
        "focus up stays local to win-b, effects={:?}",
        reply.effects
    );
    assert!(
        !reply.effects.iter().any(|e| e.target.0 == "win-a"),
        "focus up never crosses, effects={:?}",
        reply.effects
    );
}

#[test]
fn ambiguity_fences_send_and_migrate_but_focus_falls_back_local() {
    // Split edge: src wide below two outputs; upward touches are ambiguous
    // so the strict map omits them. The src domain holds two vertically
    // arranged tiles so the local up fallback has a target.
    let wide = rect(0, 600, 1600, 600);
    let up_l = rect(0, 0, 800, 600);
    let up_r = rect(800, 0, 800, 600);
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let mut report = WorldReport {
        topology: vec![
            OutputTopology {
                output: OutputId("src".to_owned()),
                native_order: vec!["ws".to_owned()],
                current_native: "ws".to_owned(),
                complete: true,
                geometry: Some(wide),
                primary: true,
            },
            OutputTopology {
                output: OutputId("up-l".to_owned()),
                native_order: vec!["wu".to_owned()],
                current_native: "wu".to_owned(),
                complete: true,
                geometry: Some(up_l),
                primary: false,
            },
            OutputTopology {
                output: OutputId("up-r".to_owned()),
                native_order: vec!["wu2".to_owned()],
                current_native: "wu2".to_owned(),
                complete: true,
                geometry: Some(up_r),
                primary: false,
            },
        ],
        domains: vec![
            domain_report(
                "src",
                "ws",
                vec![
                    engine_window_at("win-s2", "src", "ws", 10, 400),
                    engine_window_at("win-s", "src", "ws", 10, 10),
                ],
            ),
            domain_report("up-l", "wu", vec![engine_window("win-u", "up-l", "wu")]),
            domain_report("up-r", "wu2", vec![engine_window("win-u2", "up-r", "wu2")]),
        ],
        bindings: Vec::new(),
        focused_global: "win-s2".to_owned(),
        inventory_complete: true,
        ..two_output_report(1, Some(rect(0, 0, 800, 600)))
    };
    report.request_seq = 1;
    report.request_id = "req-1".to_owned();
    report.observation_seq = 1;
    // Focused is win-s2 (bottom); local up goes to win-s (top).
    accepted(&mut engine, &report);
    // Send up from the wide source refuses (no strict target).
    let mut send = report.clone();
    with_command(
        &mut send,
        2,
        21,
        WorldCommand::SendDirectional {
            window: WindowId("win-s2".to_owned()),
            direction: "up".to_owned(),
            follow: true,
        },
    );
    match engine.handle_report(&send) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-target-unknown"),
        other => panic!("expected send refusal, got {other:?}"),
    }
    // Migrate up refuses the same way.
    let mut migrate = report.clone();
    with_command(
        &mut migrate,
        3,
        22,
        WorldCommand::MigrateDirectional {
            source_output: OutputId("src".to_owned()),
            workspace: "ws".to_owned(),
            direction: "up".to_owned(),
        },
    );
    match engine.handle_report(&migrate) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "migration-refused"),
        other => panic!("expected migrate refusal, got {other:?}"),
    }
    // Focus up falls back to a local focus instead of refusing everything.
    let mut focus = report.clone();
    with_command(
        &mut focus,
        4,
        23,
        WorldCommand::FocusDirectional {
            window: WindowId("win-s2".to_owned()),
            direction: "up".to_owned(),
            float_subject: false,
        },
    );
    let reply = accepted(&mut engine, &focus);
    assert!(
        reply.effects.iter().any(|e| e.target.0 == "win-s"),
        "local up reaches win-s, effects={:?}",
        reply.effects
    );
}

#[test]
fn partial_topology_fences_send_but_keeps_local_focus() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &two_output_report(1, Some(rect(800, 0, 800, 600))));
    // Partial: out-b rect unreadable clears the live map.
    let mut partial = two_output_report(2, None);
    with_command(
        &mut partial,
        2,
        31,
        WorldCommand::SendDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            follow: false,
        },
    );
    match engine.handle_report(&partial) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-target-unknown"),
        other => panic!("expected send refusal on partial topology, got {other:?}"),
    }
    // Local focus still operates on the readable side (right to win-a2).
    let mut focus = two_output_report(3, None);
    with_command(
        &mut focus,
        3,
        32,
        WorldCommand::FocusDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            float_subject: false,
        },
    );
    let reply = accepted(&mut engine, &focus);
    assert!(
        reply.effects.iter().any(|e| e.target.0 == "win-a2"),
        "local focus still works on partial topology, effects={:?}",
        reply.effects
    );
}

#[test]
fn send_directional_follow_and_stay_name_current_view() {
    for follow in [true, false] {
        let mut engine = WorldEngine::new("gen-1").expect("valid");
        accepted(&mut engine, &two_output_report(1, Some(rect(800, 0, 800, 600))));
        let mut cmd = two_output_report(2, Some(rect(800, 0, 800, 600)));
        with_command(
            &mut cmd,
            2,
            41,
            WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow,
            },
        );
        let reply = accepted(&mut engine, &cmd);
        let transfer = reply
            .transfers
            .iter()
            .find(|t| matches!(t.kind, TransferKind::Window(_)))
            .expect("send retains a window transfer");
        assert_eq!(transfer.target_output.0, "out-b");
        assert_eq!(transfer.target_workspace, "wb1", "target current native view");
        assert_eq!(transfer.follow, follow);
    }
}

#[test]
fn migrate_directional_needs_no_active_window_and_reports_source_context() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let mut seed = two_output_report(1, Some(rect(800, 0, 800, 600)));
    seed.focused_global = String::new();
    accepted(&mut engine, &seed);
    let mut cmd = two_output_report(2, Some(rect(800, 0, 800, 600)));
    cmd.focused_global = String::new();
    with_command(
        &mut cmd,
        2,
        51,
        WorldCommand::MigrateDirectional {
            source_output: OutputId("out-a".to_owned()),
            workspace: "wa1".to_owned(),
            direction: "right".to_owned(),
        },
    );
    let reply = accepted(&mut engine, &cmd);
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)
                && t.target_output.0 == "out-b"
                && t.target_workspace == "wa1"),
        "migrate right retains a migration transfer"
    );
}

#[test]
fn live_topology_survives_unrelated_window_inventory_gap() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &two_output_report(1, Some(rect(800, 0, 800, 600))));
    // Same FULL topology but the report claims incomplete window
    // inventory; monitor completeness alone must retain the map so a
    // direction-only send still resolves.
    let mut gap = two_output_report(2, Some(rect(800, 0, 800, 600)));
    gap.inventory_complete = false;
    with_command(
        &mut gap,
        2,
        61,
        WorldCommand::SendDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            follow: true,
        },
    );
    let reply = accepted(&mut engine, &gap);
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| t.target_output.0 == "out-b"),
        "topology retention ignores window inventory gaps"
    );
}
