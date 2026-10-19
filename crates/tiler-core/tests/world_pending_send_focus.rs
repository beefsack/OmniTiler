//! Pending trailing-send deferred focus (world route): a numbered send 0
//! that mints a trailing token and pends must emit the committed Engine
//! focus result when the binding lands (stay: source MRU survivor;
//! follow: mover), exactly once.

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldEngineConfig, WorldOutcome};
use tiler_core::world_report::{
    DomainReport, OutputTopology, WORLD_CONTRACT_VERSION, WorkspaceBinding, WorldCommand,
    WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn aligned_config() -> WorldEngineConfig {
    WorldEngineConfig {
        fixed_send_admission: false,
        maximized_focus_fence: true,
        aligned_per_output_workspaces: true,
    }
}

const NATIVES: [&str; 2] = ["wa1", "wb1"];

fn x_for(id: &str) -> i32 {
    match id {
        "win-a" | "win-b" => 10,
        "win-a2" => 300,
        _ => 550,
    }
}

fn win(id: &str, out: &str, ws: &str) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        rect: rect(x_for(id), 10, 100, 100),
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

fn domain(out: &str, ws: &str, focused: &str, windows: Vec<EngineWindow>) -> DomainReport {
    DomainReport {
        domain: OutputDomain {
            id: OutputId(out.to_owned()),
            workspace: WorkspaceId(ws.to_owned()),
            bounds: rect(0, 0, 800, 600),
            gap: 0,
            adjacent: BTreeMap::new(),
        },
        outer_gap: 0,
        focused: WindowId(focused.to_owned()),
        windows,
        closed: Vec::new(),
        readable: true,
        hold: None,
    }
}

fn topo(out: &str, current: &str, primary: bool, natives: &[String]) -> OutputTopology {
    OutputTopology {
        output: OutputId(out.to_owned()),
        native_order: natives.to_vec(),
        current_native: current.to_owned(),
        complete: true,
        geometry: Some(if out == "out-b" {
            rect(800, 0, 800, 600)
        } else {
            rect(0, 0, 800, 600)
        }),
        primary,
    }
}

fn bindings() -> Vec<WorkspaceBinding> {
    vec![
        WorkspaceBinding {
            logical_id: "la-a1".to_owned(),
            native_id: "wa1".to_owned(),
            output: OutputId("out-a".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        },
        WorkspaceBinding {
            logical_id: "la-b1".to_owned(),
            native_id: "wb1".to_owned(),
            output: OutputId("out-a".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        },
        WorkspaceBinding {
            logical_id: "lb-a1".to_owned(),
            native_id: "wa1".to_owned(),
            output: OutputId("out-b".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        },
        WorkspaceBinding {
            logical_id: "lb-b1".to_owned(),
            native_id: "wb1".to_owned(),
            output: OutputId("out-b".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        },
    ]
}

/// Aligned two-output world with a fully occupied out-a ring: out-a/wa1
/// holds mover win-a plus survivor win-a2; out-a/wb1 holds occupant
/// win-occ so numbered send 0 mints a trailing token instead of reusing.
/// Both shown domains duplicate the global focus (producer shape).
fn report(seq: u64, global: &str, natives: &[String]) -> WorldReport {
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
            topo("out-a", "wa1", true, natives),
            topo("out-b", "wb1", false, natives),
        ],
        bindings: bindings(),
        domains: vec![
            domain(
                "out-a",
                "wa1",
                global,
                vec![win("win-a", "out-a", "wa1"), win("win-a2", "out-a", "wa1")],
            ),
            domain("out-a", "wb1", global, vec![win("win-occ", "out-a", "wb1")]),
            domain("out-b", "wb1", global, vec![win("win-b", "out-b", "wb1")]),
        ],
        minimized: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: global.to_owned(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: Vec::new(),
    }
}

fn natives() -> Vec<String> {
    NATIVES.iter().map(|s| s.to_string()).collect()
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

fn seed_history(engine: &mut WorldEngine, n: &[String]) {
    accepted(engine, &report(1, "win-a2", n));
    accepted(engine, &report(2, "win-a", n));
}

fn run_case(follow: bool, expect_target: &str) {
    let n = natives();
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    seed_history(&mut engine, &n);
    // Numbered send 0: ring fully occupied on out-a, so a token pends.
    let mut send = report(3, "win-a", &n);
    send.request_seq = 3;
    send.request_id = "req-3".to_owned();
    send.observation_seq = 3;
    send.command_seq = Some(30);
    send.command = Some(WorldCommand::SendNumbered {
        window: WindowId("win-a".to_owned()),
        output: OutputId("out-a".to_owned()),
        index: 0,
        follow,
    });
    let reply = accepted(&mut engine, &send);
    assert!(
        reply.transfers.is_empty(),
        "trailing create pends without a transfer: {:?}",
        reply.transfers
    );
    let token = reply
        .workspaces
        .iter()
        .find(|w| w.native_binding.is_none())
        .expect("unbound pending row")
        .id
        .clone();
    assert!(token.starts_with("wl-"), "token shape: {token}");
    assert!(
        reply.effects.is_empty(),
        "pend itself emits no focus: {:?}",
        reply.effects
    );
    // Binding lands: the compositor created wa-new on out-a.
    let mut bound_natives = n.clone();
    bound_natives.push("wa-new".to_owned());
    let mut bound = report(4, "win-a", &bound_natives);
    bound.request_seq = 4;
    bound.request_id = "req-4".to_owned();
    bound.observation_seq = 4;
    bound.bindings.push(WorkspaceBinding {
        logical_id: token.clone(),
        native_id: "wa-new".to_owned(),
        output: OutputId("out-a".to_owned()),
        pinned: false,
        displaced: false,
        managed: true,
    });
    bound
        .domains
        .push(domain("out-a", "wa-new", "win-a", vec![]));
    let reply = accepted(&mut engine, &bound);
    assert!(
        reply.transfers.iter().any(|t| {
            matches!(&t.kind, tiler_core::world_report::TransferKind::Window(_))
                && t.target_workspace == "wa-new"
                && t.operation_id == "op-30"
        }),
        "bound send retains the stable window transfer: {:?}",
        reply.transfers
    );
    assert_eq!(
        reply.effects.len(),
        1,
        "deferred commit emits exactly one focus effect, effects={:?}",
        reply.effects
    );
    assert_eq!(reply.effects[0].id, "fx-30");
    assert_eq!(
        reply.effects[0].target.0, expect_target,
        "follow={follow}: deferred focus names {expect_target}, effects={:?}",
        reply.effects
    );
    // No replay on the next observation.
    let echo = accepted(&mut engine, &report(5, "win-a", &bound_natives));
    assert!(
        echo.effects.is_empty(),
        "deferred focus never replays, effects={:?}",
        echo.effects
    );
}

#[test]
fn pending_trailing_stay_emits_deferred_mru_focus() {
    run_case(false, "win-a2");
}

#[test]
fn pending_trailing_follow_emits_deferred_mover_focus() {
    run_case(true, "win-a");
}

#[test]
fn pending_trailing_stay_emptied_source_emits_no_fallback() {
    // Lone mover: the deferred stay names nothing, never an invented peer.
    let n = natives();
    let mut lone = report(1, "win-a", &n);
    lone.domains[0].windows.retain(|w| w.window.0 == "win-a");
    // Re-seed on the lone-mover layout so history carries no survivor.
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    accepted(&mut engine, &lone);
    let mut send = report(3, "win-a", &n);
    send.domains[0].windows.retain(|w| w.window.0 == "win-a");
    send.request_seq = 3;
    send.request_id = "req-3".to_owned();
    send.observation_seq = 3;
    send.command_seq = Some(31);
    send.command = Some(WorldCommand::SendNumbered {
        window: WindowId("win-a".to_owned()),
        output: OutputId("out-a".to_owned()),
        index: 0,
        follow: false,
    });
    let reply = accepted(&mut engine, &send);
    assert!(
        reply.transfers.is_empty(),
        "trailing create pends: {:?}",
        reply.transfers
    );
    let token = reply
        .workspaces
        .iter()
        .find(|w| w.native_binding.is_none())
        .expect("unbound pending row")
        .id
        .clone();
    let mut bound_natives = n.clone();
    bound_natives.push("wa-new".to_owned());
    let mut bound = report(4, "win-a", &bound_natives);
    bound.domains[0].windows.retain(|w| w.window.0 == "win-a");
    bound.request_seq = 4;
    bound.request_id = "req-4".to_owned();
    bound.observation_seq = 4;
    bound.bindings.push(WorkspaceBinding {
        logical_id: token,
        native_id: "wa-new".to_owned(),
        output: OutputId("out-a".to_owned()),
        pinned: false,
        displaced: false,
        managed: true,
    });
    bound
        .domains
        .push(domain("out-a", "wa-new", "win-a", vec![]));
    let reply = accepted(&mut engine, &bound);
    assert!(
        reply.transfers.iter().any(|t| {
            matches!(&t.kind, tiler_core::world_report::TransferKind::Window(_))
                && t.target_workspace == "wa-new"
        }),
        "bound send retains the transfer: {:?}",
        reply.transfers
    );
    assert!(
        reply.effects.is_empty(),
        "emptied deferred stay invents no fallback, effects={:?}",
        reply.effects
    );
}
