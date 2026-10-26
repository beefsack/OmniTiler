//! Neutral workspace-tiling toggle intent: `ToggleWorkspaceTiling` flips
//! the core-retained mode at execution time with no adapter-preselected
//! boolean, so racing toggles compose. Explicit `SetWorkspaceTiling`
//! keeps its genuine-set semantics.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::workspace_scope::WorkspaceMode;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    ScreenReport, ScreenView, TilingMode, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand,
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
        native_workspace: None,
    }
}

fn report(
    request_seq: u64,
    windows: Vec<WindowFact>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
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
        workspace_mode: WorkspaceMode::PerOutputLocal,
        screens: vec![screen("out-a", 0, true)],
        windows,
        views: Vec::<(&str, &str)>::new()
            .into_iter()
            .map(|(output, view)| ScreenView {
                output: OutputId(output.to_owned()),
                view: view.to_owned(),
            })
            .collect(),
        closed: Vec::new(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        birth_clear: false,
        native: vec![],
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        born_maximized: crate::born_maximized::BornMaximized::default(),
        switch_target: crate::cursor_targeting::SwitchTarget::default(),
        fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        default_tiled: true,
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        presentation: crate::world_report::PresentationIngress::disabled(),
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn bootstrap_workspace() -> (WorldEngine, String) {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, vec![fact("win-a", "out-a", 0)], None, None),
    );
    let ws = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .expect("out-a workspace");
    assert_eq!(engine.logical_tiling("out-a", &ws), TilingMode::Tiled);
    (engine, ws)
}

fn toggle(ws: &str) -> WorldCommand {
    WorldCommand::ToggleWorkspaceTiling {
        output: OutputId("out-a".to_owned()),
        workspace: ws.to_owned(),
    }
}

#[test]
fn toggle_flips_tiled_to_floating_without_preselected_mode() {
    let (mut engine, ws) = bootstrap_workspace();
    let windows = vec![fact("win-a", "out-a", 0)];
    accepted(
        &mut engine,
        &report(2, windows.clone(), Some(11), Some(toggle(&ws))),
    );
    assert_eq!(
        engine.logical_tiling("out-a", &ws),
        TilingMode::Floating,
        "toggle resolves the retained tiled mode at execution time"
    );
}

#[test]
fn toggle_flips_floating_back_to_tiled() {
    let (mut engine, ws) = bootstrap_workspace();
    let windows = vec![fact("win-a", "out-a", 0)];
    accepted(
        &mut engine,
        &report(
            2,
            windows.clone(),
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: ws.clone(),
                tiled: false,
            }),
        ),
    );
    assert_eq!(engine.logical_tiling("out-a", &ws), TilingMode::Floating);
    accepted(
        &mut engine,
        &report(3, windows, Some(12), Some(toggle(&ws))),
    );
    assert_eq!(
        engine.logical_tiling("out-a", &ws),
        TilingMode::Tiled,
        "toggle re-enables from the retained floating mode"
    );
}

#[test]
fn queued_racing_toggles_compose() {
    let (mut engine, ws) = bootstrap_workspace();
    let windows = vec![fact("win-a", "out-a", 0)];
    // Two toggles queued back to back with no refreshed snapshot between
    // them: each execution re-reads the retained mode, so the pair composes
    // back to tiled instead of double-applying one stale preselection.
    accepted(
        &mut engine,
        &report(2, windows.clone(), Some(11), Some(toggle(&ws))),
    );
    assert_eq!(engine.logical_tiling("out-a", &ws), TilingMode::Floating);
    accepted(
        &mut engine,
        &report(3, windows, Some(12), Some(toggle(&ws))),
    );
    assert_eq!(
        engine.logical_tiling("out-a", &ws),
        TilingMode::Tiled,
        "second toggle observes the first toggle's retained mode"
    );
}

#[test]
fn explicit_set_keeps_genuine_set_semantics() {
    let (mut engine, ws) = bootstrap_workspace();
    let windows = vec![fact("win-a", "out-a", 0)];
    let set_false = || WorldCommand::SetWorkspaceTiling {
        output: OutputId("out-a".to_owned()),
        workspace: ws.clone(),
        tiled: false,
    };
    accepted(
        &mut engine,
        &report(2, windows.clone(), Some(11), Some(set_false())),
    );
    accepted(
        &mut engine,
        &report(3, windows, Some(12), Some(set_false())),
    );
    assert_eq!(
        engine.logical_tiling("out-a", &ws),
        TilingMode::Floating,
        "repeated explicit set(false) stays floating: no flip"
    );
}

#[test]
fn toggle_unknown_workspace_refuses_like_set() {
    let (mut engine, _ws) = bootstrap_workspace();
    let windows = vec![fact("win-a", "out-a", 0)];
    match engine.handle_report(&report(2, windows, Some(11), Some(toggle("ws-nope")))) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, crate::world_engine::KIND_TILING_TARGET);
            assert_eq!(message, crate::world_engine::MSG_TILING_TARGET);
        }
        other => panic!("expected tiling-target refusal, got {other:?}"),
    }
}
