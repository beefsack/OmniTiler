//! Direction-only world intents (core-owned topology): KDE sends
//! win+direction or source+direction only with empty wire adjacency; the
//! core resolves the target from retained FULL rects. Moves and
//! directional sends rank multiple edge-touch candidates by mover-centre
//! projection, then span overlap, then left/top (2026-10-09) with
//! reciprocal edge existence (reverse uniqueness not required); migration
//! ranks by largest shared edge then left/top, never window position.
//! Focus keeps the strict unique-reciprocal rule and only crosses
//! horizontally. Unreadable topology refuses; no candidate runs local
//! (moves/focus) or refuses (sends/migration); no wrap. Covers
//! follow/stay sends, source-explicit migrate with no active window,
//! topology retention independent of window inventory, and GlobalUnique
//! mode.

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    DomainReport, HoldReason, OutputTopology, TransferKind, WORLD_CONTRACT_VERSION,
    WorkspaceBinding, WorldCommand, WorldReport,
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
    let x = if id.ends_with("2") || id == "win-b" {
        400
    } else {
        10
    };
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
        focused: WindowId(
            windows
                .first()
                .map(|w| w.window.0.clone())
                .unwrap_or_default(),
        ),
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

fn accepted(
    engine: &mut WorldEngine,
    report: &WorldReport,
) -> tiler_core::world_report::WorldReply {
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
fn move_directional_windowed_cross_and_single_output_local() {
    // Windowed selection: with an edge-touch candidate, the cross pair
    // returns directly (no local fallback). Distinct workspaces, two
    // tiles in the source. The mover rect (x=10, 100 wide) centres on the
    // single right candidate.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &two_output_report(1, Some(rect(800, 0, 800, 600))),
    );
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
    // Cross path accepted (Engine resolves via the pair; no topology
    // refusal and no silent local relabel).
    accepted(&mut engine, &cross);
    // Single output: no strict target, so the core runs local R1-R3.
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
    accepted(&mut solo_engine, &local);
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
fn split_edge_move_send_migrate_select_but_focus_stays_local() {
    // Split edge: src wide below two outputs; upward touches are ambiguous
    // under the strict map, but windowed moves/sends rank by the mover
    // and migration ranks by largest edge. The src domain holds two
    // vertically arranged tiles so the local up fallback has a target.
    let wide = rect(0, 600, 1600, 600);
    let up_l = rect(0, 0, 800, 600);
    let up_r = rect(800, 0, 800, 600);
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
        bindings: vec![
            WorkspaceBinding {
                logical_id: "lsrc".to_owned(),
                native_id: "ws".to_owned(),
                output: OutputId("src".to_owned()),
                pinned: false,
                displaced: false,
                managed: true,
            },
            WorkspaceBinding {
                logical_id: "lul".to_owned(),
                native_id: "wu".to_owned(),
                output: OutputId("up-l".to_owned()),
                pinned: false,
                displaced: false,
                managed: true,
            },
            WorkspaceBinding {
                logical_id: "lur".to_owned(),
                native_id: "wu2".to_owned(),
                output: OutputId("up-r".to_owned()),
                pinned: false,
                displaced: false,
                managed: true,
            },
        ],
        focused_global: "win-s2".to_owned(),
        inventory_complete: true,
        ..two_output_report(1, Some(rect(0, 0, 800, 600)))
    };
    report.request_seq = 1;
    report.request_id = "req-1".to_owned();
    report.observation_seq = 1;
    // Focused is win-s2 (bottom); local up goes to win-s (top). Each leg
    // below runs on a fresh engine so retained transfers never leak
    // across legs.
    // Send up from the wide source selects by mover centre: win-s2 at
    // x=10 (100 wide, centre x=60) falls in up-l's shared edge [0,800).
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &report);
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
    let reply = accepted(&mut engine, &send);
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Window(_)))
        .expect("windowed send retains a transfer");
    assert_eq!(transfer.target_output.0, "up-l", "centre x=60 selects up-l");
    // Migration up ignores the window and ranks largest edge: the tie
    // (800 vs 800) breaks left/top to up-l.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &report);
    let mut migrate = report.clone();
    with_command(
        &mut migrate,
        2,
        22,
        WorldCommand::MigrateDirectional {
            source_output: OutputId("src".to_owned()),
            workspace: "ws".to_owned(),
            direction: "up".to_owned(),
        },
    );
    let reply = accepted(&mut engine, &migrate);
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration) && t.target_output.0 == "up-l"),
        "migration tie breaks left/top to up-l"
    );
    // Focus up falls back to a local focus instead of refusing everything.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &report);
    let mut focus = report.clone();
    with_command(
        &mut focus,
        2,
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
    accepted(
        &mut engine,
        &two_output_report(1, Some(rect(800, 0, 800, 600))),
    );
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
        accepted(
            &mut engine,
            &two_output_report(1, Some(rect(800, 0, 800, 600))),
        );
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
        assert_eq!(
            transfer.target_workspace, "wb1",
            "target current native view"
        );
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
    accepted(
        &mut engine,
        &two_output_report(1, Some(rect(800, 0, 800, 600))),
    );
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
        reply.transfers.iter().any(|t| t.target_output.0 == "out-b"),
        "topology retention ignores window inventory gaps"
    );
}

fn unreadable_target_report(seq: u64) -> WorldReport {
    // Same-workspace strict edge (out-a|out-b share ws1, FULL rects touch
    // at x=800), source two tiles so a local right would succeed, target
    // unreadable. Strict parity must refuse with no local rearrangement.
    let mut report = same_ws_report(seq);
    report.domains[0] = domain_report(
        "out-a",
        "ws1",
        vec![
            engine_window("win-a", "out-a", "ws1"),
            engine_window("win-a2", "out-a", "ws1"),
        ],
    );
    report.domains[1] = DomainReport {
        domain: OutputDomain {
            id: OutputId("out-b".to_owned()),
            workspace: WorkspaceId("ws1".to_owned()),
            bounds: rect(0, 0, 800, 600),
            gap: 0,
            adjacent: BTreeMap::new(),
        },
        outer_gap: 0,
        focused: WindowId(String::new()),
        windows: Vec::new(),
        closed: Vec::new(),
        readable: false,
        hold: Some(HoldReason::Unreadable),
    };
    report.focused_global = "win-a".to_owned();
    report
}

#[test]
fn strict_target_unreadable_refuses_without_local_fallback() {
    // Focus right: strict edge exists, target unreadable. Must refuse
    // operation-refused with no local effect (old fallback would have
    // focused win-a2 locally).
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let report = unreadable_target_report(1);
    let before = engine.state_revision();
    let mut cmd = report.clone();
    with_command(
        &mut cmd,
        1,
        71,
        WorldCommand::FocusDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            float_subject: false,
        },
    );
    match engine.handle_report(&cmd) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "operation-refused"),
        other => panic!("expected operation refusal, got {other:?}"),
    }
    assert_eq!(
        engine.state_revision(),
        before,
        "refusal mutates nothing (no local rearrangement)"
    );
    // Move right: same strict edge, same unreadable target. Must refuse
    // rather than swapping locally.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let before = engine.state_revision();
    let mut cmd = unreadable_target_report(1);
    with_command(
        &mut cmd,
        1,
        72,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    match engine.handle_report(&cmd) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "operation-refused"),
        other => panic!("expected operation refusal, got {other:?}"),
    }
    assert_eq!(
        engine.state_revision(),
        before,
        "refusal mutates nothing (no local swap)"
    );
}

#[test]
fn global_unique_retains_topology_and_fences_directionally() {
    // GlobalUnique mode: same strict geometry, distinct workspaces.
    // Retention, directional resolution, and partial fencing are
    // mode-independent (shared retains identically; see WorldEngine docs).
    fn global_report(seq: u64, geometry_b: Option<Rect>) -> WorldReport {
        let mut report = two_output_report(seq, geometry_b);
        report.workspace_mode = WorkspaceMode::GlobalUnique;
        // Single window per domain so cross has no local competitor.
        report.domains[0] =
            domain_report("out-a", "wa1", vec![engine_window("win-a", "out-a", "wa1")]);
        report
    }
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &global_report(1, Some(rect(800, 0, 800, 600))));
    // Focus right crosses in GlobalUnique (strict edge, readable target).
    let mut cross = global_report(2, Some(rect(800, 0, 800, 600)));
    with_command(
        &mut cross,
        2,
        81,
        WorldCommand::FocusDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            float_subject: false,
        },
    );
    let reply = accepted(&mut engine, &cross);
    assert!(
        reply.effects.iter().any(|e| e.target.0 == "win-b"),
        "GlobalUnique focus crosses, effects={:?}",
        reply.effects
    );
    // Partial geometry fences sends even in GlobalUnique.
    let mut partial = global_report(3, None);
    with_command(
        &mut partial,
        3,
        82,
        WorldCommand::SendDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            follow: false,
        },
    );
    match engine.handle_report(&partial) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-target-unknown"),
        other => panic!("expected send fence on partial topology, got {other:?}"),
    }
}

// ---- world split-edge selection (2026-10-09 through the world route) ----
//
// R-MOV-12 discriminators, R-WS-27 two-candidate migration, strict
// split-edge focus, ordinary two-output behavior, unreadable/no-candidate
// fences, pinned R4 identity across mover relocation, and valid
// reverse-ambiguous pairs. Geometries mirror the spec tables and the
// shared selector fixtures; the world route resolves them through
// MoveDirectional/SendDirectional/MigrateDirectional plus the operation
// R4 pair, never carried adjacency.

fn homing(reply: &tiler_core::world_report::WorldReply, window: &str) -> Option<String> {
    reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == window)
        .map(|w| format!("{}/{}", w.output.0, w.workspace))
}

fn window_rect(id: &str, out: &str, ws: &str, x: i32, y: i32, w: i32, h: i32) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        rect: rect(x, y, w, h),
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

/// Three-output world with caller-controlled FULL geometries, one shared
/// workspace id, and empty wire adjacency. `mover` is the focused sole
/// window on `src`; each upper output holds one window.
fn three_output_report(
    seq: u64,
    src: Rect,
    u1: Rect,
    u2: Rect,
    mover: EngineWindow,
) -> WorldReport {
    WorldReport {
        topology: vec![
            OutputTopology {
                output: OutputId("src".to_owned()),
                native_order: vec!["ws".to_owned()],
                current_native: "ws".to_owned(),
                complete: true,
                geometry: Some(src),
                primary: true,
            },
            OutputTopology {
                output: OutputId("u1".to_owned()),
                native_order: vec!["ws".to_owned()],
                current_native: "ws".to_owned(),
                complete: true,
                geometry: Some(u1),
                primary: false,
            },
            OutputTopology {
                output: OutputId("u2".to_owned()),
                native_order: vec!["ws".to_owned()],
                current_native: "ws".to_owned(),
                complete: true,
                geometry: Some(u2),
                primary: false,
            },
        ],
        domains: vec![
            domain_report("src", "ws", vec![mover]),
            domain_report(
                "u1",
                "ws",
                vec![window_rect("win-u1", "u1", "ws", 10, 10, 100, 100)],
            ),
            domain_report(
                "u2",
                "ws",
                vec![window_rect("win-u2", "u2", "ws", 970, 10, 100, 100)],
            ),
        ],
        bindings: Vec::new(),
        focused_global: "win-a".to_owned(),
        inventory_complete: true,
        ..two_output_report(seq, Some(rect(0, 0, 800, 600)))
    }
}

fn move_up_once(engine: &mut WorldEngine, report: &WorldReport, seq: u64) -> HomingReply {
    let mut cmd = report.clone();
    with_command(
        &mut cmd,
        seq,
        seq + 100,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "up".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    accepted(engine, &cmd)
}

type HomingReply = tiler_core::world_report::WorldReply;

#[test]
fn r_mov12_world_seam_prefers_upper_interval() {
    // Row 1: U1 [0,960), U2 [960,1920); A x=860 w=200 centre=960.
    // Half-open containment puts the seam in the right edge: U2.
    let mover = window_rect("win-a", "src", "ws", 860, 1150, 200, 200);
    let report = three_output_report(
        1,
        rect(0, 1080, 1920, 1080),
        rect(0, 0, 960, 1080),
        rect(960, 0, 960, 1080),
        mover,
    );
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &report);
    let reply = move_up_once(&mut engine, &report, 2);
    assert_eq!(
        homing(&reply, "win-a"),
        Some("u2/ws".to_owned()),
        "seam centre selects u2"
    );
}

#[test]
fn r_mov12_world_overlap_fallback_selects_larger_span() {
    // Row 2: source [0,1600); uppers [0,400), [1200,1600); centre 850 in
    // the gap; window [300,1400) overlaps 200 right vs 100 left: U2.
    let mover = window_rect("win-a", "src", "ws", 300, 1150, 1100, 200);
    let report = three_output_report(
        1,
        rect(0, 1080, 1600, 1080),
        rect(0, 0, 400, 1080),
        rect(1200, 0, 400, 1080),
        mover,
    );
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &report);
    let reply = move_up_once(&mut engine, &report, 2);
    assert_eq!(
        homing(&reply, "win-a"),
        Some("u2/ws".to_owned()),
        "larger span selects u2"
    );
}

#[test]
fn r_mov12_world_both_centred_skips_overlap_to_left_top() {
    // Row 3: overlapping uppers [0,1200), [400,1600); centre 1100 in
    // both; U2 overlaps the span more yet left/top still picks U1.
    let mover = window_rect("win-a", "src", "ws", 900, 1150, 400, 100);
    let report = three_output_report(
        1,
        rect(0, 1080, 1600, 1080),
        rect(0, 0, 1200, 1080),
        rect(400, 0, 1200, 1080),
        mover,
    );
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &report);
    let reply = move_up_once(&mut engine, &report, 2);
    assert_eq!(
        homing(&reply, "win-a"),
        Some("u1/ws".to_owned()),
        "both-centred skips overlap to u1"
    );
}

#[test]
fn r_mov12_world_send_matches_move_targets() {
    // Directional sends rank identically to exhausted moves through the
    // world route: the transfer names the selected output.
    for (mover, expect) in [
        (window_rect("win-a", "src", "ws", 860, 1150, 200, 200), "u2"),
        (
            window_rect("win-a", "src", "ws", 300, 1150, 1100, 200),
            "u2",
        ),
        (window_rect("win-a", "src", "ws", 900, 1150, 400, 100), "u1"),
    ] {
        let geometries = if expect == "u2" && mover.rect.w == 200 {
            (
                rect(0, 1080, 1920, 1080),
                rect(0, 0, 960, 1080),
                rect(960, 0, 960, 1080),
            )
        } else if mover.rect.w == 1100 {
            (
                rect(0, 1080, 1600, 1080),
                rect(0, 0, 400, 1080),
                rect(1200, 0, 400, 1080),
            )
        } else {
            (
                rect(0, 1080, 1600, 1080),
                rect(0, 0, 1200, 1080),
                rect(400, 0, 1200, 1080),
            )
        };
        let report = three_output_report(1, geometries.0, geometries.1, geometries.2, mover);
        let mut engine = WorldEngine::new("gen-1").expect("valid");
        accepted(&mut engine, &report);
        let mut cmd = report.clone();
        with_command(
            &mut cmd,
            2,
            112,
            WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "up".to_owned(),
                follow: true,
            },
        );
        let reply = accepted(&mut engine, &cmd);
        let transfer = reply
            .transfers
            .iter()
            .find(|t| matches!(t.kind, TransferKind::Window(_)))
            .expect("send retains a transfer");
        assert_eq!(
            transfer.target_output.0, expect,
            "send selects {expect} (mover {:?})",
            report.domains[0].windows[0].rect
        );
    }
}

#[test]
fn r_ws27_world_migration_selects_largest_edge_despite_window_centre() {
    // L (0,1080,1920,1080) shows ws2; U1 (0,0,1200,1080, edge 1200) and
    // U2 (1200,0,720,1080, edge 720) both touch above. B's centre x=1600
    // projects onto U2, but migration ignores window position: U1 wins.
    let report = WorldReport {
        topology: vec![
            OutputTopology {
                output: OutputId("out-l".to_owned()),
                native_order: vec!["ws2".to_owned()],
                current_native: "ws2".to_owned(),
                complete: true,
                geometry: Some(rect(0, 1080, 1920, 1080)),
                primary: true,
            },
            OutputTopology {
                output: OutputId("u1".to_owned()),
                native_order: vec!["wu1".to_owned()],
                current_native: "wu1".to_owned(),
                complete: true,
                geometry: Some(rect(0, 0, 1200, 1080)),
                primary: false,
            },
            OutputTopology {
                output: OutputId("u2".to_owned()),
                native_order: vec!["wu2".to_owned()],
                current_native: "wu2".to_owned(),
                complete: true,
                geometry: Some(rect(1200, 0, 720, 1080)),
                primary: false,
            },
        ],
        bindings: vec![
            WorkspaceBinding {
                logical_id: "ll".to_owned(),
                native_id: "ws2".to_owned(),
                output: OutputId("out-l".to_owned()),
                pinned: false,
                displaced: false,
                managed: true,
            },
            WorkspaceBinding {
                logical_id: "l1".to_owned(),
                native_id: "wu1".to_owned(),
                output: OutputId("u1".to_owned()),
                pinned: false,
                displaced: false,
                managed: true,
            },
            WorkspaceBinding {
                logical_id: "l2".to_owned(),
                native_id: "wu2".to_owned(),
                output: OutputId("u2".to_owned()),
                pinned: false,
                displaced: false,
                managed: true,
            },
        ],
        domains: vec![
            DomainReport {
                domain: OutputDomain {
                    id: OutputId("out-l".to_owned()),
                    workspace: WorkspaceId("ws2".to_owned()),
                    bounds: rect(0, 1080, 1920, 1080),
                    gap: 0,
                    adjacent: BTreeMap::new(),
                },
                outer_gap: 0,
                focused: WindowId("win-b".to_owned()),
                windows: vec![
                    window_rect("win-a", "out-l", "ws2", 100, 1150, 100, 100),
                    window_rect("win-b", "out-l", "ws2", 1550, 1150, 100, 100),
                ],
                closed: Vec::new(),
                readable: true,
                hold: None,
            },
            domain_report(
                "u1",
                "wu1",
                vec![window_rect("win-u1", "u1", "wu1", 10, 10, 100, 100)],
            ),
            domain_report(
                "u2",
                "wu2",
                vec![window_rect("win-u2", "u2", "wu2", 1210, 10, 100, 100)],
            ),
        ],
        focused_global: "win-b".to_owned(),
        inventory_complete: true,
        ..two_output_report(1, Some(rect(0, 0, 800, 600)))
    };
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &report);
    let mut cmd = report.clone();
    with_command(
        &mut cmd,
        2,
        121,
        WorldCommand::MigrateDirectional {
            source_output: OutputId("out-l".to_owned()),
            workspace: "ws2".to_owned(),
            direction: "up".to_owned(),
        },
    );
    let reply = accepted(&mut engine, &cmd);
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)
                && t.target_output.0 == "u1"
                && t.target_workspace == "ws2"),
        "migration selects u1 despite B centre on u2, transfers={:?}",
        reply.transfers
    );
}

#[test]
fn strict_split_edge_focus_stays_local_while_move_selects() {
    // Horizontal split: tall (0,0,800,1200) against top/bottom stacked
    // pair. Strict focus right omits the multi-touch side, so focus
    // stays local; the windowed move ranks the same side by mover y.
    let tall = rect(0, 0, 800, 1200);
    let top = rect(800, 0, 800, 600);
    let bottom = rect(800, 600, 800, 600);
    let mover = window_rect("win-a", "tall", "ws", 10, 50, 100, 100);
    let report = WorldReport {
        topology: vec![
            OutputTopology {
                output: OutputId("tall".to_owned()),
                native_order: vec!["ws".to_owned()],
                current_native: "ws".to_owned(),
                complete: true,
                geometry: Some(tall),
                primary: true,
            },
            OutputTopology {
                output: OutputId("top".to_owned()),
                native_order: vec!["ws".to_owned()],
                current_native: "ws".to_owned(),
                complete: true,
                geometry: Some(top),
                primary: false,
            },
            OutputTopology {
                output: OutputId("bot".to_owned()),
                native_order: vec!["ws".to_owned()],
                current_native: "ws".to_owned(),
                complete: true,
                geometry: Some(bottom),
                primary: false,
            },
        ],
        domains: vec![
            DomainReport {
                domain: OutputDomain {
                    id: OutputId("tall".to_owned()),
                    workspace: WorkspaceId("ws".to_owned()),
                    bounds: tall,
                    gap: 0,
                    adjacent: BTreeMap::new(),
                },
                outer_gap: 0,
                focused: WindowId("win-a".to_owned()),
                windows: vec![
                    mover.clone(),
                    window_rect("win-a2", "tall", "ws", 10, 900, 100, 100),
                ],
                closed: Vec::new(),
                readable: true,
                hold: None,
            },
            domain_report(
                "top",
                "ws",
                vec![window_rect("win-t", "top", "ws", 810, 10, 100, 100)],
            ),
            domain_report(
                "bot",
                "ws",
                vec![window_rect("win-bo", "bot", "ws", 810, 610, 100, 100)],
            ),
        ],
        bindings: Vec::new(),
        focused_global: "win-a".to_owned(),
        inventory_complete: true,
        ..two_output_report(1, Some(rect(0, 0, 800, 600)))
    };
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &report);
    // Focus right stays local (strict multi-touch omits the side): with no
    // local right neighbor the command refuses as unchanged, never
    // crossing to top/bot.
    let mut focus = report.clone();
    with_command(
        &mut focus,
        2,
        131,
        WorldCommand::FocusDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            float_subject: false,
        },
    );
    match engine.handle_report(&focus) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "unchanged"),
        other => panic!("strict split-edge focus stays local, got {other:?}"),
    }
    // The windowed move ranks the same side on a sole-leaf source:
    // mover centre y=100 falls in top's edge [0,600), so the mover
    // lands on top. (A second local tile would let local R1 win, so the
    // move leg uses a sole source like the R-MOV-12 legs above.)
    let mut move_report = report.clone();
    move_report.domains[0] = domain_report(
        "tall",
        "ws",
        vec![window_rect("win-a", "tall", "ws", 10, 50, 100, 100)],
    );
    move_report.focused_global = "win-a".to_owned();
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &move_report);
    let mut mv = move_report.clone();
    with_command(
        &mut mv,
        2,
        132,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    let reply = accepted(&mut engine, &mv);
    assert_eq!(
        homing(&reply, "win-a"),
        Some("top/ws".to_owned()),
        "windowed move selects top by centre"
    );
}

#[test]
fn ordinary_two_output_move_and_send_succeed() {
    // Single-candidate ordinary neighbors cross in every route.
    let mover = window_rect("win-a", "src", "ws", 10, 1150, 100, 100);
    let report = three_output_report(
        1,
        rect(0, 1080, 1920, 1080),
        rect(0, 0, 1920, 1080),
        rect(3000, 0, 800, 600),
        mover,
    );
    // u1 directly above src (edge [0,1920)); u2 far away, no touch.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &report);
    let reply = move_up_once(&mut engine, &report, 2);
    assert_eq!(homing(&reply, "win-a"), Some("u1/ws".to_owned()));
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &report);
    let mut send = report.clone();
    with_command(
        &mut send,
        2,
        142,
        WorldCommand::SendDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "up".to_owned(),
            follow: false,
        },
    );
    let reply = accepted(&mut engine, &send);
    assert!(
        reply.transfers.iter().any(|t| t.target_output.0 == "u1"),
        "ordinary send reaches u1"
    );
}

#[test]
fn unreadable_topology_refuses_and_gap_runs_local_or_refuses() {
    // Unreadable FULL (zero width) refuses moves/sends/migration without
    // mutation; a clean gap (no touch) runs local move / refuses send.
    let mover = window_rect("win-a", "src", "ws", 10, 1150, 100, 100);
    let good = three_output_report(
        1,
        rect(0, 1080, 800, 600),
        rect(0, 0, 800, 600),
        rect(3000, 0, 800, 600),
        mover.clone(),
    );
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &good);
    // Unreadable: u1 zero width.
    let mut bad = good.clone();
    bad.topology[1].geometry = Some(rect(0, 0, 0, 600));
    bad.request_seq = 2;
    bad.request_id = "req-2".to_owned();
    bad.observation_seq = 2;
    with_command(
        &mut bad,
        2,
        151,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "up".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    match engine.handle_report(&bad) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "operation-refused"),
        other => panic!("unreadable move refuses, got {other:?}"),
    }
    let mut bad_send = good.clone();
    bad_send.request_seq = 2;
    bad_send.request_id = "req-2".to_owned();
    bad_send.observation_seq = 2;
    with_command(
        &mut bad_send,
        2,
        152,
        WorldCommand::SendDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "up".to_owned(),
            follow: false,
        },
    );
    bad_send.topology[1].geometry = Some(rect(0, 0, 0, 600));
    match engine.handle_report(&bad_send) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-target-unknown"),
        other => panic!("unreadable send refuses, got {other:?}"),
    }
    // Gap: u1 far away (no touch). Move runs local (accepted, mover
    // stays on src); send refuses. Two tiles in the source give the local
    // move a target (a sole leaf has no local move and refuses instead).
    let mut gap = good.clone();
    gap.topology[1].geometry = Some(rect(0, 0, 800, 600));
    gap.topology[2].geometry = Some(rect(3000, 0, 800, 600));
    // Move src away from u1: shift src so nothing touches.
    gap.topology[0].geometry = Some(rect(0, 2000, 800, 600));
    gap.domains[0] = domain_report(
        "src",
        "ws",
        vec![
            mover.clone(),
            window_rect("win-a2", "src", "ws", 300, 1150, 100, 100),
        ],
    );
    gap.request_seq = 3;
    gap.request_id = "req-3".to_owned();
    gap.observation_seq = 3;
    with_command(
        &mut gap,
        3,
        153,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "up".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    let reply = accepted(&mut engine, &gap);
    assert_eq!(
        homing(&reply, "win-a"),
        Some("src/ws".to_owned()),
        "gap move stays local"
    );
    let mut gap_send = good.clone();
    gap_send.topology[0].geometry = Some(rect(0, 2000, 800, 600));
    gap_send.request_seq = 4;
    gap_send.request_id = "req-4".to_owned();
    gap_send.observation_seq = 4;
    with_command(
        &mut gap_send,
        4,
        154,
        WorldCommand::SendDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "up".to_owned(),
            follow: false,
        },
    );
    match engine.handle_report(&gap_send) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-target-unknown"),
        other => panic!("gap send refuses, got {other:?}"),
    }
}

#[test]
fn r4_flight_fences_second_move_pre_engine_without_rerank() {
    // Production-path pin proof: after an exhausted R4 move retains its
    // identity-pinned flight, a second move for the same window refuses as
    // transfer-in-flight pre-engine (no re-rank, no second transfer, no
    // sequence consumption, no mutation). Covers both the lagging cross
    // path and the gap-topology local fallback path, which shares the
    // same fence through the local envelope.
    let mover = window_rect("win-a", "src", "ws", 860, 1150, 200, 200);
    let report = three_output_report(
        1,
        rect(0, 1080, 1920, 1080),
        rect(0, 0, 960, 1080),
        rect(960, 0, 960, 1080),
        mover,
    );
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &report);
    let mut cmd = report.clone();
    with_command(
        &mut cmd,
        2,
        112,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "up".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    let reply = accepted(&mut engine, &cmd);
    assert_eq!(homing(&reply, "win-a"), Some("u2/ws".to_owned()));
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Window(_)) && t.target_output.0 == "u2")
    );
    let settled_revision = engine.state_revision();
    // Lagging observation (mover still at the source) with a fresh move
    // command: pre-engine fence, never a re-rank.
    let mut again = report.clone();
    with_command(
        &mut again,
        3,
        113,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "up".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    match engine.handle_report(&again) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "transfer-in-flight"),
        other => panic!("second move while flight outstanding refuses, got {other:?}"),
    }
    assert_eq!(
        engine.state_revision(),
        settled_revision,
        "pre-engine refusal mutates nothing"
    );
    // Gap topology forcing the no-candidate local fallback under the same
    // lagging window: the same fence fires before any local mutation.
    let mut gap = report.clone();
    gap.topology[0].geometry = Some(rect(0, 2000, 800, 600));
    gap.topology[1].geometry = Some(rect(0, 0, 800, 600));
    gap.topology[2].geometry = Some(rect(3000, 0, 800, 600));
    with_command(
        &mut gap,
        4,
        114,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "up".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    match engine.handle_report(&gap) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "transfer-in-flight"),
        other => panic!("gap local move while flight outstanding refuses, got {other:?}"),
    }
    assert_eq!(
        engine.state_revision(),
        settled_revision,
        "gap-path refusal mutates nothing"
    );
    // The retained flight survives both refusals with its stable identity:
    // an observation-only lag report keeps the single pinned transfer.
    let mut lag = report.clone();
    lag.request_seq = 5;
    lag.request_id = "req-5".to_owned();
    lag.observation_seq = 5;
    let lag_reply = accepted(&mut engine, &lag);
    assert_eq!(homing(&lag_reply, "win-a"), Some("u2/ws".to_owned()));
    let flights: Vec<_> = lag_reply
        .transfers
        .iter()
        .filter(|t| matches!(t.kind, TransferKind::Window(_)))
        .collect();
    assert_eq!(
        flights.len(),
        1,
        "exactly the pinned flight, never a second"
    );
    assert_eq!(
        flights[0].operation_id, "op-112",
        "stable identity across refusals"
    );
}

#[test]
fn reverse_ambiguous_pair_still_crosses_world_move() {
    // Forward single, reverse double: src up touches only wide; wide
    // down touches src plus src2. Strict uniqueness would omit, but the
    // world windowed pair accepts reciprocal existence.
    let wide = rect(0, 0, 1600, 600);
    let src_rect = rect(0, 600, 800, 600);
    let src2_rect = rect(800, 600, 800, 600);
    let mover = window_rect("win-a", "src", "ws", 10, 650, 100, 100);
    let report = WorldReport {
        topology: vec![
            OutputTopology {
                output: OutputId("src".to_owned()),
                native_order: vec!["ws".to_owned()],
                current_native: "ws".to_owned(),
                complete: true,
                geometry: Some(src_rect),
                primary: true,
            },
            OutputTopology {
                output: OutputId("src2".to_owned()),
                native_order: vec!["ws".to_owned()],
                current_native: "ws".to_owned(),
                complete: true,
                geometry: Some(src2_rect),
                primary: false,
            },
            OutputTopology {
                output: OutputId("wide".to_owned()),
                native_order: vec!["ws".to_owned()],
                current_native: "ws".to_owned(),
                complete: true,
                geometry: Some(wide),
                primary: false,
            },
        ],
        domains: vec![
            domain_report("src", "ws", vec![mover]),
            domain_report(
                "src2",
                "ws",
                vec![window_rect("win-s2", "src2", "ws", 810, 650, 100, 100)],
            ),
            domain_report(
                "wide",
                "ws",
                vec![window_rect("win-w", "wide", "ws", 10, 10, 100, 100)],
            ),
        ],
        bindings: Vec::new(),
        focused_global: "win-a".to_owned(),
        inventory_complete: true,
        ..two_output_report(1, Some(rect(0, 0, 800, 600)))
    };
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &report);
    let mut cmd = report.clone();
    with_command(
        &mut cmd,
        2,
        161,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "up".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    let reply = accepted(&mut engine, &cmd);
    assert_eq!(
        homing(&reply, "win-a"),
        Some("wide/ws".to_owned()),
        "reverse-ambiguous pair still crosses"
    );
}

#[test]
fn r4_exhausted_move_retains_identity_pinned_transfer_and_settles_on_arrival() {
    // Exhausted cross-output MoveDirectional retains one identity-pinned
    // window transfer (stable op id, dispatch source/target, follow to the
    // target view) using the existing send transfer contract; local
    // R1-R3 moves retain none with focus unchanged; lag keeps the flight
    // with no replay; arrival proof settles it; a fresh reverse move
    // resolves against fresh topology without reranking the relocated
    // mover frame.
    let mover = window_rect("win-a", "src", "ws", 860, 1150, 200, 200);
    let report = three_output_report(
        1,
        rect(0, 1080, 1920, 1080),
        rect(0, 0, 960, 1080),
        rect(960, 0, 960, 1080),
        mover,
    );
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &report);
    let mut cmd = report.clone();
    with_command(
        &mut cmd,
        2,
        112,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "up".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    let reply = accepted(&mut engine, &cmd);
    assert_eq!(homing(&reply, "win-a"), Some("u2/ws".to_owned()));
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Window(_)))
        .expect("exhausted R4 move retains a window transfer");
    assert_eq!(transfer.operation_id, "op-112", "stable command identity");
    assert_eq!(transfer.target_output.0, "u2");
    assert_eq!(transfer.target_workspace, "ws");
    assert!(transfer.follow, "move follows the mover to the target view");
    assert!(
        reply.effects.is_empty(),
        "local R4 move carries no focus effect; focus stays on the mover"
    );
    // Local R1-R3 move on a gap retains no transfer.
    let mut gap_engine = WorldEngine::new("gen-1").expect("valid");
    let mut gap = three_output_report(
        1,
        rect(0, 2000, 800, 600),
        rect(0, 0, 800, 600),
        rect(3000, 0, 800, 600),
        window_rect("win-a", "src", "ws", 10, 1150, 100, 100),
    );
    gap.domains[0] = domain_report(
        "src",
        "ws",
        vec![
            window_rect("win-a", "src", "ws", 10, 1150, 100, 100),
            window_rect("win-a2", "src", "ws", 300, 1150, 100, 100),
        ],
    );
    accepted(&mut gap_engine, &gap);
    let mut local = gap.clone();
    with_command(
        &mut local,
        3,
        153,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "up".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    let local_reply = accepted(&mut gap_engine, &local);
    assert_eq!(homing(&local_reply, "win-a"), Some("src/ws".to_owned()));
    assert!(
        local_reply
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Window(_))),
        "local moves retain no window transfer"
    );
    assert!(local_reply.effects.is_empty(), "local move focus unchanged");
    // Actuation lag: the mover still observes at the source; the flight
    // persists with no replay and the desired homing stays pinned.
    let mut lag = report.clone();
    lag.request_seq = 3;
    lag.request_id = "req-3".to_owned();
    lag.observation_seq = 3;
    let lag_reply = accepted(&mut engine, &lag);
    assert_eq!(homing(&lag_reply, "win-a"), Some("u2/ws".to_owned()));
    let lag_transfer = lag_reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Window(_)))
        .expect("lag keeps the pinned flight");
    assert_eq!(
        lag_transfer.operation_id, "op-112",
        "no replay: same stable identity"
    );
    assert_eq!(lag_transfer.target_output.0, "u2");
    // Arrival proof: the mover now observes at the target; the flight
    // settles exactly once with no further transfer.
    let mut arrival = report.clone();
    arrival.request_seq = 4;
    arrival.request_id = "req-4".to_owned();
    arrival.observation_seq = 4;
    arrival.domains[0] = domain_report("src", "ws", Vec::new());
    arrival.domains[2] = domain_report(
        "u2",
        "ws",
        vec![
            window_rect("win-a", "u2", "ws", 970, 10, 100, 100),
            window_rect("win-u2", "u2", "ws", 970, 10, 100, 100),
        ],
    );
    arrival.topology[0].native_order = Vec::new();
    let arrival_reply = accepted(&mut engine, &arrival);
    assert_eq!(homing(&arrival_reply, "win-a"), Some("u2/ws".to_owned()));
    assert!(
        arrival_reply.transfers.is_empty(),
        "arrival proof settles the flight exactly once"
    );
    // Fresh topology after arrival: a reverse move resolves against the
    // current rects (no stale rerank of the relocated frame).
    let mut back = arrival.clone();
    back.request_seq = 5;
    back.request_id = "req-5".to_owned();
    back.observation_seq = 5;
    back.focused_global = "win-a".to_owned();
    with_command(
        &mut back,
        5,
        113,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "down".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    let back_reply = accepted(&mut engine, &back);
    assert_eq!(homing(&back_reply, "win-a"), Some("src/ws".to_owned()));
    let back_transfer = back_reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Window(_)))
        .expect("reverse move pins a fresh transfer");
    assert_eq!(back_transfer.operation_id, "op-113");
    assert_eq!(back_transfer.target_output.0, "src");
}

#[test]
fn r4_capacity_float_fallback_retains_identity_pinned_window_transfer() {
    // Tiny occupied readable target (1x1 FULL, one resident tile): the R4
    // wrap plans but the affected projection exhausts, so the shared
    // capacity-float send carries the mover slotless with one
    // identity-pinned Window transfer (follow to the target view). The
    // mover row homes at the target with no Controlled frame authority,
    // so no invalid geometry can actuate.
    use tiler_core::world_report::ScopeAuthority;
    let tiny = rect(800, 0, 1, 1);
    let mut report = two_output_report(1, Some(tiny));
    report.topology[0].geometry = Some(rect(0, 0, 800, 600));
    report.domains[0] = domain_report("out-a", "wa1", vec![engine_window("win-a", "out-a", "wa1")]);
    report.domains[1] = DomainReport {
        domain: OutputDomain {
            id: OutputId("out-b".to_owned()),
            workspace: WorkspaceId("wb1".to_owned()),
            bounds: tiny,
            gap: 0,
            adjacent: BTreeMap::new(),
        },
        outer_gap: 0,
        focused: WindowId("win-t".to_owned()),
        windows: vec![window_rect("win-t", "out-b", "wb1", 800, 0, 1, 1)],
        closed: Vec::new(),
        readable: true,
        hold: None,
    };
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &report);
    let mut cmd = report.clone();
    with_command(
        &mut cmd,
        2,
        171,
        WorldCommand::MoveDirectional {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    let reply = accepted(&mut engine, &cmd);
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Window(_)))
        .expect("exhausted R4 floats with a pinned transfer");
    assert_eq!(transfer.operation_id, "op-171", "stable command identity");
    assert_eq!(transfer.target_output.0, "out-b");
    assert_eq!(transfer.target_workspace, "wb1", "target current view");
    assert!(transfer.follow, "R4 fallback always follows the mover");
    assert_eq!(homing(&reply, "win-a"), Some("out-b/wb1".to_owned()));
    let row = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .expect("mover row present");
    assert!(
        !matches!(row.frame_authority, ScopeAuthority::Controlled),
        "slotless float carries no frame authority, frame_authority={:?}",
        row.frame_authority
    );
    assert!(
        row.allocation.is_none(),
        "slotless float carries no allocation"
    );
}
