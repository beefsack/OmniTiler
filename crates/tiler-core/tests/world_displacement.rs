//! Hotplug displacement and return (D2, local/global-unique): a
//! disconnected output's live workspaces persist as one separate unit on
//! a surviving output (never merged), with focused-survivor view policy,
//! tiling modes following the workspace, and automatic return with
//! then-current contents. Shared mode never displaces. Full reports on
//! the production transition, plus the pure destination chooser.

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::{SurvivorInfo, WorkspaceMode, choose_displaced_destination};
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    DomainReport, OutputTopology, TilingMode, WORLD_CONTRACT_VERSION, WorkspaceBinding,
    WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn win(id: &str, out: &str, ws: &str, x: i32) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
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

fn domain(
    out: &str,
    ws: &str,
    bounds: Rect,
    windows: Vec<EngineWindow>,
    focused: &str,
) -> DomainReport {
    DomainReport {
        domain: OutputDomain {
            id: OutputId(out.to_owned()),
            workspace: WorkspaceId(ws.to_owned()),
            bounds,
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

fn binding(logical: &str, native: &str, out: &str) -> WorkspaceBinding {
    WorkspaceBinding {
        logical_id: logical.to_owned(),
        native_id: native.to_owned(),
        output: OutputId(out.to_owned()),
        pinned: false,
        displaced: false,
        managed: true,
    }
}

/// Topology entry spec: native order plus live output facts.
struct OutputSpec {
    out: &'static str,
    order: Vec<&'static str>,
    current: &'static str,
    rect: Option<Rect>,
    primary: bool,
}

fn output(out: &'static str, order: Vec<&'static str>, current: &'static str) -> OutputSpec {
    OutputSpec {
        out,
        order,
        current,
        rect: None,
        primary: false,
    }
}

fn output_at(
    out: &'static str,
    order: Vec<&'static str>,
    current: &'static str,
    rect: Option<Rect>,
    primary: bool,
) -> OutputSpec {
    OutputSpec {
        out,
        order,
        current,
        rect,
        primary,
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    mode: WorkspaceMode,
    outputs: Vec<OutputSpec>,
    domains: Vec<DomainReport>,
    bindings: Vec<WorkspaceBinding>,
    focused_global: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: mode,
        topology: outputs
            .into_iter()
            .map(
                |OutputSpec {
                     out,
                     order,
                     current,
                     rect,
                     primary,
                 }| OutputTopology {
                    output: OutputId(out.to_owned()),
                    native_order: order.into_iter().map(str::to_owned).collect(),
                    current_native: current.to_owned(),
                    complete: true,
                    geometry: rect,
                    primary,
                },
            )
            .collect(),
        bindings,
        domains,
        minimized: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        default_tiled: true,
        focused_global: focused_global.to_owned(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: Vec::new(),
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

fn workspace_row<'a>(
    reply: &'a tiler_core::world_report::WorldReply,
    id: &str,
) -> &'a tiler_core::world_report::WorkspaceDesire {
    reply
        .workspaces
        .iter()
        .find(|w| w.id == id)
        .unwrap_or_else(|| panic!("workspace row for {id}"))
}

fn view_transfers(reply: &tiler_core::world_report::WorldReply, out: &str) -> Vec<String> {
    reply
        .transfers
        .iter()
        .filter(|t| {
            matches!(t.kind, tiler_core::world_report::TransferKind::View)
                && t.target_output.0 == out
        })
        .map(|t| t.target_workspace.clone())
        .collect()
}

/// Steady two-output state: ws1 on out-1, ws2 on out-2.
fn engine_two_output() -> WorldEngine {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws1", "ws2"], "ws1"),
                output("out-2", vec!["ws1", "ws2"], "ws2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "win-keep",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(800, 0, 800, 600),
                    vec![win("win-away", "out-2", "ws2", 810)],
                    "win-away",
                ),
            ],
            vec![binding("l1", "ws1", "out-1"), binding("l2", "ws2", "out-2")],
            "win-keep",
            None,
            None,
        ),
    );
    engine
}

#[test]
fn disconnect_keeps_unit_separate_and_shows_active_displaced() {
    let mut engine = engine_two_output();
    // out-2 disconnects with the active window on it: KWin moves the
    // window output, the desktop persists, focus stays native.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![output("out-1", vec!["ws1", "ws2"], "ws1")],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "",
                ),
                domain(
                    "out-1",
                    "ws2",
                    rect(0, 0, 800, 600),
                    vec![win("win-away", "out-1", "ws2", 10)],
                    "win-away",
                ),
            ],
            vec![binding("l1", "ws1", "out-1")],
            "win-away",
            None,
            None,
        ),
    );
    // Separate rows, survivor layout never merged: both workspaces list
    // under the survivor with their own bindings and tilings.
    let keep = workspace_row(&reply, "ws1");
    let away = workspace_row(&reply, "ws2");
    assert_eq!(keep.output.0, "out-1");
    assert_eq!(away.output.0, "out-1");
    assert_eq!(away.native_binding.as_deref(), Some("ws2"));
    assert!(away.existence);
    // Disconnected-monitor active window shows its relocated workspace.
    assert_eq!(view_transfers(&reply, "out-1"), vec!["ws2".to_owned()]);
    // The displaced layout keeps tiling with then-current contents.
    let moved = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-away")
        .expect("displaced member row");
    assert_eq!(
        format!("{}/{}", moved.output.0, moved.workspace),
        "out-1/ws2"
    );
    assert!(moved.allocation.is_some());
}

#[test]
fn disconnect_preserves_survivor_view_without_stealing() {
    let mut engine = engine_two_output();
    // Active window stays on the survivor: no view change, no stealing.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![output("out-1", vec!["ws1", "ws2"], "ws1")],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "win-keep",
                ),
                domain(
                    "out-1",
                    "ws2",
                    rect(0, 0, 800, 600),
                    vec![win("win-away", "out-1", "ws2", 10)],
                    "",
                ),
            ],
            vec![binding("l1", "ws1", "out-1")],
            "win-keep",
            None,
            None,
        ),
    );
    assert!(view_transfers(&reply, "out-1").is_empty());
    assert!(workspace_row(&reply, "ws2").existence);
}

#[test]
fn reconnect_returns_then_current_contents() {
    let mut engine = engine_two_output();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![output("out-1", vec!["ws1", "ws2"], "ws1")],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "win-keep",
                ),
                domain(
                    "out-1",
                    "ws2",
                    rect(0, 0, 800, 600),
                    vec![win("win-away", "out-1", "ws2", 10)],
                    "",
                ),
            ],
            vec![binding("l1", "ws1", "out-1")],
            "win-keep",
            None,
            None,
        ),
    );
    // Membership edits while displaced: win-new into ws2, win-away out.
    accepted(
        &mut engine,
        &report(
            3,
            3,
            WorkspaceMode::PerOutputLocal,
            vec![output("out-1", vec!["ws1", "ws2"], "ws1")],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![
                        win("win-keep", "out-1", "ws1", 0),
                        win("win-away", "out-1", "ws1", 210),
                    ],
                    "win-keep",
                ),
                domain(
                    "out-1",
                    "ws2",
                    rect(0, 0, 800, 600),
                    vec![win("win-new", "out-1", "ws2", 10)],
                    "",
                ),
            ],
            vec![binding("l1", "ws1", "out-1")],
            "win-keep",
            None,
            None,
        ),
    );
    // Original output reconnects: ws2 returns with its then-current
    // contents (win-new), never a saved snapshot; win-away stays out.
    let reply = accepted(
        &mut engine,
        &report(
            4,
            4,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws1", "ws2"], "ws1"),
                output("out-2", vec!["ws1", "ws2"], "ws2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![
                        win("win-keep", "out-1", "ws1", 0),
                        win("win-away", "out-1", "ws1", 210),
                    ],
                    "win-keep",
                ),
                domain(
                    "out-1",
                    "ws2",
                    rect(0, 0, 800, 600),
                    vec![win("win-new", "out-1", "ws2", 10)],
                    "win-keep",
                ),
            ],
            vec![binding("l1", "ws1", "out-1")],
            "win-keep",
            None,
            None,
        ),
    );
    let away = workspace_row(&reply, "ws2");
    assert_eq!(
        away.output.0, "out-2",
        "displaced unit returns to its origin"
    );
    // Native lag first: win-new still shows the survivor homing, so the
    // reply carries one stable policy transfer plus the target row. The
    // identity pins the creating revision (per-cycle epoch), never the
    // window id, so a later cycle can never alias it.
    let policy: Vec<_> = reply
        .transfers
        .iter()
        .filter(|t| {
            matches!(t.kind, tiler_core::world_report::TransferKind::Window(_))
                && t.operation_id.starts_with("op-hp-")
        })
        .collect();
    assert_eq!(
        policy.len(),
        1,
        "one stable policy transfer for the lagged member"
    );
    let first_op = policy[0].operation_id.clone();
    assert_eq!(policy[0].target_output.0, "out-2");
    assert_eq!(policy[0].target_workspace, "ws2");
    assert!(!policy[0].follow);
    let homed = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-new")
        .expect("moved-in row");
    assert_eq!(
        format!("{}/{}", homed.output.0, homed.workspace),
        "out-2/ws2"
    );
    assert_eq!(
        homed.membership_authority,
        tiler_core::world_report::ScopeAuthority::Controlled
    );
    // Repeated reports never duplicate the stable intent.
    let again = accepted(
        &mut engine,
        &report(
            5,
            4,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws1", "ws2"], "ws1"),
                output("out-2", vec!["ws1", "ws2"], "ws2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![
                        win("win-keep", "out-1", "ws1", 0),
                        win("win-away", "out-1", "ws1", 210),
                    ],
                    "win-keep",
                ),
                domain(
                    "out-1",
                    "ws2",
                    rect(0, 0, 800, 600),
                    vec![win("win-new", "out-1", "ws2", 10)],
                    "win-keep",
                ),
            ],
            vec![binding("l1", "ws1", "out-1")],
            "win-keep",
            None,
            None,
        ),
    );
    assert_eq!(
        again
            .transfers
            .iter()
            .filter(|t| t.operation_id == first_op)
            .count(),
        1,
        "stable policy intent never duplicates"
    );
    // Native arrival confirms the policy transfer without pulling the
    // user-moved window back.
    let settled = accepted(
        &mut engine,
        &report(
            6,
            6,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws1", "ws2"], "ws1"),
                output("out-2", vec!["ws1", "ws2"], "ws2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![
                        win("win-keep", "out-1", "ws1", 0),
                        win("win-away", "out-1", "ws1", 210),
                    ],
                    "win-keep",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(800, 0, 800, 600),
                    vec![win("win-new", "out-2", "ws2", 810)],
                    "win-keep",
                ),
            ],
            vec![binding("l1", "ws1", "out-1")],
            "win-keep",
            None,
            None,
        ),
    );
    assert!(settled.transfers.is_empty());
    let homed = settled
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-new")
        .expect("moved-in row");
    assert_eq!(
        format!("{}/{}", homed.output.0, homed.workspace),
        "out-2/ws2"
    );
    let stayed = settled
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-away")
        .expect("moved-out row");
    assert_eq!(
        format!("{}/{}", stayed.output.0, stayed.workspace),
        "out-1/ws1"
    );
    // Survivor actives preserve view/focus with no stealing on return.
    assert!(view_transfers(&settled, "out-1").is_empty());
}

#[test]
fn reconnect_shows_returning_workspace_for_its_active() {
    let mut engine = engine_two_output();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![output("out-1", vec!["ws1", "ws2"], "ws1")],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "",
                ),
                domain(
                    "out-1",
                    "ws2",
                    rect(0, 0, 800, 600),
                    vec![win("win-away", "out-1", "ws2", 10)],
                    "win-away",
                ),
            ],
            vec![binding("l1", "ws1", "out-1")],
            "win-away",
            None,
            None,
        ),
    );
    // Reconnect with the active window in the returning workspace: the
    // origin shows it with focus retained.
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws1", "ws2"], "ws1"),
                output("out-2", vec!["ws1", "ws2"], "ws1"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(800, 0, 800, 600),
                    vec![win("win-away", "out-2", "ws2", 810)],
                    "win-away",
                ),
            ],
            vec![binding("l1", "ws1", "out-1")],
            "win-away",
            None,
            None,
        ),
    );
    assert_eq!(workspace_row(&reply, "ws2").output.0, "out-2");
    assert_eq!(view_transfers(&reply, "out-2"), vec!["ws2".to_owned()]);
}

#[test]
fn tiling_mode_follows_displacement_and_return() {
    let mut engine = engine_two_output();
    // Manual float on ws2 before disconnect.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws1", "ws2"], "ws1"),
                output("out-2", vec!["ws1", "ws2"], "ws2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "win-keep",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(800, 0, 800, 600),
                    vec![win("win-away", "out-2", "ws2", 810)],
                    "win-away",
                ),
            ],
            vec![binding("l1", "ws1", "out-1"), binding("l2", "ws2", "out-2")],
            "win-keep",
            Some(7),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-2".to_owned()),
                workspace: "ws2".to_owned(),
                tiled: false,
            }),
        ),
    );
    assert_eq!(
        workspace_row(&reply, "ws2").tiling_mode,
        TilingMode::Floating
    );
    // Disconnect: the float follows the workspace, never reseeds.
    let displaced = accepted(
        &mut engine,
        &report(
            3,
            3,
            WorkspaceMode::PerOutputLocal,
            vec![output("out-1", vec!["ws1", "ws2"], "ws1")],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "win-keep",
                ),
                domain(
                    "out-1",
                    "ws2",
                    rect(0, 0, 800, 600),
                    vec![win("win-away", "out-1", "ws2", 10)],
                    "",
                ),
            ],
            vec![binding("l1", "ws1", "out-1")],
            "win-keep",
            None,
            None,
        ),
    );
    assert_eq!(
        workspace_row(&displaced, "ws2").tiling_mode,
        TilingMode::Floating
    );
    // Return: still floating on the origin.
    let returned = accepted(
        &mut engine,
        &report(
            4,
            4,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws1", "ws2"], "ws1"),
                output("out-2", vec!["ws1", "ws2"], "ws2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "win-keep",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(800, 0, 800, 600),
                    vec![win("win-away", "out-2", "ws2", 810)],
                    "",
                ),
            ],
            vec![binding("l1", "ws1", "out-1")],
            "win-keep",
            None,
            None,
        ),
    );
    assert_eq!(
        workspace_row(&returned, "ws2").tiling_mode,
        TilingMode::Floating
    );
}

#[test]
fn shared_mode_never_displaces() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::Shared,
            vec![
                output("out-1", vec!["ws1", "ws2"], "ws1"),
                output("out-2", vec!["ws1", "ws2"], "ws2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "win-keep",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(800, 0, 800, 600),
                    vec![win("win-away", "out-2", "ws2", 810)],
                    "win-away",
                ),
            ],
            vec![],
            "win-away",
            None,
            None,
        ),
    );
    // Disconnect: the shared order keeps every live id with no
    // displacement record and no policy view.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::Shared,
            vec![output("out-1", vec!["ws1", "ws2"], "ws1")],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "",
                ),
                domain(
                    "out-1",
                    "ws2",
                    rect(0, 0, 800, 600),
                    vec![win("win-away", "out-1", "ws2", 10)],
                    "win-away",
                ),
            ],
            vec![],
            "win-away",
            None,
            None,
        ),
    );
    assert!(workspace_row(&reply, "ws2").existence);
    assert!(reply.transfers.is_empty(), "shared displaces nothing");
}

#[test]
fn disconnected_history_discards_and_toggle_rests() {
    let mut engine = engine_two_output();
    // Record a previous view on out-2 first.
    accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws1", "ws2"], "ws1"),
                output("out-2", vec!["ws1", "ws2"], "ws1"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "win-keep",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(800, 0, 800, 600),
                    vec![win("win-away", "out-2", "ws2", 810)],
                    "",
                ),
                domain("out-2", "ws1", rect(800, 0, 800, 600), Vec::new(), ""),
            ],
            vec![binding("l1", "ws1", "out-1"), binding("l2", "ws2", "out-2")],
            "win-keep",
            None,
            None,
        ),
    );
    // Disconnect, then reconnect: the previous entry for out-2 was
    // discarded with the output, so toggle is a no-op until the next
    // recorded change.
    accepted(
        &mut engine,
        &report(
            3,
            3,
            WorkspaceMode::PerOutputLocal,
            vec![output("out-1", vec!["ws1", "ws2"], "ws1")],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "win-keep",
                ),
                domain(
                    "out-1",
                    "ws2",
                    rect(0, 0, 800, 600),
                    vec![win("win-away", "out-1", "ws2", 10)],
                    "",
                ),
            ],
            vec![binding("l1", "ws1", "out-1")],
            "win-keep",
            None,
            None,
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            4,
            4,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws1", "ws2"], "ws1"),
                output("out-2", vec!["ws1", "ws2"], "ws2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "win-keep",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(800, 0, 800, 600),
                    vec![win("win-away", "out-2", "ws2", 810)],
                    "",
                ),
            ],
            vec![binding("l1", "ws1", "out-1"), binding("l2", "ws2", "out-2")],
            "win-keep",
            None,
            None,
        ),
    );
    assert!(view_transfers(&back, "out-2").is_empty());
    let mut toggle = report(
        5,
        5,
        WorkspaceMode::PerOutputLocal,
        vec![
            output("out-1", vec!["ws1", "ws2"], "ws1"),
            output("out-2", vec!["ws1", "ws2"], "ws2"),
        ],
        vec![
            domain(
                "out-1",
                "ws1",
                rect(0, 0, 800, 600),
                vec![win("win-keep", "out-1", "ws1", 0)],
                "win-keep",
            ),
            domain(
                "out-2",
                "ws2",
                rect(800, 0, 800, 600),
                vec![win("win-away", "out-2", "ws2", 810)],
                "",
            ),
        ],
        vec![binding("l1", "ws1", "out-1"), binding("l2", "ws2", "out-2")],
        "win-keep",
        Some(9),
        Some(WorldCommand::SelectPrevious {
            output: OutputId("out-2".to_owned()),
        }),
    );
    let _ = &mut toggle;
    let reply = accepted(&mut engine, &toggle);
    assert!(
        view_transfers(&reply, "out-2").is_empty(),
        "discarded history toggles to a no-op"
    );
}

#[test]
fn global_unique_keeps_single_view_target_per_output() {
    let mut engine = engine_two_output();
    // Disconnect in global-unique with the active window displaced:
    // exactly one retained view target per output, never duplicates.
    for seq in [2u64, 3] {
        let reply = accepted(
            &mut engine,
            &report(
                seq,
                seq,
                WorkspaceMode::GlobalUnique,
                vec![output("out-1", vec!["ws1", "ws2"], "ws1")],
                vec![
                    domain(
                        "out-1",
                        "ws1",
                        rect(0, 0, 800, 600),
                        vec![win("win-keep", "out-1", "ws1", 0)],
                        "",
                    ),
                    domain(
                        "out-1",
                        "ws2",
                        rect(0, 0, 800, 600),
                        vec![win("win-away", "out-1", "ws2", 10)],
                        "win-away",
                    ),
                ],
                vec![binding("l1", "ws1", "out-1")],
                "win-away",
                None,
                None,
            ),
        );
        assert!(view_transfers(&reply, "out-1").len() <= 1);
    }
}

#[test]
fn destination_chooser_mirrors_legacy_fallback_chain() {
    let rect_of = |x: i32| Rect {
        x,
        y: 0,
        w: 1200,
        h: 800,
    };
    // No survivors refuses; a single survivor wins directly.
    assert_eq!(choose_displaced_destination(&[], Some("a"), None), None);
    assert_eq!(
        choose_displaced_destination(
            &[SurvivorInfo {
                key: "a".to_owned(),
                rect: None
            }],
            None,
            None,
        ),
        Some("a".to_owned())
    );
    // Disconnect-time geometry selects the nearest survivor.
    assert_eq!(
        choose_displaced_destination(
            &[
                SurvivorInfo {
                    key: "left".to_owned(),
                    rect: Some(rect_of(0))
                },
                SurvivorInfo {
                    key: "right".to_owned(),
                    rect: Some(rect_of(2000))
                },
            ],
            Some("left"),
            Some(Rect {
                x: 2100,
                y: 100,
                w: 100,
                h: 100
            }),
        ),
        Some("right".to_owned())
    );
    // Required orientation: source at x0 gone, survivors at x1920
    // (near) and x3840 (far primary): near wins irrespective of the
    // primary.
    assert_eq!(
        choose_displaced_destination(
            &[
                SurvivorInfo {
                    key: "near".to_owned(),
                    rect: Some(rect_of(1920))
                },
                SurvivorInfo {
                    key: "far".to_owned(),
                    rect: Some(rect_of(3840))
                },
            ],
            Some("far"),
            Some(Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080
            }),
        ),
        Some("near".to_owned())
    );
    // Without removed geometry the live primary wins.
    assert_eq!(
        choose_displaced_destination(
            &[
                SurvivorInfo {
                    key: "a".to_owned(),
                    rect: Some(rect_of(0))
                },
                SurvivorInfo {
                    key: "b".to_owned(),
                    rect: Some(rect_of(1300))
                },
            ],
            Some("b"),
            None,
        ),
        Some("b".to_owned())
    );
    // Without a primary, deterministic survivor order wins.
    assert_eq!(
        choose_displaced_destination(
            &[
                SurvivorInfo {
                    key: "a".to_owned(),
                    rect: None
                },
                SurvivorInfo {
                    key: "b".to_owned(),
                    rect: None
                },
            ],
            Some("missing"),
            None,
        ),
        Some("a".to_owned())
    );
}

#[test]
fn three_output_nearest_wins_over_far_primary() {
    // Source at x0 disconnects; survivors at x1920 (near) and x3840
    // (far, active primary): disconnect-time geometry places the unit
    // near, irrespective of the primary.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            vec![
                output_at(
                    "out-0",
                    vec!["ws0", "ws1", "ws2"],
                    "ws0",
                    Some(rect(0, 0, 1920, 1080)),
                    false,
                ),
                output_at(
                    "out-1",
                    vec!["ws0", "ws1", "ws2"],
                    "ws1",
                    Some(rect(1920, 0, 1920, 1080)),
                    false,
                ),
                output_at(
                    "out-2",
                    vec!["ws0", "ws1", "ws2"],
                    "ws2",
                    Some(rect(3840, 0, 1920, 1080)),
                    true,
                ),
            ],
            vec![
                domain(
                    "out-0",
                    "ws0",
                    rect(0, 0, 1920, 1080),
                    vec![win("win-0", "out-0", "ws0", 10)],
                    "win-0",
                ),
                domain(
                    "out-1",
                    "ws1",
                    rect(1920, 0, 1920, 1080),
                    vec![win("win-1", "out-1", "ws1", 1930)],
                    "win-1",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(3840, 0, 1920, 1080),
                    vec![win("win-2", "out-2", "ws2", 3850)],
                    "win-2",
                ),
            ],
            vec![
                binding("l0", "ws0", "out-0"),
                binding("l1", "ws1", "out-1"),
                binding("l2", "ws2", "out-2"),
            ],
            "win-0",
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![
                output_at(
                    "out-1",
                    vec!["ws0", "ws1", "ws2"],
                    "ws1",
                    Some(rect(1920, 0, 1920, 1080)),
                    false,
                ),
                output_at(
                    "out-2",
                    vec!["ws0", "ws1", "ws2"],
                    "ws2",
                    Some(rect(3840, 0, 1920, 1080)),
                    true,
                ),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(1920, 0, 1920, 1080),
                    vec![win("win-1", "out-1", "ws1", 1930)],
                    "win-1",
                ),
                domain(
                    "out-1",
                    "ws0",
                    rect(1920, 0, 1920, 1080),
                    vec![win("win-0", "out-1", "ws0", 1930)],
                    "win-0",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(3840, 0, 1920, 1080),
                    vec![win("win-2", "out-2", "ws2", 3850)],
                    "win-2",
                ),
            ],
            vec![binding("l1", "ws1", "out-1"), binding("l2", "ws2", "out-2")],
            "win-0",
            None,
            None,
        ),
    );
    assert_eq!(
        workspace_row(&reply, "ws0").output.0,
        "out-1",
        "nearest survivor wins over the far primary"
    );
    assert_eq!(view_transfers(&reply, "out-1"), vec!["ws0".to_owned()]);
}

#[test]
fn missing_geometry_falls_back_to_wired_primary() {
    // No output geometry anywhere: the wired (active, not first)
    // primary hosts the unit.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            vec![
                output_at("out-0", vec!["ws0", "ws1", "ws2"], "ws0", None, false),
                output_at("out-1", vec!["ws0", "ws1", "ws2"], "ws1", None, false),
                output_at("out-2", vec!["ws0", "ws1", "ws2"], "ws2", None, true),
            ],
            vec![
                domain(
                    "out-0",
                    "ws0",
                    rect(0, 0, 800, 600),
                    vec![win("win-0", "out-0", "ws0", 10)],
                    "win-0",
                ),
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-1", "out-1", "ws1", 10)],
                    "win-1",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(0, 0, 800, 600),
                    vec![win("win-2", "out-2", "ws2", 10)],
                    "win-2",
                ),
            ],
            vec![
                binding("l0", "ws0", "out-0"),
                binding("l1", "ws1", "out-1"),
                binding("l2", "ws2", "out-2"),
            ],
            "win-0",
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![
                output_at("out-1", vec!["ws0", "ws1", "ws2"], "ws1", None, false),
                output_at("out-2", vec!["ws0", "ws1", "ws2"], "ws2", None, true),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-1", "out-1", "ws1", 10)],
                    "win-1",
                ),
                domain(
                    "out-2",
                    "ws0",
                    rect(0, 0, 800, 600),
                    vec![win("win-0", "out-2", "ws0", 10)],
                    "win-0",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(0, 0, 800, 600),
                    vec![win("win-2", "out-2", "ws2", 10)],
                    "win-2",
                ),
            ],
            vec![binding("l1", "ws1", "out-1"), binding("l2", "ws2", "out-2")],
            "win-0",
            None,
            None,
        ),
    );
    assert_eq!(
        workspace_row(&reply, "ws0").output.0,
        "out-2",
        "wired primary wins without geometry"
    );
}

#[test]
fn policy_transfers_neither_duplicate_nor_pull_outsiders() {
    let mut engine = engine_two_output();
    // Disconnect with a lagged member still showing the old output in a
    // third workspace it never belonged to: only the true roster member
    // earns a policy transfer, exactly once across repeated reports.
    let first = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![output("out-1", vec!["ws1", "ws2"], "ws1")],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![win("win-keep", "out-1", "ws1", 0)],
                    "win-keep",
                ),
                domain(
                    "out-1",
                    "ws2",
                    rect(0, 0, 800, 600),
                    vec![win("win-away", "out-1", "ws2", 10)],
                    "win-keep",
                ),
            ],
            vec![binding("l1", "ws1", "out-1")],
            "win-keep",
            None,
            None,
        ),
    );
    // win-away already shows the survivor homing: no member transfer,
    // and no view (active win-keep is not in the moving workspace).
    assert!(
        first
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, tiler_core::world_report::TransferKind::Window(_))),
        "homed members need no policy transfer"
    );
    // Lagged variant: the member still reports the survivor output but a
    // sibling workspace id it just left is irrelevant; move win-away onto
    // ws1 natively (user input): it leaves the roster and earns nothing.
    let lagged = accepted(
        &mut engine,
        &report(
            3,
            3,
            WorkspaceMode::PerOutputLocal,
            vec![output("out-1", vec!["ws1", "ws2"], "ws1")],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(0, 0, 800, 600),
                    vec![
                        win("win-keep", "out-1", "ws1", 0),
                        win("win-away", "out-1", "ws1", 210),
                    ],
                    "win-keep",
                ),
                domain("out-1", "ws2", rect(0, 0, 800, 600), Vec::new(), ""),
            ],
            vec![binding("l1", "ws1", "out-1")],
            "win-keep",
            None,
            None,
        ),
    );
    assert!(
        lagged.transfers.is_empty(),
        "user-moved outsiders are never pulled back"
    );
}

#[test]
fn policy_intents_are_unique_per_placement_cycle() {
    use tiler_core::world_report::{AttemptFeedback, AttemptKind, AttemptOutcome, TransferKind};
    // Three outputs: out-0 (x0) holds ws0/win-m; out-1 (x1920) is near;
    // out-2 (x3840) is the far active primary.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            vec![
                output_at(
                    "out-0",
                    vec!["ws0"],
                    "ws0",
                    Some(rect(0, 0, 1920, 1080)),
                    false,
                ),
                output_at(
                    "out-1",
                    vec!["ws1"],
                    "ws1",
                    Some(rect(1920, 0, 1920, 1080)),
                    false,
                ),
                output_at(
                    "out-2",
                    vec!["ws2"],
                    "ws2",
                    Some(rect(3840, 0, 1920, 1080)),
                    true,
                ),
            ],
            vec![
                domain(
                    "out-0",
                    "ws0",
                    rect(0, 0, 1920, 1080),
                    vec![win("win-m", "out-0", "ws0", 10)],
                    "win-m",
                ),
                domain(
                    "out-1",
                    "ws1",
                    rect(1920, 0, 1920, 1080),
                    vec![win("win-1", "out-1", "ws1", 1930)],
                    "win-1",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(3840, 0, 1920, 1080),
                    vec![win("win-2", "out-2", "ws2", 3850)],
                    "win-2",
                ),
            ],
            vec![
                binding("l0", "ws0", "out-0"),
                binding("l1", "ws1", "out-1"),
                binding("l2", "ws2", "out-2"),
            ],
            "win-m",
            None,
            None,
        ),
    );
    // Disconnect out-0 while win-m lags on the far survivor: the member
    // transfer pins its creating revision, and the displaced view does
    // too. Both stay stable across a repeated snapshot.
    let displaced = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![
                output_at(
                    "out-1",
                    vec!["ws0", "ws1"],
                    "ws1",
                    Some(rect(1920, 0, 1920, 1080)),
                    false,
                ),
                output_at(
                    "out-2",
                    vec!["ws0", "ws2"],
                    "ws2",
                    Some(rect(3840, 0, 1920, 1080)),
                    true,
                ),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(1920, 0, 1920, 1080),
                    vec![win("win-1", "out-1", "ws1", 1930)],
                    "win-1",
                ),
                domain(
                    "out-2",
                    "ws0",
                    rect(3840, 0, 1920, 1080),
                    vec![win("win-m", "out-2", "ws0", 3850)],
                    "win-m",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(3840, 0, 1920, 1080),
                    vec![win("win-2", "out-2", "ws2", 4100)],
                    "win-2",
                ),
            ],
            vec![binding("l1", "ws1", "out-1"), binding("l2", "ws2", "out-2")],
            "win-m",
            None,
            None,
        ),
    );
    assert_eq!(
        workspace_row(&displaced, "ws0").output.0,
        "out-1",
        "nearest survivor hosts the unit"
    );
    let member_op = displaced
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Window(_)))
        .expect("displacement member transfer")
        .operation_id
        .clone();
    assert!(member_op.starts_with("op-hp-"), "{member_op}");
    let view_op = displaced
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("displacement view transfer")
        .operation_id
        .clone();
    assert!(view_op.starts_with("op-dpv-"), "{view_op}");
    // Native arrival confirms both; the member transfer settles while a
    // repeated identical snapshot would have kept it byte-identical.
    let arrived = accepted(
        &mut engine,
        &report(
            3,
            3,
            WorkspaceMode::PerOutputLocal,
            vec![
                output_at(
                    "out-1",
                    vec!["ws0", "ws1"],
                    "ws0",
                    Some(rect(1920, 0, 1920, 1080)),
                    false,
                ),
                output_at(
                    "out-2",
                    vec!["ws0", "ws2"],
                    "ws2",
                    Some(rect(3840, 0, 1920, 1080)),
                    true,
                ),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(1920, 0, 1920, 1080),
                    vec![win("win-1", "out-1", "ws1", 1930)],
                    "win-1",
                ),
                domain(
                    "out-1",
                    "ws0",
                    rect(1920, 0, 1920, 1080),
                    vec![win("win-m", "out-1", "ws0", 1930)],
                    "win-m",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(3840, 0, 1920, 1080),
                    vec![win("win-2", "out-2", "ws2", 4100)],
                    "win-2",
                ),
            ],
            vec![binding("l1", "ws1", "out-1"), binding("l2", "ws2", "out-2")],
            "win-m",
            None,
            None,
        ),
    );
    assert!(
        arrived
            .transfers
            .iter()
            .all(|t| t.operation_id != member_op),
        "confirmed member intent settles"
    );
    // External move inside the same workspace: the user drags win-m back
    // to the far survivor natively. No intent is outstanding to fight.
    accepted(
        &mut engine,
        &report(
            4,
            4,
            WorkspaceMode::PerOutputLocal,
            vec![
                output_at(
                    "out-1",
                    vec!["ws0", "ws1"],
                    "ws1",
                    Some(rect(1920, 0, 1920, 1080)),
                    false,
                ),
                output_at(
                    "out-2",
                    vec!["ws0", "ws2"],
                    "ws2",
                    Some(rect(3840, 0, 1920, 1080)),
                    true,
                ),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(1920, 0, 1920, 1080),
                    vec![win("win-1", "out-1", "ws1", 1930)],
                    "win-1",
                ),
                domain(
                    "out-2",
                    "ws0",
                    rect(3840, 0, 1920, 1080),
                    vec![win("win-m", "out-2", "ws0", 3850)],
                    "win-m",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(3840, 0, 1920, 1080),
                    vec![win("win-2", "out-2", "ws2", 4100)],
                    "win-2",
                ),
            ],
            vec![binding("l1", "ws1", "out-1"), binding("l2", "ws2", "out-2")],
            "win-m",
            None,
            None,
        ),
    );
    // Origin returns: win-m is then-current roster on the far survivor,
    // so the return mints a NEW member intent plus a NEW view intent.
    // Both must differ from the settled displacement cycle.
    let returned = accepted(
        &mut engine,
        &report(
            5,
            5,
            WorkspaceMode::PerOutputLocal,
            vec![
                output_at(
                    "out-0",
                    vec!["ws0"],
                    "ws1",
                    Some(rect(0, 0, 1920, 1080)),
                    false,
                ),
                output_at(
                    "out-1",
                    vec!["ws0", "ws1"],
                    "ws1",
                    Some(rect(1920, 0, 1920, 1080)),
                    false,
                ),
                output_at(
                    "out-2",
                    vec!["ws0", "ws2"],
                    "ws2",
                    Some(rect(3840, 0, 1920, 1080)),
                    true,
                ),
            ],
            vec![
                domain(
                    "out-1",
                    "ws1",
                    rect(1920, 0, 1920, 1080),
                    vec![win("win-1", "out-1", "ws1", 1930)],
                    "win-1",
                ),
                domain(
                    "out-2",
                    "ws0",
                    rect(3840, 0, 1920, 1080),
                    vec![win("win-m", "out-2", "ws0", 3850)],
                    "win-m",
                ),
                domain(
                    "out-2",
                    "ws2",
                    rect(3840, 0, 1920, 1080),
                    vec![win("win-2", "out-2", "ws2", 4100)],
                    "win-2",
                ),
            ],
            vec![
                binding("l0", "ws0", "out-0"),
                binding("l1", "ws1", "out-1"),
                binding("l2", "ws2", "out-2"),
            ],
            "win-m",
            None,
            None,
        ),
    );
    assert_eq!(workspace_row(&returned, "ws0").output.0, "out-0");
    let member_op2 = returned
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Window(_)))
        .expect("return member transfer")
        .operation_id
        .clone();
    assert_ne!(
        member_op2, member_op,
        "return mints a distinct member intent"
    );
    let view_op2 = returned
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("return view transfer")
        .operation_id
        .clone();
    assert_ne!(view_op2, view_op, "return mints a distinct view intent");
    // Stale feedback for the SETTLED displacement cycle arrives late: it
    // must not settle the new return intents (the adapter confused same
    // op ids before per-cycle identities).
    let mut stale = report(
        6,
        6,
        WorkspaceMode::PerOutputLocal,
        vec![
            output_at(
                "out-0",
                vec!["ws0"],
                "ws1",
                Some(rect(0, 0, 1920, 1080)),
                false,
            ),
            output_at(
                "out-1",
                vec!["ws0", "ws1"],
                "ws1",
                Some(rect(1920, 0, 1920, 1080)),
                false,
            ),
            output_at(
                "out-2",
                vec!["ws0", "ws2"],
                "ws2",
                Some(rect(3840, 0, 1920, 1080)),
                true,
            ),
        ],
        vec![
            domain(
                "out-1",
                "ws1",
                rect(1920, 0, 1920, 1080),
                vec![win("win-1", "out-1", "ws1", 1930)],
                "win-1",
            ),
            domain(
                "out-2",
                "ws0",
                rect(3840, 0, 1920, 1080),
                vec![win("win-m", "out-2", "ws0", 3850)],
                "win-m",
            ),
            domain(
                "out-2",
                "ws2",
                rect(3840, 0, 1920, 1080),
                vec![win("win-2", "out-2", "ws2", 4100)],
                "win-2",
            ),
        ],
        vec![
            binding("l0", "ws0", "out-0"),
            binding("l1", "ws1", "out-1"),
            binding("l2", "ws2", "out-2"),
        ],
        "win-m",
        None,
        None,
    );
    stale.feedback = vec![
        AttemptFeedback {
            operation_id: member_op,
            kind: AttemptKind::Transfer,
            outcome: AttemptOutcome::SupersededExternal,
            hold: None,
        },
        AttemptFeedback {
            operation_id: view_op,
            kind: AttemptKind::View,
            outcome: AttemptOutcome::SupersededExternal,
            hold: None,
        },
    ];
    let held = accepted(&mut engine, &stale);
    assert!(
        held.transfers.iter().any(|t| t.operation_id == member_op2),
        "stale member feedback never settles the new target"
    );
    assert!(
        held.transfers.iter().any(|t| t.operation_id == view_op2),
        "stale view feedback never settles the new target"
    );
}
