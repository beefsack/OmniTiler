//! Truthful settlement regression (stage 2a): contradictory logical receipts
//! never prove arrival, host-owned unknown membership never proves arrival,
//! missing/empty/stale/external views hold narrowly, PossiblyExecuted stays
//! held across ack disappearance, and explicit closes settle across gaps.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    AttemptFeedback, AttemptKind, AttemptOutcome, ScreenReport, ScreenView, TransferKind,
    WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32, primary: bool) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 560)),
        gap: 0,
        outer_gap: 0,
        complete: true,
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

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    views: Vec<(&str, &str)>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    feedback: Vec<AttemptFeedback>,
    host_owned: bool,
    inventory_complete: bool,
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
        screens: vec![screen("out-a", 0, true), screen("out-b", 800, false)],
        windows: windows.clone(),
        views: views
            .into_iter()
            .map(|(output, view)| ScreenView {
                output: OutputId(output.to_owned()),
                view: view.to_owned(),
            })
            .collect(),
        closed: vec![],
        host_owned_workspace_domains: host_owned,
        equal_workspace_count: false,
        native: vec![],
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: windows
            .first()
            .map(|f| f.window.0.clone())
            .unwrap_or_default(),
        inventory_complete,
        gesture_barrier: false,
        switch_target: tiler_core::cursor_targeting::SwitchTarget::default(),
        fresh_placement: tiler_core::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        feedback,
        monotonic_ms: 0,
        frames: Vec::new(),
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

fn view_of(reply: &tiler_core::world_report::WorldReply, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

#[test]
fn contradictory_receipt_never_proves_send_arrival() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            false,
            true,
        ),
    );
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let target = ring[1].clone();
    let other = ring[0].clone();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target.clone(),
                follow: false,
            }),
            vec![],
            false,
            true,
        ),
    );
    let mut wrong = fact("win-a", "out-a", 0);
    wrong.observed_workspace = Some(other);
    let held = accepted(
        &mut engine,
        &report(3, 3, vec![wrong], vec![], None, None, vec![], false, true),
    );
    assert_eq!(
        held.transfers.len(),
        1,
        "contradictory holds: {:?}",
        held.transfers
    );
}

#[test]
fn migration_contradictory_receipt_does_not_settle() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            false,
            true,
        ),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
            false,
            true,
        ),
    );
    // Member observed on the target screen but naming a different workspace
    // must not settle the migration.
    let ring_b: Vec<String> = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            false,
            true,
        ),
    )
    .workspaces
    .iter()
    .filter(|row| row.output.0 == "out-b")
    .map(|row| row.id.clone())
    .collect();
    let wrong_ws = ring_b
        .into_iter()
        .find(|id| id != &view_a)
        .expect("other target workspace");
    let mut wrong = fact("win-a", "out-b", 800);
    wrong.observed_workspace = Some(wrong_ws);
    let held = accepted(
        &mut engine,
        &report(4, 4, vec![wrong], vec![], None, None, vec![], false, true),
    );
    assert!(
        held.transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "contradictory migration holds: {:?}",
        held.transfers
    );
}

#[test]
fn host_owned_unknown_holds_exact_settles() {
    // Window send, cross-output, host-owned on.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            true,
            true,
        ),
    );
    let view_b = view_of(&first, "out-b").expect("view b");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: true,
            }),
            vec![],
            true,
            true,
        ),
    );
    // Raw screen arrival with unknown membership must not settle while
    // host-owned (no translated logical receipt yet).
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-b", 800)],
            vec![],
            None,
            None,
            vec![],
            true,
            true,
        ),
    );
    assert_eq!(
        held.transfers.len(),
        1,
        "host-owned raw holds: {:?}",
        held.transfers
    );
    // Exact logical receipt settles.
    let mut exact = fact("win-a", "out-b", 800);
    exact.observed_workspace = Some(view_b);
    let settled = accepted(
        &mut engine,
        &report(4, 4, vec![exact], vec![], None, None, vec![], true, true),
    );
    assert!(
        settled.transfers.is_empty(),
        "exact settles: {:?}",
        settled.transfers
    );

    // Migration, host-owned on: unknown at target holds.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            true,
            true,
        ),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
            true,
            true,
        ),
    );
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-b", 800)],
            vec![],
            None,
            None,
            vec![],
            true,
            true,
        ),
    );
    assert!(
        held.transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "host-owned migration raw holds: {:?}",
        held.transfers
    );
}

#[test]
fn missing_empty_stale_views_hold_external_known_settles() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(1, 1, vec![], vec![], None, None, vec![], false, true),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![],
            vec![],
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: 2,
            }),
            vec![],
            false,
            true,
        ),
    );
    let target = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("view flight")
        .target_workspace
        .clone();
    // Missing echo holds.
    let held = accepted(
        &mut engine,
        &report(3, 3, vec![], vec![], None, None, vec![], false, true),
    );
    assert_eq!(held.transfers.len(), 1, "missing holds");
    // Empty echo holds.
    let held = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![],
            vec![("out-a", "")],
            None,
            None,
            vec![],
            false,
            true,
        ),
    );
    assert_eq!(held.transfers.len(), 1, "empty holds");
    // Stale echo holds.
    let held = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![],
            vec![("out-a", "")],
            None,
            None,
            vec![],
            false,
            true,
        ),
    );
    assert_eq!(held.transfers.len(), 1, "stale holds");
    // External unknown holds.
    let held = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![],
            vec![("out-a", "ws-999")],
            None,
            None,
            vec![],
            false,
            true,
        ),
    );
    assert_eq!(
        held.transfers.len(),
        1,
        "unknown holds: {:?}",
        held.transfers
    );
    // Exact echo settles.
    let settled = accepted(
        &mut engine,
        &report(
            7,
            7,
            vec![],
            vec![("out-a", target.as_str())],
            None,
            None,
            vec![],
            false,
            true,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "exact settles: {:?}",
        settled.transfers
    );
}

#[test]
fn possibly_executed_persists_held_across_disappearance() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            false,
            true,
        ),
    );
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let target = ring[1].clone();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target.clone(),
                follow: false,
            }),
            vec![],
            false,
            true,
        ),
    );
    // Ambiguous actuation holds.
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-11".to_owned(),
                kind: AttemptKind::Transfer,
                outcome: AttemptOutcome::PossiblyExecuted,
                hold: None,
            }],
            false,
            true,
        ),
    );
    assert_eq!(held.transfers.len(), 1);
    assert!(held.transfers[0].held, "possibly-executed holds");
    // Ack disappearance (no feedback row next report) must not unhold or
    // re-actuate: the flight stays held.
    let still = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            false,
            true,
        ),
    );
    assert_eq!(
        still.transfers.len(),
        1,
        "disappearance holds: {:?}",
        still.transfers
    );
    assert!(
        still.transfers[0].held,
        "stays held, never unheld: {:?}",
        still.transfers
    );
}

#[test]
fn explicit_close_settles_across_gaps() {
    // Window close during incomplete inventory settles the flight.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            false,
            true,
        ),
    );
    let first = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            false,
            true,
        ),
    );
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: ring[1].clone(),
                follow: false,
            }),
            vec![],
            false,
            true,
        ),
    );
    let mut incomplete = report(4, 4, vec![], vec![], None, None, vec![], false, false);
    incomplete.closed = vec![WindowId("win-a".to_owned())];
    let settled = accepted(&mut engine, &incomplete);
    assert!(
        settled.transfers.is_empty(),
        "close settles across gap: {:?}",
        settled.transfers
    );

    // Migration fully closed during incomplete inventory settles.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            false,
            true,
        ),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a,
            }),
            vec![],
            false,
            true,
        ),
    );
    let mut closed = report(3, 3, vec![], vec![], None, None, vec![], false, false);
    closed.closed = vec![WindowId("win-a".to_owned())];
    let settled = accepted(&mut engine, &closed);
    assert!(
        settled
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "closed migration settles across gap: {:?}",
        settled.transfers
    );
}
