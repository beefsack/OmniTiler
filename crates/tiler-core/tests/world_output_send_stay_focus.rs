//! Output-send stay/follow focus effects (world route): stay refocuses
//! the source MRU survivor, follow focuses the mover. Regression for the
//! native own-send trace where output-stay left the mover globally active
//! at the destination (core emitted `effects=[]` for every send) instead
//! of restoring the source peer.

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

fn engine_window(id: &str, out: &str, ws: &str) -> EngineWindow {
    let x = if id == "win-a2" || id == "win-b" {
        400
    } else {
        10
    };
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        rect: rect(x, 10, 100, 100),
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

fn domain_report(out: &str, ws: &str, focused: &str, windows: Vec<EngineWindow>) -> DomainReport {
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

/// Two-output world: out-a/wa1 holds mover win-a plus survivor win-a2,
/// out-b/wb1 holds win-b. `source_focused` names the source domain focus
/// (and the global focus) for this observation.
fn report(seq: u64, source_focused: &str) -> WorldReport {
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
                source_focused,
                vec![
                    engine_window("win-a", "out-a", "wa1"),
                    engine_window("win-a2", "out-a", "wa1"),
                ],
            ),
            domain_report(
                "out-b",
                "wb1",
                "win-b",
                vec![engine_window("win-b", "out-b", "wb1")],
            ),
        ],
        minimized: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: source_focused.to_owned(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: Vec::new(),
    }
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

fn send_cmd(follow: bool) -> WorldCommand {
    WorldCommand::SendDirectional {
        window: WindowId("win-a".to_owned()),
        direction: "right".to_owned(),
        follow,
    }
}

/// Seed source focus history (win-a2, then mover win-a) so the stay
/// survivor is the genuine MRU peer, mirroring the R-WS-01 fixture
/// (focus A then B).
fn seed_history(engine: &mut WorldEngine) {
    accepted(engine, &report(1, "win-a2"));
    accepted(engine, &report(2, "win-a"));
}

#[test]
fn output_send_stay_emits_source_mru_focus_effect() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    seed_history(&mut engine);
    let mut cmd = report(3, "win-a");
    with_command(&mut cmd, 3, 3, send_cmd(false));
    let reply = accepted(&mut engine, &cmd);
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, tiler_core::world_report::TransferKind::Window(_)))
        .expect("stay retains a window transfer");
    assert!(!transfer.follow, "stay transfer carries follow=false");
    assert_eq!(
        reply.effects.len(),
        1,
        "stay emits exactly one focus effect, effects={:?}",
        reply.effects
    );
    assert_eq!(reply.effects[0].id, "fx-3");
    assert_eq!(
        reply.effects[0].target.0, "win-a2",
        "stay refocuses the source MRU survivor, effects={:?}",
        reply.effects
    );
    // No replay on the next observation: the one-shot spends at-most-once.
    let echo = accepted(&mut engine, &report(4, "win-a"));
    assert!(
        echo.effects.is_empty(),
        "stay focus never replays, effects={:?}",
        echo.effects
    );
}

#[test]
fn output_send_follow_emits_mover_focus_effect() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    seed_history(&mut engine);
    let mut cmd = report(3, "win-a");
    with_command(&mut cmd, 3, 3, send_cmd(true));
    let reply = accepted(&mut engine, &cmd);
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, tiler_core::world_report::TransferKind::Window(_)))
        .expect("follow retains a window transfer");
    assert!(transfer.follow, "follow transfer carries follow=true");
    assert_eq!(
        reply.effects.len(),
        1,
        "follow emits exactly one focus effect, effects={:?}",
        reply.effects
    );
    assert_eq!(reply.effects[0].id, "fx-3");
    assert_eq!(
        reply.effects[0].target.0, "win-a",
        "follow focuses the mover, effects={:?}",
        reply.effects
    );
    let echo = accepted(&mut engine, &report(4, "win-a"));
    assert!(
        echo.effects.is_empty(),
        "follow focus never replays, effects={:?}",
        echo.effects
    );
}

// --- Aligned KDE-shaped fixtures (replicated native inventory) ---
//
// KDE repeats the global native desktop list under every output while
// bindings stay per (output, native) with distinct logical ids. The
// global focus duplicates the focused domain's focus. These helpers
// mirror that shape with two source survivors so the stay MRU pick is
// discriminated (win-a2 vs win-a3, win-b2 vs win-b3).

fn aligned_config() -> WorldEngineConfig {
    WorldEngineConfig {
        fixed_send_admission: false,
        maximized_focus_fence: true,
        aligned_per_output_workspaces: true,
    }
}

const ALIGNED_NATIVES: [&str; 4] = ["wa1", "wb1", "ws-3", "ws-4"];

fn aligned_x(id: &str) -> i32 {
    match id {
        "win-a" | "win-b" => 10,
        "win-a2" | "win-b2" => 300,
        _ => 550,
    }
}

fn aligned_win(id: &str, out: &str, ws: &str) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        rect: rect(aligned_x(id), 10, 100, 100),
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

fn aligned_domain(out: &str, ws: &str, focused: &str, windows: Vec<EngineWindow>) -> DomainReport {
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

fn aligned_topo(out: &str, current: &str, primary: bool) -> OutputTopology {
    OutputTopology {
        output: OutputId(out.to_owned()),
        native_order: ALIGNED_NATIVES.iter().map(|s| s.to_string()).collect(),
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

fn aligned_bindings() -> Vec<WorkspaceBinding> {
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

/// Aligned two-output world: out-a/wa1 holds mover win-a plus TWO
/// survivors (win-a2 MRU, win-a3 older); out-b/wb1 holds mover win-b
/// plus TWO survivors (win-b2 MRU, win-b3 older). Producer shape
/// (world-observe.ts): BOTH shown domains duplicate the global active
/// lifetime, even when it is foreign-homed; the seeded engine history
/// still carries the local MRU through that foreign focus.
fn aligned_report(seq: u64, global: &str) -> WorldReport {
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
            aligned_topo("out-a", "wa1", true),
            aligned_topo("out-b", "wb1", false),
        ],
        bindings: aligned_bindings(),
        domains: vec![
            aligned_domain(
                "out-a",
                "wa1",
                global,
                vec![
                    aligned_win("win-a", "out-a", "wa1"),
                    aligned_win("win-a2", "out-a", "wa1"),
                    aligned_win("win-a3", "out-a", "wa1"),
                ],
            ),
            aligned_domain(
                "out-b",
                "wb1",
                global,
                vec![
                    aligned_win("win-b", "out-b", "wb1"),
                    aligned_win("win-b2", "out-b", "wb1"),
                    aligned_win("win-b3", "out-b", "wb1"),
                ],
            ),
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

fn aligned_send_cmd(window: &str, direction: &str, follow: bool) -> WorldCommand {
    WorldCommand::SendDirectional {
        window: WindowId(window.to_owned()),
        direction: direction.to_owned(),
        follow,
    }
}

/// Seed source focus history so the stay survivor is the genuine MRU
/// peer: oldest survivor, then MRU survivor, then the mover.
fn seed_aligned_right(engine: &mut WorldEngine) {
    accepted(engine, &aligned_report(1, "win-a3"));
    accepted(engine, &aligned_report(2, "win-a2"));
    accepted(engine, &aligned_report(3, "win-a"));
}

fn seed_aligned_left(engine: &mut WorldEngine) {
    accepted(engine, &aligned_report(1, "win-b3"));
    accepted(engine, &aligned_report(2, "win-b2"));
    accepted(engine, &aligned_report(3, "win-b"));
}

fn aligned_engine() -> WorldEngine {
    WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid")
}

#[test]
fn aligned_stay_right_refocuses_mru_survivor() {
    let mut engine = aligned_engine();
    seed_aligned_right(&mut engine);
    let mut cmd = aligned_report(4, "win-a");
    with_command(&mut cmd, 4, 4, aligned_send_cmd("win-a", "right", false));
    let reply = accepted(&mut engine, &cmd);
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, tiler_core::world_report::TransferKind::Window(_)))
        .expect("stay retains a window transfer");
    assert!(!transfer.follow, "stay transfer carries follow=false");
    assert_eq!(
        reply.effects.len(),
        1,
        "stay emits one effect, effects={:?}",
        reply.effects
    );
    assert_eq!(reply.effects[0].id, "fx-4");
    assert_eq!(
        reply.effects[0].target.0, "win-a2",
        "stay picks MRU survivor win-a2, not arbitrary win-a3, effects={:?}",
        reply.effects
    );
    let echo = accepted(&mut engine, &aligned_report(5, "win-a"));
    assert!(
        echo.effects.is_empty(),
        "stay never replays, effects={:?}",
        echo.effects
    );
}

#[test]
fn aligned_stay_left_refocuses_mru_survivor() {
    let mut engine = aligned_engine();
    seed_aligned_left(&mut engine);
    let mut cmd = aligned_report(4, "win-b");
    with_command(&mut cmd, 4, 4, aligned_send_cmd("win-b", "left", false));
    let reply = accepted(&mut engine, &cmd);
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, tiler_core::world_report::TransferKind::Window(_)))
        .expect("stay retains a window transfer");
    assert!(!transfer.follow, "stay transfer carries follow=false");
    assert_eq!(
        reply.effects.len(),
        1,
        "stay emits one effect, effects={:?}",
        reply.effects
    );
    assert_eq!(
        reply.effects[0].target.0, "win-b2",
        "stay picks MRU survivor win-b2, not win-b3, effects={:?}",
        reply.effects
    );
    let echo = accepted(&mut engine, &aligned_report(5, "win-b"));
    assert!(
        echo.effects.is_empty(),
        "stay never replays, effects={:?}",
        echo.effects
    );
}

#[test]
fn aligned_follow_names_mover_both_directions() {
    for (mover, direction, global) in [("win-a", "right", "win-a"), ("win-b", "left", "win-b")] {
        let mut engine = aligned_engine();
        if mover == "win-a" {
            seed_aligned_right(&mut engine);
        } else {
            seed_aligned_left(&mut engine);
        }
        let mut cmd = aligned_report(4, global);
        with_command(&mut cmd, 4, 4, aligned_send_cmd(mover, direction, true));
        let reply = accepted(&mut engine, &cmd);
        assert_eq!(reply.effects.len(), 1, "follow {mover} emits one effect");
        assert_eq!(
            reply.effects[0].target.0, mover,
            "follow focuses the mover {mover}, effects={:?}",
            reply.effects
        );
        let echo = accepted(&mut engine, &aligned_report(5, global));
        assert!(echo.effects.is_empty(), "follow {mover} never replays");
    }
}

#[test]
fn aligned_settled_arrival_and_unrelated_focus_emit_no_extra_effect() {
    let mut engine = aligned_engine();
    seed_aligned_right(&mut engine);
    let mut cmd = aligned_report(4, "win-a");
    with_command(&mut cmd, 4, 4, aligned_send_cmd("win-a", "right", false));
    let reply = accepted(&mut engine, &cmd);
    assert_eq!(reply.effects.len(), 1);
    // Settled native arrival: mover observed at the destination, source
    // shows the MRU survivor focused; the one-shot must not duplicate.
    let mut arrived = aligned_report(5, "win-a2");
    arrived.domains[0].windows.retain(|w| w.window.0 != "win-a");
    arrived.domains[1]
        .windows
        .push(aligned_win("win-a", "out-b", "wb1"));
    let settled = accepted(&mut engine, &arrived);
    assert!(
        settled.effects.is_empty(),
        "settled arrival emits no duplicate, effects={:?}",
        settled.effects
    );
    match engine.handle_report(&arrived) {
        WorldOutcome::Duplicate(reply) => assert!(
            reply.effects.is_empty(),
            "duplicate arrival still quiet, effects={:?}",
            reply.effects
        ),
        WorldOutcome::Accepted(reply) => assert!(
            reply.effects.is_empty(),
            "duplicate arrival still quiet, effects={:?}",
            reply.effects
        ),
        other => panic!("expected duplicate/accepted arrival, got {other:?}"),
    }
    // Unrelated newer focus (destination peer takes global focus) stays quiet.
    // Build on the arrived layout so only focus changes, not membership.
    let mut unrelated = arrived.clone();
    unrelated.observation_seq = 6;
    unrelated.request_seq = 6;
    unrelated.request_id = "req-6".to_owned();
    unrelated.command_seq = None;
    unrelated.command = None;
    unrelated.domains[0].focused = WindowId("win-b".to_owned());
    unrelated.domains[1].focused = WindowId("win-b".to_owned());
    unrelated.focused_global = "win-b".to_owned();
    let quiet = accepted(&mut engine, &unrelated);
    assert!(quiet.effects.is_empty(), "unrelated focus emits nothing");
}

#[test]
fn aligned_emptied_source_stay_emits_no_fallback() {
    let mut engine = aligned_engine();
    // Lone mover on out-a/wa1: no survivors remain after the send.
    let mut base = aligned_report(1, "win-a");
    base.domains[0].windows.retain(|w| w.window.0 == "win-a");
    accepted(&mut engine, &base);
    let mut cmd = aligned_report(2, "win-a");
    cmd.domains[0].windows.retain(|w| w.window.0 == "win-a");
    with_command(&mut cmd, 2, 2, aligned_send_cmd("win-a", "right", false));
    let reply = accepted(&mut engine, &cmd);
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, tiler_core::world_report::TransferKind::Window(_))),
        "emptied stay still retains the window transfer"
    );
    assert!(
        reply.effects.is_empty(),
        "emptied source invents no fallback, effects={:?}",
        reply.effects
    );
}

#[test]
fn aligned_unreadable_and_refused_sends_emit_no_extra_effect() {
    // Partial topology (unreadable target geometry): send refuses with no effect.
    let mut engine = aligned_engine();
    seed_aligned_right(&mut engine);
    let mut cmd = aligned_report(4, "win-a");
    cmd.topology[1].geometry = None;
    with_command(&mut cmd, 4, 4, aligned_send_cmd("win-a", "right", false));
    match engine.handle_report(&cmd) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-target-unknown"),
        other => panic!("expected send-target refusal, got {other:?}"),
    }
    // Sticky refusal: no effect either.
    let mut engine = aligned_engine();
    seed_aligned_right(&mut engine);
    let mut sticky = aligned_report(4, "win-a");
    for w in sticky.domains[0].windows.iter_mut() {
        if w.window.0 == "win-a" {
            w.sticky = true;
        }
    }
    with_command(&mut sticky, 4, 4, aligned_send_cmd("win-a", "right", false));
    match engine.handle_report(&sticky) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-refused"),
        other => panic!("expected send-refused, got {other:?}"),
    }
    // Held source (inset exhaustion via tiny bounds + max gap):
    // scope-held with no effect. Shares the held_scopes fence with the
    // send path; gap 64 passes validation while the inset fails.
    let mut engine = aligned_engine();
    seed_aligned_right(&mut engine);
    let mut held = aligned_report(4, "win-a");
    held.domains[0].domain.bounds = rect(0, 0, 100, 100);
    held.domains[0].outer_gap = 64;
    with_command(&mut held, 4, 4, aligned_send_cmd("win-a", "right", false));
    match engine.handle_report(&held) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "scope-held"),
        other => panic!("expected scope-held, got {other:?}"),
    }
}
