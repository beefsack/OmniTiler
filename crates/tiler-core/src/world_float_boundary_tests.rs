//! Boundary, seed, close, shared-occupancy, and hotplug regressions:
//! floating sends keep source state with meaningful recipient proof,
//! default-seed changes seed only new workspaces, incomplete omission holds
//! while explicit close settles, shared lifecycle uses occupancy union, and
//! three-output displacement/return preserves the unit.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::workspace_scope::WorkspaceMode;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    ScopeAuthority, ScreenReport, ScreenView, TransferKind, WORLD_CONTRACT_VERSION, WindowFact,
    WorldCommand, WorldReport,
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
    mode: WorkspaceMode,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    views: Vec<(&str, &str)>,
    closed: Vec<&str>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    inventory_complete: bool,
    default_tiled: bool,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: mode,
        screens,
        windows,
        views: views
            .into_iter()
            .map(|(output, view)| ScreenView {
                output: OutputId(output.to_owned()),
                view: view.to_owned(),
            })
            .collect(),
        closed: closed
            .into_iter()
            .map(|id| WindowId(id.to_owned()))
            .collect(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: vec![],
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        switch_target: crate::cursor_targeting::SwitchTarget::default(),
        fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        default_tiled,
        focused_global: String::new(),
        inventory_complete,
        gesture_barrier: false,
        feedback: vec![],
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn two_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0, true), screen("out-b", 800, false)]
}

#[test]
fn floating_boundary_send_keeps_source_and_proves_recipient() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![],
            vec![],
            vec![],
            None,
            None,
            true,
            true,
        ),
    );
    let ring_a: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(ring_a.len(), 2);
    let (ws_a, ws_b) = (ring_a[0].clone(), ring_a[1].clone());
    // win-s tiled on ws_b (second workspace via observed receipt), win-t stays.
    let mut s = fact("win-s", "out-a", 0);
    s.observed_workspace = Some(ws_b.clone());
    accepted(
        &mut engine,
        &report(
            2,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![s.clone(), fact("win-t", "out-a", 100)],
            vec![],
            vec![],
            None,
            None,
            true,
            true,
        ),
    );
    // Float ws_b.
    accepted(
        &mut engine,
        &report(
            3,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![s.clone(), fact("win-t", "out-a", 100)],
            vec![],
            vec![],
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: ws_b.clone(),
                tiled: false,
            }),
            true,
            true,
        ),
    );
    // Same-output send from tiled ws_a into floating ws_b: membership-only,
    // source state maintained, recipient proof meaningful.
    let reply = accepted(
        &mut engine,
        &report(
            4,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![fact("win-t", "out-a", 100), s.clone()],
            vec![],
            vec![],
            Some(12),
            Some(WorldCommand::Send {
                window: WindowId("win-t".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: ws_b.clone(),
                follow: false,
            }),
            true,
            true,
        ),
    );
    assert_eq!(reply.transfers.len(), 1);
    let flight = &reply.transfers[0];
    assert_eq!(
        (
            flight.target_output.0.as_str(),
            flight.target_workspace.as_str()
        ),
        ("out-a", ws_b.as_str()),
        "flight targets the floating workspace: {flight:?}"
    );
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-t")
        .expect("recipient row");
    assert_eq!(
        (row.output.0.as_str(), row.workspace.as_str()),
        ("out-a", ws_b.as_str()),
        "recipient proof at the floating target: {row:?}"
    );
    assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    assert_ne!(row.frame_authority, ScopeAuthority::Controlled);
    // Source workspace still lists its survivor with no writes against it.
    assert!(
        reply
            .windows
            .iter()
            .any(|row| row.lifetime.0 == "win-s" && row.workspace == ws_b),
        "source state maintained: {:?}",
        reply.windows
    );
    let _ = ws_a;
}

#[test]
fn default_seed_changes_only_new_workspaces() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![],
            vec![],
            vec![],
            None,
            None,
            true,
            true,
        ),
    );
    let ws_a = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .expect("ws a");
    // Manual toggle to floating under the old default.
    let toggled = accepted(
        &mut engine,
        &report(
            2,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![],
            vec![],
            vec![],
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: ws_a.clone(),
                tiled: false,
            }),
            true,
            true,
        ),
    );
    let mode = toggled
        .workspaces
        .iter()
        .find(|row| row.id == ws_a)
        .map(|row| row.tiling_mode)
        .expect("mode");
    assert_eq!(mode, crate::world_report::TilingMode::Floating);
    // Flip the saved default: existing modes never reseed; only subsequently
    // created workspaces seed from the new default. Fill the live trailing
    // empty so lifecycle mints a fresh trailing seeded from the new default.
    let before: Vec<(String, crate::world_report::TilingMode)> = toggled
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| (row.id.clone(), row.tiling_mode))
        .collect();
    let trailing = toggled
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| &row.id)
        .next_back()
        .expect("trailing")
        .clone();
    let mut fill = fact("win-a", "out-a", 0);
    fill.observed_workspace = Some(trailing);
    let calm = accepted(
        &mut engine,
        &report(
            3,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![fill],
            vec![],
            vec![],
            None,
            None,
            true,
            false,
        ),
    );
    for (id, mode) in &before {
        let now = calm
            .workspaces
            .iter()
            .find(|row| row.id == *id)
            .map(|row| row.tiling_mode);
        assert_eq!(now.as_ref(), Some(mode), "existing {id} never reseeds");
    }
    let new_ids: Vec<String> = calm
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .filter(|id| !before.iter().any(|(kept, _)| kept == id))
        .collect();
    assert!(!new_ids.is_empty(), "lifecycle created: {new_ids:?}");
    for id in new_ids {
        let mode = calm
            .workspaces
            .iter()
            .find(|row| row.id == id)
            .map(|row| row.tiling_mode)
            .expect("new mode");
        assert_eq!(
            mode,
            crate::world_report::TilingMode::Floating,
            "new {id} seeds from the flipped default"
        );
    }
}

#[test]
fn incomplete_omission_holds_explicit_close_settles() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            None,
            None,
            true,
            true,
        ),
    );
    // Incomplete inventory omitting win-b without an explicit close: held,
    // never inferred closed.
    let held = accepted(
        &mut engine,
        &report(
            2,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            None,
            None,
            false,
            true,
        ),
    );
    assert!(
        held.windows.iter().any(|row| row.lifetime.0 == "win-b"),
        "omission across a gap holds: {:?}",
        held.windows
    );
    // Explicit close settles even inside the incomplete inventory.
    let settled = accepted(
        &mut engine,
        &report(
            3,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec!["win-b"],
            None,
            None,
            false,
            true,
        ),
    );
    assert!(
        settled.windows.iter().all(|row| row.lifetime.0 != "win-b"),
        "explicit close honored: {:?}",
        settled.windows
    );
}

#[test]
fn shared_occupancy_union_keeps_b_only_workspace() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            WorkspaceMode::Shared,
            two_screens(),
            vec![],
            vec![],
            vec![],
            None,
            None,
            true,
            true,
        ),
    );
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert!(ring.len() >= 2, "ring: {ring:?}");
    // Occupy the second workspace from out-b only via a truthful receipt.
    let mut b_only = fact("win-b", "out-b", 800);
    b_only.observed_workspace = Some(ring[1].clone());
    let calm = accepted(
        &mut engine,
        &report(
            2,
            WorkspaceMode::Shared,
            two_screens(),
            vec![b_only],
            vec![],
            vec![],
            None,
            None,
            true,
            true,
        ),
    );
    assert!(
        calm.workspaces.iter().any(|row| row.id == ring[1]),
        "B-only occupied workspace survives the union: {:?}",
        calm.workspaces
    );
}

#[test]
fn three_output_displacement_preserves_unit_and_returns() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let three = vec![
        screen("out-a", 0, true),
        screen("out-b", 800, false),
        screen("out-c", 1600, false),
    ];
    let first = accepted(
        &mut engine,
        &report(
            1,
            WorkspaceMode::PerOutputLocal,
            three.clone(),
            vec![fact("win-a", "out-c", 1600), fact("win-b", "out-c", 1700)],
            vec![],
            vec![],
            None,
            None,
            true,
            true,
        ),
    );
    let view_c = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-c")
        .and_then(|row| row.logical_view.clone())
        .expect("view c");
    accepted(
        &mut engine,
        &report(
            2,
            WorkspaceMode::PerOutputLocal,
            three.clone(),
            vec![fact("win-a", "out-c", 1600), fact("win-b", "out-c", 1700)],
            vec![],
            vec![],
            None,
            None,
            true,
            true,
        ),
    );
    let src_key = crate::session::DomainKey {
        output: OutputId("out-c".to_owned()),
        workspace: crate::directional::WorkspaceId(view_c.clone()),
    };
    let pre_tree = engine
        .engine()
        .session(&src_key)
        .unwrap()
        .tree_for(&src_key)
        .cloned();
    // Disconnect out-c: nearest survivor is out-b (1600 nearer 800 than 0).
    // Members observed on the survivor screen stay homed via the rehome.
    let two = vec![screen("out-a", 0, true), screen("out-b", 800, false)];
    let displaced = accepted(
        &mut engine,
        &report(
            3,
            WorkspaceMode::PerOutputLocal,
            two.clone(),
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-b", 900)],
            vec![],
            vec![],
            None,
            None,
            true,
            true,
        ),
    );
    let row = displaced
        .workspaces
        .iter()
        .find(|row| row.id == view_c)
        .expect("displaced unit");
    assert_eq!(
        row.output.0, "out-b",
        "nearest survivor hosts the unit: {row:?}"
    );
    let moved_key = crate::session::DomainKey {
        output: OutputId("out-b".to_owned()),
        workspace: crate::directional::WorkspaceId(view_c.clone()),
    };
    assert_eq!(
        engine
            .engine()
            .session(&moved_key)
            .unwrap()
            .tree_for(&moved_key),
        pre_tree.as_ref(),
        "tree/shares ride displacement"
    );
    // Reconnect: the origin reclaims its unit with contents intact.
    let returned = accepted(
        &mut engine,
        &report(
            4,
            WorkspaceMode::PerOutputLocal,
            three.clone(),
            vec![fact("win-a", "out-c", 1600), fact("win-b", "out-c", 1700)],
            vec![],
            vec![],
            None,
            None,
            true,
            true,
        ),
    );
    let row = returned
        .workspaces
        .iter()
        .find(|row| row.id == view_c)
        .expect("returned unit");
    assert_eq!(row.output.0, "out-c");
    assert_eq!(
        engine
            .engine()
            .session(&src_key)
            .unwrap()
            .tree_for(&src_key),
        pre_tree.as_ref(),
        "tree/shares ride the return"
    );
    assert!(
        !displaced
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "hotplug policy carries without command flights"
    );
}
