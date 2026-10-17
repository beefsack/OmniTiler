//! Contract-repair coverage for the world boundary: staged incarnation
//! resets, cross-output operation routes (directional pair, migration,
//! release, drag source), and minimized lifetime rows at the typed level.
//! JSON codec and service gating live in their own crates; this file stays
//! typed-only and portable.

use std::collections::BTreeSet;

use tiler_core::boundary::CoreCommand;
use tiler_core::directional::{Direction, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::{DomainKey, OutputDomain};
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    DomainReport, HoldReason, OutputTopology, TransferKind, WORLD_CONTRACT_VERSION, WorldCommand,
    WorldReport,
};

fn rect() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    }
}

fn win(id: &str, output: &str, workspace: &str) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        },
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
    output: &str,
    workspace: &str,
    adjacent: Vec<(Direction, &str)>,
    windows: Vec<EngineWindow>,
    focused: &str,
) -> DomainReport {
    DomainReport {
        domain: OutputDomain {
            id: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
            bounds: rect(),
            gap: 8,
            adjacent: adjacent
                .into_iter()
                .map(|(d, o)| (d, OutputId(o.to_owned())))
                .collect(),
        },
        outer_gap: 4,
        focused: WindowId(focused.to_owned()),
        windows,
        closed: Vec::new(),
        readable: true,
        hold: None,
    }
}

fn topology(output: &str, order: Vec<&str>, current: &str) -> OutputTopology {
    OutputTopology {
        output: OutputId(output.to_owned()),
        native_order: order.into_iter().map(str::to_owned).collect(),
        current_native: current.to_owned(),
        complete: true,
        geometry: None,
        primary: false,
    }
}

/// Two-output world: win-a on out-1/ws1, win-b on out-2/ws1, mutual
/// left/right adjacency. Single shared workspace id keeps the directional
/// counterpart resolvable.
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
            topology("out-1", vec!["ws1"], "ws1"),
            topology("out-2", vec!["ws1"], "ws1"),
        ],
        bindings: Vec::new(),
        domains: vec![
            domain(
                "out-1",
                "ws1",
                vec![(Direction::Right, "out-2")],
                vec![win("win-a", "out-1", "ws1")],
                "win-a",
            ),
            domain(
                "out-2",
                "ws1",
                vec![(Direction::Left, "out-1")],
                vec![win("win-b", "out-2", "ws1")],
                "win-b",
            ),
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
fn new_incarnation_rejection_leaves_old_state_intact() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let first = two_output_report(1, 1);
    let reply = accepted(&mut engine, &first);
    assert_eq!(reply.state_revision, 1);
    // New incarnation but invalid payload: staged reset must reject without
    // touching the previous incarnation's ordering or caches.
    let mut bad = two_output_report(1, 1);
    bad.adapter_incarnation = "adapter-2".to_owned();
    bad.owner = "bad owner".to_owned();
    // Shape validation runs before identity parsing, so a non-opaque
    // owner refuses as snapshot-invalid; the transactional point (old
    // state intact) is identical either way.
    match engine.handle_report(&bad) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "snapshot-invalid"),
        other => panic!("expected owner rejection, got {other:?}"),
    }
    // Old incarnation state is intact: the exact first report duplicates.
    match engine.handle_report(&first) {
        WorldOutcome::Duplicate(reply) => assert_eq!(reply.state_revision, 1),
        other => panic!("expected duplicate, got {other:?}"),
    }
    // A valid report under the new incarnation restarts the sequence
    // namespace (request_seq 1 classifies as new, never superseded).
    let mut migrated = two_output_report(1, 2);
    migrated.adapter_incarnation = "adapter-2".to_owned();
    let reply = accepted(&mut engine, &migrated);
    assert_eq!(reply.state_revision, 2);
    assert_eq!(reply.adapter_incarnation, "adapter-2");
}

#[test]
fn cross_output_move_without_adjacency_refuses_instead_of_degrading() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &two_output_report(1, 1));
    // Strip adjacency: no resolvable target. The request refuses; it never
    // silently runs as a local move.
    let mut report = two_output_report(2, 2);
    for d in &mut report.domains {
        d.domain.adjacent.clear();
    }
    with_command(
        &mut report,
        2,
        11,
        WorldCommand::Operation(CoreCommand::Move {
            window: "win-a".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: true,
            same_axis_move: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        }),
    );
    match engine.handle_report(&report) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "operation-refused"),
        other => panic!("expected operation refusal, got {other:?}"),
    }
}

#[test]
fn cross_output_focus_reaches_engine_through_report_pair() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &two_output_report(1, 1));
    let mut report = two_output_report(2, 2);
    with_command(
        &mut report,
        2,
        12,
        WorldCommand::Operation(CoreCommand::Focus {
            window: "win-a".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: true,
            float_subject: false,
        }),
    );
    // The report-derived pair routes to the engine: a one-shot focus
    // effect lands on the cross-output target (never a wholesale refusal
    // and never a silent local focus).
    match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => {
            assert_eq!(reply.effects.len(), 1);
            assert_eq!(reply.effects[0].target.0, "win-b");
        }
        other => panic!("expected cross-output focus effect, got {other:?}"),
    }
}

#[test]
fn operation_migration_with_adjacency_retains_transfer() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    // Global-unique mapping so the scope policy can migrate ws-b from
    // out-1 to out-2 (ws-c is out-2's current).
    let base = WorldReport {
        workspace_mode: WorkspaceMode::GlobalUnique,
        topology: vec![
            topology("out-1", vec!["ws-a", "ws-b"], "ws-a"),
            topology("out-2", vec!["ws-c"], "ws-c"),
        ],
        bindings: vec![
            tiler_core::world_report::WorkspaceBinding {
                logical_id: "desk-a".to_owned(),
                native_id: "ws-a".to_owned(),
                output: OutputId("out-1".to_owned()),
                pinned: false,
                displaced: false,
                managed: true,
            },
            tiler_core::world_report::WorkspaceBinding {
                logical_id: "desk-b".to_owned(),
                native_id: "ws-b".to_owned(),
                output: OutputId("out-1".to_owned()),
                pinned: false,
                displaced: false,
                managed: true,
            },
            tiler_core::world_report::WorkspaceBinding {
                logical_id: "desk-c".to_owned(),
                native_id: "ws-c".to_owned(),
                output: OutputId("out-2".to_owned()),
                pinned: false,
                displaced: false,
                managed: true,
            },
        ],
        domains: vec![
            domain(
                "out-1",
                "ws-a",
                vec![(Direction::Right, "out-2")],
                vec![win("win-a", "out-1", "ws-a")],
                "win-a",
            ),
            domain(
                "out-1",
                "ws-b",
                vec![(Direction::Right, "out-2")],
                vec![win("win-c", "out-1", "ws-b")],
                "win-c",
            ),
            domain(
                "out-2",
                "ws-c",
                vec![(Direction::Left, "out-1")],
                vec![win("win-b", "out-2", "ws-c")],
                "win-b",
            ),
        ],
        ..two_output_report(1, 1)
    };
    accepted(&mut engine, &base);
    // Migrate ws-b rightwards from its homing domain's adjacency.
    let mut migrate = base.clone();
    // Focus the command on the ws-b domain: drop win-a's domain from the
    // homing lookup is unnecessary; operation_domain falls back to the
    // first readable domain, so scope the fixture to ws-b only.
    migrate.domains.remove(0);
    migrate.request_seq = 2;
    migrate.request_id = "req-2".to_owned();
    migrate.observation_seq = 2;
    with_command(
        &mut migrate,
        2,
        21,
        WorldCommand::Operation(CoreCommand::MigrateWorkspace {
            direction: "right".to_owned(),
        }),
    );
    match engine.handle_report(&migrate) {
        WorldOutcome::Accepted(reply) => {
            assert!(
                reply
                    .transfers
                    .iter()
                    .any(|t| matches!(t.kind, TransferKind::Migration)
                        && t.target_workspace == "ws-b"
                        && t.target_output.0 == "out-2"),
                "migration retained: {:?}",
                reply.transfers
            );
        }
        other => panic!("expected accepted migration, got {other:?}"),
    }
}

#[test]
fn operation_release_drops_retained_session() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &two_output_report(1, 1));
    let key = DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws1".to_owned()),
    };
    assert!(engine.engine().contains(&key));
    let mut report = two_output_report(2, 2);
    with_command(
        &mut report,
        2,
        22,
        WorldCommand::Operation(CoreCommand::ReleaseDomain),
    );
    accepted(&mut engine, &report);
    assert!(!engine.engine().contains(&key));
}

#[test]
fn drag_preview_same_domain_routes_without_wholesale_refusal() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &two_output_report(1, 1));
    let mut report = two_output_report(2, 2);
    with_command(
        &mut report,
        2,
        23,
        WorldCommand::Operation(CoreCommand::DragPreview {
            window: "win-a".to_owned(),
            x: 400,
            y: 300,
            hover_prior: None,
            source: Some(DomainKey {
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws1".to_owned()),
            }),
        }),
    );
    // Same-domain source keeps the legacy singleton route: the engine
    // decides (preview or layout refusal), never the wholesale
    // operation refusal.
    match engine.handle_report(&report) {
        WorldOutcome::Accepted(_) => {}
        WorldOutcome::Rejected { kind, .. } => {
            assert_ne!(kind, "operation-refused", "legacy route must run");
        }
        other => panic!("unexpected outcome {other:?}"),
    }
}

#[test]
fn held_scope_rows_carry_no_write_authority() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &two_output_report(1, 1));
    let mut report = two_output_report(2, 2);
    report.domains[1].readable = false;
    report.domains[1].hold = Some(HoldReason::Unreadable);
    report.domains[1].windows.clear();
    let reply = accepted(&mut engine, &report);
    let rows: Vec<_> = reply
        .windows
        .iter()
        .filter(|w| w.lifetime.0 == "win-b")
        .collect();
    assert_eq!(rows.len(), 1, "exactly one homing for win-b");
    assert_eq!(
        rows[0].frame_authority.hold_reason(),
        Some(HoldReason::Unreadable)
    );
    assert!(rows[0].allocation.is_none());
    // No live tiled row shadows the hold.
    let _ = BTreeSet::<String>::new();
}

fn three_workspace_report(request_seq: u64, observation_seq: u64) -> WorldReport {
    let mut report = two_output_report(request_seq, observation_seq);
    report.workspace_mode = WorkspaceMode::PerOutputLocal;
    report.topology = vec![topology("out-1", vec!["ws1", "ws2", "ws3"], "ws1")];
    report.bindings = Vec::new();
    report.domains = vec![
        domain(
            "out-1",
            "ws1",
            vec![],
            vec![win("win-a", "out-1", "ws1")],
            "win-a",
        ),
        domain(
            "out-1",
            "ws2",
            vec![],
            vec![win("win-b", "out-1", "ws2")],
            "win-b",
        ),
        domain("out-1", "ws3", vec![], Vec::new(), ""),
    ];
    report
}

#[test]
fn tiling_toggle_releases_and_readopts_without_frame_writes() {
    use tiler_core::world_report::{ScopeAuthority, TilingMode};
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &two_output_report(1, 1));
    // Disable tiling for the live workspace: planner-owned, core-side.
    let mut toggle = two_output_report(2, 2);
    with_command(
        &mut toggle,
        2,
        31,
        WorldCommand::SetWorkspaceTiling {
            output: OutputId("out-1".to_owned()),
            workspace: "ws1".to_owned(),
            tiled: false,
        },
    );
    let reply = accepted(&mut engine, &toggle);
    let scope = reply
        .scopes
        .iter()
        .find(|s| s.scope == "out-1/ws1")
        .expect("floating scope row");
    assert_eq!(scope.authority, ScopeAuthority::Untouched);
    let row = reply
        .workspaces
        .iter()
        .find(|w| w.id == "ws1" && w.output.0 == "out-1")
        .expect("floating workspace row");
    assert_eq!(row.tiling_mode, TilingMode::Floating);
    // The output row carries the floating mode of its current view.
    let output_row = reply
        .outputs
        .iter()
        .find(|o| o.output.0 == "out-1")
        .expect("out-1 output row");
    assert_eq!(output_row.tiling_mode, TilingMode::Floating);
    // No frame writes while floating: homed lifetimes carry explicit
    // untouched rows (never silent absence), and the retained session is
    // released.
    let floated_rows: Vec<_> = reply
        .windows
        .iter()
        .filter(|w| w.output.0 == "out-1" && w.workspace == "ws1")
        .collect();
    assert!(
        !floated_rows.is_empty(),
        "floating workspaces publish explicit untouched rows"
    );
    assert!(
        floated_rows.iter().all(|w| {
            w.allocation.is_none()
                && w.effective_frame.is_none()
                && w.frame_authority == ScopeAuthority::Untouched
                && w.membership_authority == ScopeAuthority::Untouched
        }),
        "floating rows authorize nothing"
    );
    let key = DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws1".to_owned()),
    };
    assert!(!engine.engine().contains(&key));
    // Boundary send touching the floating scope transfers project
    // membership without a two-domain Engine plan: the transfer retains
    // with a membership-only row at the tiled target, and no floating
    // frame is written.
    let mut send = two_output_report(3, 2);
    with_command(
        &mut send,
        3,
        32,
        WorldCommand::Send {
            window: WindowId("win-a".to_owned()),
            target_output: OutputId("out-2".to_owned()),
            target_workspace: "ws1".to_owned(),
            follow: false,
        },
    );
    let reply = accepted(&mut engine, &send);
    assert_eq!(reply.transfers.len(), 1);
    assert_eq!(reply.transfers[0].operation_id, "op-32");
    // Only the tiled side reconciles: the first reply carries the mover
    // at the target homing with membership only (no Engine plan ran, so
    // the tiled target converges on later reports), while no
    // floating-side frame is written.
    let moved = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .expect("boundary mover row at the tiled target");
    assert_eq!(
        format!("{}/{}", moved.output.0, moved.workspace),
        "out-2/ws1"
    );
    assert_eq!(moved.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(moved.frame_authority, ScopeAuthority::Untouched);
    assert!(moved.allocation.is_none());
    assert!(
        reply
            .windows
            .iter()
            .filter(|w| w.output.0 == "out-1" && w.workspace == "ws1")
            .all(|w| w.frame_authority == ScopeAuthority::Untouched
                && w.membership_authority == ScopeAuthority::Untouched),
        "no tiled-side geometry on floats"
    );
    // Observation confirms the boundary transfer: native membership moved
    // it, so the transfer settles with no tiled-side geometry on floats.
    let mut confirmed = two_output_report(4, 3);
    confirmed.domains[0].windows.clear();
    confirmed.domains[0].focused = WindowId(String::new());
    confirmed.domains[1]
        .windows
        .push(win("win-a", "out-2", "ws1"));
    let settled = accepted(&mut engine, &confirmed);
    assert!(settled.transfers.is_empty());
    // Re-enable: the next report re-adopts through the fresh route with
    // full frame authority.
    let mut retile = two_output_report(5, 4);
    with_command(
        &mut retile,
        5,
        33,
        WorldCommand::SetWorkspaceTiling {
            output: OutputId("out-1".to_owned()),
            workspace: "ws1".to_owned(),
            tiled: true,
        },
    );
    let reply = accepted(&mut engine, &retile);
    let scope = reply
        .scopes
        .iter()
        .find(|s| s.scope == "out-1/ws1")
        .expect("retiled scope row");
    assert_eq!(scope.authority, ScopeAuthority::Controlled);
    let floated = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .expect("win-a row after retile");
    assert_eq!(floated.membership_authority, ScopeAuthority::Controlled);
    assert!(floated.allocation.is_some());
    // Unknown workspaces refuse without mutation.
    let mut bad = two_output_report(6, 4);
    with_command(
        &mut bad,
        6,
        34,
        WorldCommand::SetWorkspaceTiling {
            output: OutputId("out-1".to_owned()),
            workspace: "nope".to_owned(),
            tiled: false,
        },
    );
    match engine.handle_report(&bad) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "tiling-target-unknown"),
        other => panic!("expected tiling-target-unknown, got {other:?}"),
    }
}

#[test]
fn predicate_ingress_selects_admission_without_reclassification() {
    use tiler_core::size_hints::{FixedSizePredicate, WindowSizeHints};
    fn fixed_width_report(predicate: FixedSizePredicate) -> WorldReport {
        let mut report = two_output_report(1, 1);
        let mut fixed = win("win-f", "out-1", "ws1");
        // Width-only pin (height present but unequal): fixed under
        // either-axis, tiled under both-axes. Only the final equality is
        // predicate-selected.
        fixed.hints = WindowSizeHints {
            min_w: Some(200),
            max_w: Some(200),
            min_h: Some(100),
            max_h: Some(150),
        };
        report.domains[0].windows.push(fixed);
        report.fixed_size_predicate = predicate;
        report
    }
    // Either-axis ingress floats a width-only fixed window (membership
    // only, never a frame write).
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = accepted(
        &mut engine,
        &fixed_width_report(FixedSizePredicate::EitherAxis),
    );
    assert_eq!(
        engine.engine().fixed_size_predicate(),
        FixedSizePredicate::EitherAxis
    );
    let row = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-f")
        .expect("win-f row");
    assert!(row.allocation.is_none());
    assert_eq!(
        row.frame_authority,
        tiler_core::world_report::ScopeAuthority::Untouched
    );
    // Both-axes ingress tiles the very same observation.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = accepted(
        &mut engine,
        &fixed_width_report(FixedSizePredicate::BothAxes),
    );
    let row = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-f")
        .expect("win-f row");
    assert!(row.allocation.is_some());
}

#[test]
fn send_numbered_positional_never_view_switches() {
    for follow in [true, false] {
        let mut engine = WorldEngine::new("gen-1").expect("valid");
        accepted(&mut engine, &three_workspace_report(1, 1));
        let mut send = three_workspace_report(2, 2);
        with_command(
            &mut send,
            2,
            41,
            WorldCommand::SendNumbered {
                window: WindowId("win-a".to_owned()),
                output: OutputId("out-1".to_owned()),
                index: 2,
                follow,
            },
        );
        let reply = accepted(&mut engine, &send);
        assert!(
            matches!(
                reply.transfers.as_slice(),
                [t] if matches!(t.kind, TransferKind::Window(_))
                    && t.target_workspace == "ws2"
                    && t.follow == follow
            ),
            "follow={follow}: positional send to ws2, no view target: {:?}",
            reply.transfers
        );
        // Actuation lag: identical observation keeps the window transfer
        // with still no view switch.
        let lagged = accepted(&mut engine, &three_workspace_report(3, 2));
        assert_eq!(lagged.transfers.len(), 1);
        assert!(
            lagged
                .transfers
                .iter()
                .all(|t| matches!(t.kind, TransferKind::Window(_))),
            "follow={follow}: lagged transfers stay window-only",
            follow = follow,
        );
        // Confirmed by observation: settled with no view effect.
        let mut confirmed = three_workspace_report(4, 3);
        confirmed.domains[0].windows.clear();
        confirmed.domains[0].focused = WindowId(String::new());
        confirmed.domains[1].windows =
            vec![win("win-b", "out-1", "ws2"), win("win-a", "out-1", "ws2")];
        let settled = accepted(&mut engine, &confirmed);
        assert!(settled.transfers.is_empty());
        assert!(settled.effects.is_empty());
    }
    // Out-of-ring positions refuse without mutation.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &three_workspace_report(1, 1));
    let mut bad = three_workspace_report(2, 2);
    with_command(
        &mut bad,
        2,
        43,
        WorldCommand::SendNumbered {
            window: WindowId("win-a".to_owned()),
            output: OutputId("out-1".to_owned()),
            index: 9,
            follow: false,
        },
    );
    match engine.handle_report(&bad) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-target-unknown"),
        other => panic!("expected send-target-unknown, got {other:?}"),
    }
    // Relative sends resolve through the same ring with the same
    // never-switch guarantee: win-b on ws2 steps forward into ws3.
    let mut relative = three_workspace_report(2, 2);
    with_command(
        &mut relative,
        2,
        44,
        WorldCommand::SendRelative {
            window: WindowId("win-b".to_owned()),
            output: OutputId("out-1".to_owned()),
            delta: 1,
            follow: false,
        },
    );
    let reply = accepted(&mut engine, &relative);
    assert!(
        matches!(
            reply.transfers.as_slice(),
            [t] if matches!(t.kind, TransferKind::Window(_)) && t.target_workspace == "ws3"
        ),
        "relative send resolves through the ring: {:?}",
        reply.transfers
    );
}

#[test]
fn send_numbered_trailing_create_pends_until_bound() {
    use tiler_core::world_report::ScopeAuthority;
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &two_output_report(1, 1));
    // Both ring slots occupied: index 0 mints a stable token and pends
    // the send with no view target and no token-as-native write.
    let mut numbered = two_output_report(2, 2);
    with_command(
        &mut numbered,
        2,
        51,
        WorldCommand::SendNumbered {
            window: WindowId("win-a".to_owned()),
            output: OutputId("out-1".to_owned()),
            index: 0,
            follow: false,
        },
    );
    let reply = accepted(&mut engine, &numbered);
    assert!(
        reply.transfers.is_empty(),
        "unbound send publishes no transfer yet: {:?}",
        reply.transfers
    );
    let token = reply
        .workspaces
        .iter()
        .find(|w| w.native_binding.is_none())
        .expect("unbound pending row")
        .id
        .clone();
    assert!(token.starts_with("wl-"));
    // The binding names the native id: the engine send starts now under
    // the original stable operation identity, still with no view switch.
    let mut bound = two_output_report(3, 3);
    bound.topology[0].native_order.push("ws3".to_owned());
    bound
        .bindings
        .push(tiler_core::world_report::WorkspaceBinding {
            logical_id: token.clone(),
            native_id: "ws3".to_owned(),
            output: OutputId("out-1".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        });
    bound
        .domains
        .push(domain("out-1", "ws3", vec![], Vec::new(), ""));
    let reply = accepted(&mut engine, &bound);
    assert!(
        matches!(
            reply.transfers.as_slice(),
            [t] if matches!(t.kind, TransferKind::Window(_))
                && t.target_workspace == "ws3"
                && t.operation_id == "op-51"
                && !t.follow
        ),
        "bound send retains the stable window transfer: {:?}",
        reply.transfers
    );
    // Stay semantics: the source view is untouched at every step.
    assert_eq!(bound.topology[0].current_native, "ws1");
    let view = reply
        .outputs
        .iter()
        .find(|o| o.output.0 == "out-1")
        .expect("output row");
    assert_eq!(view.hold, None);
    let _ = ScopeAuthority::Controlled;
}

#[test]
fn minimized_list_is_explicit_and_keeps_occupancy() {
    use tiler_core::directional::WorkspaceId;
    use tiler_core::world_report::{MinimizedEntry, ScopeAuthority};
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &two_output_report(1, 1));
    // win-b minimizes: absent from membership with an explicit entry, no
    // explicit close. Its workspace stays occupied (retained, not removed).
    let mut minimized = two_output_report(2, 2);
    minimized.domains[1].windows.clear();
    minimized.domains[1].focused = WindowId(String::new());
    minimized.minimized = vec![MinimizedEntry {
        window: WindowId("win-b".to_owned()),
        output: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws1".to_owned()),
    }];
    let reply = accepted(&mut engine, &minimized);
    let rows: Vec<_> = reply
        .windows
        .iter()
        .filter(|w| w.lifetime.0 == "win-b")
        .collect();
    assert_eq!(rows.len(), 1, "exactly one homing for win-b");
    assert_eq!(
        rows[0].frame_authority.hold_reason(),
        Some(HoldReason::Minimized)
    );
    assert_eq!(rows[0].membership_authority, ScopeAuthority::Controlled);
    assert!(rows[0].allocation.is_none());
    assert_eq!(
        format!("{}/{}", rows[0].output.0, rows[0].workspace),
        "out-2/ws1"
    );
    // Occupancy counts the minimized lifetime: the trailing policy keeps
    // the sole-minimized workspace instead of removing it.
    let ws_row = reply
        .workspaces
        .iter()
        .find(|w| w.id == "ws1" && w.output.0 == "out-2")
        .expect("ws1 row on out-2");
    assert!(ws_row.existence, "minimized-occupied workspace is retained");
    // A repeated omission is still a minimize, never an inferred close:
    // the explicit row persists across the gap.
    let mut again = two_output_report(3, 2);
    again.domains[1].windows.clear();
    again.domains[1].focused = WindowId(String::new());
    again.minimized = minimized.minimized.clone();
    let repeat = accepted(&mut engine, &again);
    let rows: Vec<_> = repeat
        .windows
        .iter()
        .filter(|w| w.lifetime.0 == "win-b")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].frame_authority.hold_reason(),
        Some(HoldReason::Minimized)
    );
    // An explicit close of the minimized window forgets the slot: the row
    // disappears without taking the workspace down silently.
    let mut closed = two_output_report(4, 3);
    closed.domains[1].windows.clear();
    closed.domains[1].focused = WindowId(String::new());
    closed.domains[1].closed = vec![WindowId("win-b".to_owned())];
    let reply = accepted(&mut engine, &closed);
    assert!(
        reply.windows.iter().all(|w| w.lifetime.0 != "win-b"),
        "explicitly closed minimized window leaves no row"
    );
}

#[test]
fn fresh_seed_commits_complete_roster_without_hold() {
    use tiler_core::world_report::ScopeAuthority;
    fn bulk_report(count: usize, seq: u64) -> WorldReport {
        let mut report = two_output_report(seq, seq);
        report.topology = vec![topology("out-1", vec!["ws1"], "ws1")];
        report.domains = vec![DomainReport {
            domain: OutputDomain {
                id: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws1".to_owned()),
                bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 600,
                },
                gap: 8,
                adjacent: std::collections::BTreeMap::new(),
            },
            outer_gap: 4,
            focused: WindowId(String::new()),
            windows: (0..count)
                .map(|i| {
                    let mut w = win(&format!("win-{i:06}"), "out-1", "ws1");
                    w.rect = Rect {
                        x: ((i * 37) % 600) as i32,
                        y: 0,
                        w: 200,
                        h: 200,
                    };
                    w
                })
                .collect(),
            closed: Vec::new(),
            readable: true,
            hold: None,
        }];
        report
    }
    // Thirteen tiled windows seed in one full report with complete rows.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = accepted(&mut engine, &bulk_report(13, 1));
    assert_eq!(reply.windows.len(), 13);
    assert!(
        reply
            .scopes
            .iter()
            .all(|s| s.authority == ScopeAuthority::Controlled)
    );
    assert!(reply.windows.iter().all(|w| w.allocation.is_some()));
    // Fourteen overlapping observations no longer hold the whole scope
    // (best effort, selected 2026-10-17): the serial seed commits a
    // complete roster in one report with every lifetime keeping exactly
    // one row. Largest-fallback packing tiles all fourteen here; the
    // scope stays controlled with full write authority on tiles.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seeded = accepted(&mut engine, &bulk_report(14, 1));
    assert_eq!(seeded.scopes.len(), 1);
    assert_eq!(
        seeded.scopes[0].authority,
        ScopeAuthority::Controlled,
        "a committable roster never holds"
    );
    assert_eq!(
        seeded.windows.len(),
        14,
        "every observed lifetime keeps exactly one row"
    );
    let mut seeded_ids: Vec<&str> = seeded
        .windows
        .iter()
        .map(|w| w.lifetime.0.as_str())
        .collect();
    seeded_ids.sort();
    let expected: Vec<String> = (0..14).map(|i| format!("win-{i:06}")).collect();
    assert_eq!(
        seeded_ids,
        expected.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert!(
        seeded.windows.iter().all(|w| w.allocation.is_some()),
        "all fourteen tile in 800x600/gap-8 under largest fallback"
    );
    assert!(seeded.transfers.is_empty() && seeded.effects.is_empty());
    // No churn: the same observation under a fresh sequence republishes
    // identical desired rows (only the revision advances).
    let again = accepted(&mut engine, &bulk_report(14, 2));
    assert_eq!(again.state_revision, seeded.state_revision + 1);
    assert_eq!(again.scopes, seeded.scopes);
    assert_eq!(again.windows, seeded.windows);
    // Fewer windows still re-adopt fully through the next full report.
    // The dropped lifetime closes explicitly (a bare omission on a
    // retained session is a minimize, never an inferred close).
    let mut fewer = bulk_report(13, 3);
    fewer.domains[0].closed = vec![WindowId("win-000013".to_owned())];
    let recovered = accepted(&mut engine, &fewer);
    assert!(
        recovered
            .scopes
            .iter()
            .all(|s| s.authority == ScopeAuthority::Controlled)
    );
    assert_eq!(recovered.windows.len(), 13);
    assert!(recovered.windows.iter().all(|w| w.allocation.is_some()));
    // Recovery through larger bounds, not just fewer windows.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &bulk_report(14, 1));
    let mut roomy = bulk_report(14, 2);
    roomy.domains[0].domain.bounds = Rect {
        x: 0,
        y: 0,
        w: 1600,
        h: 1200,
    };
    let recovered = accepted(&mut engine, &roomy);
    assert!(
        recovered
            .scopes
            .iter()
            .all(|s| s.authority == ScopeAuthority::Controlled),
        "geometric room recovers the same inventory: {:?}",
        recovered.scopes
    );
    assert_eq!(recovered.windows.len(), 14);
}

#[test]
fn capacity_mixed_roster_keeps_minimized_rows() {
    use tiler_core::directional::WorkspaceId;
    use tiler_core::world_report::{MinimizedEntry, ScopeAuthority};
    // Fourteen live windows seed a complete controlled roster; one
    // minimized lifetime homed there keeps its explicit minimized row
    // (best effort, selected 2026-10-17: no capacity hold).
    let mut report = {
        let mut base = two_output_report(1, 1);
        base.topology = vec![topology("out-1", vec!["ws1"], "ws1")];
        base.domains = vec![DomainReport {
            domain: OutputDomain {
                id: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws1".to_owned()),
                bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 600,
                },
                gap: 8,
                adjacent: std::collections::BTreeMap::new(),
            },
            outer_gap: 4,
            focused: WindowId(String::new()),
            windows: (0..14)
                .map(|i| win(&format!("win-{i:06}"), "out-1", "ws1"))
                .collect(),
            closed: Vec::new(),
            readable: true,
            hold: None,
        }];
        base.minimized = vec![MinimizedEntry {
            window: WindowId("win-min".to_owned()),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws1".to_owned()),
        }];
        base
    };
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = accepted(&mut engine, &report);
    assert_eq!(
        reply.scopes,
        vec![tiler_core::world_report::ScopeStatus {
            scope: "out-1/ws1".to_owned(),
            authority: ScopeAuthority::Controlled,
        }]
    );
    // Fourteen live rows plus the minimized row: complete, with no
    // capacity hold anywhere.
    assert_eq!(reply.windows.len(), 15);
    let minimized = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-min")
        .expect("minimized row in mixed roster");
    assert_eq!(
        minimized.frame_authority.hold_reason(),
        Some(HoldReason::Minimized)
    );
    assert!(minimized.allocation.is_none());
    // Recovery keeps the minimized row while the live inventory tiles.
    report.domains[0].windows.truncate(13);
    report.request_seq = 2;
    report.request_id = "req-2".to_owned();
    report.observation_seq = 2;
    let recovered = accepted(&mut engine, &report);
    assert!(
        recovered
            .scopes
            .iter()
            .all(|s| s.authority == ScopeAuthority::Controlled)
    );
    assert!(
        recovered.windows.iter().any(|w| w.lifetime.0 == "win-min"
            && w.frame_authority.hold_reason() == Some(HoldReason::Minimized)),
        "minimized row survives recovery"
    );
}

#[test]
fn floating_tiling_survives_plain_reports_without_adapter_votes() {
    use tiler_core::world_report::{ScopeAuthority, TilingMode};
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &two_output_report(1, 1));
    // Planner-owned float: out-1/ws1 releases its session.
    let mut toggle = two_output_report(2, 2);
    with_command(
        &mut toggle,
        2,
        61,
        WorldCommand::SetWorkspaceTiling {
            output: OutputId("out-1".to_owned()),
            workspace: "ws1".to_owned(),
            tiled: false,
        },
    );
    accepted(&mut engine, &toggle);
    // Plain observation-only reports preserve the retained mode: the scope
    // stays untouched with explicit untouched rows (never emptiness) and
    // no frame writes, across repeated reports with no command.
    for seq in [3u64, 4] {
        let reply = accepted(&mut engine, &two_output_report(seq, seq));
        let scope = reply
            .scopes
            .iter()
            .find(|s| s.scope == "out-1/ws1")
            .expect("floating scope row");
        assert_eq!(scope.authority, ScopeAuthority::Untouched);
        let floated: Vec<_> = reply
            .windows
            .iter()
            .filter(|w| w.output.0 == "out-1" && w.workspace == "ws1")
            .collect();
        assert!(
            !floated.is_empty(),
            "seq {seq}: floating workspaces publish explicit untouched rows"
        );
        assert!(
            floated.iter().all(|w| {
                w.allocation.is_none()
                    && w.frame_authority == ScopeAuthority::Untouched
                    && w.membership_authority == ScopeAuthority::Untouched
            }),
            "seq {seq}: floating rows authorize nothing"
        );
        let row = reply
            .workspaces
            .iter()
            .find(|w| w.id == "ws1" && w.output.0 == "out-1")
            .expect("floating workspace row");
        assert_eq!(row.tiling_mode, TilingMode::Floating);
        let output_row = reply
            .outputs
            .iter()
            .find(|o| o.output.0 == "out-1")
            .expect("out-1 output row");
        assert_eq!(output_row.tiling_mode, TilingMode::Floating);
    }
    // Re-enabling re-adopts through the fresh route with full authority.
    let mut retile = two_output_report(5, 5);
    with_command(
        &mut retile,
        5,
        62,
        WorldCommand::SetWorkspaceTiling {
            output: OutputId("out-1".to_owned()),
            workspace: "ws1".to_owned(),
            tiled: true,
        },
    );
    let reply = accepted(&mut engine, &retile);
    assert!(
        reply
            .windows
            .iter()
            .any(|w| w.lifetime.0 == "win-a" && w.allocation.is_some()),
        "retile restores frame authority"
    );
}

#[test]
fn late_command_refusal_under_new_incarnation_keeps_old_state() {
    // A new incarnation whose staged report refuses AFTER the reset point
    // (command execution, not the pre-staging gates) must leave the previous
    // incarnation's flights, ordering, and caches exactly intact. Same-output
    // sends keep the engine pair local (no adjacency needed).
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let first = three_workspace_report(1, 1);
    accepted(&mut engine, &first);
    // Retain a window transfer under adapter-1: win-a from ws1 to ws2.
    let mut send = three_workspace_report(2, 2);
    with_command(
        &mut send,
        2,
        71,
        WorldCommand::Send {
            window: WindowId("win-a".to_owned()),
            target_output: OutputId("out-1".to_owned()),
            target_workspace: "ws2".to_owned(),
            follow: false,
        },
    );
    let reply = accepted(&mut engine, &send);
    assert_eq!(reply.transfers.len(), 1);
    // New incarnation, valid shape, but the command refuses late (unknown
    // send window): staged reset must not commit.
    let mut refused = three_workspace_report(1, 2);
    refused.adapter_incarnation = "adapter-2".to_owned();
    with_command(
        &mut refused,
        1,
        72,
        WorldCommand::Send {
            window: WindowId("win-ghost".to_owned()),
            target_output: OutputId("out-1".to_owned()),
            target_workspace: "ws2".to_owned(),
            follow: false,
        },
    );
    match engine.handle_report(&refused) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-window-unknown"),
        other => panic!("expected late send refusal, got {other:?}"),
    }
    // Old incarnation state is intact: the exact latest report still
    // duplicates (same sequence, same bytes, no new revision) and the
    // retained transfer still publishes on the next observation.
    match engine.handle_report(&send) {
        WorldOutcome::Duplicate(_) => {}
        other => panic!("expected duplicate of the pre-refusal report, got {other:?}"),
    }
    let lagged = accepted(&mut engine, &three_workspace_report(3, 2));
    assert_eq!(lagged.transfers.len(), 1);
    // A valid report under the new incarnation then re bootstraps cleanly.
    let mut migrated = three_workspace_report(1, 3);
    migrated.adapter_incarnation = "adapter-2".to_owned();
    let reply = accepted(&mut engine, &migrated);
    assert_eq!(reply.adapter_incarnation, "adapter-2");
    assert!(reply.transfers.is_empty());
}

#[test]
fn reply_bounds_cover_pending_transfer_effect_and_presentation() {
    use tiler_core::directional::{OutputId, WindowId};
    use tiler_core::world_report::{
        EffectKind, OneShotEffect, OutputDesire, ScopeAuthority, ScopeStatus, TilingMode,
        TransferKind, TransferTarget, WORLD_CONTRACT_VERSION, WorkspaceDesire, WorldReply,
        encode_reply_canonical, reply_estimate, reply_upper_bound,
    };
    fn base_reply() -> WorldReply {
        WorldReply {
            contract_version: WORLD_CONTRACT_VERSION,
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            request_id: "req-1".to_owned(),
            based_on_observation_seq: 1,
            state_revision: 1,
            ready: true,
            command_consumed: false,
            scopes: vec![ScopeStatus {
                scope: "out/ws".to_owned(),
                authority: ScopeAuthority::Controlled,
            }],
            workspaces: Vec::new(),
            outputs: vec![OutputDesire {
                output: OutputId("out".to_owned()),
                logical_view: None,
                tiling_mode: TilingMode::Tiled,
                hold: None,
            }],
            windows: Vec::new(),
            transfers: Vec::new(),
            effects: Vec::new(),
            command_result: None,
            presentation_bytes: 0,
        }
    }
    let base = base_reply();
    let base_est = reply_estimate(&base);
    assert!(base_est >= encode_reply_canonical(&base).len());
    // One pending-creation workspace row (unbound token, maximal ids)
    // fits inside the 448 pending-creation slack.
    let long = "w".repeat(128);
    let mut pending = base.clone();
    pending.workspaces.push(WorkspaceDesire {
        id: "wl-18446744073709551615".to_owned(),
        output: OutputId(long.clone()),
        native_binding: None,
        logical_id: Some("wl-18446744073709551615".to_owned()),
        scope: long.clone(),
        order: usize::MAX,
        tiling_mode: TilingMode::Floating,
        existence: true,
        hold: None,
    });
    let pending_est = reply_estimate(&pending);
    assert!(pending_est >= encode_reply_canonical(&pending).len());
    assert!(
        pending_est - base_est <= 448,
        "pending row delta fits the upper-bound slack"
    );
    // One transfer row per kind fits inside the 512 transfer slack.
    for (label, kind) in [
        ("window", TransferKind::Window(WindowId(long.clone()))),
        ("view", TransferKind::View),
        ("migration", TransferKind::Migration),
    ] {
        let mut reply = base.clone();
        reply.transfers.push(TransferTarget {
            operation_id: "op-18446744073709551615".to_owned(),
            kind,
            target_output: OutputId(long.clone()),
            target_workspace: long.clone(),
            follow: true,
            held: true,
        });
        let estimate = reply_estimate(&reply);
        assert!(estimate >= encode_reply_canonical(&reply).len(), "{label}");
        assert!(
            estimate - base_est <= 512,
            "{label} transfer row delta fits the upper-bound slack"
        );
    }
    // One engine-minted effect (`fx-{u64}`, never an arbitrary id) fits
    // inside the 256 effect slack.
    let mut reply = base.clone();
    reply.effects.push(OneShotEffect {
        id: "fx-18446744073709551615".to_owned(),
        kind: EffectKind::Stack,
        target: WindowId(long.clone()),
        issued_revision: u64::MAX,
    });
    let estimate = reply_estimate(&reply);
    assert!(estimate >= encode_reply_canonical(&reply).len());
    assert!(
        estimate - base_est <= 256,
        "effect row delta fits the upper-bound slack"
    );
    // Presentation reservation: reserved bytes widen the estimate without
    // changing the exact encoding (sections unimplemented, envelope only).
    let mut reserved = base.clone();
    reserved.presentation_bytes = 65536;
    assert_eq!(
        reply_estimate(&reserved),
        base_est + 65536,
        "reservation is exact headroom, not a shape change"
    );
    assert!(reply_estimate(&reserved) >= encode_reply_canonical(&reserved).len());
    // One retained-missing held scope row (maximal scope string) fits
    // inside the 336 missing-session slack.
    let mut reply = base.clone();
    reply.scopes.push(ScopeStatus {
        scope: format!("{long}/{long}"),
        authority: ScopeAuthority::Hold(tiler_core::world_report::HoldReason::IncompleteInventory),
    });
    let estimate = reply_estimate(&reply);
    assert!(estimate >= encode_reply_canonical(&reply).len());
    assert!(
        estimate - base_est <= 336,
        "missing-scope row delta fits the upper-bound slack"
    );
    // The pre-mutation upper bound covers a real small reply end to end.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let report = two_output_report(1, 1);
    let reply = accepted(&mut engine, &report);
    let bound = reply_upper_bound(&report, 0, engine.engine().domain_keys().len(), 0);
    assert!(
        bound >= reply_estimate(&reply),
        "upper bound covers the published reply"
    );
}
