//! Floating-workspace boundary sends (D2): sends touching exactly one
//! floating workspace transfer project membership with no two-domain
//! Engine plan while the tiled side reconciles; floating-side frames stay
//! untouched. All four tiled/floating endpoint combinations plus sticky
//! refusal, real-flag D16/combo retention, and follow/stay carriage go
//! through full reports on the production transition.

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    DomainReport, OutputTopology, ScopeAuthority, TilingMode, WORLD_CONTRACT_VERSION, WorldCommand,
    WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn win(id: &str, ws: &str, x: i32) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId("out".to_owned()),
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

/// Real adapter flags for an intentional float, exactly as world-observe
/// emits them (floating implies fit-excluded).
fn float_win(id: &str, ws: &str, x: i32) -> EngineWindow {
    let mut w = win(id, ws, x);
    w.floating = true;
    w.fit_excluded = true;
    w
}

fn domain(ws: &str, windows: Vec<EngineWindow>, focused: &str) -> DomainReport {
    DomainReport {
        domain: OutputDomain {
            id: OutputId("out".to_owned()),
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

fn report(
    request_seq: u64,
    observation_seq: u64,
    domains: Vec<DomainReport>,
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
        workspace_mode: WorkspaceMode::PerOutputLocal,
        topology: vec![OutputTopology {
            output: OutputId("out".to_owned()),
            native_order: vec!["ws1".to_owned(), "ws2".to_owned()],
            current_native: "ws1".to_owned(),
            complete: true,
            geometry: None,
            primary: false,
        }],
        bindings: Vec::new(),
        domains,
        minimized: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        default_tiled: true,
        focused_global: String::new(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: Vec::new(),
    }
}

fn send_command(
    seq: u64,
    window: &str,
    target_ws: &str,
    follow: bool,
) -> (Option<u64>, Option<WorldCommand>) {
    (
        Some(seq),
        Some(WorldCommand::Send {
            window: WindowId(window.to_owned()),
            target_output: OutputId("out".to_owned()),
            target_workspace: target_ws.to_owned(),
            follow,
        }),
    )
}

fn tiling_command(seq: u64, ws: &str, tiled: bool) -> (Option<u64>, Option<WorldCommand>) {
    (
        Some(seq),
        Some(WorldCommand::SetWorkspaceTiling {
            output: OutputId("out".to_owned()),
            workspace: ws.to_owned(),
            tiled,
        }),
    )
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

fn row<'a>(
    reply: &'a tiler_core::world_report::WorldReply,
    id: &str,
) -> &'a tiler_core::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == id)
        .unwrap_or_else(|| panic!("row for {id}"))
}

fn assert_no_floating_frames(reply: &tiler_core::world_report::WorldReply, ws: &str) {
    for w in reply.windows.iter().filter(|w| w.workspace == ws) {
        assert_eq!(
            w.frame_authority,
            ScopeAuthority::Untouched,
            "{} keeps its floating frame",
            w.lifetime.0
        );
        assert!(
            w.allocation.is_none(),
            "{} carries no tiled-side geometry",
            w.lifetime.0
        );
    }
}

/// Float ws2 through the planner-owned toggle; ws1 stays tiled.
fn engine_with_ws2_floating() -> WorldEngine {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                domain("ws1", vec![win("win-a", "ws1", 0)], "win-a"),
                domain("ws2", vec![win("win-b", "ws2", 0)], "win-b"),
            ],
            None,
            None,
        ),
    );
    let (seq, cmd) = tiling_command(1, "ws2", false);
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                domain("ws1", vec![win("win-a", "ws1", 0)], "win-a"),
                domain("ws2", vec![win("win-b", "ws2", 0)], "win-b"),
            ],
            seq,
            cmd,
        ),
    );
    assert_eq!(
        reply
            .workspaces
            .iter()
            .find(|w| w.id == "ws2")
            .expect("ws2 row")
            .tiling_mode,
        TilingMode::Floating
    );
    engine
}

#[test]
fn tiled_to_tiled_keeps_engine_plan() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                domain("ws1", vec![win("win-a", "ws1", 0)], "win-a"),
                domain("ws2", vec![win("win-b", "ws2", 0)], "win-b"),
            ],
            None,
            None,
        ),
    );
    let (seq, cmd) = send_command(5, "win-a", "ws2", true);
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                domain("ws1", vec![win("win-a", "ws1", 0)], "win-a"),
                domain("ws2", vec![win("win-b", "ws2", 0)], "win-b"),
            ],
            seq,
            cmd,
        ),
    );
    assert_eq!(reply.transfers.len(), 1);
    assert!(reply.transfers[0].follow);
    let moved = row(&reply, "win-a");
    assert_eq!(format!("{}/{}", moved.output.0, moved.workspace), "out/ws2");
}

#[test]
fn floating_to_tiled_transfers_membership_with_tiled_reconcile() {
    let mut engine = engine_with_ws2_floating();
    // Ordinary mover out of the floating workspace into tiled ws1.
    let (seq, cmd) = send_command(5, "win-b", "ws1", true);
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                domain("ws1", vec![win("win-a", "ws1", 0)], "win-a"),
                domain("ws2", vec![win("win-b", "ws2", 0)], "win-b"),
            ],
            seq,
            cmd,
        ),
    );
    assert_eq!(reply.transfers.len(), 1);
    assert_eq!(reply.transfers[0].operation_id, "op-5");
    assert!(reply.transfers[0].follow);
    // Membership-only at the target on the first reply; the floating
    // side writes no frames.
    let moved = row(&reply, "win-b");
    assert_eq!(format!("{}/{}", moved.output.0, moved.workspace), "out/ws1");
    assert_eq!(moved.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(moved.frame_authority, ScopeAuthority::Untouched);
    assert_no_floating_frames(&reply, "ws2");
    // Actuation lag: the tiled side reconciles the shielded mover while
    // native membership still shows the source.
    let lagged = accepted(
        &mut engine,
        &report(
            4,
            3,
            vec![
                domain("ws1", vec![win("win-a", "ws1", 0)], "win-a"),
                domain("ws2", vec![win("win-b", "ws2", 0)], "win-b"),
            ],
            None,
            None,
        ),
    );
    assert_eq!(lagged.transfers.len(), 1);
    // Native arrival confirms: the transfer settles.
    let settled = accepted(
        &mut engine,
        &report(
            5,
            4,
            vec![
                domain(
                    "ws1",
                    vec![win("win-a", "ws1", 0), win("win-b", "ws1", 210)],
                    "win-b",
                ),
                domain("ws2", Vec::new(), ""),
            ],
            None,
            None,
        ),
    );
    assert!(settled.transfers.is_empty());
}

#[test]
fn tiled_to_floating_transfers_membership_without_float_geometry() {
    let mut engine = engine_with_ws2_floating();
    // Ordinary mover from tiled ws1 into floating ws2, send-and-stay.
    let (seq, cmd) = send_command(5, "win-a", "ws2", false);
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                domain("ws1", vec![win("win-a", "ws1", 0)], "win-a"),
                domain("ws2", vec![win("win-b", "ws2", 0)], "win-b"),
            ],
            seq,
            cmd,
        ),
    );
    assert_eq!(reply.transfers.len(), 1);
    assert!(!reply.transfers[0].follow);
    // One membership-only row at the floating target; the source sheds
    // the shielded mover while remaining members keep tiling.
    let moved = row(&reply, "win-a");
    assert_eq!(format!("{}/{}", moved.output.0, moved.workspace), "out/ws2");
    assert_eq!(moved.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(moved.frame_authority, ScopeAuthority::Untouched);
    assert!(moved.allocation.is_none());
    assert_no_floating_frames(&reply, "ws2");
    // Native arrival confirms without any tiled-side geometry on floats.
    let settled = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                domain("ws1", Vec::new(), ""),
                domain(
                    "ws2",
                    vec![win("win-b", "ws2", 0), win("win-a", "ws2", 210)],
                    "win-b",
                ),
            ],
            None,
            None,
        ),
    );
    assert!(settled.transfers.is_empty());
    assert_no_floating_frames(&settled, "ws2");
}

#[test]
fn floating_to_floating_sends_membership_follow_and_stay() {
    // Legacy native-only routing sends when either side is unmanaged
    // (`!sourceTiled || !targetTiled`), including both: membership-only
    // retained transfers with no Engine plan and no frame writes.
    for follow in [true, false] {
        let mut engine = engine_with_ws2_floating();
        // Float ws1 as well: both endpoints floating.
        let (tseq, tcmd) = tiling_command(2, "ws1", false);
        accepted(
            &mut engine,
            &report(
                3,
                3,
                vec![
                    domain("ws1", vec![win("win-a", "ws1", 0)], "win-a"),
                    domain("ws2", vec![win("win-b", "ws2", 0)], "win-b"),
                ],
                tseq,
                tcmd,
            ),
        );
        let (seq, cmd) = send_command(6, "win-a", "ws2", follow);
        let reply = accepted(
            &mut engine,
            &report(
                4,
                4,
                vec![
                    domain("ws1", vec![win("win-a", "ws1", 0)], "win-a"),
                    domain("ws2", vec![win("win-b", "ws2", 0)], "win-b"),
                ],
                seq,
                cmd,
            ),
        );
        assert_eq!(reply.transfers.len(), 1);
        assert_eq!(reply.transfers[0].follow, follow);
        let moved = row(&reply, "win-a");
        assert_eq!(format!("{}/{}", moved.output.0, moved.workspace), "out/ws2");
        assert_eq!(moved.membership_authority, ScopeAuthority::Controlled);
        assert_eq!(moved.frame_authority, ScopeAuthority::Untouched);
        assert!(moved.allocation.is_none());
        assert_no_floating_frames(&reply, "ws1");
        assert_no_floating_frames(&reply, "ws2");
        // Native arrival confirms without any frame ever written.
        let settled = accepted(
            &mut engine,
            &report(
                5,
                5,
                vec![
                    domain("ws1", Vec::new(), ""),
                    domain(
                        "ws2",
                        vec![win("win-b", "ws2", 0), win("win-a", "ws2", 210)],
                        "win-b",
                    ),
                ],
                None,
                None,
            ),
        );
        assert!(settled.transfers.is_empty());
        assert_no_floating_frames(&settled, "ws2");
    }
}

#[test]
fn sticky_movers_refuse_on_the_boundary() {
    let mut engine = engine_with_ws2_floating();
    let mut sticky = win("win-s", "ws1", 100);
    sticky.sticky = true;
    sticky.floating = true;
    sticky.fit_excluded = true;
    let (seq, cmd) = send_command(5, "win-s", "ws2", true);
    match engine.handle_report(&report(
        3,
        3,
        vec![
            domain("ws1", vec![win("win-a", "ws1", 0), sticky], "win-a"),
            domain("ws2", vec![win("win-b", "ws2", 0)], "win-b"),
        ],
        seq,
        cmd,
    )) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-refused"),
        other => panic!("expected send-refused, got {other:?}"),
    }
    assert!(engine.retained_transfers().is_empty());
}

#[test]
fn real_flag_float_sends_retain_membership_without_frame() {
    // World-observe always pairs floating with fit-excluded: the engine
    // decides admission through the deferred slotless carry.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                domain(
                    "ws1",
                    vec![win("win-a", "ws1", 0), float_win("win-f", "ws1", 210)],
                    "win-f",
                ),
                domain("ws2", vec![win("win-b", "ws2", 0)], "win-b"),
            ],
            None,
            None,
        ),
    );
    let (seq, cmd) = send_command(5, "win-f", "ws2", true);
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                domain(
                    "ws1",
                    vec![win("win-a", "ws1", 0), float_win("win-f", "ws1", 210)],
                    "win-f",
                ),
                domain("ws2", vec![win("win-b", "ws2", 0)], "win-b"),
            ],
            seq,
            cmd,
        ),
    );
    assert_eq!(reply.transfers.len(), 1);
    let moved = row(&reply, "win-f");
    assert_eq!(format!("{}/{}", moved.output.0, moved.workspace), "out/ws2");
    assert_eq!(moved.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(moved.frame_authority, ScopeAuthority::Untouched);
    assert!(moved.allocation.is_none());
}

#[test]
fn float_fullscreen_combo_rides_the_boundary_untouched() {
    let mut engine = engine_with_ws2_floating();
    // Floating+fullscreen mover from tiled ws1 into floating ws2: the
    // combo carries exactly as-is with no geometry or state writes.
    let mut combo = win("win-c", "ws1", 100);
    combo.floating = true;
    combo.fit_excluded = true;
    combo.fullscreen = true;
    let (seq, cmd) = send_command(5, "win-c", "ws2", true);
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                domain("ws1", vec![win("win-a", "ws1", 0), combo.clone()], "win-a"),
                domain("ws2", vec![win("win-b", "ws2", 0)], "win-b"),
            ],
            seq,
            cmd,
        ),
    );
    assert_eq!(reply.transfers.len(), 1);
    // Float movers keep the D16 exception row at the observed homing
    // with membership authority riding the transfer (frame untouched);
    // the transfer target carries the membership-only move.
    let moved = row(&reply, "win-c");
    assert_eq!(format!("{}/{}", moved.output.0, moved.workspace), "out/ws1");
    assert_eq!(moved.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(moved.frame_authority, ScopeAuthority::Untouched);
    assert!(moved.allocation.is_none());
    assert_no_floating_frames(&reply, "ws2");
    // Actuation lag: the source sheds the shielded mover and the target
    // homing carries the membership-only row.
    let lagged = accepted(
        &mut engine,
        &report(
            4,
            3,
            vec![
                domain("ws1", vec![win("win-a", "ws1", 0), combo.clone()], "win-a"),
                domain("ws2", vec![win("win-b", "ws2", 0)], "win-b"),
            ],
            None,
            None,
        ),
    );
    assert_eq!(lagged.transfers.len(), 1);
    let lagged_moved = row(&lagged, "win-c");
    assert_eq!(
        format!("{}/{}", lagged_moved.output.0, lagged_moved.workspace),
        "out/ws2"
    );
    assert_eq!(
        lagged_moved.membership_authority,
        ScopeAuthority::Controlled
    );
    assert_eq!(lagged_moved.frame_authority, ScopeAuthority::Untouched);
    // Native arrival confirms with the combo intact and still no frames.
    let mut arrived = win("win-c", "ws2", 210);
    arrived.floating = true;
    arrived.fit_excluded = true;
    arrived.fullscreen = true;
    let settled = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![
                domain("ws1", vec![win("win-a", "ws1", 0)], "win-a"),
                domain("ws2", vec![win("win-b", "ws2", 0), arrived], "win-b"),
            ],
            None,
            None,
        ),
    );
    assert!(settled.transfers.is_empty());
    assert_no_floating_frames(&settled, "ws2");
}
