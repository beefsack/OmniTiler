//! World-level directional/operation/tiling/displacement routing coverage.
//!
//! The stage 2a logical-core replacement removed the native-observing test
//! worlds (`world_directional_intents`, `world_engine_tests`,
//! `world_contract_repairs`, `world_default_tiling`, `world_displacement`).
//! The pure geometry selectors (`select_for_window` centre/span/left-top,
//! `select_for_migration` largest-edge) and the world pair routes survived
//! unchanged, but no world-level test proved the routing end to end. These
//! tests re-prove the accepted behaviors through the native-free v2Logical
//! contract: stacked screens plus window-frame facts only, core-minted
//! logical ids throughout.

use crate::boundary::CoreCommand;
use crate::directional::{OutputId, SameAxisMove, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    CommandResult, ScopeAuthority, ScreenReport, ScreenView, TransferKind, WORLD_CONTRACT_VERSION,
    WindowFact, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen_full(output: &str, full: Rect, primary: bool) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(full),
        work_area: Some(full),
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary,
    }
}

fn incomplete_screen(output: &str, primary: bool) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: None,
        work_area: None,
        gap: 0,
        outer_gap: 0,
        complete: false,
        primary,
    }
}

fn fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect: Some(rect(x, 0, 40, 90)),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
        minimized: false,
        observed_workspace: None,
    }
}

fn fact_rect(window: &str, output: &str, r: Rect) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect: Some(r),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
        minimized: false,
        observed_workspace: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    report_full(
        request_seq,
        observation_seq,
        screens,
        windows,
        focused,
        command_seq,
        command,
        crate::size_hints::FixedSizePredicate::BothAxes,
        true,
        Vec::new(),
    )
}

#[allow(clippy::too_many_arguments)]
fn report_full(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    predicate: crate::size_hints::FixedSizePredicate,
    default_tiled: bool,
    feedback: Vec<crate::world_report::AttemptFeedback>,
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
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens,
        windows,
        views: Vec::new(),
        closed: Vec::new(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: Vec::new(),
        fixed_size_predicate: predicate,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        default_tiled,
        focused_global: focused.to_owned(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback,
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn row_of<'a>(
    reply: &'a crate::world_report::WorldReply,
    window: &str,
) -> &'a crate::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing row {window}"))
}

fn output_view(reply: &crate::world_report::WorldReply, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

fn window_transfer(reply: &crate::world_report::WorldReply, window: &str) -> TransferKind {
    let transfers: Vec<_> = reply
        .transfers
        .iter()
        .filter(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == window))
        .collect();
    assert_eq!(
        transfers.len(),
        1,
        "exactly one window transfer for {window}: {:?}",
        reply.transfers
    );
    transfers[0].kind.clone()
}

/// Stacked three-output world: a wide source below, two outputs above.
/// The mover frame selects the upward target through the retained
/// centre/span/left-top ranking (never carried adjacency).
fn stacked_screens() -> Vec<ScreenReport> {
    vec![
        screen_full("src", rect(0, 1080, 1920, 1080), false),
        screen_full("u1", rect(0, 0, 960, 1080), false),
        screen_full("u2", rect(960, 0, 960, 1080), true),
    ]
}

fn move_up(window: &str) -> WorldCommand {
    WorldCommand::MoveDirectional {
        window: WindowId(window.to_owned()),
        direction: "up".to_owned(),
        same_axis: SameAxisMove::GroupWithNeighbor,
    }
}

fn send_up(window: &str, follow: bool) -> WorldCommand {
    WorldCommand::SendDirectional {
        window: WindowId(window.to_owned()),
        direction: "up".to_owned(),
        follow,
    }
}

fn bootstrap_stacked(mover: WindowFact) -> (WorldEngine, crate::world_report::WorldReply) {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    // The source holds only the mover: a sole root leaf exhausts (R4)
    // instead of restructuring locally (R1-R3).
    let first = accepted(
        &mut engine,
        &report(1, 1, stacked_screens(), vec![mover], "", None, None),
    );
    (engine, first)
}

#[test]
fn world_seam_prefers_upper_interval() {
    // Row 1: U1 [0,960), U2 [960,1920); mover x=860 w=200 centre=960.
    // Half-open containment puts the seam in the upper interval: U2.
    let mover = fact_rect("win-a", "src", rect(860, 1150, 200, 200));
    let (mut engine, _) = bootstrap_stacked(mover);
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            stacked_screens(),
            vec![fact_rect("win-a", "src", rect(860, 1150, 200, 200))],
            "win-a",
            Some(11),
            Some(move_up("win-a")),
        ),
    );
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(_)))
        .expect("cross-output move retains a window transfer");
    assert_eq!(transfer.target_output.0, "u2", "seam centre selects u2");
    assert_eq!(row_of(&reply, "win-a").output.0, "u2");
}

#[test]
fn world_overlap_fallback_selects_larger_span() {
    // No containing interval (mover centre in a true gap): the larger span
    // overlap wins. Uppers [0,400) and [1200,1600) over source [0,1600):
    // window [300,1400) centre=850 is in the gap, overlapping u2 by 200
    // vs u1 by 100: U2.
    let screens = vec![
        screen_full("src", rect(0, 1080, 1600, 1080), false),
        screen_full("u1", rect(0, 0, 400, 1080), false),
        screen_full("u2", rect(1200, 0, 400, 1080), true),
    ];
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![fact_rect("win-a", "src", rect(300, 1150, 1100, 200))],
            "",
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            screens,
            vec![fact_rect("win-a", "src", rect(300, 1150, 1100, 200))],
            "win-a",
            Some(11),
            Some(move_up("win-a")),
        ),
    );
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(_)))
        .expect("cross-output move retains a window transfer");
    assert_eq!(
        transfer.target_output.0, "u2",
        "larger span selects u2: {:?}",
        reply.transfers
    );
    assert_eq!(row_of(&reply, "win-a").output.0, "u2");
}

#[test]
fn world_both_centred_skips_overlap_to_left_top() {
    // Overlapping uppers [0,1200) and [400,1600); mover [900,1300)
    // centre=1100 sits in both, so overlap is skipped and left/top picks
    // U1 even though U2 overlaps the span more.
    let screens = vec![
        screen_full("src", rect(0, 1080, 1600, 1080), false),
        screen_full("u1", rect(0, 0, 1200, 1080), false),
        screen_full("u2", rect(400, 0, 1200, 1080), true),
    ];
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![fact_rect("win-a", "src", rect(900, 1150, 400, 100))],
            "",
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            screens,
            vec![fact_rect("win-a", "src", rect(900, 1150, 400, 100))],
            "win-a",
            Some(11),
            Some(move_up("win-a")),
        ),
    );
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(_)))
        .expect("cross-output move retains a window transfer");
    assert_eq!(
        transfer.target_output.0, "u1",
        "both-centred skips overlap to u1"
    );
    assert_eq!(row_of(&reply, "win-a").output.0, "u1");
}

#[test]
fn world_send_matches_move_targets() {
    // Directional sends rank identically to exhausted moves through the
    // retained windowed selector: the transfer names the selected output.
    for (mover, screens, expect) in [
        (
            fact_rect("win-a", "src", rect(860, 1150, 200, 200)),
            stacked_screens(),
            "u2",
        ),
        (
            fact_rect("win-a", "src", rect(900, 1150, 400, 100)),
            vec![
                screen_full("src", rect(0, 1080, 1600, 1080), false),
                screen_full("u1", rect(0, 0, 1200, 1080), false),
                screen_full("u2", rect(400, 0, 1200, 1080), true),
            ],
            "u1",
        ),
    ] {
        let mut engine = WorldEngine::new("gen-1").expect("valid generation");
        accepted(
            &mut engine,
            &report(1, 1, screens.clone(), vec![mover], "", None, None),
        );
        let reply = accepted(
            &mut engine,
            &report(
                2,
                2,
                screens,
                vec![if expect == "u2" {
                    fact_rect("win-a", "src", rect(860, 1150, 200, 200))
                } else {
                    fact_rect("win-a", "src", rect(900, 1150, 400, 100))
                }],
                "win-a",
                Some(11),
                Some(send_up("win-a", true)),
            ),
        );
        let transfer = reply
            .transfers
            .iter()
            .find(|t| matches!(&t.kind, TransferKind::Window(_)))
            .expect("directional send retains a window transfer");
        assert_eq!(transfer.target_output.0, expect, "send matches move");
        let _ = window_transfer(&reply, "win-a");
    }
}

#[test]
fn world_migration_selects_largest_edge_despite_window_centre() {
    // L (0,1080,1920,1080) below U1 (0,0,1200,1080, edge 1200) and U2
    // (1200,0,720,1080, edge 720). The focused window sits at x=1550
    // (projecting onto U2), but migration ignores window position: the
    // largest shared edge selects U1.
    let screens = vec![
        screen_full("out-l", rect(0, 1080, 1920, 1080), true),
        screen_full("u1", rect(0, 0, 1200, 1080), false),
        screen_full("u2", rect(1200, 0, 720, 1080), false),
    ];
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![
                fact_rect("win-a", "out-l", rect(100, 1150, 100, 100)),
                fact_rect("win-b", "out-l", rect(1550, 1150, 100, 100)),
                fact("win-u1", "u1", 10),
                fact("win-u2", "u2", 1210),
            ],
            "win-b",
            None,
            None,
        ),
    );
    let view_l = output_view(&first, "out-l").expect("view l");
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            screens,
            vec![
                fact_rect("win-a", "out-l", rect(100, 1150, 100, 100)),
                fact_rect("win-b", "out-l", rect(1550, 1150, 100, 100)),
                fact("win-u1", "u1", 10),
                fact("win-u2", "u2", 1210),
            ],
            "win-b",
            Some(21),
            Some(WorldCommand::MigrateDirectional {
                source_output: OutputId("out-l".to_owned()),
                workspace: view_l,
                direction: "up".to_owned(),
            }),
        ),
    );
    let migration = reply
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Migration))
        .expect("migration retains a migration transfer");
    assert_eq!(
        migration.target_output.0, "u1",
        "largest edge wins despite window centre: {:?}",
        reply.transfers
    );
    assert_eq!(migration.operation_id, "op-21", "stable command identity");
    let CommandResult::Migration { source, target, .. } =
        reply.command_result.clone().expect("migration provenance")
    else {
        panic!("expected migration result, got {:?}", reply.command_result);
    };
    assert_eq!(source.output.0, "out-l");
    assert_eq!(target.output.0, "u1");
}

#[test]
fn flight_fences_second_move_pre_engine_without_rerank() {
    // After an exhausted move retains its identity-pinned flight, a second
    // move for the same window refuses as transfer-in-flight pre-engine
    // (no re-rank, no second transfer, no sequence consumption, no
    // mutation). Covers both the lagging cross path and the gap-topology
    // local fallback path, which shares the same fence.
    let mover = fact_rect("win-a", "src", rect(860, 1150, 200, 200));
    let (mut engine, _) = bootstrap_stacked(mover);
    let cmd = report(
        2,
        2,
        stacked_screens(),
        vec![fact_rect("win-a", "src", rect(860, 1150, 200, 200))],
        "win-a",
        Some(112),
        Some(move_up("win-a")),
    );
    let reply = accepted(&mut engine, &cmd);
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(_)))
        .expect("exhausted move retains a window transfer");
    assert_eq!(transfer.target_output.0, "u2");
    assert_eq!(transfer.operation_id, "op-112");
    let settled_revision = engine.state_revision();
    // Lagging observation (mover still at the source) with a fresh move
    // command: pre-engine fence, never a re-rank.
    let mut again = report(
        3,
        3,
        stacked_screens(),
        vec![fact_rect("win-a", "src", rect(860, 1150, 200, 200))],
        "win-a",
        Some(113),
        Some(move_up("win-a")),
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
    let gap = vec![
        screen_full("src", rect(0, 2000, 800, 600), false),
        screen_full("u1", rect(0, 0, 800, 600), false),
        screen_full("u2", rect(3000, 0, 800, 600), true),
    ];
    again = report(
        4,
        4,
        gap,
        vec![fact_rect("win-a", "src", rect(10, 2050, 100, 100))],
        "win-a",
        Some(114),
        Some(move_up("win-a")),
    );
    match engine.handle_report(&again) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "transfer-in-flight"),
        other => panic!("gap-path move while flight outstanding refuses, got {other:?}"),
    }
    assert_eq!(
        engine.state_revision(),
        settled_revision,
        "gap-path refusal mutates nothing"
    );
    // The retained flight survives both refusals with its stable identity:
    // an observation-only lag report keeps the single pinned transfer.
    let lag = report(
        5,
        5,
        stacked_screens(),
        vec![fact_rect("win-a", "src", rect(860, 1150, 200, 200))],
        "win-a",
        None,
        None,
    );
    let lag_reply = accepted(&mut engine, &lag);
    let transfers: Vec<_> = lag_reply
        .transfers
        .iter()
        .filter(|t| matches!(&t.kind, TransferKind::Window(_)))
        .collect();
    assert_eq!(transfers.len(), 1, "single pinned flight survives");
    assert_eq!(transfers[0].operation_id, "op-112");
}

#[test]
fn exhausted_move_retains_identity_pinned_transfer_and_settles_on_arrival() {
    // Exhausted cross-output MoveDirectional retains one identity-pinned
    // window transfer (stable op id, target, follow); lag keeps the flight
    // with no replay; arrival proof settles it; a fresh reverse move
    // resolves against fresh topology without reranking the relocated
    // mover frame.
    let mover = fact_rect("win-a", "src", rect(860, 1150, 200, 200));
    let (mut engine, _) = bootstrap_stacked(mover);
    let cmd = report(
        2,
        2,
        stacked_screens(),
        vec![fact_rect("win-a", "src", rect(860, 1150, 200, 200))],
        "win-a",
        Some(112),
        Some(move_up("win-a")),
    );
    let reply = accepted(&mut engine, &cmd);
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(_)))
        .expect("exhausted move retains a window transfer")
        .clone();
    assert_eq!(transfer.operation_id, "op-112", "stable command identity");
    assert_eq!(transfer.target_output.0, "u2");
    assert!(transfer.follow, "move follows the mover to the target");
    assert!(
        reply.effects.is_empty(),
        "cross-output move carries no focus effect"
    );
    // Local move on a gap topology retains no window transfer.
    let gap = vec![
        screen_full("src", rect(0, 2000, 800, 600), false),
        screen_full("u1", rect(0, 0, 800, 600), false),
        screen_full("u2", rect(3000, 0, 800, 600), true),
    ];
    let mut gap_engine = WorldEngine::new("gen-1").expect("valid generation");
    accepted(
        &mut gap_engine,
        &report(
            1,
            1,
            gap.clone(),
            vec![
                fact_rect("win-a", "src", rect(10, 2050, 100, 100)),
                fact_rect("win-a2", "src", rect(300, 2050, 100, 100)),
            ],
            "",
            None,
            None,
        ),
    );
    let local = accepted(
        &mut gap_engine,
        &report(
            3,
            3,
            gap,
            vec![
                fact_rect("win-a", "src", rect(10, 2050, 100, 100)),
                fact_rect("win-a2", "src", rect(300, 2050, 100, 100)),
            ],
            "win-a",
            Some(153),
            Some(move_up("win-a")),
        ),
    );
    assert!(
        local
            .transfers
            .iter()
            .all(|t| !matches!(&t.kind, TransferKind::Window(_))),
        "local moves retain no window transfer"
    );
    // Actuation lag: the mover still observes at the source; the flight
    // persists with the desired homing pinned.
    let lag = report(
        3,
        3,
        stacked_screens(),
        vec![fact_rect("win-a", "src", rect(860, 1150, 200, 200))],
        "win-a",
        None,
        None,
    );
    let lag_reply = accepted(&mut engine, &lag);
    let lagged: Vec<_> = lag_reply
        .transfers
        .iter()
        .filter(|t| matches!(&t.kind, TransferKind::Window(_)))
        .collect();
    assert_eq!(lagged.len(), 1, "lag keeps the flight");
    assert_eq!(lagged[0].operation_id, "op-112");
    assert_eq!(row_of(&lag_reply, "win-a").output.0, "u2");
    // Arrival proof (observed at the target screen with the exact target
    // logical membership) settles the flight.
    let mut arrived = fact("win-a", "u2", 960);
    arrived.observed_workspace = Some(transfer.target_workspace.clone());
    let settled = accepted(
        &mut engine,
        &report(4, 4, stacked_screens(), vec![arrived], "win-a", None, None),
    );
    assert!(
        settled.transfers.is_empty(),
        "arrival proof settles: {:?}",
        settled.transfers
    );
    assert_eq!(row_of(&settled, "win-a").output.0, "u2");
    // A fresh reverse move resolves against fresh topology: the relocated
    // mover frame is never re-ranked into a stale target.
    let back = accepted(
        &mut engine,
        &report(
            5,
            5,
            stacked_screens(),
            vec![fact("win-a", "u2", 960)],
            "win-a",
            Some(115),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "down".to_owned(),
                same_axis: SameAxisMove::GroupWithNeighbor,
            }),
        ),
    );
    let reverse = back
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(_)))
        .expect("reverse move retains a window transfer");
    assert_eq!(reverse.target_output.0, "src");
}

#[test]
fn capacity_float_fallback_retains_identity_pinned_window_transfer() {
    // Tiny occupied readable target: the windowed selector still picks the
    // only touching output, but the affected projection exhausts, so the
    // shared capacity-float path carries the mover slotless with one
    // identity-pinned Window transfer (follow to the target). The mover
    // row homes at the target with no Controlled frame authority, so no
    // invalid geometry can actuate.
    let screens = vec![
        screen_full("out-a", rect(0, 0, 800, 600), true),
        screen_full("out-b", rect(800, 0, 1, 1), false),
    ];
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![
                fact("win-a", "out-a", 0),
                fact_rect("win-t", "out-b", rect(800, 0, 1, 1)),
            ],
            "win-a",
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            screens,
            vec![
                fact("win-a", "out-a", 0),
                fact_rect("win-t", "out-b", rect(800, 0, 1, 1)),
            ],
            "win-a",
            Some(171),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                same_axis: SameAxisMove::GroupWithNeighbor,
            }),
        ),
    );
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(_)))
        .expect("exhausted move floats with a pinned transfer")
        .clone();
    assert_eq!(transfer.operation_id, "op-171", "stable command identity");
    assert_eq!(transfer.target_output.0, "out-b");
    assert!(transfer.follow, "fallback always follows the mover");
    assert_eq!(row_of(&reply, "win-a").output.0, "out-b");
    let row = row_of(&reply, "win-a");
    assert_ne!(
        row.frame_authority,
        ScopeAuthority::Controlled,
        "slotless float carries no frame authority: {row:?}"
    );
    assert!(
        row.allocation.is_none(),
        "slotless float carries no allocation"
    );
}

fn two_output_world() -> (WorldEngine, Vec<ScreenReport>, String, Vec<String>) {
    let screens = vec![
        screen_full("out-a", rect(0, 0, 800, 600), true),
        screen_full("out-b", rect(800, 0, 800, 600), false),
    ];
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    (engine, screens, view_a, ring)
}

#[test]
fn drag_preview_same_domain_routes_and_carries_core_rect_and_hover() {
    // Read-only preview from the live pointer with no cross-output source:
    // the rect and hover come from the core plan (never the oracle), with
    // exact domain/source provenance, coexisting with the unchanged
    // snapshot ordering. Same-domain capture never refuses wholesale.
    let (mut engine, screens, _, _) = two_output_world();
    let preview = report(
        2,
        2,
        screens,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        Some(81),
        Some(WorldCommand::Operation(CoreCommand::DragPreview {
            window: "win-a".to_owned(),
            x: 155,
            y: 50,
            hover_prior: None,
            source: None,
        })),
    );
    let reply = match engine.handle_report(&preview) {
        WorldOutcome::Accepted(reply) => reply,
        WorldOutcome::Rejected { kind, .. } => {
            panic!("same-domain preview must run the legacy route, got {kind}")
        }
        other => panic!("unexpected outcome {other:?}"),
    };
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
    assert_eq!(domain.output.0, "out-a");
    assert_eq!(source, domain, "same-domain capture");
    assert_ne!(target.0, "win-a", "neighbor target, never the mover");
    assert_eq!(target.0, "win-b", "insertion neighbor identity");
    let hover = hover.expect("core hover prior always carried");
    assert_eq!(hover.prior_domain, domain);
    assert_eq!(hover.revision, revision);
    assert_eq!(row_of(&reply, "win-a").output.0, "out-a");
    assert_eq!(row_of(&reply, "win-b").output.0, "out-a");
    assert!(reply.transfers.is_empty());
    assert!(reply.effects.is_empty());
    let encoded = crate::world_report::encode_reply_canonical(&reply);
    assert!(encoded.contains("\"op\":\"drag-preview\""));
    assert!(encoded.contains("\"win\":\"win-b\""), "{encoded}");
}

#[test]
fn active_group_found_and_no_group() {
    let (mut engine, screens, _, _) = two_output_world();
    let query = report(
        2,
        2,
        screens,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        Some(83),
        Some(WorldCommand::Operation(CoreCommand::ActiveGroup)),
    );
    let reply = accepted(&mut engine, &query);
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
    let encoded = crate::world_report::encode_reply_canonical(&reply);
    assert!(encoded.contains("\"op\":\"active-group\""));
    // A lone root leaf has no parent group: explicit no-group with a
    // reason (the renderer clears), never an empty silent result.
    let mut solo = WorldEngine::new("gen-1").expect("valid generation");
    let solo_screens = vec![screen_full("out", rect(0, 0, 800, 600), true)];
    accepted(
        &mut solo,
        &report(
            1,
            1,
            solo_screens.clone(),
            vec![fact("win-a", "out", 0)],
            "",
            None,
            None,
        ),
    );
    let query = report(
        2,
        2,
        solo_screens,
        vec![fact("win-a", "out", 0)],
        "win-a",
        Some(84),
        Some(WorldCommand::Operation(CoreCommand::ActiveGroup)),
    );
    let reply = accepted(&mut solo, &query);
    assert!(
        matches!(reply.command_result, Some(CommandResult::NoGroup { .. })),
        "lone leaf reports no-group, got {:?}",
        reply.command_result
    );
}

#[test]
fn move_and_resize_ride_the_snapshot() {
    // A local move changes the snapshot geometry with no separate result:
    // the snapshot IS the result (never silently discarded). Admission
    // orders win-b left of win-a, so the left leaf moves right to swap.
    let (mut engine, screens, _, _) = two_output_world();
    let before_reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            screens.clone(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-b",
            None,
            None,
        ),
    );
    let before = row_of(&before_reply, "win-b").allocation;
    // Resize rides the snapshot with no separate result. win-b holds the
    // left leaf, so growing its shared right boundary is feasible.
    let resize = report(
        3,
        3,
        screens.clone(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-b",
        Some(85),
        Some(WorldCommand::Operation(CoreCommand::Resize {
            window: "win-b".to_owned(),
            direction: "right".to_owned(),
            mode: "outwards".to_owned(),
            press_index: 0,
        })),
    );
    let reply = accepted(&mut engine, &resize);
    assert!(reply.command_result.is_none());
    let step = report(
        4,
        4,
        screens,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-b",
        Some(86),
        Some(WorldCommand::Operation(CoreCommand::Move {
            window: "win-b".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: false,
            same_axis_move: SameAxisMove::GroupWithNeighbor,
        })),
    );
    let reply = accepted(&mut engine, &step);
    assert!(reply.command_result.is_none());
    assert!(
        reply.windows.iter().all(|w| w.allocation.is_some()),
        "local move keeps managed geometry"
    );
    assert_ne!(
        row_of(&reply, "win-b").allocation,
        before,
        "local move rewrites geometry in the snapshot"
    );
}

#[test]
fn drag_drop_pair_resolves_pointer_target() {
    // Two side-by-side outputs: the mover still homes the source (native
    // reassignment has not happened) while the pointer names the
    // destination. The world op resolves the pointer domain, never the
    // mover homing.
    let screens = vec![
        screen_full("out-a", rect(0, 0, 300, 100), true),
        screen_full("out-b", rect(800, 0, 300, 100), false),
    ];
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "win-a",
            None,
            None,
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    let drop = report(
        2,
        2,
        screens,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        "win-a",
        Some(87),
        Some(WorldCommand::Operation(CoreCommand::DragDrop {
            window: "win-a".to_owned(),
            x: 810,
            y: 50,
            hover_prior: None,
            source: Some(crate::session::DomainKey {
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId(view_a),
            }),
        })),
    );
    let reply = accepted(&mut engine, &drop);
    // Pair transaction committed: the mover arrives in the pointer domain
    // with a retained window transfer. The drop geometry rides the
    // snapshot, so no separate result.
    assert!(reply.command_result.is_none());
    assert!(
        matches!(
            reply.transfers.as_slice(),
            [t] if matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a")
                && t.target_output.0 == "out-b"
                && t.operation_id == "op-87"
        ),
        "pointer-domain transfer: {:?}",
        reply.transfers
    );
    assert_eq!(row_of(&reply, "win-a").output.0, "out-b");
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
fn operation_send_retains_and_foreign_routes_refuse() {
    let (mut engine, screens, view_a, ring) = two_output_world();
    // Read-only operation reconciles with no effect.
    let read = report(
        2,
        2,
        screens.clone(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        Some(31),
        Some(WorldCommand::Operation(CoreCommand::Reconcile)),
    );
    let reply = accepted(&mut engine, &read);
    assert!(reply.effects.is_empty());
    assert_eq!(row_of(&reply, "win-a").workspace, view_a);
    // Operation send retains exactly like an explicit send.
    assert_eq!(ring.len(), 2);
    let send = report(
        3,
        3,
        screens.clone(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        Some(32),
        Some(WorldCommand::Operation(CoreCommand::SendToWorkspace {
            window: "win-a".to_owned(),
            target_output: "out-a".to_owned(),
            target_workspace: ring[1].clone(),
            follow: true,
        })),
    );
    let reply = accepted(&mut engine, &send);
    assert_eq!(row_of(&reply, "win-a").workspace, ring[1]);
    // Same-output follow retains the window flight plus its own view
    // flight under a derived identity, exactly like an explicit send.
    assert_eq!(reply.transfers.len(), 2);
    assert!(matches!(&reply.transfers[0].kind, TransferKind::View));
    assert!(matches!(&reply.transfers[1].kind, TransferKind::Window(_)));
    assert_eq!(reply.transfers[0].target_workspace, ring[1]);
    assert_eq!(reply.transfers[1].target_workspace, ring[1]);
    assert_eq!(reply.transfers[0].operation_id, "op-32-follow");
    // Operation directional migration with a single live output has no
    // resolvable target and refuses without mutation, never silently
    // local. (Single output here; the strict map cannot resolve.)
    let mut solo = WorldEngine::new("gen-1").expect("valid generation");
    let solo_screens = vec![screen_full("out", rect(0, 0, 800, 600), true)];
    accepted(
        &mut solo,
        &report(
            1,
            1,
            solo_screens.clone(),
            vec![fact("win-a", "out", 0)],
            "",
            None,
            None,
        ),
    );
    let refused = report(
        2,
        2,
        solo_screens,
        vec![fact("win-a", "out", 0)],
        "win-a",
        Some(33),
        Some(WorldCommand::Operation(CoreCommand::MigrateWorkspace {
            direction: "right".to_owned(),
        })),
    );
    match solo.handle_report(&refused) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "operation-refused"),
        other => panic!("expected operation refusal, got {other:?}"),
    }
}

#[test]
fn float_send_authorizes_membership_and_guarded_sends_refuse() {
    let (mut engine, screens, _, ring) = two_output_world();
    assert_eq!(ring.len(), 2);
    let mut floating = fact("win-f", "out-a", 200);
    floating.floating = true;
    let mut game = fact("win-g", "out-a", 300);
    game.fit_excluded = true;
    let mut pin = fact("win-s", "out-a", 400);
    pin.sticky = true;
    // A tiled resident on the target workspace: the exception joins an
    // occupied scope, exactly like the pre-replacement float fixture.
    let mut resident = fact("win-t", "out-a", 500);
    resident.observed_workspace = Some(ring[1].clone());
    accepted(
        &mut engine,
        &report(
            2,
            2,
            screens.clone(),
            vec![
                fact("win-a", "out-a", 0),
                floating.clone(),
                game.clone(),
                pin.clone(),
                resident.clone(),
            ],
            "win-a",
            None,
            None,
        ),
    );
    // Excluded and sticky sends have no automatic permission. These
    // probes run with no outstanding flight, so the refusal vocabulary
    // is exact and nothing is retained.
    let mut seq = 3;
    for window in ["win-g", "win-s"] {
        let send = report(
            seq,
            3,
            screens.clone(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-f", "out-a", 200),
                resident.clone(),
                game.clone(),
                pin.clone(),
            ],
            window,
            Some(42),
            Some(WorldCommand::Send {
                window: WindowId(window.to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: ring[1].clone(),
                follow: true,
            }),
        );
        seq += 1;
        match engine.handle_report(&send) {
            WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-refused"),
            other => panic!("{window}: expected send-refused, got {other:?}"),
        }
    }
    assert!(
        engine.retained_transfers().is_empty(),
        "refused commands retain nothing"
    );
    // Float send authorizes membership without a frame write. A
    // follow send moves the focused window, so the mover is focused.
    let send = report(
        seq,
        4,
        screens.clone(),
        vec![
            fact("win-a", "out-a", 0),
            floating,
            resident.clone(),
            game.clone(),
            pin.clone(),
        ],
        "win-f",
        Some(41),
        Some(WorldCommand::Send {
            window: WindowId("win-f".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: ring[1].clone(),
            follow: true,
        }),
    );
    seq += 1;
    let reply = accepted(&mut engine, &send);
    // Follow retains the window flight plus its own view flight under a
    // derived identity, exactly like an explicit send.
    assert_eq!(reply.transfers.len(), 2);
    assert!(matches!(&reply.transfers[0].kind, TransferKind::View));
    assert!(matches!(&reply.transfers[1].kind, TransferKind::Window(_)));
    assert_eq!(reply.transfers[0].operation_id, "op-41-follow");
    let row = row_of(&reply, "win-f");
    assert_eq!(row.workspace, ring[1]);
    assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    assert!(row.allocation.is_none());
    let flight = engine.retained_transfers();
    assert!(
        matches!(
            flight.as_slice(),
            [v, t] if v.operation_id == "op-41-follow"
                && matches!(&v.kind, TransferKind::View)
                && t.operation_id == "op-41"
                && matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-f")
        ),
        "window plus follow-view flights retained under their identities: {flight:?}"
    );
    // Membership proof settles the window flight while the echoed view
    // confirms the follow-view flight; a fresh ordinary send then
    // still works.
    let mut arrived = fact("win-f", "out-a", 200);
    arrived.observed_workspace = Some(ring[1].clone());
    let mut settle_report = report(
        seq,
        5,
        screens.clone(),
        vec![
            fact("win-a", "out-a", 0),
            arrived,
            resident.clone(),
            game.clone(),
            pin.clone(),
        ],
        "win-a",
        None,
        None,
    );
    settle_report.views.push(ScreenView {
        output: OutputId("out-a".to_owned()),
        view: ring[1].clone(),
    });
    let settled = accepted(&mut engine, &settle_report);
    seq += 1;
    assert!(
        settled.transfers.is_empty(),
        "membership proof settles: {:?}",
        settled.transfers
    );
    let send = report(
        seq,
        5,
        screens.clone(),
        vec![
            fact("win-a", "out-a", 0),
            fact("win-f", "out-a", 200),
            resident,
            game,
            pin,
        ],
        "win-a",
        Some(43),
        Some(WorldCommand::Send {
            window: WindowId("win-a".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: ring[1].clone(),
            follow: true,
        }),
    );
    accepted(&mut engine, &send);
    assert_eq!(engine.retained_transfers().len(), 2);
}

#[test]
fn deterministic_sequence_replay_matches_exactly() {
    fn script(screens: &[ScreenReport], ring: &[String]) -> Vec<WorldReport> {
        let mut reports = vec![report(
            1,
            1,
            screens.to_vec(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        )];
        reports.push(report(
            2,
            2,
            screens.to_vec(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some(5),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: ring[1].clone(),
                follow: true,
            }),
        ));
        reports.push(report(
            3,
            2,
            screens.to_vec(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            None,
            None,
        ));
        let mut arrived = fact("win-a", "out-a", 0);
        arrived.observed_workspace = Some(ring[1].clone());
        reports.push(report(
            4,
            3,
            screens.to_vec(),
            vec![arrived, fact("win-b", "out-a", 100)],
            "win-b",
            None,
            None,
        ));
        reports.push(report(
            5,
            3,
            screens.to_vec(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-b",
            Some(7),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "left".to_owned(),
            }),
        ));
        reports
    }
    let (_engine, screens, _, ring) = two_output_world();
    assert_eq!(ring.len(), 2);
    let run = |reports: &[WorldReport]| {
        let mut engine = WorldEngine::new("gen-1").expect("valid generation");
        let mut replies = Vec::new();
        for report in reports {
            match engine.handle_report(report) {
                WorldOutcome::Accepted(reply)
                | WorldOutcome::CommandConsumed(reply)
                | WorldOutcome::Duplicate(reply) => replies.push(reply),
                WorldOutcome::Rejected { kind, message } => {
                    panic!("replay rejected: {kind} {message}")
                }
            }
        }
        (replies, engine.state_revision())
    };
    let reports = script(&screens, &ring);
    let (first, first_rev) = run(&reports);
    let (second, second_rev) = run(&reports);
    assert_eq!(first_rev, second_rev);
    assert_eq!(
        first
            .iter()
            .map(crate::world_report::encode_reply_canonical)
            .collect::<Vec<_>>(),
        second
            .iter()
            .map(crate::world_report::encode_reply_canonical)
            .collect::<Vec<_>>(),
        "deterministic replay matches exactly"
    );
}

#[test]
fn predicate_ingress_selects_admission_without_reclassification() {
    use crate::size_hints::{FixedSizePredicate, WindowSizeHints};
    fn width_fixed_report(predicate: FixedSizePredicate, seq: u64) -> WorldReport {
        let mut fixed = fact("win-f", "out-a", 200);
        // Width-only pin (height present but unequal): fixed under
        // either-axis, tiled under both-axes.
        fixed.hints = WindowSizeHints {
            min_w: Some(200),
            max_w: Some(200),
            min_h: Some(100),
            max_h: Some(150),
        };
        let screens = vec![
            screen_full("out-a", rect(0, 0, 800, 600), true),
            screen_full("out-b", rect(800, 0, 800, 600), false),
        ];
        report_full(
            seq,
            seq,
            screens,
            vec![fact("win-a", "out-a", 0), fixed],
            "",
            None,
            None,
            predicate,
            true,
            Vec::new(),
        )
    }
    // Either-axis ingress floats a width-only fixed window (membership
    // only, never a frame write).
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let reply = accepted(
        &mut engine,
        &width_fixed_report(FixedSizePredicate::EitherAxis, 1),
    );
    assert_eq!(
        engine.engine().fixed_size_predicate(),
        FixedSizePredicate::EitherAxis
    );
    let row = row_of(&reply, "win-f");
    assert!(row.allocation.is_none());
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    // Both-axes ingress tiles the very same observation.
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let reply = accepted(
        &mut engine,
        &width_fixed_report(FixedSizePredicate::BothAxes, 1),
    );
    let row = row_of(&reply, "win-f");
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    assert!(row.allocation.is_some());
    // Retained tiles never reclassify when the predicate later changes.
    let later = accepted(
        &mut engine,
        &width_fixed_report(FixedSizePredicate::EitherAxis, 2),
    );
    let row = row_of(&later, "win-f");
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
}

#[test]
fn hotplug_incomplete_survivor_holds_unit_and_full_report_recovers() {
    // Disconnect onto a survivor whose screen is incomplete: the geometry
    // fence holds the unit (exactly one homing, held frame, no
    // allocation, no duplication, no loss) instead of guessing from
    // work-area or window-frame proxies. The next FULL complete report
    // hosts the unit with controlled rows.
    let three = vec![
        screen_full("out-a", rect(0, 0, 800, 600), true),
        screen_full("out-b", rect(800, 0, 800, 600), false),
        screen_full("out-c", rect(1600, 0, 800, 600), false),
    ];
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            three,
            vec![fact("win-a", "out-c", 1600), fact("win-b", "out-c", 1700)],
            "",
            None,
            None,
        ),
    );
    let view_c = output_view(&first, "out-c").expect("view c");
    // out-c disconnects with no readable geometry anywhere: the primary
    // fallback hosts the unit (never a work-area or window-frame proxy),
    // members observed at the destination carry without actuation, and
    // topology-dependent crossing fences instead of guessing.
    let held = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                incomplete_screen("out-a", true),
                incomplete_screen("out-b", false),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    let units: Vec<_> = held
        .workspaces
        .iter()
        .filter(|row| row.id == view_c)
        .collect();
    assert_eq!(units.len(), 1, "unit preserved exactly once: {units:?}");
    assert_eq!(units[0].output.0, "out-a", "primary fallback hosts");
    let rows: Vec<_> = held
        .windows
        .iter()
        .filter(|row| row.lifetime.0 == "win-a" || row.lifetime.0 == "win-b")
        .collect();
    assert_eq!(rows.len(), 2, "exactly one homing per member: {rows:?}");
    for row in &rows {
        assert_eq!(row.workspace, view_c, "unit identity preserved: {row:?}");
        assert_eq!(row.output.0, "out-a", "{row:?}");
        assert!(
            row.allocation.is_none(),
            "held rows carry no allocation: {row:?}"
        );
    }
    assert!(
        held.transfers.is_empty(),
        "observed-at-destination carries without flights: {:?}",
        held.transfers
    );
    // Topology-dependent crossing fences on the unreadable world instead
    // of running silently local.
    let fenced = report(
        3,
        3,
        vec![
            incomplete_screen("out-a", true),
            incomplete_screen("out-b", false),
        ],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        Some(11),
        Some(move_up("win-a")),
    );
    match engine.handle_report(&fenced) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "operation-refused"),
        other => panic!("crossing without topology refuses, got {other:?}"),
    }
    // The next FULL complete report recovers controlled rows with no
    // duplication and no invented flights.
    let recovered = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                screen_full("out-a", rect(0, 0, 800, 600), true),
                screen_full("out-b", rect(800, 0, 800, 600), false),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    let units: Vec<_> = recovered
        .workspaces
        .iter()
        .filter(|row| row.id == view_c)
        .collect();
    assert_eq!(units.len(), 1, "still exactly one unit: {units:?}");
    assert_eq!(units[0].output.0, "out-a");
    let rows: Vec<_> = recovered
        .windows
        .iter()
        .filter(|row| row.lifetime.0 == "win-a" || row.lifetime.0 == "win-b")
        .collect();
    assert_eq!(rows.len(), 2, "still exactly one homing: {rows:?}");
    for row in &rows {
        assert_eq!(row.workspace, view_c, "{row:?}");
        assert_eq!(row.output.0, "out-a", "{row:?}");
        assert_eq!(row.frame_authority, ScopeAuthority::Controlled, "{row:?}");
        assert!(row.allocation.is_some(), "{row:?}");
    }
    assert!(
        recovered.transfers.is_empty(),
        "no invented flights: {:?}",
        recovered.transfers
    );
}

#[test]
fn hotplug_policy_member_transfer_is_unique_and_ignores_outsiders() {
    // Disconnect with one stray member still observed on a non-destination
    // screen: exactly one policy Window transfer names it (stable
    // identity, follow=false), repeated reports never duplicate it, and
    // windows outside the displaced roster (newcomers, sticky strays)
    // earn no transfer entries.
    let three = vec![
        screen_full("out-a", rect(0, 0, 800, 600), true),
        screen_full("out-b", rect(800, 0, 800, 600), false),
        screen_full("out-c", rect(1600, 0, 800, 600), false),
    ];
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            three,
            vec![
                fact("win-keep", "out-a", 0),
                fact("win-stray", "out-c", 1600),
                fact("win-home", "out-c", 1700),
            ],
            "",
            None,
            None,
        ),
    );
    let view_c = output_view(&first, "out-c").expect("view c");
    // out-c disconnects onto out-b (nearest); win-stray lags on out-a,
    // win-home already shows the survivor; a newcomer and a sticky stray
    // observe alongside.
    let stray = fact("win-stray", "out-a", 50);
    let _ = &view_c;
    let mut sticky = fact("win-pin", "out-a", 60);
    sticky.sticky = true;
    let displaced = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                screen_full("out-a", rect(0, 0, 800, 600), true),
                screen_full("out-b", rect(800, 0, 800, 600), false),
            ],
            vec![
                fact("win-keep", "out-a", 0),
                stray.clone(),
                fact("win-home", "out-b", 800),
                fact("win-new", "out-b", 900),
                sticky.clone(),
            ],
            "",
            None,
            None,
        ),
    );
    let unit_rows: Vec<_> = displaced
        .workspaces
        .iter()
        .filter(|row| row.id == view_c)
        .collect();
    assert_eq!(
        unit_rows.len(),
        1,
        "unit hosted exactly once: {unit_rows:?}"
    );
    assert_eq!(unit_rows[0].output.0, "out-b", "nearest hosts the unit");
    let mut policy: Vec<_> = displaced
        .transfers
        .iter()
        .filter(|t| matches!(&t.kind, TransferKind::Window(_)))
        .collect();
    policy.sort_by(|a, b| a.operation_id.cmp(&b.operation_id));
    let stray_policy: Vec<_> = policy
        .iter()
        .filter(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-stray"))
        .collect();
    assert_eq!(
        stray_policy.len(),
        1,
        "one policy intent for the stray: {:?}",
        displaced.transfers
    );
    assert!(
        stray_policy[0].operation_id.starts_with("op-hp-"),
        "policy identity: {:?}",
        stray_policy[0]
    );
    assert!(!stray_policy[0].follow, "policy carries membership only");
    assert!(
        policy
            .iter()
            .all(|t| !matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-new" || w.0 == "win-pin" || w.0 == "win-home" || w.0 == "win-keep")),
        "outsiders earn no policy transfer: {:?}",
        displaced.transfers
    );
    // Repeated reports never duplicate the intent: the same stable
    // identity persists.
    let again = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                screen_full("out-a", rect(0, 0, 800, 600), true),
                screen_full("out-b", rect(800, 0, 800, 600), false),
            ],
            vec![
                fact("win-keep", "out-a", 0),
                stray,
                fact("win-home", "out-b", 800),
                fact("win-new", "out-b", 900),
                sticky,
            ],
            "",
            None,
            None,
        ),
    );
    let policy: Vec<_> = again
        .transfers
        .iter()
        .filter(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-stray"))
        .collect();
    assert_eq!(
        policy.len(),
        1,
        "policy intent stays unique: {:?}",
        again.transfers
    );
    assert_eq!(policy[0].operation_id, stray_policy[0].operation_id);
}

#[test]
fn unknown_outputs_and_views_hold_fail_closed() {
    // Unknown focus targets refuse fail-closed with no mutation; unknown
    // view echoes are ignored (adapter truth never invents a workspace)
    // while the known world keeps converging.
    let (mut engine, screens, _, _) = two_output_world();
    let focus_ghost = report(
        2,
        2,
        screens.clone(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        Some(91),
        Some(WorldCommand::Focus {
            window: WindowId("win-ghost".to_owned()),
            direction: "left".to_owned(),
        }),
    );
    match engine.handle_report(&focus_ghost) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "focus-unknown"),
        other => panic!("unknown focus refuses fail-closed, got {other:?}"),
    }
    // Unknown view echoes never authorize homing or views.
    let mut echoed = report(
        3,
        3,
        screens.clone(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "",
        None,
        None,
    );
    echoed.views = vec![crate::world_report::ScreenView {
        output: OutputId("out-a".to_owned()),
        view: "ws-ghost".to_owned(),
    }];
    let calm = accepted(&mut engine, &echoed);
    assert!(
        calm.outputs
            .iter()
            .all(|row| row.logical_view.as_deref() != Some("ws-ghost")),
        "unknown echo invents nothing: {:?}",
        calm.outputs
    );
    assert_eq!(row_of(&calm, "win-a").output.0, "out-a");
    assert!(calm.transfers.is_empty());
}
