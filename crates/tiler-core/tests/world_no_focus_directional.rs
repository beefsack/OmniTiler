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
    EffectKind, PresentationIngressV4, PresentationRowV4, ScreenReport, WORLD_CONTRACT_VERSION_V4,
    WindowFact, WorldCommand, WorldReportV4,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 560)),
        complete: true,
        scale: 1.0,
        identity: tiler_core::world_report::OutputIdentity::unknown(),
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
        native_workspace: tiler_core::world_report::NativeWorkspaceFact::Omitted,
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
        window_type: Some(tiler_core::world_report::WindowType::Normal),
        activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
        layer: Some(0),
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: None,
        visibility: tiler_core::world_report::VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
        last_write: None,
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
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq,
        command,
        screens,
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: vec![],
        native: vec![],
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        foreground_known: !focused.is_empty(),
        action_foreground: action_foreground.map(|output| OutputId(output.to_owned())),
        action_cached: None,
        action_outputs: vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())],
        primary_output: None,
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: vec![],
        member_attempts: vec![],
        view_attempts: vec![],
        clear_attempts: vec![],
        creation_attempts: vec![],
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn accepted(
    engine: &mut WorldEngine,
    report: &WorldReportV4,
) -> tiler_core::world_report::WorldReplyV4 {
    match engine.handle_report(report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn focus_targets(reply: &tiler_core::world_report::WorldReplyV4) -> Vec<String> {
    reply
        .effects
        .iter()
        .filter(|effect| effect.kind == EffectKind::Focus)
        .map(|effect| effect.target.0.clone())
        .collect()
}

fn two_outputs() -> Vec<ScreenReport> {
    vec![screen("out-a", 0), screen("out-b", 800)]
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
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![screen("out-a", 0)];
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
