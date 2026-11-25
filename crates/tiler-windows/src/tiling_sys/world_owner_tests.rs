#![forbid(unsafe_code)]
//! Targeted producer-shaped regressions for the single runtime WorldEngine
//! owner (stage 2b holder boundary).
//!
//! These pin the holder contract without live effects: observation dispatch
//! applies and projects a read-only presentation, select commands retain a
//! view transfer without self-echo, close receipts prune the native queue,
//! restoration never fabricates rings (memory tables are not durable; fresh
//! and staged states alike carry no ingress), and a clamped target leaves
//! the committed world unchanged without retaining a transfer.

use super::super::rmax03_adapter_tests;
use super::super::{MonitorArea, ObservedTarget, ObservedWindow, RetainedRow};
use super::AckOutcome;
use super::WorldOwner;
use tiler_core::geometry::Rect;
use tiler_core::world_engine::WorldOutcome;
use tiler_core::world_report::WorldCommand;

fn area() -> MonitorArea {
    MonitorArea {
        device: "DEV1".to_owned(),
        work: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 560,
        },
        full: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        dpi: 96,
    }
}

fn plain_facts() -> crate::tiling::WindowFacts {
    crate::tiling::WindowFacts {
        visible: true,
        minimized: false,
        maximized: false,
        cloaked: false,
        elevated: false,
        shell: false,
        tool_window: false,
        owned: false,
        captionless_fullscreen: false,
        no_activate: false,
        topmost: false,
        dialog: false,
    }
}

fn observed_window() -> ObservedWindow {
    let visible = Rect {
        x: 0,
        y: 0,
        w: 200,
        h: 200,
    };
    let outer = Rect {
        x: -8,
        y: -8,
        w: 216,
        h: 216,
    };
    ObservedWindow {
        hwnd: 101,
        token: "w1".to_owned(),
        outer,
        visible,
        insets: crate::tiling::FrameInsets::measure(outer, visible),
        identity: ObservedTarget {
            hwnd: 101,
            pid: 202,
            process_creation: "proc-1".to_owned(),
            exe_path: "C:\\Apps\\app.exe".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            tag: "tag-1".to_owned(),
        },
        facts: plain_facts(),
        member_tag: Some("tag-1".to_owned()),
    }
}

fn staged_state() -> super::super::TileLoop {
    let mut state = rmax03_adapter_tests::test_state();
    state.ensure_device("DEV1");
    let key = crate::native_guards::WindowKey {
        hwnd: 101,
        pid: 202,
        creation: "proc-1".to_owned(),
    };
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state.member_tags.insert(key.clone(), "tag-1".to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: 202,
            process_creation: "proc-1".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        },
    );
    state
}

#[test]
fn dispatch_reports_scripted_pointer_witness_and_warp_spend() {
    let state = staged_state();
    let owner = "tiler-windows".to_owned();
    let generation = "rmax03-adapter".to_owned();
    let mut world = WorldOwner::bootstrap_ids(&owner, &generation).expect("world bootstrap");
    let observed = vec![observed_window()];
    let retained: Vec<RetainedRow> = Vec::new();
    let areas = vec![area()];
    // Offline seam: script both fields exactly; never touch real raw input
    // or a real cursor setter.
    world.script_report_witness(Some(7), Some("warp-2".to_owned()));
    let report = world
        .try_dispatch_observation(&state.inputs(), Some(&observed), &retained, &areas, 0, 0)
        .expect("dispatch");
    assert_eq!(report.pointer_activity, Some(7));
    assert_eq!(report.warp_spent.as_deref(), Some("warp-2"));
}

#[test]
fn observation_dispatch_applies_and_projects_presentation() {
    let state = staged_state();
    // The holder bootstraps from the same owner/generation bindings
    // as the shared harness state.
    let owner = "tiler-windows".to_owned();
    let generation = "rmax03-adapter".to_owned();
    let mut world = WorldOwner::bootstrap_ids(&owner, &generation).expect("world bootstrap");
    let observed = vec![observed_window()];
    let retained: Vec<RetainedRow> = Vec::new();
    let areas = vec![area()];
    let report = world
        .try_dispatch_observation(&state.inputs(), Some(&observed), &retained, &areas, 0, 0)
        .expect("dispatch");
    let outcome = world.deliver_production(&report);
    let reply = match outcome {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(reply.ready);
    let mut pending = state.pending_closed.clone();
    let (ack, _) = world.ack(&reply, &mut pending);
    assert!(matches!(ack, AckOutcome::Applied(_)));
    assert!(world.is_ready());
    // Applied reply fields read directly: logical views per output plus
    // the uniform presentation section.
    let views: std::collections::BTreeMap<String, String> = reply
        .outputs
        .iter()
        .filter_map(|output| {
            output
                .logical_view
                .clone()
                .map(|view| (output.output.0.clone(), view))
        })
        .collect();
    assert!(!views.is_empty(), "applied reply projects a view");
    // Uniform presentation rides the same applied reply: the empty-foreground
    // test ingress hides both overlays, and the observed member paints with
    // its live frame. Border enabled is the core default; an absent target is
    // the truthful hide reason, not `Disabled`.
    assert_eq!(reply.presentation.foreground, "");
    assert_eq!(
        reply.presentation.border.reason,
        Some(tiler_core::world_report::BorderHideReasonV4::NoTarget)
    );
    assert!(matches!(
        reply.presentation.underlay,
        tiler_core::world_presentation::PresentationUnderlay::Hidden { .. }
    ));
    assert!(
        reply
            .presentation
            .pager
            .iter()
            .flat_map(|screen| screen.workspaces.iter())
            .flat_map(|workspace| workspace.windows.iter())
            .any(|window| window.window.0 == "w1"),
        "observed member paints from the applied reply"
    );
}

#[test]
fn select_command_retains_view_transfer_without_self_echo() {
    let state = staged_state();
    let owner = "tiler-windows".to_owned();
    let generation = "rmax03-adapter".to_owned();
    let mut world = WorldOwner::bootstrap_ids(&owner, &generation).expect("world bootstrap");
    let observed = vec![observed_window()];
    let retained: Vec<RetainedRow> = Vec::new();
    let areas = vec![area()];
    let first = world
        .try_dispatch_observation(&state.inputs(), Some(&observed), &retained, &areas, 0, 0)
        .expect("dispatch");
    let reply = match world.deliver_production(&first) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let mut pending = state.pending_closed.clone();
    let _ = world.ack(&reply, &mut pending);
    // Second observation still proves nothing (no native arrival proven):
    // v4 carries no view-receipt lane at all.
    world.invalidate();
    let second = world
        .try_dispatch_observation(&state.inputs(), Some(&observed), &retained, &areas, 0, 0)
        .expect("dispatch");
    let second_reply = match world.deliver_production(&second) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let mut pending = state.pending_closed.clone();
    let _ = world.ack(&second_reply, &mut pending);
    let out_token = state.output_token("DEV1");
    let current = second_reply
        .outputs
        .iter()
        .find(|o| o.output.0 == out_token)
        .and_then(|o| o.logical_view.clone())
        .expect("current view");
    let target = second_reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == out_token && row.id != current)
        .expect("second workspace")
        .id
        .clone();
    world.invalidate();
    let commanded = world
        .try_dispatch_command(
            &state.inputs(),
            Some(&observed),
            &retained,
            &areas,
            0,
            0,
            WorldCommand::SelectWorkspace {
                output: tiler_core::directional::OutputId(out_token),
                workspace: tiler_core::directional::WorkspaceId(target.clone()),
            },
            None,
        )
        .expect("command dispatch");
    let reply = match world.deliver_production(&commanded) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(
        reply.transfers.iter().any(|t| t.target_workspace == target),
        "view flight retained, got {:?}",
        reply.transfers
    );
}

#[test]
fn clamped_target_holds_scope_without_transfer() {
    // v4 core owns cross-output capacity: a tiled mover aimed at a tiny
    // occupied target holds the target scope as `ConstraintClamp` without
    // consuming the command or floating a transferred row. The report advances
    // one revision but retains no transfer; observation-only catch-up
    // afterwards applies with no transfer.
    use tiler_core::directional::{OutputId, SameAxisMove, WindowId};
    use tiler_core::size_hints::WindowSizeHints;
    use tiler_core::world_report::{ScreenReport, WORLD_CONTRACT_VERSION, WindowFact};

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    fn screen(output: &str, full: Rect, primary: bool) -> ScreenReport {
        let _ = primary;
        ScreenReport {
            output: OutputId(output.to_owned()),
            full_rect: Some(full),
            work_area: Some(full),
            complete: true,
            scale: 1.0,
            identity: tiler_core::world_report::OutputIdentity::unknown(),
        }
    }
    fn fact(window: &str, output: &str, r: Rect) -> WindowFact {
        WindowFact {
            window: WindowId(window.to_owned()),
            output: Some(OutputId(output.to_owned())),
            rect: Some(r),
            fullscreen: Some(false),
            maximized: Some(false),
            sticky: None,
            minimized: Some(false),
            hints: Some(WindowSizeHints::none()),
            native_workspace: tiler_core::world_report::NativeWorkspaceFact::Omitted,
            host_hidden: Some(false),
            privileged: None,
            managed: None,
            accepts_input: None,
            activation_inhibited: None,
            tool_window: Some(false),
            owned: Some(false),
            transient: None,
            modal: None,
            resizable: None,
            above: None,
            below: None,
            owner: None,
            parent: None,
            role: None,
            app_id: None,
            executable: Some("C:\\Apps\\test.exe".to_owned()),
            process_id: Some("424242".to_owned()),
            class: None,
            title: None,
            window_type: Some(tiler_core::world_report::WindowType::Normal),
            activation_policy: None,
            layer: None,
            fullscreen_owner: None,
            fullscreen_exitable: None,
            suppression_available: None,
            visibility: tiler_core::world_report::VisibilityFact {
                visible: Some(true),
                suppression_owned: None,
            },
        }
    }
    let owner = "tiler-windows".to_owned();
    let generation = "rmax03-adapter".to_owned();
    let mut world = WorldOwner::bootstrap_ids(&owner, &generation).expect("world bootstrap");
    // Tiny occupied target: the directional selector still picks out-b, but
    // the affected projection exhausts and floats the staged mover row.
    let screens = vec![
        screen("out-a", rect(0, 0, 800, 600), true),
        screen("out-b", rect(800, 0, 1, 1), false),
    ];
    let windows = vec![
        fact("win-a", "out-a", rect(0, 0, 40, 90)),
        fact("win-t", "out-b", rect(800, 0, 1, 1)),
    ];
    let mut seq_no = 0u64;
    let mut report_with =
        |windows: Vec<WindowFact>, command_seq: Option<u64>, command: Option<WorldCommand>| {
            seq_no += 1;
            tiler_core::world_report::WorldReportV4 {
                contract_version: WORLD_CONTRACT_VERSION,
                adapter_incarnation: owner.clone(),
                core_generation: generation.clone(),
                observation_seq: seq_no,
                request_seq: seq_no,
                request_id: format!("req-{seq_no}"),
                command_seq,
                command,
                screens: screens.clone(),
                held_screens: Vec::new(),
                windows,
                held_windows: Vec::new(),
                closed: Vec::new(),
                native: Vec::new(),
                held_native: Vec::new(),
                cursor: None,
                pointer_activity: None,
                warp_spent: None,
                cursor_hit: None,
                focused_global: String::new(),
                foreground_known: false,
                focus_changes: None,
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
                creation_attempts: Vec::new(),
                held_attempts: Vec::new(),
                state_attempts: Vec::new(),
                visibility_attempts: Vec::new(),
                focus_feedback: Vec::new(),
                presentation: tiler_core::world_report::PresentationRowV4::Valid(
                    tiler_core::world_report::PresentationIngressV4::default(),
                ),
                intent_records: Vec::new(),
                renderer_input: None,
            }
        };
    let first = report_with(windows.clone(), None, None);
    let base_rev = match world.deliver_production(&first) {
        WorldOutcome::Accepted(reply) => {
            assert!(reply.ready);
            reply.state_revision
        }
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(base_rev > 0);
    let before_retained = world.engine().retained_transfers();
    // Cross-output directional move into the exhausted target: the core
    // holds the target scope and leaves the command unconsumed.
    let mover = WorldCommand::MoveDirectional {
        window: tiler_core::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
        direction: tiler_core::directional::Direction::Right,
        same_axis: SameAxisMove::GroupWithNeighbor,
    };
    let clamped = report_with(windows.clone(), Some(171), Some(mover));
    match world.deliver_production(&clamped) {
        WorldOutcome::Accepted(reply) => {
            assert!(!reply.command_consumed);
            assert!(
                reply.transfers.is_empty(),
                "clamped target retains no transfer: {:?}",
                reply.transfers
            );
            assert!(
                reply.scopes.iter().any(|scope| matches!(
                    scope.authority,
                    tiler_core::world_report::ScopeAuthority::Hold(
                        tiler_core::world_report::HoldReason::ConstraintClamp
                    )
                )),
                "target scope clamps: {:?}",
                reply.scopes
            );
        }
        other => panic!("expected clamp acceptance, got {other:?}"),
    }
    assert_eq!(world.engine().state_revision(), base_rev + 1);
    assert_eq!(world.engine().retained_transfers(), before_retained);
    // Observation-only catch-up applies without a transfer.
    let catchup = report_with(windows.clone(), None, None);
    match world.deliver_production(&catchup) {
        WorldOutcome::Accepted(reply) => {
            assert!(reply.state_revision >= base_rev);
            assert!(
                reply.transfers.is_empty(),
                "clamped flight retains nothing: {:?}",
                reply.transfers
            );
        }
        other => panic!("expected catch-up accepted, got {other:?}"),
    }
}

#[test]
fn tiled_mover_adopts_once_and_floating_mover_half_snaps() {
    // A tiled mover aimed at a roomy target adopts exactly once (Accepted,
    // never Duplicate): the pinned Window transfer lands under the command
    // op id with one revision step. v4 core owns a floating mover's
    // directional move as a local half-snap with no cross-output transfer.
    use tiler_core::directional::{OutputId, SameAxisMove, WindowId};
    use tiler_core::size_hints::WindowSizeHints;
    use tiler_core::world_report::{
        ScreenReport, TransferKind, WORLD_CONTRACT_VERSION, WindowFact,
    };

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    fn screen(output: &str, full: Rect, primary: bool) -> ScreenReport {
        let _ = primary;
        ScreenReport {
            output: OutputId(output.to_owned()),
            full_rect: Some(full),
            work_area: Some(full),
            complete: true,
            scale: 1.0,
            identity: tiler_core::world_report::OutputIdentity::unknown(),
        }
    }
    fn fact(window: &str, output: &str, r: Rect, floating: bool) -> WindowFact {
        // A dialog kind is the v4 float origin for this half-snap proof: the
        // fixed-size alternative would legitimately refuse the 1920x540 half
        // snap at the constraint gate.
        let window_type = if floating {
            tiler_core::world_report::WindowType::Dialog
        } else {
            tiler_core::world_report::WindowType::Normal
        };
        WindowFact {
            window: WindowId(window.to_owned()),
            output: Some(OutputId(output.to_owned())),
            rect: Some(r),
            fullscreen: Some(false),
            maximized: Some(false),
            sticky: None,
            minimized: Some(false),
            hints: Some(WindowSizeHints::none()),
            native_workspace: tiler_core::world_report::NativeWorkspaceFact::Omitted,
            host_hidden: Some(false),
            privileged: None,
            managed: None,
            accepts_input: None,
            activation_inhibited: None,
            tool_window: Some(false),
            owned: Some(false),
            transient: None,
            modal: None,
            resizable: None,
            above: None,
            below: None,
            owner: None,
            parent: None,
            role: None,
            app_id: None,
            executable: Some("C:\\Apps\\test.exe".to_owned()),
            process_id: Some("424242".to_owned()),
            class: None,
            title: None,
            window_type: Some(window_type),
            activation_policy: None,
            layer: None,
            fullscreen_owner: None,
            fullscreen_exitable: None,
            suppression_available: None,
            visibility: tiler_core::world_report::VisibilityFact {
                visible: Some(true),
                suppression_owned: None,
            },
        }
    }
    for floating_source in [false, true] {
        let owner = "tiler-windows".to_owned();
        let generation = "rmax03-adapter".to_owned();
        let mut world = WorldOwner::bootstrap_ids(&owner, &generation).expect("world bootstrap");
        // Stacked topology from the core routing contract: the mover frame
        // selects the upward target through centre/span ranking.
        let screens = vec![
            screen("src", rect(0, 1080, 1920, 1080), false),
            screen("u1", rect(0, 0, 960, 1080), false),
            screen("u2", rect(960, 0, 960, 1080), true),
        ];
        let windows = vec![fact(
            "win-a",
            "src",
            rect(860, 1150, 200, 200),
            floating_source,
        )];
        let mut seq_no = 0u64;
        let mut report_with =
            |windows: Vec<WindowFact>, command_seq: Option<u64>, command: Option<WorldCommand>| {
                seq_no += 1;
                tiler_core::world_report::WorldReportV4 {
                    contract_version: WORLD_CONTRACT_VERSION,
                    adapter_incarnation: owner.clone(),
                    core_generation: generation.clone(),
                    observation_seq: seq_no,
                    request_seq: seq_no,
                    request_id: format!("req-{seq_no}"),
                    command_seq,
                    command,
                    screens: screens.clone(),
                    held_screens: Vec::new(),
                    windows,
                    held_windows: Vec::new(),
                    closed: Vec::new(),
                    native: Vec::new(),
                    held_native: Vec::new(),
                    cursor: None,
                    pointer_activity: None,
                    warp_spent: None,
                    cursor_hit: None,
                    focused_global: String::new(),
                    foreground_known: false,
                    focus_changes: None,
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
                    creation_attempts: Vec::new(),
                    held_attempts: Vec::new(),
                    state_attempts: Vec::new(),
                    visibility_attempts: Vec::new(),
                    focus_feedback: Vec::new(),
                    presentation: tiler_core::world_report::PresentationRowV4::Valid(
                        tiler_core::world_report::PresentationIngressV4::default(),
                    ),
                    intent_records: Vec::new(),
                    renderer_input: None,
                }
            };
        let base = match world.deliver_production(&report_with(windows.clone(), None, None)) {
            WorldOutcome::Accepted(reply) => reply.state_revision,
            other => panic!("expected accepted, got {other:?}"),
        };
        // Both union members use the same directional move: v4 routes a
        // tiled mover across outputs and half-snaps a floating mover locally.
        let mover = WorldCommand::MoveDirectional {
            window: tiler_core::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
            direction: tiler_core::directional::Direction::Up,
            same_axis: SameAxisMove::GroupWithNeighbor,
        };
        match world.deliver_production(&report_with(windows.clone(), Some(171), Some(mover))) {
            WorldOutcome::Accepted(reply) => {
                assert_eq!(
                    reply.state_revision,
                    base + 1,
                    "single evaluation adopts once"
                );
                if floating_source {
                    assert!(
                        reply.transfers.is_empty(),
                        "floating half-snap retains no transfer: {:?}",
                        reply.transfers
                    );
                    let row = reply
                        .windows
                        .iter()
                        .find(|row| row.lifetime.0 == "win-a")
                        .expect("floating mover row");
                    assert!(row.floating);
                    assert_eq!(row.output.0, "src");
                    assert!(matches!(
                        world.last_command_result(),
                        Some(tiler_core::world_report::CommandResult::Directional {
                            outcome:
                                tiler_core::world_directional::DirectionalOutcome::MoveFloatApplied,
                            ..
                        })
                    ));
                } else {
                    let transfer = reply
                        .transfers
                        .iter()
                        .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a"))
                        .unwrap_or_else(|| {
                            panic!(
                                "tiled mover transfer: {:?} outputs={:?} windows={:?} diag={:?}",
                                reply.transfers, reply.outputs, reply.windows, reply.diagnostics
                            )
                        });
                    assert_eq!(transfer.operation_id, "op-171");
                    assert_eq!(transfer.target_output.0, "u2");
                }
            }
            other => panic!("floating={floating_source}: expected accepted, got {other:?}"),
        }
    }
}

#[test]
fn trace_proof_advances_with_snapshot_and_ignores_replay() {
    // The bounded accepted trace-proof row advances with the snapshot it
    // proves and survives non-applied outcomes: a replayed ack consumes no
    // flight (Ignored) and retains the fallback snapshot; later state can
    // never relabel the captured row.
    let state = staged_state();
    let owner = "tiler-windows".to_owned();
    let generation = "rmax03-adapter".to_owned();
    let mut world = WorldOwner::bootstrap_ids(&owner, &generation).expect("world bootstrap");
    assert!(world.trace_proof().is_none());
    let observed = vec![observed_window()];
    let retained: Vec<RetainedRow> = Vec::new();
    let areas = vec![area()];
    let report = world
        .try_dispatch_observation(&state.inputs(), Some(&observed), &retained, &areas, 0, 0)
        .expect("dispatch");
    let reply = match world.deliver_production(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let mut pending = state.pending_closed.clone();
    let (ack, _) = world.ack(&reply, &mut pending);
    assert!(matches!(ack, AckOutcome::Applied(_)));
    let mut row = serde_json::json!({
        "event": "world-apply",
        "delivery": "accepted",
        "request": reply.request_id.clone(),
        "revision": reply.state_revision,
        "observation": reply.based_on_observation_seq,
    });
    world.note_trace_proof(Some(row.clone()));
    assert_eq!(world.trace_proof().expect("proof stored"), &row);
    // Later state cannot relabel the captured proof: mutating the source
    // leaves the stored snapshot untouched.
    row["revision"] = serde_json::json!(9999);
    assert_ne!(
        world.trace_proof().expect("proof retained")["revision"],
        serde_json::json!(9999)
    );
    // A replayed ack consumes no flight (Ignored) and retains the fallback.
    let (replay, _) = world.ack(&reply, &mut pending);
    assert!(matches!(replay, AckOutcome::Ignored { .. }));
    assert!(world.trace_proof().is_some(), "ignored ack retains proof");
    // Explicit clear drops it.
    world.note_trace_proof(None);
    assert!(world.trace_proof().is_none());
}

/// Presentation foreground fact for the R20 witness tests: one genuinely
/// observed lifetime plus the frame read in the same capture.
fn foreground_fact(
    token: &str,
    frame: Option<Rect>,
) -> super::super::world_capture::PresentationForeground {
    super::super::world_capture::PresentationForeground {
        token: token.to_owned(),
        minimized: false,
        maximized: false,
        fullscreen: false,
        host_hidden: false,
        hidden_workspace: false,
        frame,
    }
}

fn dispatch_observation_with_foreground(
    inputs: &super::super::world_capture::OwnerInputs<'_>,
    observed: &[ObservedWindow],
) -> (WorldOwner, tiler_core::world_report::WorldReportV4) {
    let owner = "tiler-windows".to_owned();
    let generation = "rmax03-adapter".to_owned();
    let mut world = WorldOwner::bootstrap_ids(&owner, &generation).expect("world bootstrap");
    let retained: Vec<RetainedRow> = Vec::new();
    let areas = vec![area()];
    let report = world
        .try_dispatch_observation(inputs, Some(observed), &retained, &areas, 0, 0)
        .expect("dispatch");
    (world, report)
}

#[test]
fn assemble_witness_names_the_observed_foreground_frame() {
    use tiler_core::directional::WindowId;
    let state = staged_state();
    let observed = vec![observed_window()];
    let frame = Rect {
        x: 0,
        y: 0,
        w: 200,
        h: 200,
    };
    let mut inputs = state.inputs();
    inputs.presentation_foreground = Some(foreground_fact("w1", Some(frame)));
    let (mut world, report) = dispatch_observation_with_foreground(&inputs, &observed);
    let reply = match world.deliver_production(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let ingress = match &report.presentation {
        tiler_core::world_report::PresentationRowV4::Valid(ingress) => ingress,
        other => panic!("expected a valid presentation ingress, got {other:?}"),
    };
    let witness = ingress
        .witness
        .as_ref()
        .expect("joined foreground frame yields a witness");
    assert_eq!(witness.win, WindowId("w1".to_owned()));
    assert_eq!(witness.foreground, WindowId("w1".to_owned()));
    assert_eq!(witness.frame, frame);
    // The witness observation leg is this report's own capture watermark.
    assert_eq!(witness.obs, reply.based_on_observation_seq);
    assert_eq!(reply.presentation.foreground, "w1");
}

#[test]
fn assemble_without_a_readable_foreground_frame_stays_unwitnessed() {
    let state = staged_state();
    let observed = vec![observed_window()];
    let mut inputs = state.inputs();
    inputs.presentation_foreground = Some(foreground_fact("w1", None));
    let (mut world, report) = dispatch_observation_with_foreground(&inputs, &observed);
    let reply = match world.deliver_production(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let ingress = match &report.presentation {
        tiler_core::world_report::PresentationRowV4::Valid(ingress) => ingress,
        other => panic!("expected a valid presentation ingress, got {other:?}"),
    };
    assert!(ingress.witness.is_none());
    assert_eq!(reply.presentation.foreground, "w1");
    assert!(ingress.foreground_out.is_none());
}

#[test]
fn assemble_refuses_a_witness_when_fact_and_frame_disagree() {
    use tiler_core::directional::OutputId;
    let state = staged_state();
    let observed = vec![observed_window()];
    // Same output, different rectangle: the WindowFact and the presentation
    // frame are not the same observation, so the border witness stays
    // unknown even though the observed output itself still resolves.
    let mut inputs = state.inputs();
    inputs.presentation_foreground = Some(foreground_fact(
        "w1",
        Some(Rect {
            x: 10,
            y: 0,
            w: 50,
            h: 50,
        }),
    ));
    let (mut world, report) = dispatch_observation_with_foreground(&inputs, &observed);
    let reply = match world.deliver_production(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let ingress = match &report.presentation {
        tiler_core::world_report::PresentationRowV4::Valid(ingress) => ingress,
        other => panic!("expected a valid presentation ingress, got {other:?}"),
    };
    assert!(ingress.witness.is_none());
    assert_eq!(reply.presentation.foreground, "w1");
    assert_eq!(
        ingress.foreground_out,
        Some(OutputId(state.output_token("DEV1")))
    );
}

/// Seed one applied observation and return the exact output wire token, the
/// live current view, a valid sibling workspace, and the applied presentation
/// revision that a pager cell would have been drawn from.
fn seeded_select_ground(
    world: &mut WorldOwner,
    state: &super::super::TileLoop,
    observed: &[ObservedWindow],
) -> (String, String, String, u64) {
    let retained: Vec<RetainedRow> = Vec::new();
    let areas = vec![area()];
    let report = world
        .try_dispatch_observation(&state.inputs(), Some(observed), &retained, &areas, 0, 0)
        .expect("dispatch");
    let reply = match world.deliver_production(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let mut pending = state.pending_closed.clone();
    let _ = world.ack(&reply, &mut pending);
    // A second observation settles the full workspace projection (the
    // bootstrap reply lists only the proven view); this mirrors the
    // production select path's pre-command observation.
    world.invalidate();
    let second = world
        .try_dispatch_observation(&state.inputs(), Some(observed), &retained, &areas, 0, 0)
        .expect("dispatch");
    let reply = match world.deliver_production(&second) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let mut pending = state.pending_closed.clone();
    let _ = world.ack(&reply, &mut pending);
    let out_token = state.output_token("DEV1");
    let current = reply
        .outputs
        .iter()
        .find(|o| o.output.0 == out_token)
        .and_then(|o| o.logical_view.clone())
        .expect("current view");
    let target = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == out_token && row.id != current)
        .expect("second workspace")
        .id
        .clone();
    (out_token, current, target, reply.presentation.rev)
}

fn workspace_renderer(rev: u64, out: &str, ws: &str) -> tiler_core::world_report::RendererInput {
    use tiler_core::directional::{OutputId, WorkspaceId};
    tiler_core::world_report::RendererInput {
        rev,
        target: tiler_core::world_report::RenderTarget::Workspace {
            out: OutputId(out.to_owned()),
            ws: WorkspaceId(ws.to_owned()),
        },
    }
}

#[test]
fn command_report_carries_the_exact_renderer_input() {
    use tiler_core::directional::{OutputId, WorkspaceId};
    let state = staged_state();
    let observed = vec![observed_window()];
    let owner = "tiler-windows".to_owned();
    let generation = "rmax03-adapter".to_owned();
    let mut world = WorldOwner::bootstrap_ids(&owner, &generation).expect("world bootstrap");
    let (out_token, _current, target, rev) = seeded_select_ground(&mut world, &state, &observed);
    let input = workspace_renderer(rev, &out_token, &target);
    let retained: Vec<RetainedRow> = Vec::new();
    let areas = vec![area()];
    world.invalidate();
    let report = world
        .try_dispatch_command(
            &state.inputs(),
            Some(&observed),
            &retained,
            &areas,
            0,
            0,
            WorldCommand::SelectWorkspace {
                output: OutputId(out_token),
                workspace: WorkspaceId(target.clone()),
            },
            Some(input.clone()),
        )
        .expect("command dispatch");
    assert_eq!(report.renderer_input, Some(input));
    assert_eq!(report.command_seq, Some(1));
    let outcome = world.deliver_production(&report);
    assert!(matches!(outcome, WorldOutcome::Accepted(_)));
    assert_eq!(
        world.last_command_result(),
        None,
        "accepted select has no refusal result"
    );
}

#[test]
fn stale_drawn_revision_refuses_without_applying() {
    use tiler_core::directional::{OutputId, WorkspaceId};
    use tiler_core::world_report::{CommandRejectKind, CommandResult};
    let state = staged_state();
    let observed = vec![observed_window()];
    let owner = "tiler-windows".to_owned();
    let generation = "rmax03-adapter".to_owned();
    let mut world = WorldOwner::bootstrap_ids(&owner, &generation).expect("world bootstrap");
    let (out_token, current, target, _rev) = seeded_select_ground(&mut world, &state, &observed);
    let input = workspace_renderer(u64::MAX, &out_token, &target);
    let retained: Vec<RetainedRow> = Vec::new();
    let areas = vec![area()];
    world.invalidate();
    let report = world
        .try_dispatch_command(
            &state.inputs(),
            Some(&observed),
            &retained,
            &areas,
            0,
            0,
            WorldCommand::SelectWorkspace {
                output: OutputId(out_token.clone()),
                workspace: WorkspaceId(target.clone()),
            },
            Some(input.clone()),
        )
        .expect("command dispatch");
    assert_eq!(report.renderer_input, Some(input));
    let outcome = world.deliver_production(&report);
    assert!(
        matches!(outcome, WorldOutcome::Accepted(_)),
        "refused command still settles on a report: {outcome:?}"
    );
    assert_eq!(
        world.last_command_result(),
        Some(&CommandResult::Rejected {
            kind: CommandRejectKind::StaleRenderTarget
        })
    );
    assert_eq!(
        world.engine().logical_view(&out_token).as_deref(),
        Some(current.as_str()),
        "a stale drawn revision must not apply the view"
    );
}
