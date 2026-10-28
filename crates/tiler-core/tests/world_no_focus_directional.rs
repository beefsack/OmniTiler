//! World acceptance for directional focus with no focused window (User
//! 2026-10-26): full multi-report journeys through the public engine API.
//! With no focused window, direction-only focus starts at the current
//! workspace/output and crosses to the adjacent output by the entry-edge
//! selection, otherwise it stays quiet and never picks a
//! current-workspace window. Host-caused focus loss on observation-only
//! reports emits no forced focus, and the crossing converges without a
//! refocus fight on the follow-up observation.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    EffectKind, ScreenReport, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
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

#[allow(clippy::too_many_arguments)]
fn report(
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    action_foreground: Option<&str>,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq,
        command,
        workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens,
        windows,
        views: vec![],
        closed: vec![],
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        birth_clear: false,
        native: vec![],
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        born_maximized: tiler_core::born_maximized::BornMaximized::default(),
        switch_target: tiler_core::cursor_targeting::SwitchTarget::default(),
        fresh_placement: tiler_core::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        default_tiled: true,
        focused_global: focused.to_owned(),
        action_foreground: action_foreground.map(|output| OutputId(output.to_owned())),
        action_cached: None,
        action_outputs: vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())],
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: vec![],
        member_attempts: vec![],
        view_attempts: vec![],
        clear_attempts: vec![],
        creation_attempts: vec![],
        presentation: tiler_core::world_report::PresentationIngress::disabled(),
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

fn focus_targets(reply: &tiler_core::world_report::WorldReply) -> Vec<String> {
    reply
        .effects
        .iter()
        .filter(|effect| effect.kind == EffectKind::Focus)
        .map(|effect| effect.target.0.clone())
        .collect()
}

fn two_outputs() -> Vec<ScreenReport> {
    vec![screen("out-a", 0, true), screen("out-b", 800, false)]
}

fn four_windows() -> Vec<WindowFact> {
    vec![
        fact("win-a1", "out-a", 10),
        fact("win-a2", "out-a", 100),
        fact("win-b1", "out-b", 810),
        fact("win-b2", "out-b", 900),
    ]
}

#[test]
fn no_focus_journey_focuses_remembered_then_converges() {
    // Focus win-a2 and win-b1, lose focus to the desktop (observation
    // only, no forced refocus), then cross right from out-a: the effect
    // names the entry-edge selection, never a current-workspace window.
    // The 40x90 facts admit the out-b pair side by side (H group, DFS
    // [win-b2, win-b1]), so only the left leaf win-b2 touches the Right
    // entry edge; the coherent win-b1 history (last-active plus MRU, both
    // naming win-b1 after report 3) cannot win because win-b1 is filtered
    // as a non-edge candidate. The follow-up observation of the new focus
    // converges without minting another effect.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = two_outputs();
    accepted(
        &mut engine,
        &report(1, screens.clone(), four_windows(), "", None, None, None),
    );
    accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            four_windows(),
            "win-a2",
            None,
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report(
            3,
            screens.clone(),
            four_windows(),
            "win-b1",
            None,
            None,
            None,
        ),
    );
    let loss = accepted(
        &mut engine,
        &report(4, screens.clone(), four_windows(), "", None, None, None),
    );
    assert!(
        focus_targets(&loss).is_empty(),
        "host loss stays quiet: {:?}",
        loss.effects
    );
    let crossed = accepted(
        &mut engine,
        &report(
            5,
            screens.clone(),
            four_windows(),
            "",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId(String::new()),
                direction: "right".to_owned(),
            }),
            Some("out-a"),
        ),
    );
    assert_eq!(
        focus_targets(&crossed),
        vec!["win-b2".to_owned()],
        "entry-edge destination selection: {:?}",
        crossed.effects
    );
    let arrival = accepted(
        &mut engine,
        &report(6, screens, four_windows(), "win-b2", None, None, None),
    );
    assert!(
        focus_targets(&arrival).is_empty(),
        "convergence adopts without refocus: {:?}",
        arrival.effects
    );
    assert!(
        arrival.command_result.is_none(),
        "quiet arrival: {:?}",
        arrival.command_result
    );
}

#[test]
fn no_focus_noop_journey_stays_quiet() {
    // Single output: losing focus then pressing into the void retains
    // without effects, and the follow-up observation stays quiet too.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = vec![screen("out-a", 0, true)];
    let windows = || vec![fact("win-a1", "out-a", 10), fact("win-a2", "out-a", 100)];
    accepted(
        &mut engine,
        &report(1, screens.clone(), windows(), "win-a1", None, None, None),
    );
    let loss = accepted(
        &mut engine,
        &report(2, screens.clone(), windows(), "", None, None, None),
    );
    assert!(focus_targets(&loss).is_empty());
    let noop = accepted(
        &mut engine,
        &report(
            3,
            screens.clone(),
            windows(),
            "",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId(String::new()),
                direction: "right".to_owned(),
            }),
            Some("out-a"),
        ),
    );
    assert!(
        focus_targets(&noop).is_empty(),
        "no neighbor: {:?}",
        noop.effects
    );
    match &noop.command_result {
        Some(tiler_core::world_report::CommandResult::Directional { outcome, cause, .. }) => {
            assert_eq!(outcome.as_str(), "focus-unfocused-retained");
            assert_eq!(cause.as_deref(), Some("no-adjacent-output"));
        }
        other => panic!("expected retained no-op, got {other:?}"),
    }
    let still = accepted(
        &mut engine,
        &report(4, screens, windows(), "", None, None, None),
    );
    assert!(focus_targets(&still).is_empty());
}
