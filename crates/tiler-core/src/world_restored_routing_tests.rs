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
use crate::session::DomainKey;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    CommandRejectKind, CommandResult, FrameSetter, MemberAttempt, PresentationIngressV4,
    PresentationRowV4, ScopeAuthority, ScreenReport, TransferKind, WORLD_CONTRACT_VERSION_V4,
    WindowFact, WorkspaceRef, WorldCommand, WorldReportV4,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen_full(output: &str, full: Rect) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(full),
        work_area: Some(full),
        complete: true,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    }
}

fn incomplete_screen(output: &str) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: None,
        work_area: None,
        complete: false,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    }
}

fn fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId(output.to_owned())),
        rect: Some(rect(x, 0, 40, 90)),
        fullscreen: Some(false),
        maximized: Some(false),
        sticky: Some(false),
        hints: Some(WindowSizeHints::none()),
        minimized: Some(false),
        native_workspace: crate::world_report::NativeWorkspaceFact::Omitted,
        host_hidden: Some(false),
        privileged: Some(false),
        managed: Some(true),
        accepts_input: Some(true),
        activation_inhibited: Some(false),
        tool_window: Some(false),
        owned: Some(false),
        transient: Some(false),
        modal: Some(false),
        resizable: Some(true),
        above: Some(false),
        below: Some(false),
        owner: None,
        parent: None,
        role: None,
        app_id: None,
        executable: Some("synthetic.exe".to_owned()),
        process_id: Some("foreign-pid".to_owned()),
        class: Some("Synthetic".to_owned()),
        title: None,
        window_type: Some(crate::world_report::WindowType::Normal),
        activation_policy: Some(crate::world_report::ActivationPolicy::Regular),
        layer: Some(0),
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: None,
        visibility: crate::world_report::VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
        last_write: None,
    }
}

fn fact_rect(window: &str, output: &str, r: Rect) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId(output.to_owned())),
        rect: Some(r),
        fullscreen: Some(false),
        maximized: Some(false),
        sticky: Some(false),
        hints: Some(WindowSizeHints::none()),
        minimized: Some(false),
        native_workspace: crate::world_report::NativeWorkspaceFact::Omitted,
        host_hidden: Some(false),
        privileged: Some(false),
        managed: Some(true),
        accepts_input: Some(true),
        activation_inhibited: Some(false),
        tool_window: Some(false),
        owned: Some(false),
        transient: Some(false),
        modal: Some(false),
        resizable: Some(true),
        above: Some(false),
        below: Some(false),
        owner: None,
        parent: None,
        role: None,
        app_id: None,
        executable: Some("synthetic.exe".to_owned()),
        process_id: Some("foreign-pid".to_owned()),
        class: Some("Synthetic".to_owned()),
        title: None,
        window_type: Some(crate::world_report::WindowType::Normal),
        activation_policy: Some(crate::world_report::ActivationPolicy::Regular),
        layer: Some(0),
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: None,
        visibility: crate::world_report::VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
        last_write: None,
    }
}

fn report(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReportV4 {
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
    _predicate: crate::size_hints::FixedSizePredicate,
    _default_tiled: bool,
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        screens,
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: Vec::new(),
        native: Vec::new(),
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        foreground_known: !focused.is_empty(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> crate::world_report::WorldReplyV4 {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn row_of<'a>(
    reply: &'a crate::world_report::WorldReplyV4,
    window: &str,
) -> &'a crate::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing row {window}"))
}

fn output_view(reply: &crate::world_report::WorldReplyV4, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

fn window_transfer(reply: &crate::world_report::WorldReplyV4, window: &str) -> TransferKind {
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
        screen_full("src", rect(0, 1080, 1920, 1080)),
        screen_full("u1", rect(0, 0, 960, 1080)),
        screen_full("u2", rect(960, 0, 960, 1080)),
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

fn bootstrap_stacked(mover: WindowFact) -> (WorldEngine, crate::world_report::WorldReplyV4) {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
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
        screen_full("src", rect(0, 1080, 1600, 1080)),
        screen_full("u1", rect(0, 0, 400, 1080)),
        screen_full("u2", rect(1200, 0, 400, 1080)),
    ];
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
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
        screen_full("src", rect(0, 1080, 1600, 1080)),
        screen_full("u1", rect(0, 0, 1200, 1080)),
        screen_full("u2", rect(400, 0, 1200, 1080)),
    ];
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
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
                screen_full("src", rect(0, 1080, 1600, 1080)),
                screen_full("u1", rect(0, 0, 1200, 1080)),
                screen_full("u2", rect(400, 0, 1200, 1080)),
            ],
            "u1",
        ),
    ] {
        let mut engine =
            WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
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
        screen_full("out-l", rect(0, 1080, 1920, 1080)),
        screen_full("u1", rect(0, 0, 1200, 1080)),
        screen_full("u2", rect(1200, 0, 720, 1080)),
    ];
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
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
    // Direction-only command: the staged out-l foreground resolves the
    // source with its current view; the largest shared edge still selects
    // U1 over the window-centre projection.
    let mut cmd_report = report(
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
            direction: "up".to_owned(),
        }),
    );
    cmd_report.action_foreground = Some(OutputId("out-l".to_owned()));
    cmd_report.action_outputs = vec![
        OutputId("out-l".to_owned()),
        OutputId("u1".to_owned()),
        OutputId("u2".to_owned()),
    ];
    let reply = accepted(&mut engine, &cmd_report);
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
    // (no re-rank, no second transfer, no mutation; the refused sequence
    // still consumes). Covers both the lagging cross path and the gap-topology
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::TransferInFlight
            }),
            "second move while flight outstanding refuses"
        ),
        other => panic!("second move while flight outstanding refuses, got {other:?}"),
    }
    assert_eq!(
        engine.state_revision(),
        settled_revision + 1,
        "pre-engine refusal issues"
    );
    // Gap topology forcing the no-candidate local fallback under the same
    // lagging window: the same fence fires before any local mutation.
    let gap = vec![
        screen_full("src", rect(0, 2000, 800, 600)),
        screen_full("u1", rect(0, 0, 800, 600)),
        screen_full("u2", rect(3000, 0, 800, 600)),
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::TransferInFlight
            }),
            "gap-path move while flight outstanding refuses"
        ),
        other => panic!("gap-path move while flight outstanding refuses, got {other:?}"),
    }
    assert_eq!(
        engine.state_revision(),
        settled_revision + 2,
        "gap-path refusal issues"
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
    // The tiled directional move keeps focus on the mover through one
    // spent core effect (the old adapter follow-focus lane, now
    // core-owned at-most-once): exactly one, naming the mover.
    let focus_effects: Vec<_> = reply
        .effects
        .iter()
        .filter(|effect| effect.kind == crate::world_report::EffectKind::Focus)
        .collect();
    assert_eq!(
        focus_effects.len(),
        1,
        "tiled move carries one follow-focus effect, got {:?}",
        reply.effects
    );
    assert_eq!(
        focus_effects[0].target,
        crate::directional::WindowId("win-a".to_owned())
    );
    // Local move on a gap topology retains no window transfer.
    let gap = vec![
        screen_full("src", rect(0, 2000, 800, 600)),
        screen_full("u1", rect(0, 0, 800, 600)),
        screen_full("u2", rect(3000, 0, 800, 600)),
    ];
    let mut gap_engine =
        WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
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
    // logical membership) settles the cross-output follow parent. The
    // legacy setup also sent the exact follow-view echo, but echoes are
    // inert (the staged echo derives solely from the native current map,
    // never from this lane), so the v4 port observes the same arrival
    // with an empty lane.
    let arrived = fact("win-a", "u2", 960);
    let arrival_report = report(4, 4, stacked_screens(), vec![arrived], "win-a", None, None);
    let settled = accepted(&mut engine, &arrival_report);
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
        screen_full("out-a", rect(0, 0, 800, 600)),
        screen_full("out-b", rect(800, 0, 1, 1)),
    ];
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            // Fixtures carry native sticky and maximize facts; the lane
            // under test is the native route.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![(
            "outer".to_owned(),
            crate::connection::RawSettingValue::Number(0.0),
        )],
    )
    .expect("valid generation");
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
        screen_full("out-a", rect(0, 0, 800, 600)),
        screen_full("out-b", rect(800, 0, 800, 600)),
    ];
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
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
    // No core v4 canonical writer exists; the typed assertions above
    // already pin the drag-preview op and the win-b neighbor identity,
    // and the exact-byte delta below proves the result rides the reply
    // bytes (protocol `encode_reply_v4` field arithmetic).
    use crate::world_sizing_v4::reply_exact_bytes_v4;
    let mut stripped = reply.clone();
    stripped.command_result = None;
    assert!(
        reply_exact_bytes_v4(&reply) > reply_exact_bytes_v4(&stripped),
        "exact bytes carry the drag-preview result"
    );
}

fn presentation_query(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    foreground: &str,
) -> WorldReportV4 {
    // v4 ingress carries only foreground/frame/chord/pager/witness lanes;
    // border/underlay styling lives in composed settings (contract 261-272,
    // 1221 removes the ingress sizes).
    let mut base = report(
        request_seq,
        observation_seq,
        screens,
        windows,
        focused,
        None,
        None,
    );
    base.presentation = PresentationRowV4::Valid(PresentationIngressV4 {
        foreground: foreground.to_owned(),
        foreground_minimized: Some(false),
        foreground_maximized: Some(false),
        foreground_fullscreen: Some(false),
        foreground_host_hidden: Some(false),
        foreground_hidden_workspace: Some(false),
        foreground_frame: Some(rect(0, 0, 40, 90)),
        chord_held: true,
        ..PresentationIngressV4::default()
    });
    base
}

#[test]
fn uniform_presentation_resolves_found_and_root_leaf() {
    use crate::world_presentation::PresentationUnderlay;
    let (mut engine, screens, _, _) = two_output_world();
    let members = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    // A second identical report latches managed focus behind the newcomer
    // before the presentation reads retained focus.
    accepted(
        &mut engine,
        &presentation_query(2, 2, screens.clone(), members.clone(), "win-a", "win-a"),
    );
    // Styled border/underlay sizes have no v4 ingress slot (contract 1221);
    // visible-border geometry ports via settings+witness in
    // world_presentation_v4_tests::genuine_descriptor_positive, and the
    // outer-fill exact twin is underlay_found_scaled.
    let reply = accepted(
        &mut engine,
        &presentation_query(3, 3, screens, members, "win-a", "win-a"),
    );
    let PresentationUnderlay::Found(found) = reply.presentation.underlay.clone() else {
        panic!(
            "expected found underlay, got {:?}",
            reply.presentation.underlay
        );
    };
    assert_eq!(found.focused_window.0, "win-a");
    assert_eq!(found.members.len(), 2);
    assert!(found.union_rect.w > 0 && found.union_rect.h > 0);
    assert!(found.outer_rect.w >= found.union_rect.w);
    assert_eq!(reply.presentation.foreground, "win-a");
    // A lone root leaf has no parent group: hidden with a reason (the
    // renderer clears), never an empty silent result.
    let mut solo = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let solo_screens = vec![screen_full("out", rect(0, 0, 800, 600))];
    accepted(
        &mut solo,
        &presentation_query(
            1,
            1,
            solo_screens.clone(),
            vec![fact("win-a", "out", 0)],
            "",
            "win-a",
        ),
    );
    accepted(
        &mut solo,
        &presentation_query(
            2,
            2,
            solo_screens.clone(),
            vec![fact("win-a", "out", 0)],
            "win-a",
            "win-a",
        ),
    );
    let reply = accepted(
        &mut solo,
        &presentation_query(
            3,
            3,
            solo_screens,
            vec![fact("win-a", "out", 0)],
            "win-a",
            "win-a",
        ),
    );
    assert!(
        matches!(
            reply.presentation.underlay,
            PresentationUnderlay::Hidden { .. }
        ),
        "lone leaf hides, got {:?}",
        reply.presentation.underlay
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
        screen_full("out-a", rect(0, 0, 300, 100)),
        screen_full("out-b", rect(800, 0, 300, 100)),
    ];
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
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
    // Same-output follow retains the single window flight under the stable
    // command identity: no `{op}-follow` child, no view flight. These
    // fixture facts carry no suppression support, so the mover is
    // selection-ineligible and keeps the legacy direct focus with no
    // frozen binding.
    assert_eq!(reply.transfers.len(), 1);
    assert!(matches!(&reply.transfers[0].kind, TransferKind::Window(_)));
    assert_eq!(reply.transfers[0].target_workspace, ring[1]);
    assert_eq!(reply.transfers[0].operation_id, "op-32");
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "ineligible mover binds nothing"
    );
    assert_eq!(
        output_view(&reply, "out-a").as_deref(),
        Some(ring[1].as_str()),
        "ineligible follow still moves the logical view"
    );
    assert_eq!(reply.effects.len(), 1, "legacy direct focus");
    assert!(
        !reply.effects[0].await_shown,
        "ineligible follow does not wait on the destination"
    );
    // Operation directional migration with a single live output has no
    // resolvable target and refuses without mutation, never silently
    // local. (Single output here; the strict map cannot resolve.)
    let mut solo = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let solo_screens = vec![screen_full("out", rect(0, 0, 800, 600))];
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "expected operation refusal"
        ),
        other => panic!("expected operation refusal, got {other:?}"),
    }
}

#[test]
fn float_send_authorizes_membership_and_guarded_sends_refuse() {
    let (mut engine, screens, _, ring) = two_output_world();
    assert_eq!(ring.len(), 2);
    let mut floating = fact("win-f", "out-a", 200);
    floating.transient = Some(true);
    let mut game = fact("win-g", "out-a", 300);
    game.privileged = Some(true);
    let mut pin = fact("win-s", "out-a", 400);
    pin.sticky = Some(true);
    // A tiled resident on the target workspace: the exception joins an
    // occupied scope, exactly like the pre-replacement float fixture.
    let resident = fact("win-t", "out-a", 500);
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
    for (i, window) in ["win-g", "win-s"].into_iter().enumerate() {
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
            Some(42 + i as u64),
            Some(WorldCommand::Send {
                window: WindowId(window.to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: ring[1].clone(),
                follow: true,
            }),
        );
        seq += 1;
        match engine.handle_report(&send) {
            WorldOutcome::Accepted(reply) => assert_eq!(
                reply.command_result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::SendRefused
                }),
                "{window}: expected send-refused"
            ),
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
        Some(44),
        Some(WorldCommand::Send {
            window: WindowId("win-f".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: ring[1].clone(),
            follow: true,
        }),
    );
    seq += 1;
    let reply = accepted(&mut engine, &send);
    // Floating-boundary follow keeps the membership-only window flight
    // under the stable command identity: no `{op}-follow` child, no view
    // flight, no frozen binding (legacy direct focus only).
    assert_eq!(reply.transfers.len(), 1);
    assert!(matches!(&reply.transfers[0].kind, TransferKind::Window(_)));
    assert_eq!(reply.transfers[0].operation_id, "op-44");
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "floating follow binds nothing"
    );
    assert_eq!(
        output_view(&reply, "out-a").as_deref(),
        Some(ring[1].as_str()),
        "floating follow still moves the logical view"
    );
    let row = row_of(&reply, "win-f");
    assert_eq!(row.workspace, ring[1]);
    assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    assert!(row.allocation.is_none());
    let flight = engine.retained_transfers();
    assert!(
        matches!(
            flight.as_slice(),
            [t] if t.operation_id == "op-44"
                && matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-f")
        ),
        "membership-only window flight retained under its identity: {flight:?}"
    );
    // Membership proof settles the window flight (accepted member
    // attempt naming the target); the echoed view proves nothing further
    // on this route. A fresh ordinary send then still works.
    let arrived = fact("win-f", "out-a", 200);
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
    // Membership proof settles the window flight (accepted member
    // attempt naming the target). The legacy setup also pushed the
    // echoed view, but the comment there already records the echo
    // proves nothing further on this route, so the v4 port carries an
    // empty lane.
    settle_report.member_attempts.push(MemberAttempt {
        operation_id: "op-44".to_owned(),
        window: WindowId("win-f".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: ring[1].clone(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(ring[1].clone()),
        }),
        attempt_ms: 41,
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
        Some(45),
        Some(WorldCommand::Send {
            window: WindowId("win-a".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: ring[1].clone(),
            follow: true,
        }),
    );
    let tail = accepted(&mut engine, &send);
    assert_eq!(engine.retained_transfers().len(), 1);
    assert!(
        !engine
            .retained_selection
            .contains_key(&OutputId("out-a".to_owned())),
        "ineligible mover binds nothing"
    );
    assert_eq!(row_of(&tail, "win-a").workspace, ring[1]);
    assert_eq!(
        output_view(&tail, "out-a").as_deref(),
        Some(ring[1].as_str()),
        "ineligible follow still moves the logical view"
    );
}

#[test]
fn deterministic_sequence_replay_matches_exactly() {
    fn script(screens: &[ScreenReport], ring: &[String]) -> Vec<WorldReportV4> {
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
        let arrived = fact("win-a", "out-a", 0);
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
    let run = |reports: &[WorldReportV4]| {
        let mut engine =
            WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
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
    // No core v4 canonical writer exists; WorldReplyV4 is PartialEq, so
    // typed exact equality is the stronger deterministic-replay check
    // (protocol `encode_reply_v4` is a pure function of the typed reply).
    assert_eq!(first, second, "deterministic replay matches exactly");
}

#[test]
fn predicate_ingress_selects_admission_without_reclassification() {
    use crate::size_hints::{FixedSizePredicate, WindowSizeHints};
    fn width_fixed_report(predicate: FixedSizePredicate, seq: u64) -> WorldReportV4 {
        let mut fixed = fact("win-f", "out-a", 200);
        // Width-only pin (height present but unequal): fixed under
        // either-axis, tiled under both-axes.
        fixed.hints = Some(WindowSizeHints {
            min_w: Some(200),
            max_w: Some(200),
            min_h: Some(100),
            max_h: Some(150),
        });
        let screens = vec![
            screen_full("out-a", rect(0, 0, 800, 600)),
            screen_full("out-b", rect(800, 0, 800, 600)),
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
        )
    }

    // Either-axis ingress floats a width-only fixed window (membership
    // only, never a frame write).
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            // Fixtures carry native sticky and maximize facts; the lane
            // under test is the native route.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![(
            "predicate".to_owned(),
            crate::connection::RawSettingValue::String(
                FixedSizePredicate::EitherAxis.as_wire_str().to_owned(),
            ),
        )],
    )
    .expect("valid generation");
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
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let reply = accepted(
        &mut engine,
        &width_fixed_report(FixedSizePredicate::BothAxes, 1),
    );
    let row = row_of(&reply, "win-f");
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    assert!(row.allocation.is_some());
    // Retained tiles never reclassify when the predicate later changes.
    engine
        .apply_test_settings(
            1,
            vec![(
                "predicate".to_owned(),
                crate::connection::RawSettingValue::String(
                    FixedSizePredicate::EitherAxis.as_wire_str().to_owned(),
                ),
            )],
        )
        .expect("settings accepted");
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
        screen_full("out-a", rect(0, 0, 800, 600)),
        screen_full("out-b", rect(800, 0, 800, 600)),
        screen_full("out-c", rect(1600, 0, 800, 600)),
    ];
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
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
            vec![incomplete_screen("out-a"), incomplete_screen("out-b")],
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
        vec![incomplete_screen("out-a"), incomplete_screen("out-b")],
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        Some(11),
        Some(move_up("win-a")),
    );
    match engine.handle_report(&fenced) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "crossing without topology refuses"
        ),
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
                screen_full("out-a", rect(0, 0, 800, 600)),
                screen_full("out-b", rect(800, 0, 800, 600)),
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
        screen_full("out-a", rect(0, 0, 800, 600)),
        screen_full("out-b", rect(800, 0, 800, 600)),
        screen_full("out-c", rect(1600, 0, 800, 600)),
    ];
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
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
    sticky.sticky = Some(true);
    let displaced = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                screen_full("out-a", rect(0, 0, 800, 600)),
                screen_full("out-b", rect(800, 0, 800, 600)),
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
                screen_full("out-a", rect(0, 0, 800, 600)),
                screen_full("out-b", rect(800, 0, 800, 600)),
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown
            }),
            "unknown focus refuses fail-closed"
        ),
        other => panic!("unknown focus refuses fail-closed, got {other:?}"),
    }
    // Unknown view echoes never authorize homing or views. v4 carries
    // no view-echo lane (and the legacy-only unknown-echo publish hold
    // has no v4 route), so the v4 port observes the same facts with an
    // empty lane: nothing invented, homing truthful, no transfers.
    let echoed = report(
        3,
        3,
        screens.clone(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "",
        None,
        None,
    );
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

/// The configured outer gap is shared across outputs with asymmetric
/// non-full work areas. Every reachable non-gesture operation route must
/// leave both staged gaps untouched; the drag-pair builder defect
/// (destination gap in `outer_gap`, `None` target) reset them before the fix.
fn gapped_pair_screens() -> Vec<ScreenReport> {
    vec![
        ScreenReport {
            output: OutputId("out-a".to_owned()),
            full_rect: Some(rect(0, 0, 800, 600)),
            work_area: Some(rect(0, 20, 800, 560)),
            complete: true,
            scale: 1.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        },
        ScreenReport {
            output: OutputId("out-b".to_owned()),
            full_rect: Some(rect(800, 0, 800, 600)),
            work_area: Some(rect(810, 10, 780, 570)),
            complete: true,
            scale: 1.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        },
    ]
}

fn bootstrap_gapped_pair() -> (WorldEngine, Vec<ScreenReport>, String, String, Vec<String>) {
    let screens = gapped_pair_screens();
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![
                fact_rect("win-a", "out-a", rect(10, 40, 40, 90)),
                fact_rect("win-s", "out-a", rect(100, 40, 40, 90)),
                fact_rect("win-b", "out-b", rect(850, 60, 40, 90)),
            ],
            "win-a",
            None,
            None,
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    let view_b = output_view(&first, "out-b").expect("view b");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    (engine, screens, view_a, view_b, ring)
}

fn pair_keys(view_a: &str, view_b: &str) -> (DomainKey, DomainKey) {
    (
        DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(view_a.to_owned()),
        },
        DomainKey {
            output: OutputId("out-b".to_owned()),
            workspace: WorkspaceId(view_b.to_owned()),
        },
    )
}

fn assert_pair_gaps(engine: &WorldEngine, key_a: &DomainKey, key_b: &DomainKey) {
    let keys = engine.engine().domain_keys();
    assert_eq!(
        engine.engine().outer_gap(key_a),
        Some(8),
        "source outer gap retained; keys={keys:?} want={key_a:?}/{key_b:?}"
    );
    assert_eq!(
        engine.engine().outer_gap(key_b),
        Some(8),
        "configured outer gap is shared across outputs"
    );
}

#[test]
fn operation_drag_pair_preserves_configured_outer_across_outputs() {
    // Preview-then-drop and direct-drop over distinct work areas: both
    // retain the configured outer gap, the drop commits the previewed slot,
    // and a follow-up same-output placement on the destination routes locally.
    for chained in [false, true] {
        let (mut engine, screens, view_a, view_b, _) = bootstrap_gapped_pair();
        let (key_a, key_b) = pair_keys(&view_a, &view_b);
        let facts = vec![
            fact_rect("win-a", "out-a", rect(10, 40, 40, 90)),
            fact_rect("win-s", "out-a", rect(100, 40, 40, 90)),
            fact_rect("win-b", "out-b", rect(850, 60, 40, 90)),
        ];
        // Observation-only round materializes both sessions before any
        // command; both staged gaps must be stored verbatim.
        accepted(
            &mut engine,
            &report(2, 2, screens.clone(), facts.clone(), "win-a", None, None),
        );
        assert_pair_gaps(&engine, &key_a, &key_b);
        let source = Some(key_a.clone());
        // Both outputs use the configured outer gap 8.
        let source_inset = rect(8, 28, 784, 544);
        let dest_inset = rect(818, 18, 764, 554);
        let destination_left = rect(818, 18, 378, 554);
        let mut preview_rect = None;
        if chained {
            let preview = report(
                3,
                3,
                screens.clone(),
                facts.clone(),
                "win-a",
                Some(81),
                Some(WorldCommand::Operation(CoreCommand::DragPreview {
                    window: "win-a".to_owned(),
                    x: 860,
                    y: 120,
                    hover_prior: None,
                    source: source.clone(),
                })),
            );
            let reply = accepted(&mut engine, &preview);
            let Some(CommandResult::DragPreview { rect, domain, .. }) =
                reply.command_result.clone()
            else {
                panic!(
                    "expected drag-preview result, got {:?}",
                    reply.command_result
                );
            };
            assert_eq!(domain.output.0, "out-b", "preview names the pointer output");
            assert_eq!(rect, destination_left, "preview names the left inset half");
            assert!(reply.transfers.is_empty(), "preview retains nothing");
            preview_rect = Some(rect);
            assert_pair_gaps(&engine, &key_a, &key_b);
        }
        let drop_seq = if chained { 82 } else { 81 };
        let drop = report(
            4,
            4,
            screens.clone(),
            facts,
            "win-a",
            Some(drop_seq),
            Some(WorldCommand::Operation(CoreCommand::DragDrop {
                window: "win-a".to_owned(),
                x: 860,
                y: 120,
                hover_prior: None,
                source: source.clone(),
            })),
        );
        let reply = accepted(&mut engine, &drop);
        assert!(
            matches!(
                reply.transfers.as_slice(),
                [t] if matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a")
                    && t.target_output.0 == "out-b"
                    && t.operation_id == format!("op-{drop_seq}")
            ),
            "pointer-domain transfer: {:?}",
            reply.transfers
        );
        assert_eq!(row_of(&reply, "win-a").output.0, "out-b");
        let allocation = row_of(&reply, "win-a")
            .allocation
            .expect("drop carries full geometry");
        let neighbor = row_of(&reply, "win-b")
            .allocation
            .expect("neighbor carries full geometry");
        let stayer = row_of(&reply, "win-s")
            .allocation
            .expect("source stayer carries full geometry");
        assert_eq!(
            allocation, destination_left,
            "drop names the left inset half"
        );
        assert_eq!(stayer, source_inset, "source stayer fills source inset");
        let union = Rect {
            x: allocation.x.min(neighbor.x),
            y: allocation.y.min(neighbor.y),
            w: (allocation.x + allocation.w).max(neighbor.x + neighbor.w)
                - allocation.x.min(neighbor.x),
            h: (allocation.y + allocation.h).max(neighbor.y + neighbor.h)
                - allocation.y.min(neighbor.y),
        };
        assert_eq!(union, dest_inset, "destination rows fill destination inset");
        if let Some(slot) = preview_rect {
            assert_eq!(allocation, slot, "drop commits the previewed slot");
        }
        assert_pair_gaps(&engine, &key_a, &key_b);
        // Same-output placement on the destination resolves locally: facts
        // echo the retained destination allocations (native arrival) and the
        // pointer targets the far right edge of the neighbor slot, which is
        // in the right half while the post-drop mover sits left.
        let px = neighbor.x + neighbor.w - 2;
        let py = neighbor.y + neighbor.h / 2;
        let local = report(
            5,
            5,
            screens.clone(),
            vec![
                fact_rect("win-a", "out-b", allocation),
                fact_rect("win-b", "out-b", neighbor),
                fact_rect("win-s", "out-a", stayer),
            ],
            "win-a",
            Some(83),
            Some(WorldCommand::Operation(CoreCommand::DragPreview {
                window: "win-a".to_owned(),
                x: px,
                y: py,
                hover_prior: None,
                source: None,
            })),
        );
        let local_reply = accepted(&mut engine, &local);
        let Some(CommandResult::DragPreview {
            rect: local_rect, ..
        }) = local_reply.command_result.clone()
        else {
            panic!("local placement runs, got {:?}", local_reply.command_result)
        };
        assert_eq!(
            local_rect, neighbor,
            "local placement names the right inset half"
        );
        assert_pair_gaps(&engine, &key_a, &key_b);
    }
}

#[test]
fn operation_routes_preserve_configured_outer_across_outputs() {
    // Eight reachable non-gesture routes: typed/operation move and focus,
    // operation workspace send and typed directional send (follow/stay).
    for case in 0..8 {
        let (mut engine, screens, view_a, view_b, ring) = bootstrap_gapped_pair();
        assert!(ring.len() >= 2, "same-output send needs a ring");
        let (key_a, key_b) = pair_keys(&view_a, &view_b);
        // Cross-output moves exhaust only with a sole mover on the source
        // (a stayer restructures locally with no transfer); focus likewise
        // resolves against the sole pair. Sends keep a stayer so the
        // source scope survives.
        let sole_mover = matches!(case, 0..=3);
        let mut facts = vec![
            fact_rect("win-a", "out-a", rect(10, 40, 40, 90)),
            fact_rect("win-b", "out-b", rect(850, 60, 40, 90)),
        ];
        if !sole_mover {
            facts.push(fact_rect("win-s", "out-a", rect(100, 40, 40, 90)));
        }
        let report_in = report(2, 2, screens.clone(), facts.clone(), "win-a", None, None);
        let _ = accepted(&mut engine, &report_in);
        assert_pair_gaps(&engine, &key_a, &key_b);
        let command = match case {
            0 => WorldCommand::MoveDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                same_axis: SameAxisMove::GroupWithNeighbor,
            },
            1 => WorldCommand::Operation(CoreCommand::Move {
                window: "win-a".to_owned(),
                direction: "right".to_owned(),
                cross_output_transfer: true,
                same_axis_move: SameAxisMove::GroupWithNeighbor,
            }),
            2 => WorldCommand::FocusDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
            },
            3 => WorldCommand::Operation(CoreCommand::Focus {
                window: "win-a".to_owned(),
                direction: "right".to_owned(),
                cross_output_transfer: true,
                float_subject: false,
            }),
            4 => WorldCommand::Operation(CoreCommand::SendToWorkspace {
                window: "win-a".to_owned(),
                target_output: "out-a".to_owned(),
                target_workspace: ring[1].clone(),
                follow: true,
            }),
            5 => WorldCommand::Operation(CoreCommand::SendToWorkspace {
                window: "win-a".to_owned(),
                target_output: "out-a".to_owned(),
                target_workspace: ring[1].clone(),
                follow: false,
            }),
            6 => WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: true,
            },
            _ => WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: false,
            },
        };
        let report_in = report(3, 3, screens, facts, "win-a", Some(11), Some(command));
        let reply = match engine.handle_report(&report_in) {
            WorldOutcome::Accepted(reply) => reply,
            WorldOutcome::Rejected { kind, message } => {
                panic!("case {case}: expected accept, rejected {kind} {message}")
            }
            other => panic!("case {case}: expected accept, got {other:?}"),
        };
        match case {
            0 | 1 => {
                assert!(
                    reply.transfers.iter().any(
                        |t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a")
                            && t.target_output.0 == "out-b"
                    ),
                    "case {case}: move crosses to out-b: {:?}",
                    reply.transfers
                );
                assert_eq!(row_of(&reply, "win-a").output.0, "out-b");
                // R4 cross moves stage pending: both sessions survive with
                // their own gaps (checked by the shared assert below).
            }
            2 | 3 => {
                let focus: Vec<_> = reply
                    .effects
                    .iter()
                    .filter(|e| e.kind == crate::world_report::EffectKind::Focus)
                    .collect();
                assert_eq!(focus.len(), 1, "case {case}: one focus effect");
                assert_ne!(focus[0].target.0, "win-a", "focus crosses the output");
            }
            4 => {
                // Ineligible same-output follow (no suppression support in
                // these facts): the single window flight under the stable
                // command identity with no frozen binding, legacy direct
                // focus, but the logical view still moves.
                assert_eq!(reply.transfers.len(), 1, "window flight only");
                assert!(matches!(&reply.transfers[0].kind, TransferKind::Window(_)));
                assert_eq!(reply.transfers[0].target_workspace, ring[1]);
                assert_eq!(reply.transfers[0].operation_id, "op-11");
                assert_eq!(row_of(&reply, "win-a").workspace, ring[1]);
                assert_eq!(
                    output_view(&reply, "out-a").as_deref(),
                    Some(ring[1].as_str())
                );
                assert!(
                    !engine
                        .retained_selection
                        .contains_key(&OutputId("out-a".to_owned())),
                    "ineligible mover binds nothing"
                );
                assert_eq!(reply.effects.len(), 1, "legacy direct focus");
                assert!(!reply.effects[0].await_shown);
            }
            5 => {
                assert_eq!(reply.transfers.len(), 1, "stay keeps only the window");
                assert_eq!(row_of(&reply, "win-a").workspace, ring[1]);
            }
            _ => {
                assert!(
                    reply.transfers.iter().any(
                        |t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a")
                            && t.target_output.0 == "out-b"
                    ),
                    "case {case}: send crosses to out-b: {:?}",
                    reply.transfers
                );
            }
        }
        assert_pair_gaps(&engine, &key_a, &key_b);
        if matches!(case, 4 | 5) {
            let workspace_target = DomainKey {
                output: key_a.output.clone(),
                workspace: WorkspaceId(ring[1].clone()),
            };
            assert_eq!(engine.engine().outer_gap(&workspace_target), Some(8));
        }
        for row in &reply.windows {
            if let Some(allocation) = row.allocation {
                let inset = if row.output == key_a.output {
                    rect(8, 28, 784, 544)
                } else {
                    rect(818, 18, 764, 554)
                };
                assert_eq!(allocation.y, inset.y, "case {case}: work-area top");
                assert_eq!(allocation.h, inset.h, "case {case}: work-area height");
                assert!(
                    allocation.x >= inset.x && allocation.x + allocation.w <= inset.x + inset.w
                );
            }
        }
    }
}

#[test]
fn explicit_output_send_uses_output_route() {
    // Explicit Operation(SendToOutput) cross-output must use the
    // cross-output builder (SendToOutput/MoveToOutput), not the
    // same-output workspace builder: the configured gap over non-full work
    // areas has exact inset layout, follow is honored, and logical parity
    // with the directional output-send route. Table-driven over follow.
    let source_inset = rect(8, 28, 784, 544);
    let dest_inset = rect(818, 18, 764, 554);
    for follow in [true, false] {
        let (mut engine, screens, view_a, view_b, _) = bootstrap_gapped_pair();
        let (key_a, key_b) = pair_keys(&view_a, &view_b);
        let facts = vec![
            fact_rect("win-a", "out-a", rect(10, 40, 40, 90)),
            fact_rect("win-s", "out-a", rect(100, 40, 40, 90)),
            fact_rect("win-b", "out-b", rect(850, 60, 40, 90)),
        ];
        let _ = accepted(
            &mut engine,
            &report(2, 2, screens.clone(), facts.clone(), "win-s", None, None),
        );
        assert_pair_gaps(&engine, &key_a, &key_b);
        let command = WorldCommand::Operation(CoreCommand::SendToOutput {
            window: "win-a".to_owned(),
            target_output: "out-b".to_owned(),
            target_workspace: view_b.clone(),
            follow,
        });
        let reply = match engine.handle_report(&report(
            3,
            3,
            screens.clone(),
            facts.clone(),
            "win-a",
            Some(11),
            Some(command),
        )) {
            WorldOutcome::Accepted(reply) => reply,
            WorldOutcome::Rejected { kind, message } => {
                panic!("follow={follow}: explicit SendToOutput must accept, got {kind} {message}")
            }
            other => panic!("follow={follow}: expected accept, got {other:?}"),
        };
        let transfer = reply
            .transfers
            .iter()
            .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a"))
            .expect("explicit output send retains a window transfer");
        assert_eq!(transfer.target_output.0, "out-b", "follow={follow}");
        assert_eq!(transfer.target_workspace, view_b, "follow={follow}");
        assert_eq!(transfer.follow, follow, "follow honored (R-OUT-04)");
        assert_eq!(transfer.operation_id, "op-11", "follow={follow}");
        assert_eq!(row_of(&reply, "win-a").output.0, "out-b");
        assert_eq!(row_of(&reply, "win-a").workspace, view_b);
        assert_pair_gaps(&engine, &key_a, &key_b);
        let focus: Vec<_> = reply
            .effects
            .iter()
            .filter(|e| e.kind == crate::world_report::EffectKind::Focus)
            .collect();
        if follow {
            assert_eq!(focus.len(), 1, "follow names one focus effect");
            assert_eq!(focus[0].target.0, "win-a");
        } else {
            // Stay leaves the source selected: the surviving stayer keeps
            // focus, never the departed mover.
            assert_eq!(focus.len(), 1, "stay names one focus effect");
            assert_eq!(focus[0].target.0, "win-s");
        }
        // Exact inset layout: every allocated row fills its output's
        // work-area-derived inset in y/h and stays inside it in x.
        for window in ["win-a", "win-s", "win-b"] {
            let row = row_of(&reply, window);
            let allocation = row.allocation.expect("tiled row has allocated geometry");
            let inset = if row.output == key_a.output {
                source_inset
            } else {
                dest_inset
            };
            assert_eq!(allocation.y, inset.y, "follow={follow}: work-area top");
            assert_eq!(allocation.h, inset.h, "follow={follow}: work-area height");
            assert!(
                allocation.x >= inset.x && allocation.x + allocation.w <= inset.x + inset.w,
                "follow={follow}: row inside inset: {allocation:?} in {inset:?}"
            );
        }
        // Parity with the working directional output-send route on a
        // fresh engine: same target cell, follow flag, focus outcome,
        // transfer shape, and per-window geometry.
        let (mut parity, pscreens, _, pview_b, _) = bootstrap_gapped_pair();
        let pfacts = vec![
            fact_rect("win-a", "out-a", rect(10, 40, 40, 90)),
            fact_rect("win-s", "out-a", rect(100, 40, 40, 90)),
            fact_rect("win-b", "out-b", rect(850, 60, 40, 90)),
        ];
        let _ = accepted(
            &mut parity,
            &report(2, 2, pscreens.clone(), pfacts.clone(), "win-s", None, None),
        );
        let preply = accepted(
            &mut parity,
            &report(
                3,
                3,
                pscreens,
                pfacts,
                "win-a",
                Some(11),
                Some(WorldCommand::SendDirectional {
                    window: WindowId("win-a".to_owned()),
                    direction: "right".to_owned(),
                    follow,
                }),
            ),
        );
        let ptransfer = preply
            .transfers
            .iter()
            .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a"))
            .expect("directional output send retains a window transfer");
        assert_eq!(ptransfer.target_output.0, "out-b");
        assert_eq!(ptransfer.target_workspace, pview_b);
        assert_eq!(ptransfer.target_workspace, view_b, "same destination cell");
        assert_eq!(ptransfer.follow, follow);
        assert_eq!(ptransfer.operation_id, transfer.operation_id);
        assert_eq!(
            preply.transfers.len(),
            reply.transfers.len(),
            "same transfer shape"
        );
        let pfocus: Vec<_> = preply
            .effects
            .iter()
            .filter(|e| e.kind == crate::world_report::EffectKind::Focus)
            .map(|e| e.target.0.clone())
            .collect();
        let efocus: Vec<_> = reply
            .effects
            .iter()
            .filter(|e| e.kind == crate::world_report::EffectKind::Focus)
            .map(|e| e.target.0.clone())
            .collect();
        assert_eq!(efocus, pfocus, "same focus outcome");
        for window in ["win-a", "win-s", "win-b"] {
            assert_eq!(
                row_of(&reply, window).allocation,
                row_of(&preply, window).allocation,
                "same geometry for {window}"
            );
            assert_eq!(
                row_of(&reply, window).output,
                row_of(&preply, window).output,
                "same homing for {window}"
            );
            assert_eq!(
                row_of(&reply, window).workspace,
                row_of(&preply, window).workspace,
                "same workspace for {window}"
            );
        }
    }
    // Explicit target workspace is carried unchanged: a valid
    // non-current workspace on the destination output lands exactly
    // there, never re-ranked to the current destination view.
    {
        let (mut engine, screens, _, view_b, _) = bootstrap_gapped_pair();
        let facts = vec![
            fact_rect("win-a", "out-a", rect(10, 40, 40, 90)),
            fact_rect("win-s", "out-a", rect(100, 40, 40, 90)),
            fact_rect("win-b", "out-b", rect(850, 60, 40, 90)),
        ];
        let observed = accepted(
            &mut engine,
            &report(2, 2, screens.clone(), facts.clone(), "win-a", None, None),
        );
        let mut ring_b: Vec<String> = observed
            .workspaces
            .iter()
            .filter(|row| row.output.0 == "out-b")
            .map(|row| row.id.clone())
            .collect();
        ring_b.sort();
        let target_ws = ring_b
            .iter()
            .find(|id| *id != &view_b)
            .cloned()
            .expect("destination output names a non-current workspace");
        let reply = accepted(
            &mut engine,
            &report(
                3,
                3,
                screens,
                facts,
                "win-a",
                Some(11),
                Some(WorldCommand::Operation(CoreCommand::SendToOutput {
                    window: "win-a".to_owned(),
                    target_output: "out-b".to_owned(),
                    target_workspace: target_ws.clone(),
                    follow: true,
                })),
            ),
        );
        let transfer = reply
            .transfers
            .iter()
            .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a"))
            .expect("non-current explicit send retains a window transfer");
        assert_eq!(transfer.target_output.0, "out-b");
        assert_eq!(transfer.target_workspace, target_ws);
        assert_ne!(
            transfer.target_workspace, view_b,
            "no re-rank to current view"
        );
        assert_eq!(row_of(&reply, "win-a").workspace, target_ws);
    }
}

#[test]
fn explicit_output_send_fences_match_output_route() {
    // Sticky, fit-excluded, same-window in-flight, and
    // migration-destination fences: identical vocabulary to the output
    // builder. Refusal probes run under both follow flags.
    for follow in [true, false] {
        let (mut engine, screens, _view_a, view_b, _) = bootstrap_gapped_pair();
        let mut sticky = fact_rect("win-a", "out-a", rect(10, 40, 40, 90));
        sticky.sticky = Some(true);
        let facts = vec![
            sticky,
            fact_rect("win-s", "out-a", rect(100, 40, 40, 90)),
            fact_rect("win-b", "out-b", rect(850, 60, 40, 90)),
        ];
        let _ = accepted(
            &mut engine,
            &report(2, 2, screens.clone(), facts.clone(), "win-a", None, None),
        );
        let refused = report(
            3,
            3,
            screens.clone(),
            facts,
            "win-a",
            Some(11),
            Some(WorldCommand::Operation(CoreCommand::SendToOutput {
                window: "win-a".to_owned(),
                target_output: "out-b".to_owned(),
                target_workspace: view_b.clone(),
                follow,
            })),
        );
        match engine.handle_report(&refused) {
            WorldOutcome::Accepted(reply) => assert_eq!(
                reply.command_result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::SendRefused
                }),
                "follow={follow}: sticky must refuse"
            ),
            other => panic!("follow={follow}: sticky must refuse, got {other:?}"),
        }
        let (mut engine, screens, _view_a, view_b, _) = bootstrap_gapped_pair();
        let mut excluded = fact_rect("win-a", "out-a", rect(10, 40, 40, 90));
        excluded.privileged = Some(true);
        let facts = vec![
            excluded,
            fact_rect("win-s", "out-a", rect(100, 40, 40, 90)),
            fact_rect("win-b", "out-b", rect(850, 60, 40, 90)),
        ];
        let _ = accepted(
            &mut engine,
            &report(2, 2, screens.clone(), facts.clone(), "win-a", None, None),
        );
        let refused = report(
            3,
            3,
            screens.clone(),
            facts,
            "win-a",
            Some(11),
            Some(WorldCommand::Operation(CoreCommand::SendToOutput {
                window: "win-a".to_owned(),
                target_output: "out-b".to_owned(),
                target_workspace: view_b.clone(),
                follow,
            })),
        );
        match engine.handle_report(&refused) {
            WorldOutcome::Accepted(reply) => assert_eq!(
                reply.command_result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::SendRefused
                }),
                "follow={follow}: fit-excluded must refuse"
            ),
            other => panic!("follow={follow}: fit-excluded must refuse, got {other:?}"),
        }
    }
    // In-flight fence: a second same-window explicit output send
    // refuses while the first flight is outstanding, under both flags.
    for follow in [true, false] {
        let (mut engine, screens, _view_a, view_b, _) = bootstrap_gapped_pair();
        let facts = vec![
            fact_rect("win-a", "out-a", rect(10, 40, 40, 90)),
            fact_rect("win-s", "out-a", rect(100, 40, 40, 90)),
            fact_rect("win-b", "out-b", rect(850, 60, 40, 90)),
        ];
        let _ = accepted(
            &mut engine,
            &report(2, 2, screens.clone(), facts.clone(), "win-a", None, None),
        );
        let first = report(
            3,
            3,
            screens.clone(),
            facts.clone(),
            "win-a",
            Some(11),
            Some(WorldCommand::Operation(CoreCommand::SendToOutput {
                window: "win-a".to_owned(),
                target_output: "out-b".to_owned(),
                target_workspace: view_b.clone(),
                follow,
            })),
        );
        let reply = match engine.handle_report(&first) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("follow={follow}: first send must accept, got {other:?}"),
        };
        assert!(
            reply
                .transfers
                .iter()
                .any(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a")),
            "follow={follow}: first send retains the flight"
        );
        let again = report(
            4,
            4,
            screens,
            facts,
            "win-a",
            Some(12),
            Some(WorldCommand::Operation(CoreCommand::SendToOutput {
                window: "win-a".to_owned(),
                target_output: "out-b".to_owned(),
                target_workspace: view_b,
                follow,
            })),
        );
        match engine.handle_report(&again) {
            WorldOutcome::Accepted(reply) => assert_eq!(
                reply.command_result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::TransferInFlight
                }),
                "follow={follow}: second send must fence"
            ),
            other => panic!("follow={follow}: second send must fence, got {other:?}"),
        }
    }
    // Migration-destination fence: a retained whole-workspace migration
    // owns its destination cell, so an explicit output send onto that
    // cell refuses as send-target without double-booking, under both
    // flags, while the migration flight survives.
    for follow in [true, false] {
        let (mut engine, screens, view_a, _view_b, ring) = bootstrap_gapped_pair();
        assert!(ring.len() >= 2, "pioneer needs a second workspace");
        let pioneer = fact_rect("win-x", "out-a", rect(200, 40, 40, 90));
        let residents = vec![
            fact_rect("win-a", "out-a", rect(10, 40, 40, 90)),
            fact_rect("win-s", "out-a", rect(100, 40, 40, 90)),
            fact_rect("win-b", "out-b", rect(850, 60, 40, 90)),
        ];
        let facts = vec![
            fact_rect("win-a", "out-a", rect(10, 40, 40, 90)),
            fact_rect("win-s", "out-a", rect(100, 40, 40, 90)),
            fact_rect("win-b", "out-b", rect(850, 60, 40, 90)),
            pioneer,
        ];
        // Real selection to the spare before the newcomer arrives, so the
        // pioneer newcomer homes there; residents keep their retained
        // homing. (Logical-echo views are inert, so the old views-echo
        // setup is dead.)
        let _ = accepted(
            &mut engine,
            &report(
                2,
                2,
                screens.clone(),
                residents.clone(),
                "win-a",
                None,
                None,
            ),
        );
        let _ = accepted(
            &mut engine,
            &report(
                3,
                3,
                screens.clone(),
                residents,
                "win-a",
                Some(10),
                Some(WorldCommand::SelectWorkspace {
                    output: OutputId("out-a".to_owned()),
                    workspace: ring[1].clone(),
                }),
            ),
        );
        let _ = accepted(
            &mut engine,
            &report(4, 4, screens.clone(), facts.clone(), "win-a", None, None),
        );
        let migrated = accepted(
            &mut engine,
            &report(
                5,
                5,
                screens.clone(),
                facts.clone(),
                "win-a",
                Some(11),
                Some(WorldCommand::MigrateWorkspace {
                    source_output: OutputId("out-a".to_owned()),
                    target_output: OutputId("out-b".to_owned()),
                    workspace: view_a.clone(),
                }),
            ),
        );
        let cell = migrated
            .transfers
            .iter()
            .find(|t| matches!(&t.kind, TransferKind::Migration))
            .expect("migration retains its destination cell");
        assert_eq!(cell.target_output.0, "out-b");
        assert_eq!(cell.target_workspace, view_a);
        let probe = report(
            6,
            6,
            screens,
            facts,
            "win-x",
            Some(12),
            Some(WorldCommand::Operation(CoreCommand::SendToOutput {
                window: "win-x".to_owned(),
                target_output: "out-b".to_owned(),
                target_workspace: view_a.clone(),
                follow,
            })),
        );
        match engine.handle_report(&probe) {
            WorldOutcome::Accepted(reply) => {
                // KIND_SEND_TARGET is "send-target-unknown"; the
                // migration-destination fence runs before the
                // mapping/staged checks, so with the matching cell
                // retained this refusal is necessarily that fence.
                assert_eq!(
                    reply.command_result,
                    Some(CommandResult::Rejected {
                        kind: CommandRejectKind::SendTargetUnknown
                    }),
                    "follow={follow}: migration cell"
                )
            }
            other => panic!("follow={follow}: send onto migration cell must refuse, got {other:?}"),
        }
        assert!(
            engine
                .retained_transfers()
                .iter()
                .any(|t| matches!(&t.kind, TransferKind::Migration) && t.operation_id == "op-11"),
            "follow={follow}: migration flight survives the refused send"
        );
        assert!(
            engine
                .retained_transfers()
                .iter()
                .all(|t| !matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-x")),
            "follow={follow}: refused send retains no window flight"
        );
    }
}

fn identity_panel(tag: &str) -> crate::world_report::OutputIdentity {
    crate::world_report::OutputIdentity {
        manufacturer: Some("SynthMfg".to_owned()),
        model: Some(format!("SynthModel-{tag}")),
        serial: Some(format!("serial-{tag}")),
        connector: None,
    }
}

fn screen_panel(output: &str, full: Rect, tag: &str) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(full),
        work_area: Some(full),
        complete: true,
        scale: 1.0,
        identity: identity_panel(tag),
    }
}

fn screen_same_name_new_panel(output: &str, full: Rect) -> ScreenReport {
    screen_panel(output, full, &format!("other-{output}"))
}

#[test]
fn physical_return_same_panel_new_handle_reclaims() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_panel("out-a", rect(0, 0, 800, 600), "a"),
                screen_panel("out-b", rect(800, 0, 800, 600), "b"),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    let view_b = output_view(&first, "out-b").expect("view b");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen_panel("out-a", rect(0, 0, 800, 600), "a")],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(engine.displaced.contains_key("out-b"), "unit displaced");
    let back = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                screen_panel("out-a", rect(0, 0, 800, 600), "a"),
                screen_panel("out-c", rect(800, 0, 800, 600), "b"),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-c", 800)],
            "",
            None,
            None,
        ),
    );
    assert!(
        !engine.displaced.contains_key("out-b"),
        "same panel under a new handle reclaims: {:?}",
        engine.displaced
    );
    let units: Vec<_> = back
        .workspaces
        .iter()
        .filter(|row| row.id == view_b)
        .collect();
    assert_eq!(units.len(), 1, "unit hosted exactly once: {units:?}");
    assert_eq!(
        units[0].output.0, "out-c",
        "matched handle hosts: {units:?}"
    );
}

#[test]
fn physical_return_same_name_different_panel_holds() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_panel("out-a", rect(0, 0, 800, 600), "a"),
                screen_panel("out-b", rect(800, 0, 800, 600), "b"),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    let view_b = output_view(&first, "out-b").expect("view b");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen_panel("out-a", rect(0, 0, 800, 600), "a")],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(engine.displaced.contains_key("out-b"));
    let back = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                screen_panel("out-a", rect(0, 0, 800, 600), "a"),
                screen_same_name_new_panel("out-b", rect(800, 0, 800, 600)),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine.displaced.contains_key("out-b"),
        "different panel under the same name must hold: {:?}",
        engine.displaced
    );
    let units: Vec<_> = back
        .workspaces
        .iter()
        .filter(|row| row.id == view_b)
        .collect();
    assert_eq!(units.len(), 1, "old unit stays hosted once: {units:?}");
    assert_eq!(
        units[0].output.0, "out-a",
        "old unit stays on survivor: {units:?}"
    );
}

#[test]
fn physical_return_unknown_identity_holds_by_name() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_full("out-a", rect(0, 0, 800, 600)),
                screen_full("out-b", rect(800, 0, 800, 600)),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    let view_b = output_view(&first, "out-b").expect("view b");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen_full("out-a", rect(0, 0, 800, 600))],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(engine.displaced.contains_key("out-b"));
    let back = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                screen_full("out-a", rect(0, 0, 800, 600)),
                screen_full("out-b", rect(800, 0, 800, 600)),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine.displaced.contains_key("out-b"),
        "unknown identity must hold even under the same name: {:?}",
        engine.displaced
    );
    let units: Vec<_> = back
        .workspaces
        .iter()
        .filter(|row| row.id == view_b)
        .collect();
    assert_eq!(
        units.len(),
        1,
        "displaced unit stays hosted once: {units:?}"
    );
    assert_eq!(units[0].output.0, "out-a", "{units:?}");
}

fn screen_ident(
    output: &str,
    full: Rect,
    identity: crate::world_report::OutputIdentity,
) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(full),
        work_area: Some(full),
        complete: true,
        scale: 1.0,
        identity,
    }
}

fn tuple_only(tag: &str) -> crate::world_report::OutputIdentity {
    crate::world_report::OutputIdentity {
        manufacturer: Some("SynthMfg".to_owned()),
        model: Some(format!("SynthModel-{tag}")),
        serial: Some(format!("serial-{tag}")),
        connector: None,
    }
}

fn tuple_conn(tag: &str, conn: &str) -> crate::world_report::OutputIdentity {
    crate::world_report::OutputIdentity {
        manufacturer: Some("SynthMfg".to_owned()),
        model: Some(format!("SynthModel-{tag}")),
        serial: Some(format!("serial-{tag}")),
        connector: Some(conn.to_owned()),
    }
}

fn unknown_conn(conn: Option<&str>) -> crate::world_report::OutputIdentity {
    crate::world_report::OutputIdentity {
        manufacturer: None,
        model: None,
        serial: None,
        connector: conn.map(str::to_owned),
    }
}

#[test]
fn physical_return_duplicate_tuple_holds_without_connector() {
    // Two live screens claim the same tuple with null connectors (the KDE
    // shape): ambiguous reconnect holds rather than picking a candidate.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), tuple_only("a")),
                screen_ident("out-b", rect(800, 0, 800, 600), tuple_only("b")),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    let view_b = output_view(&first, "out-b").expect("view b");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen_ident("out-a", rect(0, 0, 800, 600), tuple_only("a"))],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(engine.displaced.contains_key("out-b"));
    let back = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), tuple_only("a")),
                screen_ident("out-b", rect(800, 0, 800, 600), tuple_only("b")),
                screen_ident("out-c", rect(1600, 0, 800, 600), tuple_only("b")),
            ],
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-b", 800),
                fact("win-c", "out-c", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine.displaced.contains_key("out-b"),
        "duplicated tuple with null connectors must hold: {:?}",
        engine.displaced
    );
    let units: Vec<_> = back
        .workspaces
        .iter()
        .filter(|row| row.id == view_b)
        .collect();
    assert_eq!(units.len(), 1, "{units:?}");
    assert_eq!(
        units[0].output.0, "out-a",
        "unit stays on survivor: {units:?}"
    );
}

#[test]
fn physical_return_duplicate_tuple_connector_fallback_returns() {
    // Duplicated tuple with proved distinct connectors: the connector lane
    // disambiguates the return onto the matching live handle.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), tuple_conn("a", "conn-a")),
                screen_ident("out-b", rect(800, 0, 800, 600), tuple_conn("b", "conn-b")),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    let view_b = output_view(&first, "out-b").expect("view b");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen_ident(
                "out-a",
                rect(0, 0, 800, 600),
                tuple_conn("a", "conn-a"),
            )],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(engine.displaced.contains_key("out-b"));
    let back = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), tuple_conn("a", "conn-a")),
                screen_ident("out-b", rect(800, 0, 800, 600), tuple_conn("b", "conn-b")),
                screen_ident("out-c", rect(1600, 0, 800, 600), tuple_conn("b", "conn-x")),
            ],
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-b", 800),
                fact("win-c", "out-c", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        !engine.displaced.contains_key("out-b"),
        "connector lane disambiguates: {:?}",
        engine.displaced
    );
    let units: Vec<_> = back
        .workspaces
        .iter()
        .filter(|row| row.id == view_b)
        .collect();
    assert_eq!(units.len(), 1, "{units:?}");
    assert_eq!(units[0].output.0, "out-b", "{units:?}");
}

#[test]
fn physical_return_connector_fallback_unknown_tuple_renamed() {
    // Unreadable tuple with a proved connector reclaims under a new live
    // handle; a null connector under a new handle holds (never an
    // arbitrary candidate, never the live name).
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let _ = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), unknown_conn(Some("conn-a"))),
                screen_ident(
                    "out-b",
                    rect(800, 0, 800, 600),
                    unknown_conn(Some("conn-b")),
                ),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen_ident(
                "out-a",
                rect(0, 0, 800, 600),
                unknown_conn(Some("conn-a")),
            )],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(engine.displaced.contains_key("out-b"));
    let back = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), unknown_conn(Some("conn-a"))),
                screen_ident(
                    "out-c",
                    rect(800, 0, 800, 600),
                    unknown_conn(Some("conn-b")),
                ),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-c", 800)],
            "",
            None,
            None,
        ),
    );
    assert!(
        !engine.displaced.contains_key("out-b"),
        "proved connector reclaims under a new handle: {:?}",
        engine.displaced
    );
    assert!(
        back.workspaces.iter().any(|row| row.output.0 == "out-c"),
        "matched handle hosts: {:?}",
        back.workspaces
    );

    // Null connector variant: same shape but nothing proved must hold.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let _ = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), unknown_conn(None)),
                screen_ident("out-b", rect(800, 0, 800, 600), unknown_conn(None)),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen_ident(
                "out-a",
                rect(0, 0, 800, 600),
                unknown_conn(None),
            )],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    let back = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), unknown_conn(None)),
                screen_ident("out-z", rect(800, 0, 800, 600), unknown_conn(None)),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-z", 800)],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine.displaced.contains_key("out-b"),
        "unproved connector never takes an arbitrary candidate: {:?}",
        engine.displaced
    );
    assert!(
        back.workspaces.iter().any(|row| row.output.0 == "out-z"),
        "new handle bootstraps its own ring: {:?}",
        back.workspaces
    );
}

#[test]
fn physical_return_unplug_replug_cycles_keep_unit_and_views() {
    // Repeated unplug/replug of the same panel preserves the unit id and
    // contents, keeps the survivor view stable, and mints no focus effects.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_panel("out-a", rect(0, 0, 800, 600), "a"),
                screen_panel("out-b", rect(800, 0, 800, 600), "b"),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    let view_b = output_view(&first, "out-b").expect("view b");
    for cycle in 0..2 {
        let seq = 2 + cycle * 2;
        let away = accepted(
            &mut engine,
            &report(
                seq,
                seq,
                vec![screen_panel("out-a", rect(0, 0, 800, 600), "a")],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
                "",
                None,
                None,
            ),
        );
        assert!(
            engine.displaced.contains_key("out-b"),
            "cycle {cycle}: displaced"
        );
        assert!(
            away.effects.is_empty(),
            "cycle {cycle}: displacement mints no focus effects: {:?}",
            away.effects
        );
        assert_eq!(
            output_view(&away, "out-a"),
            Some(view_a.clone()),
            "cycle {cycle}: survivor view continuity"
        );
        let back = accepted(
            &mut engine,
            &report(
                seq + 1,
                seq + 1,
                vec![
                    screen_panel("out-a", rect(0, 0, 800, 600), "a"),
                    screen_panel("out-b", rect(800, 0, 800, 600), "b"),
                ],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
                "",
                None,
                None,
            ),
        );
        assert!(
            !engine.displaced.contains_key("out-b"),
            "cycle {cycle}: reclaimed: {:?}",
            engine.displaced
        );
        assert!(
            back.effects.is_empty(),
            "cycle {cycle}: return mints no focus effects: {:?}",
            back.effects
        );
        let units: Vec<_> = back
            .workspaces
            .iter()
            .filter(|row| row.id == view_b)
            .collect();
        assert_eq!(units.len(), 1, "cycle {cycle}: unit id stable: {units:?}");
        assert_eq!(units[0].output.0, "out-b", "cycle {cycle}: {units:?}");
        let rows: Vec<_> = back
            .windows
            .iter()
            .filter(|row| row.lifetime.0 == "win-b")
            .collect();
        assert_eq!(rows.len(), 1, "cycle {cycle}: exactly one homing: {rows:?}");
        assert_eq!(
            rows[0].workspace, view_b,
            "cycle {cycle}: contents ride: {rows:?}"
        );
    }
}

#[test]
fn physical_return_explicit_send_away_not_pulled_back() {
    // A member explicitly sent to the survivor's own workspace while its
    // unit is displaced stays where sent: the return reclaims the unit
    // but issues no policy pull-back for the departed member.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_panel("out-a", rect(0, 0, 800, 600), "a"),
                screen_panel("out-b", rect(800, 0, 800, 600), "b"),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    let _view_b = output_view(&first, "out-b").expect("view b");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen_panel("out-a", rect(0, 0, 800, 600), "a")],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(engine.displaced.contains_key("out-b"));
    // Explicitly send the displaced member onto the survivor's own unit.
    // The mover must be the observed focus for the transfer to issue.
    let sent = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen_panel("out-a", rect(0, 0, 800, 600), "a")],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-b",
            Some(11),
            Some(crate::world_report::WorldCommand::Send {
                window: WindowId("win-b".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: view_a.clone(),
                follow: false,
            }),
        ),
    );
    assert!(
        sent.transfers.iter().any(|t| t.operation_id == "op-11"),
        "explicit send executes: {:?}",
        sent.transfers
    );
    // The panel returns while the explicit departure flight is still
    // outstanding: the departed member stays where sent and the return
    // issues no policy pull-back for it.
    let back = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                screen_panel("out-a", rect(0, 0, 800, 600), "a"),
                screen_panel("out-b", rect(800, 0, 800, 600), "b"),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(
        !engine.displaced.contains_key("out-b"),
        "unit still reclaims: {:?}",
        engine.displaced
    );
    let rows: Vec<_> = back
        .windows
        .iter()
        .filter(|row| row.lifetime.0 == "win-b")
        .collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(
        rows[0].workspace, view_a,
        "departed member not pulled back: {rows:?}"
    );
    assert_eq!(rows[0].output.0, "out-a", "{rows:?}");
    assert!(
        back.transfers.iter().all(
            |t| !(matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-b")
                && t.operation_id.starts_with("op-hp-"))
        ),
        "no pull-back flight for the departed member: {:?}",
        back.transfers
    );
    assert!(
        back.transfers.iter().any(|t| t.operation_id == "op-11"),
        "explicit departure association survives the return: {:?}",
        back.transfers
    );
    // The reclaimed unit itself is empty now (its only member was sent
    // away), so trailing lifecycle may trim its id; the panel itself is
    // live again with its own ring while the departed member stays put.
    assert!(
        !engine
            .mapping
            .get("out-b")
            .unwrap_or(&Vec::new())
            .is_empty(),
        "returned panel hosts a live ring: {:?}",
        engine.mapping
    );
}

#[test]
fn physical_disconnect_no_survivor_holds_unit_for_retry() {
    // The last screen disconnects with no survivor: the ring is kept
    // untouched (no placement, no loss, no focus effects) and the next
    // report with the same panel restores it.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen_panel("out-a", rect(0, 0, 800, 600), "a")],
            vec![fact("win-a", "out-a", 0)],
            "",
            None,
            None,
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    let empty = report(
        2,
        2,
        Vec::new(),
        vec![fact("win-a", "out-a", 0)],
        "",
        None,
        None,
    );
    let held = match engine.handle_report(&empty) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("empty-screen disconnect must hold, got {other:?}"),
    };
    assert!(
        held.effects.is_empty(),
        "no-survivor displacement mints no focus effects: {:?}",
        held.effects
    );
    assert!(
        held.transfers.is_empty(),
        "no-survivor displacement mints no flights: {:?}",
        held.transfers
    );
    assert!(
        engine
            .mapping
            .get("out-a")
            .is_some_and(|ring| ring.contains(&view_a)),
        "unit kept untouched for retry: {:?}",
        engine.mapping
    );
    assert!(
        engine.displaced.is_empty(),
        "nothing displaced without a survivor: {:?}",
        engine.displaced
    );
    let back = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen_panel("out-a", rect(0, 0, 800, 600), "a")],
            vec![fact("win-a", "out-a", 0)],
            "",
            None,
            None,
        ),
    );
    let units: Vec<_> = back
        .workspaces
        .iter()
        .filter(|row| row.id == view_a)
        .collect();
    assert_eq!(units.len(), 1, "same panel restores its unit: {units:?}");
    assert_eq!(units[0].output.0, "out-a", "{units:?}");
}

#[test]
fn physical_return_shared_tuple_solo_intermediate_holds_null() {
    // A and B share one tuple with null connectors. B disconnects; later
    // A-only reports must not consume B's association onto A: the tuple
    // never uniquely identified B.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), tuple_only("dup")),
                screen_ident("out-b", rect(800, 0, 800, 600), tuple_only("dup")),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    let view_b = output_view(&first, "out-b").expect("view b");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen_ident(
                "out-a",
                rect(0, 0, 800, 600),
                tuple_only("dup"),
            )],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(engine.displaced.contains_key("out-b"), "B displaced");
    // Intermediate A-only report while B is still absent: hold, keep the
    // association and the units on the survivor.
    let mid = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen_ident(
                "out-a",
                rect(0, 0, 800, 600),
                tuple_only("dup"),
            )],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine.displaced.contains_key("out-b"),
        "shared tuple must not reclaim onto the peer: {:?}",
        engine.displaced
    );
    let units: Vec<_> = mid
        .workspaces
        .iter()
        .filter(|row| row.id == view_b)
        .collect();
    assert_eq!(units.len(), 1, "{units:?}");
    assert_eq!(
        units[0].output.0, "out-a",
        "unit stays on survivor: {units:?}"
    );
}

#[test]
fn physical_return_shared_tuple_solo_intermediate_holds_distinct_connector() {
    // Same shape with proved distinct connectors: A-only intermediates hold
    // (connector mismatch), and the true B later reclaims via its connector.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), tuple_conn("dup", "conn-a")),
                screen_ident("out-b", rect(800, 0, 800, 600), tuple_conn("dup", "conn-b")),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    let view_b = output_view(&first, "out-b").expect("view b");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen_ident(
                "out-a",
                rect(0, 0, 800, 600),
                tuple_conn("dup", "conn-a"),
            )],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(engine.displaced.contains_key("out-b"), "B displaced");
    let mid = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen_ident(
                "out-a",
                rect(0, 0, 800, 600),
                tuple_conn("dup", "conn-a"),
            )],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine.displaced.contains_key("out-b"),
        "connector mismatch must hold: {:?}",
        engine.displaced
    );
    assert!(
        mid.workspaces
            .iter()
            .any(|row| row.id == view_b && row.output.0 == "out-a"),
        "unit stays on survivor: {:?}",
        mid.workspaces
    );
    // The true B returns alongside A: the connector lane disambiguates.
    let back = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), tuple_conn("dup", "conn-a")),
                screen_ident("out-b", rect(800, 0, 800, 600), tuple_conn("dup", "conn-b")),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    assert!(
        !engine.displaced.contains_key("out-b"),
        "true panel reclaims via connector: {:?}",
        engine.displaced
    );
    assert!(
        back.workspaces
            .iter()
            .any(|row| row.id == view_b && row.output.0 == "out-b"),
        "unit reclaimed onto B: {:?}",
        back.workspaces
    );
}

#[test]
fn physical_return_shared_tuple_solo_b_returns_after_a_leaves() {
    // Distinct connectors: A leaves too and only B is live; the connector
    // lane still honors the true return.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), tuple_conn("dup", "conn-a")),
                screen_ident("out-b", rect(800, 0, 800, 600), tuple_conn("dup", "conn-b")),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    let view_b = output_view(&first, "out-b").expect("view b");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen_ident(
                "out-a",
                rect(0, 0, 800, 600),
                tuple_conn("dup", "conn-a"),
            )],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(engine.displaced.contains_key("out-b"), "B displaced");
    // A leaves as well while B returns solo: B reclaims, A displaces.
    let back = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen_ident(
                "out-b",
                rect(800, 0, 800, 600),
                tuple_conn("dup", "conn-b"),
            )],
            vec![fact("win-a", "out-b", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    assert!(
        !engine.displaced.contains_key("out-b"),
        "solo true panel reclaims: {:?}",
        engine.displaced
    );
    assert!(
        back.workspaces
            .iter()
            .any(|row| row.id == view_b && row.output.0 == "out-b"),
        "unit reclaimed onto B: {:?}",
        back.workspaces
    );
}

#[test]
fn physical_return_cross_lane_same_target_holds() {
    // Two displaced origins claim one live panel through different lanes
    // (tuple vs connector): reclaiming both would conflate two physical
    // identities, so both claims hold.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let _ = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-s", rect(0, 0, 800, 600), tuple_only("s")),
                screen_ident("out-x", rect(800, 0, 800, 600), tuple_conn("x", "conn-x")),
                screen_ident(
                    "out-y",
                    rect(1600, 0, 800, 600),
                    unknown_conn(Some("conn-y")),
                ),
            ],
            vec![
                fact("win-s", "out-s", 0),
                fact("win-x", "out-x", 800),
                fact("win-y", "out-y", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen_ident("out-s", rect(0, 0, 800, 600), tuple_only("s"))],
            vec![
                fact("win-s", "out-s", 0),
                fact("win-x", "out-s", 100),
                fact("win-y", "out-s", 200),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(engine.displaced.contains_key("out-x"), "X displaced");
    assert!(engine.displaced.contains_key("out-y"), "Y displaced");
    // out-p carries X's tuple with Y's connector: both lanes hit one panel.
    let mixed = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                screen_ident("out-s", rect(0, 0, 800, 600), tuple_only("s")),
                screen_ident(
                    "out-p",
                    rect(800, 0, 800, 600),
                    crate::world_report::OutputIdentity {
                        manufacturer: Some("SynthMfg".to_owned()),
                        model: Some("SynthModel-x".to_owned()),
                        serial: Some("serial-x".to_owned()),
                        connector: Some("conn-y".to_owned()),
                    },
                ),
            ],
            vec![
                fact("win-s", "out-s", 0),
                fact("win-x", "out-p", 800),
                fact("win-y", "out-p", 900),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine.displaced.contains_key("out-x") && engine.displaced.contains_key("out-y"),
        "colliding claims must both hold: {:?}",
        engine.displaced
    );
    assert!(
        mixed
            .transfers
            .iter()
            .all(|t| !t.operation_id.starts_with("op-hp-")),
        "no policy pull-back on ambiguity: {:?}",
        mixed.transfers
    );
}

#[test]
fn physical_return_connector_fallback_live_tuple_unreadable() {
    // Origin snapshot has a readable tuple plus a proved connector; the
    // returned panel proves the same connector but one tuple arm is null.
    // Unreadable either side still falls back to the connector lane.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), tuple_only("a")),
                screen_ident("out-b", rect(800, 0, 800, 600), tuple_conn("b", "conn-b")),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    let view_b = output_view(&first, "out-b").expect("view b");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen_ident("out-a", rect(0, 0, 800, 600), tuple_only("a"))],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(engine.displaced.contains_key("out-b"), "B displaced");
    let partial = crate::world_report::OutputIdentity {
        manufacturer: None,
        model: Some("SynthModel-b".to_owned()),
        serial: Some("serial-b".to_owned()),
        connector: Some("conn-b".to_owned()),
    };
    let back = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), tuple_only("a")),
                screen_ident("out-b2", rect(800, 0, 800, 600), partial),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b2", 800)],
            "",
            None,
            None,
        ),
    );
    assert!(
        !engine.displaced.contains_key("out-b"),
        "connector reclaim works with live tuple unreadable: {:?}",
        engine.displaced
    );
    assert!(
        back.workspaces
            .iter()
            .any(|row| row.id == view_b && row.output.0 == "out-b2"),
        "unit reclaimed onto renamed panel: {:?}",
        back.workspaces
    );
}

#[test]
fn physical_return_connector_fallback_readable_mismatch_holds() {
    // Control: the same connector carrying a READABLE different tuple is
    // a different physical panel, never a connector reclaim.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), tuple_only("a")),
                screen_ident("out-b", rect(800, 0, 800, 600), tuple_conn("b", "conn-b")),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "",
            None,
            None,
        ),
    );
    let view_b = output_view(&first, "out-b").expect("view b");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen_ident("out-a", rect(0, 0, 800, 600), tuple_only("a"))],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
        ),
    );
    assert!(engine.displaced.contains_key("out-b"), "B displaced");
    let back = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                screen_ident("out-a", rect(0, 0, 800, 600), tuple_only("a")),
                screen_ident(
                    "out-x",
                    rect(800, 0, 800, 600),
                    tuple_conn("other", "conn-b"),
                ),
            ],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-x", 800)],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine.displaced.contains_key("out-b"),
        "readable different tuple must hold: {:?}",
        engine.displaced
    );
    assert!(
        back.workspaces
            .iter()
            .any(|row| row.id == view_b && row.output.0 == "out-a"),
        "old unit stays on survivor: {:?}",
        back.workspaces
    );
    assert!(
        back.workspaces.iter().any(|row| row.output.0 == "out-x"),
        "new panel bootstraps its own ring: {:?}",
        back.workspaces
    );
}

#[test]
fn physical_return_survivor_leaves_then_origin_returns_renamed() {
    // S displaces onto T, T leaves (chain onto U), then S returns renamed:
    // the exact viewed unit id, its tiling, and its contents restore.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-s", rect(0, 0, 800, 600), tuple_only("s")),
                screen_ident("out-t", rect(800, 0, 800, 600), tuple_only("t")),
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
            ],
            vec![
                fact("win-s", "out-s", 0),
                fact("win-t", "out-t", 800),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    let view_s = output_view(&first, "out-s").expect("view s");
    let tiling_s = first
        .workspaces
        .iter()
        .find(|row| row.id == view_s)
        .map(|row| row.tiling_mode)
        .expect("tiling s");
    let away = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                screen_ident("out-t", rect(800, 0, 800, 600), tuple_only("t")),
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
            ],
            vec![
                fact("win-s", "out-t", 800),
                fact("win-t", "out-t", 900),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    assert_eq!(
        engine.displaced.get("out-s").map(|e| e.dest.as_str()),
        Some("out-t")
    );
    assert!(
        away.effects.is_empty(),
        "first displacement mints no focus: {:?}",
        away.effects
    );
    let chained = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen_ident(
                "out-u",
                rect(1600, 0, 800, 600),
                tuple_only("u"),
            )],
            vec![
                fact("win-s", "out-u", 1600),
                fact("win-t", "out-u", 1700),
                fact("win-u", "out-u", 1800),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine.displaced.contains_key("out-s"),
        "S association survives"
    );
    assert!(
        engine.displaced.contains_key("out-t"),
        "T displaces in turn"
    );
    assert_eq!(
        engine.displaced.get("out-s").map(|e| e.dest.as_str()),
        Some("out-u"),
        "chain refresh tracks the live host: {:?}",
        engine.displaced
    );
    assert!(
        chained.effects.is_empty(),
        "chained displacement mints no focus: {:?}",
        chained.effects
    );
    let s_units: Vec<String> = engine
        .displaced
        .get("out-s")
        .map(|entry| entry.workspaces.clone())
        .unwrap_or_default();
    assert!(
        s_units.contains(&view_s),
        "displaced record keeps the viewed unit: {s_units:?}",
    );
    let back = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
                screen_ident("out-s2", rect(0, 0, 800, 600), tuple_only("s")),
            ],
            vec![
                fact("win-s", "out-s2", 0),
                fact("win-t", "out-u", 1700),
                fact("win-u", "out-u", 1800),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        !engine.displaced.contains_key("out-s"),
        "renamed S reclaims: {:?}",
        engine.displaced
    );
    // Chained reclaim restores the exact viewed unit with its tiling
    // and contents; every member stays exactly homed once and the return
    // mints no focus effects or pull-back flights. The fresh handle
    // keeps its bootstrap ring alongside the restored unit, as in the
    // single-hop renamed return; the viewed id itself must survive.
    let restored_rows: Vec<_> = back
        .workspaces
        .iter()
        .filter(|row| row.id == view_s)
        .collect();
    assert_eq!(
        restored_rows.len(),
        1,
        "viewed unit hosted exactly once: {:?}",
        back.workspaces
    );
    assert_eq!(
        restored_rows[0].output.0, "out-s2",
        "viewed unit hosts on the renamed panel: {:?}",
        restored_rows[0],
    );
    assert_eq!(
        restored_rows[0].tiling_mode, tiling_s,
        "tiling rides the chain: {:?}",
        restored_rows[0],
    );
    let rows: Vec<_> = back
        .windows
        .iter()
        .filter(|row| ["win-s", "win-t", "win-u"].contains(&row.lifetime.0.as_str()))
        .collect();
    assert_eq!(rows.len(), 3, "every member exactly homed once: {rows:?}");
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-s"
            && row.output.0 == "out-s2"
            && row.workspace == view_s),
        "S member rides its exact unit home: {rows:?}",
    );
    assert!(
        back.effects.is_empty(),
        "renamed return mints no focus: {:?}",
        back.effects
    );
    assert!(
        back.transfers
            .iter()
            .all(|t| !t.operation_id.starts_with("op-hp-")),
        "no pull-back flights: {:?}",
        back.transfers
    );
}

#[test]
fn physical_chain_return_s_first_then_t_keeps_exact_affinity() {
    // S -> T -> U chain, then S returns renamed, then T returns renamed.
    // Final affinity must be exact: each viewed unit hosted once on its
    // own panel with its own member and tiling, no yank of the already
    // reclaimed S unit when T returns.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-s", rect(0, 0, 800, 600), tuple_only("s")),
                screen_ident("out-t", rect(800, 0, 800, 600), tuple_only("t")),
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
            ],
            vec![
                fact("win-s", "out-s", 0),
                fact("win-t", "out-t", 800),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    let view_s = output_view(&first, "out-s").expect("view s");
    let view_t = output_view(&first, "out-t").expect("view t");
    let view_u = output_view(&first, "out-u").expect("view u");
    let tiling_s = first
        .workspaces
        .iter()
        .find(|row| row.id == view_s)
        .map(|row| row.tiling_mode)
        .expect("tiling s");
    let tiling_t = first
        .workspaces
        .iter()
        .find(|row| row.id == view_t)
        .map(|row| row.tiling_mode)
        .expect("tiling t");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                screen_ident("out-t", rect(800, 0, 800, 600), tuple_only("t")),
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
            ],
            vec![
                fact("win-s", "out-t", 800),
                fact("win-t", "out-t", 900),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    assert_eq!(
        engine.displaced.get("out-s").map(|e| e.dest.as_str()),
        Some("out-t")
    );
    let _ = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen_ident(
                "out-u",
                rect(1600, 0, 800, 600),
                tuple_only("u"),
            )],
            vec![
                fact("win-s", "out-u", 1600),
                fact("win-t", "out-u", 1700),
                fact("win-u", "out-u", 1800),
            ],
            "",
            None,
            None,
        ),
    );
    assert_eq!(
        engine.displaced.get("out-s").map(|e| e.dest.as_str()),
        Some("out-u"),
        "chain refresh tracks the live host: {:?}",
        engine.displaced
    );
    assert_eq!(
        engine.displaced.get("out-t").map(|e| e.dest.as_str()),
        Some("out-u")
    );
    assert!(
        engine
            .displaced
            .get("out-t")
            .map(|entry| entry.workspaces.contains(&view_s))
            .unwrap_or(false),
        "chained T record overlaps the S unit: {:?}",
        engine.displaced
    );
    let s_back = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
                screen_ident("out-s2", rect(0, 0, 800, 600), tuple_only("s")),
            ],
            vec![
                fact("win-s", "out-s2", 0),
                fact("win-t", "out-u", 1700),
                fact("win-u", "out-u", 1800),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        !engine.displaced.contains_key("out-s"),
        "renamed S reclaims: {:?}",
        engine.displaced
    );
    assert!(
        engine.displaced.contains_key("out-t"),
        "T still displaced: {:?}",
        engine.displaced
    );
    let s_rows: Vec<_> = s_back
        .workspaces
        .iter()
        .filter(|row| row.id == view_s)
        .collect();
    assert_eq!(
        s_rows.len(),
        1,
        "viewed S unit hosted once: {:?}",
        s_back.workspaces
    );
    assert_eq!(s_rows[0].output.0, "out-s2", "{:?}", s_rows[0]);
    assert_eq!(s_rows[0].tiling_mode, tiling_s, "{:?}", s_rows[0]);
    let s_members: Vec<_> = s_back
        .windows
        .iter()
        .filter(|row| ["win-s", "win-t", "win-u"].contains(&row.lifetime.0.as_str()))
        .collect();
    assert_eq!(
        s_members.len(),
        3,
        "every member exactly homed once: {s_members:?}"
    );
    assert!(
        s_members.iter().any(|row| row.lifetime.0 == "win-s"
            && row.output.0 == "out-s2"
            && row.workspace == view_s),
        "S member rides its exact unit home: {s_members:?}"
    );
    assert!(
        s_members.iter().any(|row| row.lifetime.0 == "win-t"
            && row.output.0 == "out-u"
            && row.workspace == view_t),
        "T member waits on the survivor unit: {s_members:?}"
    );
    assert!(
        s_members.iter().any(|row| row.lifetime.0 == "win-u"
            && row.output.0 == "out-u"
            && row.workspace == view_u),
        "U member stays on its unit: {s_members:?}"
    );
    assert!(
        s_back
            .transfers
            .iter()
            .all(|t| !t.operation_id.starts_with("op-hp-")),
        "no pull-back flights: {:?}",
        s_back.transfers
    );
    let both_back = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
                screen_ident("out-s2", rect(0, 0, 800, 600), tuple_only("s")),
                screen_ident("out-t2", rect(800, 0, 800, 600), tuple_only("t")),
            ],
            vec![
                fact("win-s", "out-s2", 0),
                fact("win-t", "out-t2", 800),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine.displaced.is_empty(),
        "both origins reclaim: {:?}",
        engine.displaced
    );
    for (id, output, tiling) in [(&view_s, "out-s2", tiling_s), (&view_t, "out-t2", tiling_t)] {
        let rows: Vec<_> = both_back
            .workspaces
            .iter()
            .filter(|row| &row.id == id)
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "unit {id} hosted exactly once: {:?}",
            both_back.workspaces
        );
        assert_eq!(rows[0].output.0, output, "{:?}", rows[0]);
        assert_eq!(rows[0].tiling_mode, tiling, "{:?}", rows[0]);
    }
    let u_rows: Vec<_> = both_back
        .workspaces
        .iter()
        .filter(|row| row.id == view_u)
        .collect();
    assert_eq!(
        u_rows.len(),
        1,
        "viewed U unit hosted once: {:?}",
        both_back.workspaces
    );
    assert_eq!(u_rows[0].output.0, "out-u", "{:?}", u_rows[0]);
    let rows: Vec<_> = both_back
        .windows
        .iter()
        .filter(|row| ["win-s", "win-t", "win-u"].contains(&row.lifetime.0.as_str()))
        .collect();
    assert_eq!(rows.len(), 3, "every member exactly homed once: {rows:?}");
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-s"
            && row.output.0 == "out-s2"
            && row.workspace == view_s),
        "S member stays home after T returns: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-t"
            && row.output.0 == "out-t2"
            && row.workspace == view_t),
        "T member rides its exact unit home: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-u"
            && row.output.0 == "out-u"
            && row.workspace == view_u),
        "U member stays on its unit: {rows:?}"
    );
    assert!(
        both_back.effects.is_empty(),
        "returns mint no focus: {:?}",
        both_back.effects
    );
    assert!(
        both_back
            .transfers
            .iter()
            .all(|t| !t.operation_id.starts_with("op-hp-")),
        "no pull-back flights: {:?}",
        both_back.transfers
    );
}

#[test]
fn physical_chain_return_t_first_then_s_keeps_exact_affinity() {
    // Same S -> T -> U chain, opposite return order: T returns renamed
    // first while S is still absent. The S unit must stay on the survivor
    // until S itself returns; the final partition is exact.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-s", rect(0, 0, 800, 600), tuple_only("s")),
                screen_ident("out-t", rect(800, 0, 800, 600), tuple_only("t")),
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
            ],
            vec![
                fact("win-s", "out-s", 0),
                fact("win-t", "out-t", 800),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    let view_s = output_view(&first, "out-s").expect("view s");
    let view_t = output_view(&first, "out-t").expect("view t");
    let view_u = output_view(&first, "out-u").expect("view u");
    let tiling_s = first
        .workspaces
        .iter()
        .find(|row| row.id == view_s)
        .map(|row| row.tiling_mode)
        .expect("tiling s");
    let tiling_t = first
        .workspaces
        .iter()
        .find(|row| row.id == view_t)
        .map(|row| row.tiling_mode)
        .expect("tiling t");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                screen_ident("out-t", rect(800, 0, 800, 600), tuple_only("t")),
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
            ],
            vec![
                fact("win-s", "out-t", 800),
                fact("win-t", "out-t", 900),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    let _ = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen_ident(
                "out-u",
                rect(1600, 0, 800, 600),
                tuple_only("u"),
            )],
            vec![
                fact("win-s", "out-u", 1600),
                fact("win-t", "out-u", 1700),
                fact("win-u", "out-u", 1800),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine
            .displaced
            .get("out-t")
            .map(|entry| entry.workspaces.contains(&view_s))
            .unwrap_or(false),
        "chained T record overlaps the S unit: {:?}",
        engine.displaced
    );
    let t_back = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
                screen_ident("out-t2", rect(800, 0, 800, 600), tuple_only("t")),
            ],
            vec![
                fact("win-s", "out-u", 1600),
                fact("win-t", "out-t2", 800),
                fact("win-u", "out-u", 1800),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        !engine.displaced.contains_key("out-t"),
        "renamed T reclaims: {:?}",
        engine.displaced
    );
    assert!(
        engine.displaced.contains_key("out-s"),
        "S still displaced: {:?}",
        engine.displaced
    );
    let s_rows: Vec<_> = t_back
        .workspaces
        .iter()
        .filter(|row| row.id == view_s)
        .collect();
    assert_eq!(
        s_rows.len(),
        1,
        "S unit hosted exactly once: {:?}",
        t_back.workspaces
    );
    assert_eq!(
        s_rows[0].output.0, "out-u",
        "S unit waits on survivor: {:?}",
        s_rows[0]
    );
    let t_rows: Vec<_> = t_back
        .workspaces
        .iter()
        .filter(|row| row.id == view_t)
        .collect();
    assert_eq!(
        t_rows.len(),
        1,
        "T unit hosted exactly once: {:?}",
        t_back.workspaces
    );
    assert_eq!(t_rows[0].output.0, "out-t2", "{:?}", t_rows[0]);
    assert_eq!(t_rows[0].tiling_mode, tiling_t, "{:?}", t_rows[0]);
    let members: Vec<_> = t_back
        .windows
        .iter()
        .filter(|row| ["win-s", "win-t", "win-u"].contains(&row.lifetime.0.as_str()))
        .collect();
    assert_eq!(
        members.len(),
        3,
        "every member exactly homed once: {members:?}"
    );
    assert!(
        members.iter().any(|row| row.lifetime.0 == "win-s"
            && row.output.0 == "out-u"
            && row.workspace == view_s),
        "S member waits on the survivor unit: {members:?}"
    );
    assert!(
        members.iter().any(|row| row.lifetime.0 == "win-t"
            && row.output.0 == "out-t2"
            && row.workspace == view_t),
        "T member rides its exact unit home: {members:?}"
    );
    assert!(
        members.iter().any(|row| row.lifetime.0 == "win-u"
            && row.output.0 == "out-u"
            && row.workspace == view_u),
        "U member stays on its unit: {members:?}"
    );
    assert!(
        t_back
            .transfers
            .iter()
            .all(|t| !t.operation_id.starts_with("op-hp-")),
        "no pull-back flights: {:?}",
        t_back.transfers
    );
    let both_back = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
                screen_ident("out-t2", rect(800, 0, 800, 600), tuple_only("t")),
                screen_ident("out-s2", rect(0, 0, 800, 600), tuple_only("s")),
            ],
            vec![
                fact("win-s", "out-s2", 0),
                fact("win-t", "out-t2", 800),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine.displaced.is_empty(),
        "both origins reclaim: {:?}",
        engine.displaced
    );
    for (id, output, tiling) in [(&view_s, "out-s2", tiling_s), (&view_t, "out-t2", tiling_t)] {
        let rows: Vec<_> = both_back
            .workspaces
            .iter()
            .filter(|row| &row.id == id)
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "unit {id} hosted exactly once: {:?}",
            both_back.workspaces
        );
        assert_eq!(rows[0].output.0, output, "{:?}", rows[0]);
        assert_eq!(rows[0].tiling_mode, tiling, "{:?}", rows[0]);
    }
    let u_rows: Vec<_> = both_back
        .workspaces
        .iter()
        .filter(|row| row.id == view_u)
        .collect();
    assert_eq!(
        u_rows.len(),
        1,
        "viewed U unit hosted once: {:?}",
        both_back.workspaces
    );
    assert_eq!(u_rows[0].output.0, "out-u", "{:?}", u_rows[0]);
    let rows: Vec<_> = both_back
        .windows
        .iter()
        .filter(|row| ["win-s", "win-t", "win-u"].contains(&row.lifetime.0.as_str()))
        .collect();
    assert_eq!(rows.len(), 3, "every member exactly homed once: {rows:?}");
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-s"
            && row.output.0 == "out-s2"
            && row.workspace == view_s),
        "S member rides its exact unit home: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-t"
            && row.output.0 == "out-t2"
            && row.workspace == view_t),
        "T member stays home after S returns: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-u"
            && row.output.0 == "out-u"
            && row.workspace == view_u),
        "U member stays on its unit: {rows:?}"
    );
    assert!(
        both_back.effects.is_empty(),
        "returns mint no focus: {:?}",
        both_back.effects
    );
    assert!(
        both_back
            .transfers
            .iter()
            .all(|t| !t.operation_id.starts_with("op-hp-")),
        "no pull-back flights: {:?}",
        both_back.transfers
    );
}

#[test]
fn physical_chain_return_simultaneous_partition_keeps_exact_affinity() {
    // S -> T -> U chain, then S and T return together in one report on
    // renamed handles. Each viewed unit must land once on its original
    // panel; the batch must not let one returner yank the other's unit.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-s", rect(0, 0, 800, 600), tuple_only("s")),
                screen_ident("out-t", rect(800, 0, 800, 600), tuple_only("t")),
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
            ],
            vec![
                fact("win-s", "out-s", 0),
                fact("win-t", "out-t", 800),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    let view_s = output_view(&first, "out-s").expect("view s");
    let view_t = output_view(&first, "out-t").expect("view t");
    let view_u = output_view(&first, "out-u").expect("view u");
    let tiling_s = first
        .workspaces
        .iter()
        .find(|row| row.id == view_s)
        .map(|row| row.tiling_mode)
        .expect("tiling s");
    let tiling_t = first
        .workspaces
        .iter()
        .find(|row| row.id == view_t)
        .map(|row| row.tiling_mode)
        .expect("tiling t");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                screen_ident("out-t", rect(800, 0, 800, 600), tuple_only("t")),
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
            ],
            vec![
                fact("win-s", "out-t", 800),
                fact("win-t", "out-t", 900),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    let _ = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen_ident(
                "out-u",
                rect(1600, 0, 800, 600),
                tuple_only("u"),
            )],
            vec![
                fact("win-s", "out-u", 1600),
                fact("win-t", "out-u", 1700),
                fact("win-u", "out-u", 1800),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine
            .displaced
            .get("out-t")
            .map(|entry| entry.workspaces.contains(&view_s))
            .unwrap_or(false),
        "chained T record overlaps the S unit: {:?}",
        engine.displaced
    );
    let both_back = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
                screen_ident("out-s2", rect(0, 0, 800, 600), tuple_only("s")),
                screen_ident("out-t2", rect(800, 0, 800, 600), tuple_only("t")),
            ],
            vec![
                fact("win-s", "out-s2", 0),
                fact("win-t", "out-t2", 800),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine.displaced.is_empty(),
        "both origins reclaim: {:?}",
        engine.displaced
    );
    for (id, output, tiling) in [(&view_s, "out-s2", tiling_s), (&view_t, "out-t2", tiling_t)] {
        let rows: Vec<_> = both_back
            .workspaces
            .iter()
            .filter(|row| &row.id == id)
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "unit {id} hosted exactly once: {:?}",
            both_back.workspaces
        );
        assert_eq!(rows[0].output.0, output, "{:?}", rows[0]);
        assert_eq!(rows[0].tiling_mode, tiling, "{:?}", rows[0]);
    }
    let u_rows: Vec<_> = both_back
        .workspaces
        .iter()
        .filter(|row| row.id == view_u)
        .collect();
    assert_eq!(
        u_rows.len(),
        1,
        "viewed U unit hosted once: {:?}",
        both_back.workspaces
    );
    assert_eq!(u_rows[0].output.0, "out-u", "{:?}", u_rows[0]);
    let rows: Vec<_> = both_back
        .windows
        .iter()
        .filter(|row| ["win-s", "win-t", "win-u"].contains(&row.lifetime.0.as_str()))
        .collect();
    assert_eq!(rows.len(), 3, "every member exactly homed once: {rows:?}");
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-s"
            && row.output.0 == "out-s2"
            && row.workspace == view_s),
        "S member rides its exact unit home: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-t"
            && row.output.0 == "out-t2"
            && row.workspace == view_t),
        "T member rides its exact unit home: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-u"
            && row.output.0 == "out-u"
            && row.workspace == view_u),
        "U member stays on its unit: {rows:?}"
    );
    assert!(
        both_back.effects.is_empty(),
        "returns mint no focus: {:?}",
        both_back.effects
    );
    assert!(
        both_back
            .transfers
            .iter()
            .all(|t| !t.operation_id.starts_with("op-hp-")),
        "no pull-back flights: {:?}",
        both_back.transfers
    );
}

#[test]
fn physical_chain_return_simultaneous_reverse_order_keeps_exact_affinity() {
    // B -> A -> U chain, then A and B return together in one report on
    // renamed handles. The chained A record overlaps the B unit but owns
    // only its own; the batch processes A first (lexical order), so the
    // returner must not yank the other's unit in either order.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-b", rect(0, 0, 800, 600), tuple_only("b")),
                screen_ident("out-a", rect(800, 0, 800, 600), tuple_only("a")),
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
            ],
            vec![
                fact("win-b", "out-b", 0),
                fact("win-a", "out-a", 800),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    let view_b = output_view(&first, "out-b").expect("view b");
    let view_a = output_view(&first, "out-a").expect("view a");
    let view_u = output_view(&first, "out-u").expect("view u");
    // Initial ring: the viewed unit plus its trailing spare, both
    // originally B's.
    let mut ring_b: Vec<_> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-b")
        .collect();
    ring_b.sort_by_key(|row| row.order);
    let ring_b: Vec<String> = ring_b.into_iter().map(|row| row.id.clone()).collect();
    let tiling_b = first
        .workspaces
        .iter()
        .find(|row| row.id == view_b)
        .map(|row| row.tiling_mode)
        .expect("tiling b");
    let tiling_a = first
        .workspaces
        .iter()
        .find(|row| row.id == view_a)
        .map(|row| row.tiling_mode)
        .expect("tiling a");
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                screen_ident("out-a", rect(800, 0, 800, 600), tuple_only("a")),
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
            ],
            vec![
                fact("win-b", "out-a", 800),
                fact("win-a", "out-a", 900),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    let _ = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen_ident(
                "out-u",
                rect(1600, 0, 800, 600),
                tuple_only("u"),
            )],
            vec![
                fact("win-b", "out-u", 1600),
                fact("win-a", "out-u", 1700),
                fact("win-u", "out-u", 1800),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine
            .displaced
            .get("out-a")
            .map(|entry| entry.workspaces.contains(&view_b))
            .unwrap_or(false),
        "chained A record overlaps the B unit: {:?}",
        engine.displaced
    );
    assert_eq!(
        engine
            .displaced
            .get("out-b")
            .map(|entry| entry.owned.clone())
            .unwrap_or_default(),
        ring_b,
        "B owns its original ring: {:?}",
        engine.displaced
    );
    assert_eq!(
        engine
            .displaced
            .get("out-a")
            .map(|entry| entry.owned.clone())
            .unwrap_or_default(),
        vec![view_a.clone()],
        "A owns only its original unit: {:?}",
        engine.displaced
    );
    let both_back = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
                screen_ident("out-b2", rect(0, 0, 800, 600), tuple_only("b")),
                screen_ident("out-a2", rect(800, 0, 800, 600), tuple_only("a")),
            ],
            vec![
                fact("win-b", "out-b2", 0),
                fact("win-a", "out-a2", 800),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine.displaced.is_empty(),
        "both origins reclaim: {:?}",
        engine.displaced
    );
    for (id, output, tiling) in [(&view_b, "out-b2", tiling_b), (&view_a, "out-a2", tiling_a)] {
        let rows: Vec<_> = both_back
            .workspaces
            .iter()
            .filter(|row| &row.id == id)
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "unit {id} hosted exactly once: {:?}",
            both_back.workspaces
        );
        assert_eq!(rows[0].output.0, output, "{:?}", rows[0]);
        assert_eq!(rows[0].tiling_mode, tiling, "{:?}", rows[0]);
    }
    let u_rows: Vec<_> = both_back
        .workspaces
        .iter()
        .filter(|row| row.id == view_u)
        .collect();
    assert_eq!(
        u_rows.len(),
        1,
        "viewed U unit hosted once: {:?}",
        both_back.workspaces
    );
    assert_eq!(u_rows[0].output.0, "out-u", "{:?}", u_rows[0]);
    let rows: Vec<_> = both_back
        .windows
        .iter()
        .filter(|row| ["win-b", "win-a", "win-u"].contains(&row.lifetime.0.as_str()))
        .collect();
    assert_eq!(rows.len(), 3, "every member exactly homed once: {rows:?}");
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-b"
            && row.output.0 == "out-b2"
            && row.workspace == view_b),
        "B member rides its exact unit home: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-a"
            && row.output.0 == "out-a2"
            && row.workspace == view_a),
        "A member rides its exact unit home: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-u"
            && row.output.0 == "out-u"
            && row.workspace == view_u),
        "U member stays on its unit: {rows:?}"
    );
    assert!(
        both_back.effects.is_empty(),
        "returns mint no focus: {:?}",
        both_back.effects
    );
    assert!(
        both_back
            .transfers
            .iter()
            .all(|t| !t.operation_id.starts_with("op-hp-")),
        "no pull-back flights: {:?}",
        both_back.transfers
    );
}

#[test]
fn physical_chain_return_moved_out_member_stays_away() {
    // S -> T -> U chain, then win-s explicitly moves out of its unit onto
    // the survivor's own unit while displaced. Both origins return together
    // in one report: the S unit returns with then-current contents (without
    // the moved member) and no return pulls the member back.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                screen_ident("out-s", rect(0, 0, 800, 600), tuple_only("s")),
                screen_ident("out-t", rect(800, 0, 800, 600), tuple_only("t")),
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
            ],
            vec![
                fact("win-s", "out-s", 0),
                fact("win-t", "out-t", 800),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    let view_s = output_view(&first, "out-s").expect("view s");
    let view_t = output_view(&first, "out-t").expect("view t");
    let view_u = output_view(&first, "out-u").expect("view u");
    // Initial ring: the viewed unit plus its trailing spare, both
    // originally S's.
    let mut ring_s: Vec<_> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-s")
        .collect();
    ring_s.sort_by_key(|row| row.order);
    let ring_s: Vec<String> = ring_s.into_iter().map(|row| row.id.clone()).collect();
    let tiling_t = first
        .workspaces
        .iter()
        .find(|row| row.id == view_t)
        .map(|row| row.tiling_mode)
        .expect("tiling t");
    let spare_s = ring_s
        .iter()
        .find(|id| *id != &view_s)
        .expect("S trailing spare")
        .clone();
    let _ = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                screen_ident("out-t", rect(800, 0, 800, 600), tuple_only("t")),
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
            ],
            vec![
                fact("win-s", "out-t", 800),
                fact("win-t", "out-t", 900),
                fact("win-u", "out-u", 1600),
            ],
            "",
            None,
            None,
        ),
    );
    let _ = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen_ident(
                "out-u",
                rect(1600, 0, 800, 600),
                tuple_only("u"),
            )],
            vec![
                fact("win-s", "out-u", 1600),
                fact("win-t", "out-u", 1700),
                fact("win-u", "out-u", 1800),
            ],
            "",
            None,
            None,
        ),
    );
    assert_eq!(
        engine
            .displaced
            .get("out-s")
            .map(|entry| entry.owned.clone())
            .unwrap_or_default(),
        ring_s,
        "S owns its original ring: {:?}",
        engine.displaced
    );
    assert_eq!(
        engine
            .displaced
            .get("out-t")
            .map(|entry| entry.owned.clone())
            .unwrap_or_default(),
        vec![view_t.clone()],
        "T owns only its original unit: {:?}",
        engine.displaced
    );
    // Explicit move-out while displaced: win-s leaves its unit for the
    // survivor's own unit through the ordinary send route.
    let moved = match engine.handle_report(&report(
        4,
        4,
        vec![screen_ident(
            "out-u",
            rect(1600, 0, 800, 600),
            tuple_only("u"),
        )],
        vec![
            fact("win-s", "out-u", 1600),
            fact("win-t", "out-u", 1700),
            fact("win-u", "out-u", 1800),
        ],
        "win-s",
        Some(11),
        Some(WorldCommand::Operation(CoreCommand::SendToWorkspace {
            window: "win-s".to_owned(),
            target_output: "out-u".to_owned(),
            target_workspace: view_u.clone(),
            follow: false,
        })),
    )) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("explicit move-out must accept, got {other:?}"),
    };
    let transfer = moved
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-s"))
        .expect("explicit move-out retains a window transfer");
    assert_eq!(transfer.target_output.0, "out-u", "{transfer:?}");
    assert_eq!(transfer.target_workspace, view_u, "{transfer:?}");
    assert_eq!(row_of(&moved, "win-s").output.0, "out-u");
    assert_eq!(row_of(&moved, "win-s").workspace, view_u);
    let both_back = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![
                screen_ident("out-u", rect(1600, 0, 800, 600), tuple_only("u")),
                screen_ident("out-s2", rect(0, 0, 800, 600), tuple_only("s")),
                screen_ident("out-t2", rect(800, 0, 800, 600), tuple_only("t")),
            ],
            vec![
                fact("win-s", "out-u", 1600),
                fact("win-t", "out-t2", 800),
                fact("win-u", "out-u", 1800),
            ],
            "",
            None,
            None,
        ),
    );
    assert!(
        engine.displaced.is_empty(),
        "both origins reclaim: {:?}",
        engine.displaced
    );
    // The emptied S unit dissolves through ordinary trailing lifecycle
    // (unviewed and unoccupied after the move-out); affinity is proven by
    // its spare coming home, its members staying put, and no pull-back
    // flight. The unit itself must never surface on another host.
    let s_rows: Vec<_> = both_back
        .workspaces
        .iter()
        .filter(|row| row.id == view_s)
        .collect();
    assert!(
        s_rows.iter().all(|row| row.output.0 == "out-s2"),
        "S unit never yanked elsewhere: {:?}",
        both_back.workspaces
    );
    let spare_rows: Vec<_> = both_back
        .workspaces
        .iter()
        .filter(|row| row.id == *spare_s)
        .collect();
    assert_eq!(
        spare_rows.len(),
        1,
        "S spare hosted exactly once: {:?}",
        both_back.workspaces
    );
    assert_eq!(spare_rows[0].output.0, "out-s2", "{:?}", spare_rows[0]);
    {
        let (id, output, tiling) = (&view_t, "out-t2", tiling_t);
        let rows: Vec<_> = both_back
            .workspaces
            .iter()
            .filter(|row| &row.id == id)
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "unit {id} hosted exactly once: {:?}",
            both_back.workspaces
        );
        assert_eq!(rows[0].output.0, output, "{:?}", rows[0]);
        assert_eq!(rows[0].tiling_mode, tiling, "{:?}", rows[0]);
    }
    let u_rows: Vec<_> = both_back
        .workspaces
        .iter()
        .filter(|row| row.id == view_u)
        .collect();
    assert_eq!(
        u_rows.len(),
        1,
        "viewed U unit hosted once: {:?}",
        both_back.workspaces
    );
    assert_eq!(u_rows[0].output.0, "out-u", "{:?}", u_rows[0]);
    let rows: Vec<_> = both_back
        .windows
        .iter()
        .filter(|row| ["win-s", "win-t", "win-u"].contains(&row.lifetime.0.as_str()))
        .collect();
    assert_eq!(rows.len(), 3, "every member exactly homed once: {rows:?}");
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-s"
            && row.output.0 == "out-u"
            && row.workspace == view_u),
        "moved-out member is not pulled back: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-t"
            && row.output.0 == "out-t2"
            && row.workspace == view_t),
        "T member rides its exact unit home: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.lifetime.0 == "win-u"
            && row.output.0 == "out-u"
            && row.workspace == view_u),
        "U member stays on its unit: {rows:?}"
    );
    assert!(
        both_back
            .windows
            .iter()
            .filter(|row| row.lifetime.0 == "win-s")
            .all(|row| row.workspace != view_s),
        "S unit returns with then-current contents: {:?}",
        both_back.windows
    );
    assert!(
        both_back.effects.is_empty(),
        "returns mint no focus: {:?}",
        both_back.effects
    );
    assert!(
        both_back
            .transfers
            .iter()
            .all(|t| !t.operation_id.starts_with("op-hp-")),
        "no pull-back flights: {:?}",
        both_back.transfers
    );
}
