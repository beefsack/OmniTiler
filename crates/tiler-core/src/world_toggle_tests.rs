//! Neutral workspace-tiling toggle intent: `ToggleWorkspaceTiling` flips
//! the core-retained mode at execution time with no adapter-preselected
//! boolean, so racing toggles compose. Explicit `SetWorkspaceTiling`
//! keeps its genuine-set semantics.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    CommandRejectKind, CommandResult, PresentationIngressV4, PresentationRowV4, ScreenReport,
    TilingMode, WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand, WorldReplyV4, WorldReportV4,
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

fn empty_ingress() -> PresentationIngressV4 {
    PresentationIngressV4 {
        foreground: String::new(),
        foreground_minimized: Some(false),
        foreground_maximized: Some(false),
        foreground_fullscreen: Some(false),
        foreground_host_hidden: Some(false),
        foreground_hidden_workspace: Some(false),
        foreground_frame: None,
        chord_held: false,
        move_window: String::new(),
        pager_order: Vec::new(),
        pager_frames: Vec::new(),
        witness: None,
        foreground_out: None,
    }
}

fn report(
    request_seq: u64,
    windows: Vec<WindowFact>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        screens: vec![screen("out-a", 0)],
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: Vec::new(),
        native: vec![],
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: String::new(),
        foreground_known: false,
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
        presentation: PresentationRowV4::Valid(empty_ingress()),

        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> WorldReplyV4 {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn bootstrap_workspace() -> (WorldEngine, String) {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::TilingTargetUnknown
            }),
            "expected tiling-target refusal"
        ),
        other => panic!("expected tiling-target refusal, got {other:?}"),
    }
}
