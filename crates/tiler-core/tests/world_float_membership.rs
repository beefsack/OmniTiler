//! Floating membership retention: toggling a workspace to floating
//! releases the Engine session, but the core membership owner keeps every
//! member homed. Switching the view then repeating the unchanged
//! observation never rehomes floating members or authorizes frame writes;
//! re-tiling restores float intent.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    ScopeAuthority, ScreenReport, ScreenView, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand,
    WorldReport,
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

fn float_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        floating: true,
        ..fact(window, output, x)
    }
}

fn fixed_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        floating: true,
        fixed_auto: true,
        hints: WindowSizeHints {
            min_w: Some(40),
            min_h: Some(90),
            max_w: Some(40),
            max_h: Some(90),
        },
        ..fact(window, output, x)
    }
}

fn minimized_fact(window: &str, output: &str) -> WindowFact {
    WindowFact {
        minimized: true,
        rect: None,
        ..fact(window, output, 0)
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
) -> WorldReport {
    WorldReport {
        restored: None,
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens: vec![screen("out-a", 0, true), screen("out-b", 800, false)],
        windows,
        views: views
            .into_iter()
            .map(|(output, view)| ScreenView {
                output: OutputId(output.to_owned()),
                view: view.to_owned(),
            })
            .collect(),
        closed: vec![],
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: vec![],
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: String::new(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: vec![],
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

#[test]
fn floating_toggle_keeps_membership_across_view_switch_and_unchanged_report() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, 1, vec![fact("win-t", "out-a", 0)], vec![], None, None),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    // Admit one tiled, one user float, one fixed float, one minimized member.
    let members = vec![
        fact("win-t", "out-a", 0),
        float_fact("win-u", "out-a", 100),
        fixed_fact("win-x", "out-a", 200),
        minimized_fact("win-m", "out-a"),
    ];
    let admitted = accepted(
        &mut engine,
        &report(2, 2, members.clone(), vec![], None, None),
    );
    for win in ["win-t", "win-u", "win-x", "win-m"] {
        let row = admitted
            .windows
            .iter()
            .find(|row| row.lifetime.0 == win)
            .unwrap_or_else(|| panic!("missing row {win}"));
        assert_eq!(row.workspace, view_a, "{win}: {row:?}");
    }
    // Toggle the workspace to floating: the Engine session releases (the old
    // session-only homing path goes empty), but membership must persist.
    let toggled = accepted(
        &mut engine,
        &report(
            3,
            3,
            members.clone(),
            vec![],
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: view_a.clone(),
                tiled: false,
            }),
        ),
    );
    let float_key = tiler_core::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: tiler_core::directional::WorkspaceId(view_a.clone()),
    };
    assert!(
        engine.engine().session(&float_key).is_none(),
        "toggle releases the layout session (old homing source is empty)"
    );
    let scope = toggled
        .scopes
        .iter()
        .find(|status| status.scope == format!("out-a/{view_a}"))
        .expect("floating scope");
    assert_eq!(scope.authority, ScopeAuthority::Untouched);
    for win in ["win-t", "win-u", "win-x"] {
        let row = toggled
            .windows
            .iter()
            .find(|row| row.lifetime.0 == win)
            .unwrap_or_else(|| panic!("missing floating row {win}"));
        assert_eq!(row.workspace, view_a, "{win}: {row:?}");
        assert_eq!(row.membership_authority, ScopeAuthority::Untouched);
        assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
        assert!(row.allocation.is_none(), "{win}: no frame writes");
    }
    // Switch the view to another workspace, then repeat the unchanged
    // observation: no member may rehome to the new current, and no frame
    // writes may appear.
    let ring_a: Vec<String> = toggled
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert!(ring_a.len() >= 2, "ring: {ring_a:?}");
    let other = ring_a
        .iter()
        .find(|id| *id != &view_a)
        .expect("second workspace")
        .clone();
    let switched = accepted(
        &mut engine,
        &report(
            4,
            4,
            members.clone(),
            vec![],
            Some(12),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    assert_eq!(
        switched
            .outputs
            .iter()
            .find(|row| row.output.0 == "out-a")
            .and_then(|row| row.logical_view.clone())
            .as_deref(),
        Some(other.as_str())
    );
    let calm = accepted(
        &mut engine,
        &report(5, 5, members.clone(), vec![], None, None),
    );
    for win in ["win-t", "win-u", "win-x", "win-m"] {
        let row = calm
            .windows
            .iter()
            .find(|row| row.lifetime.0 == win)
            .unwrap_or_else(|| panic!("missing calm row {win}"));
        assert_eq!(
            (row.output.0.as_str(), row.workspace.as_str()),
            ("out-a", view_a.as_str()),
            "{win} stays on the floating workspace: {row:?}"
        );
        assert!(
            row.allocation.is_none(),
            "{win}: unchanged report writes nothing: {row:?}"
        );
    }
    let scope = calm
        .scopes
        .iter()
        .find(|status| status.scope == format!("out-a/{view_a}"))
        .expect("floating scope stays");
    assert_eq!(scope.authority, ScopeAuthority::Untouched);
    // Re-tile restores float intent: the user float rides the tiled scope as
    // an explicit float exception again.
    let retiled = accepted(
        &mut engine,
        &report(
            6,
            6,
            members.clone(),
            vec![("out-a", other.as_str())],
            Some(13),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: view_a.clone(),
                tiled: true,
            }),
        ),
    );
    let row = retiled
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-u")
        .expect("restored float row");
    assert_eq!(row.workspace, view_a, "{row:?}");
    assert!(row.floating, "retile restores float intent: {row:?}");
}
