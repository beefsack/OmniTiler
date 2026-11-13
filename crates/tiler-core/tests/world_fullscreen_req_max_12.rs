#![forbid(unsafe_code)]
//! REQ-MAX-12 shared core regressions:
//! - Host-owned explicit workspace switch/return while focused fullscreen
//!   preserves native fullscreen with no unsolicited fullscreen/frame/stack
//!   writes, and return focuses the fullscreen cover via the native-owned
//!   `awaitShown` route.
//! - Native-free route preserves existing safety/visibility gates (rejects
//!   fullscreen from `selection_route_eligible` and `Controlled` visibility).
//! - Focused fullscreen parent orientation toggle refuses/no-ops.
//! - Workspace tiling toggle off/on preserves fullscreen with no subject
//!   geometry/fullscreen/focus automatic effect.
//! - Existing fullscreen move fence remains intact.

use tiler_core::boundary::CoreCommand;
use tiler_core::connection::HostCapabilities;
use tiler_core::directional::{OutputId, SameAxisMove, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    CommandRejectKind, CommandResult, NativeObservation, NativeOccupancy, NativeWorkspaceFact,
    PresentationIngressV4, PresentationRowV4, ScopeAuthority, ScreenReport, TilingMode,
    WORLD_CONTRACT_VERSION_V4, WindowDesire, WindowFact, WorldCommand, WorldReplyV4, WorldReportV4,
};

const N1: &str = "native-1";
const N2: &str = "native-2";

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

fn host_fact(
    window: &str,
    output: &str,
    frame: Rect,
    native: &str,
    fullscreen: bool,
) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId(output.to_owned())),
        rect: Some(frame),
        fullscreen: Some(fullscreen),
        maximized: Some(false),
        sticky: Some(false),
        hints: Some(WindowSizeHints::none()),
        minimized: Some(false),
        native_workspace: NativeWorkspaceFact::Value(native.to_owned()),
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
        fullscreen_exitable: Some(true),
        suppression_available: None,
        visibility: tiler_core::world_report::VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
        last_write: None,
    }
}

fn native_free_fact(window: &str, output: &str, frame: Rect, fullscreen: bool) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId(output.to_owned())),
        rect: Some(frame),
        fullscreen: Some(fullscreen),
        maximized: Some(false),
        sticky: None,
        hints: Some(WindowSizeHints::none()),
        minimized: Some(false),
        native_workspace: NativeWorkspaceFact::Omitted,
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
        fullscreen_exitable: Some(true),
        suppression_available: Some(true),
        visibility: tiler_core::world_report::VisibilityFact {
            visible: Some(true),
            suppression_owned: Some(false),
        },
        last_write: None,
    }
}

fn native_obs(output: &str, order: &[&str], current: Option<&str>) -> NativeObservation {
    NativeObservation {
        output: OutputId(output.to_owned()),
        native_order: order.iter().map(|s| (*s).to_owned()).collect(),
        current_native: current.map(str::to_owned),
        occupancy: order
            .iter()
            .map(|s| NativeOccupancy {
                native: (*s).to_owned(),
                count: None,
            })
            .collect(),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_report(
    seq: u64,
    windows: Vec<WindowFact>,
    native: Vec<NativeObservation>,
    focused_global: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
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
        screens: vec![screen("out-a", 0)],
        windows,
        closed: Vec::new(),
        native,
        held_native: Vec::new(),
        focused_global: focused_global.to_owned(),
        foreground_known: !focused_global.is_empty(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        held_screens: Vec::new(),
        held_windows: Vec::new(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        cursor: None,
        cursor_hit: None,
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

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> WorldReplyV4 {
    match engine.handle_report(report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected Accepted, got {other:?}"),
    }
}

fn desire<'a>(reply: &'a WorldReplyV4, window: &str) -> &'a WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing WindowDesire for {window}: {reply:?}"))
}

fn host_owned_fs_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            host_owned: true,
            fullscreen: true,
            native_sticky: true,
            maximize: true,
            ..HostCapabilities::default()
        },
    )
    .expect("host-owned fullscreen engine")
}

fn native_free_fs_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            host_owned: false,
            window_suppression: true,
            fullscreen: true,
            maximize: true,
            ..HostCapabilities::default()
        },
    )
    .expect("native-free fullscreen engine")
}

#[test]
fn host_owned_workspace_switch_and_return_preserves_fullscreen_and_focuses_cover() {
    let mut engine = host_owned_fs_engine();

    // Bootstrap with two native desktops (N1 -> ws1, N2 -> ws2).
    // w1 is focused fullscreen on N1; w2 is tiled on N2.
    let w1_fs = host_fact("w1", "out-a", rect(0, 0, 800, 600), N1, true);
    let mut w2_away = host_fact("w2", "out-a", rect(0, 0, 400, 560), N2, false);
    w2_away.visibility.visible = Some(false);
    let boot = build_report(
        1,
        vec![w1_fs.clone(), w2_away.clone()],
        vec![native_obs("out-a", &[N1, N2], Some(N1))],
        "w1",
        None,
        None,
    );
    let boot_reply = accepted(&mut engine, &boot);
    let ws1 = boot_reply
        .workspaces
        .iter()
        .find(|w| w.native_binding.as_deref() == Some(N1))
        .expect("N1 bound")
        .id
        .clone();
    let ws2 = boot_reply
        .workspaces
        .iter()
        .find(|w| w.native_binding.as_deref() == Some(N2))
        .expect("N2 bound")
        .id
        .clone();

    // Verify initial fullscreen desire has no allocation, no frame, and no state writes.
    let w1_boot = desire(&boot_reply, "w1");
    assert_eq!(w1_boot.allocation, None);
    assert_eq!(w1_boot.effective_frame, None);
    assert_ne!(w1_boot.frame_authority, ScopeAuthority::Controlled);
    assert!(w1_boot.state.is_empty());
    assert_eq!(w1_boot.visibility, None);

    // Explicit switch from ws1 (focused fullscreen w1) to ws2.
    let switch_away = build_report(
        2,
        vec![w1_fs.clone(), w2_away],
        vec![native_obs("out-a", &[N1, N2], Some(N1))],
        "w1",
        Some(1),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: ws2,
        }),
    );
    let away_reply = accepted(&mut engine, &switch_away);
    let w1_away = desire(&away_reply, "w1");
    assert_eq!(w1_away.allocation, None, "no allocation on switch away");
    assert_eq!(
        w1_away.effective_frame, None,
        "no frame write on switch away"
    );
    assert_ne!(w1_away.frame_authority, ScopeAuthority::Controlled);
    assert!(w1_away.state.is_empty(), "no unsolicited fullscreen write");
    assert_eq!(w1_away.visibility, None);
    assert_eq!(w1_away.visibility_authority, ScopeAuthority::Untouched);
    assert_eq!(w1_away.frame_operation, None);
    assert_eq!(w1_away.clear_operation, None);
    assert!(away_reply.effects.is_empty());

    // Observe arrival on N2 with w2 visible and focused.
    let mut w1_off = w1_fs;
    w1_off.visibility.visible = Some(false);
    let w2_on = host_fact("w2", "out-a", rect(0, 0, 800, 560), N2, false);
    let on_ws2 = build_report(
        3,
        vec![w1_off.clone(), w2_on.clone()],
        vec![native_obs("out-a", &[N1, N2], Some(N2))],
        "w2",
        None,
        None,
    );
    let _ = accepted(&mut engine, &on_ws2);

    // Explicit return from ws2 back to ws1: must explicitly focus fullscreen cover w1
    // via native-owned awaitShown route, with no allocation/frame/state writes.
    let return_ws1 = build_report(
        4,
        vec![w1_off, w2_on],
        vec![native_obs("out-a", &[N1, N2], Some(N2))],
        "w2",
        Some(2),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: ws1,
        }),
    );
    let return_reply = accepted(&mut engine, &return_ws1);
    let focus_req = return_reply
        .focus_request
        .as_ref()
        .expect("return must issue explicit focus request for fullscreen cover");
    assert_eq!(focus_req.target.0, "w1");
    assert!(
        focus_req.after_shown.is_some(),
        "host-owned return to fullscreen cover must use awaitShown route: {focus_req:?}"
    );
    let w1_ret = desire(&return_reply, "w1");
    assert_eq!(w1_ret.allocation, None, "no allocation on return");
    assert_eq!(w1_ret.effective_frame, None, "no frame write on return");
    assert_ne!(w1_ret.frame_authority, ScopeAuthority::Controlled);
    assert!(w1_ret.state.is_empty(), "no unsolicited fullscreen write");
    assert_eq!(w1_ret.visibility, None);
    assert_eq!(w1_ret.visibility_authority, ScopeAuthority::Untouched);
    assert_eq!(w1_ret.frame_operation, None);
    assert_eq!(w1_ret.clear_operation, None);
    assert!(return_reply.effects.is_empty());
}

#[test]
fn host_owned_focused_fullscreen_toggle_orientation_and_move_refuse() {
    let mut engine = host_owned_fs_engine();

    // Start with two tiled windows w1 and w2 on N1 so w1 has a parent group.
    let w1_tiled = host_fact("w1", "out-a", rect(0, 0, 400, 560), N1, false);
    let w2_tiled = host_fact("w2", "out-a", rect(400, 0, 400, 560), N1, false);
    let boot = build_report(
        1,
        vec![w1_tiled, w2_tiled.clone()],
        vec![native_obs("out-a", &[N1], Some(N1))],
        "w1",
        None,
        None,
    );
    let boot_reply = accepted(&mut engine, &boot);
    let w2_initial_alloc = desire(&boot_reply, "w2").allocation;
    assert!(w2_initial_alloc.is_some(), "w2 initially tiled");

    // w1 enters fullscreen (observed fullscreen=true while focused).
    let w1_fs = host_fact("w1", "out-a", rect(0, 0, 800, 600), N1, true);
    let fs_obs = build_report(
        2,
        vec![w1_fs.clone(), w2_tiled.clone()],
        vec![native_obs("out-a", &[N1], Some(N1))],
        "w1",
        None,
        None,
    );
    let fs_reply = accepted(&mut engine, &fs_obs);
    let w2_before_toggle = desire(&fs_reply, "w2").allocation;

    // Focused fullscreen parent orientation toggle must refuse / no-op.
    let toggle_orient = build_report(
        3,
        vec![w1_fs.clone(), w2_tiled.clone()],
        vec![native_obs("out-a", &[N1], Some(N1))],
        "w1",
        Some(1),
        Some(WorldCommand::Operation(CoreCommand::ToggleOrientation {
            window: "w1".to_owned(),
        })),
    );
    let toggle_reply = accepted(&mut engine, &toggle_orient);
    assert!(
        !toggle_reply.command_consumed,
        "fullscreen ToggleOrientation must not consume as accepted mutation: {toggle_reply:?}"
    );
    assert_eq!(
        toggle_reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::OperationRefused,
        })
    );
    assert_eq!(desire(&toggle_reply, "w2").allocation, w2_before_toggle);
    let w1_toggle = desire(&toggle_reply, "w1");
    assert_eq!(w1_toggle.effective_frame, None);
    assert_eq!(w1_toggle.frame_authority, ScopeAuthority::Untouched);
    assert!(w1_toggle.state.is_empty());
    assert!(toggle_reply.effects.is_empty());
    assert_eq!(toggle_reply.focus_request, None);

    // Subsequent observation confirms w2 allocation is unchanged and w1 stays fullscreen/frameless.
    let after_toggle = build_report(
        4,
        vec![w1_fs.clone(), w2_tiled.clone()],
        vec![native_obs("out-a", &[N1], Some(N1))],
        "w1",
        None,
        None,
    );
    let after_reply = accepted(&mut engine, &after_toggle);
    assert_eq!(desire(&after_reply, "w2").allocation, w2_before_toggle);
    let w1_after = desire(&after_reply, "w1");
    assert_eq!(w1_after.effective_frame, None);
    assert_eq!(w1_after.frame_authority, ScopeAuthority::Untouched);
    assert!(w1_after.state.is_empty());

    // Keep existing fullscreen move fence: MoveDirectional on focused fullscreen w1 refuses.
    let move_fs = build_report(
        5,
        vec![w1_fs, w2_tiled],
        vec![native_obs("out-a", &[N1], Some(N1))],
        "w1",
        Some(2),
        Some(WorldCommand::MoveDirectional {
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            same_axis: SameAxisMove::default(),
        }),
    );
    let move_reply = accepted(&mut engine, &move_fs);
    assert!(
        move_reply.command_result.is_some(),
        "MoveDirectional on fullscreen produces directional refusal result: {move_reply:?}"
    );
    assert_eq!(desire(&move_reply, "w2").allocation, w2_before_toggle);
    assert!(desire(&move_reply, "w1").state.is_empty());
}

#[test]
fn workspace_tiling_toggle_off_and_on_preserves_fullscreen_without_automatic_effects() {
    let mut engine = host_owned_fs_engine();

    // Bootstrap with w1 (tiled) and w2 (tiled) on N1.
    let w1_tiled = host_fact("w1", "out-a", rect(0, 0, 400, 560), N1, false);
    let w2_tiled = host_fact("w2", "out-a", rect(400, 0, 400, 560), N1, false);
    let boot = build_report(
        1,
        vec![w1_tiled, w2_tiled.clone()],
        vec![native_obs("out-a", &[N1], Some(N1))],
        "w1",
        None,
        None,
    );
    let boot_reply = accepted(&mut engine, &boot);
    let ws1 = boot_reply
        .workspaces
        .iter()
        .find(|w| w.native_binding.as_deref() == Some(N1))
        .expect("N1 bound")
        .id
        .clone();

    // w1 becomes fullscreen.
    let w1_fs = host_fact("w1", "out-a", rect(0, 0, 800, 600), N1, true);
    let fs_obs = build_report(
        2,
        vec![w1_fs.clone(), w2_tiled.clone()],
        vec![native_obs("out-a", &[N1], Some(N1))],
        "w1",
        None,
        None,
    );
    let _ = accepted(&mut engine, &fs_obs);

    // Toggle workspace tiling OFF.
    let toggle_off = build_report(
        3,
        vec![w1_fs.clone(), w2_tiled.clone()],
        vec![native_obs("out-a", &[N1], Some(N1))],
        "w1",
        Some(1),
        Some(WorldCommand::ToggleWorkspaceTiling {
            output: OutputId("out-a".to_owned()),
            workspace: ws1.clone(),
        }),
    );
    let off_reply = accepted(&mut engine, &toggle_off);
    assert_eq!(
        off_reply
            .workspaces
            .iter()
            .find(|w| w.id == ws1)
            .expect("ws1")
            .tiling_mode,
        TilingMode::Floating
    );
    let w1_off = desire(&off_reply, "w1");
    assert_eq!(w1_off.allocation, None);
    assert_eq!(w1_off.effective_frame, None);
    assert_ne!(w1_off.frame_authority, ScopeAuthority::Controlled);
    assert!(w1_off.state.is_empty());
    assert_eq!(w1_off.frame_operation, None);
    assert_eq!(w1_off.clear_operation, None);
    assert!(off_reply.effects.is_empty());
    assert_eq!(off_reply.focus_request, None);
    assert_eq!(desire(&off_reply, "w2").allocation, None);

    // Toggle workspace tiling back ON.
    let toggle_on = build_report(
        4,
        vec![w1_fs, w2_tiled],
        vec![native_obs("out-a", &[N1], Some(N1))],
        "w1",
        Some(2),
        Some(WorldCommand::ToggleWorkspaceTiling {
            output: OutputId("out-a".to_owned()),
            workspace: ws1.clone(),
        }),
    );
    let on_reply = accepted(&mut engine, &toggle_on);
    assert_eq!(
        on_reply
            .workspaces
            .iter()
            .find(|w| w.id == ws1)
            .expect("ws1")
            .tiling_mode,
        TilingMode::Tiled
    );
    let w1_on = desire(&on_reply, "w1");
    assert_eq!(w1_on.allocation, None, "fullscreen w1 remains unallocated");
    assert_eq!(w1_on.effective_frame, None, "fullscreen w1 gets no frame");
    assert_ne!(w1_on.frame_authority, ScopeAuthority::Controlled);
    assert!(w1_on.state.is_empty(), "no unsolicited fullscreen write");
    assert_eq!(w1_on.frame_operation, None);
    assert_eq!(w1_on.clear_operation, None);
    assert!(on_reply.effects.is_empty(), "no automatic effects");
    assert_eq!(on_reply.focus_request, None, "no automatic focus request");
    let w2_on = desire(&on_reply, "w2");
    assert!(w2_on.allocation.is_some(), "sibling w2 re-tiles");
    assert_eq!(w2_on.frame_authority, ScopeAuthority::Controlled);
}

#[test]
fn native_free_workspace_return_to_fullscreen_preserves_safety_gates() {
    let mut engine = native_free_fs_engine();

    // Bootstrap native-free engine on ws-1 with focused fullscreen w1.
    let w1_fs = native_free_fact("w1", "out-a", rect(0, 0, 800, 600), true);
    let boot = build_report(1, vec![w1_fs.clone()], Vec::new(), "w1", None, None);
    let _ = accepted(&mut engine, &boot);

    // Switch to ws-2.
    let switch_ws2 = build_report(
        2,
        vec![w1_fs.clone()],
        Vec::new(),
        "w1",
        Some(1),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: "ws-2".to_owned(),
        }),
    );
    let _ = accepted(&mut engine, &switch_ws2);

    // Return to ws-1: native-free selection_route_eligible rejects fullscreen,
    // so focus_request falls back to legacy explicit focus (after_shown: None)
    // and Controlled visibility is withheld for the fullscreen row.
    let mut w1_hidden = w1_fs;
    w1_hidden.visibility.visible = Some(false);
    let return_ws1 = build_report(
        3,
        vec![w1_hidden],
        Vec::new(),
        "",
        Some(2),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: "ws-1".to_owned(),
        }),
    );
    let reply = accepted(&mut engine, &return_ws1);
    let focus_req = reply
        .focus_request
        .as_ref()
        .expect("legacy direct focus request emitted");
    assert_eq!(focus_req.target.0, "w1");
    assert_eq!(
        focus_req.after_shown, None,
        "native-free safety gate must not grant awaitShown for fullscreen: {focus_req:?}"
    );
    let w1_desire = desire(&reply, "w1");
    assert_ne!(
        w1_desire.visibility_authority,
        ScopeAuthority::Controlled,
        "native-free suppression gate must not grant Controlled visibility to fullscreen"
    );
    assert!(w1_desire.state.is_empty());
}
