#![forbid(unsafe_code)]
//! Targeted producer-shaped regressions for the single runtime WorldEngine
//! owner (stage 2b holder boundary).
//!
//! These pin the holder contract without live effects: observation dispatch
//! applies and projects a read-only presentation, select commands retain a
//! view transfer without self-echo, close receipts prune the native queue,
//! restoration never fabricates rings (memory tables are not durable; fresh
//! and staged states alike carry no ingress), and the T2 staged check leaves
//! the committed world unchanged on an unsupported tiled-to-float output
//! crossing (functional decline facts only, never `FloatOrigin`).

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
    // Fresh bootstrap carries no view receipts: the core mints
    // rings/views from the live screens, and receipts only ever echo
    // proven native arrival (never desire).
    assert!(report.views.is_empty(), "no receipts before proof");
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
    let presentation = world.presentation().expect("presentation");
    assert_eq!(presentation.revision, reply.state_revision);
    assert!(
        !presentation.views.is_empty(),
        "applied reply projects a view"
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
    // Fresh bootstrap proves nothing yet: no view receipts (the core
    // mints rings/views from the live screens).
    assert!(first.views.is_empty(), "no receipts before proof");
    let reply = match world.deliver_production(&first) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let mut pending = state.pending_closed.clone();
    let _ = world.ack(&reply, &mut pending);
    // Second observation still proves nothing (no native arrival proven):
    // the report stays receipt-free instead of echoing desire.
    world.invalidate();
    let second = world
        .try_dispatch_observation(&state.inputs(), Some(&observed), &retained, &areas, 0, 0)
        .expect("dispatch");
    assert_eq!(
        second.views, first.views,
        "no receipt before proof, got {:?}",
        second.views
    );
    let second_reply = match world.deliver_production(&second) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let mut pending = state.pending_closed.clone();
    let _ = world.ack(&second_reply, &mut pending);
    // Command an inactive core-minted workspace: the unproven report still
    // carries no view, so the flight retains without any self-echo.
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
                workspace: target.clone(),
            },
        )
        .expect("command dispatch");
    assert_eq!(
        commanded.views, second.views,
        "actual view proven, never desired, got {:?}",
        commanded.views
    );
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
fn t2_staged_decline_leaves_committed_world_unchanged() {
    // Production T2 refusal on the real send path: a tiled source fact plus
    // a cross-output directional move into a tiny occupied target floats the
    // staged row, so `deliver_production` refuses as `float-no-remap` with
    // the committed engine bit-identical (revision, retained transfers,
    // debug). Same-output transfers never decline, and observation-only
    // catch-up applies afterwards with no replay. Never reads `FloatOrigin`.
    use tiler_core::directional::{OutputId, SameAxisMove, WindowId};
    use tiler_core::size_hints::WindowSizeHints;
    use tiler_core::world_report::{ScreenReport, WORLD_CONTRACT_VERSION, WindowFact};

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    fn screen(output: &str, full: Rect, primary: bool) -> ScreenReport {
        ScreenReport {
            output: OutputId(output.to_owned()),
            full_rect: Some(full),
            work_area: Some(full),
            gap: 0,
            outer_gap: 0,
            complete: true,
            primary,
        }
    }
    fn fact(window: &str, output: &str, r: Rect) -> WindowFact {
        WindowFact {
            window: WindowId(window.to_owned()),
            output: OutputId(output.to_owned()),
            rect: Some(r),
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
            tiler_core::world_report::WorldReport {
                contract_version: WORLD_CONTRACT_VERSION,
                owner: owner.clone(),
                adapter_incarnation: owner.clone(),
                core_generation: generation.clone(),
                observation_seq: seq_no,
                request_seq: seq_no,
                request_id: format!("req-{seq_no}"),
                command_seq,
                command,
                workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
                screens: screens.clone(),
                windows,
                views: Vec::new(),
                closed: Vec::new(),
                host_owned_workspace_domains: false,
                equal_workspace_count: false,
                native: Vec::new(),
                fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
                migration_source_refill:
                    tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
                anchor_fallback: tiler_core::admission::AnchorFallback::default(),
                no_space_result: tiler_core::admission::NoSpaceResult::default(),
                switch_target: tiler_core::cursor_targeting::SwitchTarget::Cursor,
                fresh_placement: tiler_core::cursor_targeting::FreshPlacement::CursorOutputMru,
                focus_follows_cursor: false,
                cursor_warp: false,
                cursor: None,
                cursor_hit: None,
                default_tiled: true,
                focused_global: "win-a".to_owned(),
                inventory_complete: true,
                gesture_barrier: false,
                gesture: None,
                feedback: Vec::new(),
                monotonic_ms: 0,
                frames: Vec::new(),
                member_attempts: Vec::new(),
                view_attempts: Vec::new(),
                clear_attempts: Vec::new(),
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
    let before_debug = format!("{:?}", world.engine());
    let before_retained = world.engine().retained_transfers();
    // Cross-output directional move into the exhausted target: production
    // refuses without touching the committed world.
    let mover = WorldCommand::MoveDirectional {
        window: WindowId("win-a".to_owned()),
        direction: "right".to_owned(),
        same_axis: SameAxisMove::GroupWithNeighbor,
    };
    let refused = report_with(windows.clone(), Some(171), Some(mover));
    match world.deliver_production(&refused) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "float-no-remap"),
        other => panic!("expected float-no-remap, got {other:?}"),
    }
    assert_eq!(world.engine().state_revision(), base_rev);
    assert_eq!(world.engine().retained_transfers(), before_retained);
    assert_eq!(format!("{:?}", world.engine()), before_debug);
    // Observation-only catch-up applies with the source world intact.
    let catchup = report_with(windows.clone(), None, None);
    match world.deliver_production(&catchup) {
        WorldOutcome::Accepted(reply) => {
            assert!(reply.state_revision > base_rev);
            assert!(
                reply.transfers.is_empty(),
                "refused flight retains nothing: {:?}",
                reply.transfers
            );
        }
        other => panic!("expected catch-up accepted, got {other:?}"),
    }
}

#[test]
fn t2_supported_and_floating_movers_adopt_once() {
    // Roomy target adopts exactly once (Accepted, never Duplicate): the
    // pinned Window transfer lands under the command op id with one revision
    // step. An already-floating mover adopts too (no decline on floating
    // sources). Both evaluate the actual send through `deliver_production`.
    use tiler_core::directional::{OutputId, SameAxisMove, WindowId};
    use tiler_core::size_hints::WindowSizeHints;
    use tiler_core::world_report::{
        ScreenReport, TransferKind, WORLD_CONTRACT_VERSION, WindowFact,
    };

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    fn screen(output: &str, full: Rect, primary: bool) -> ScreenReport {
        ScreenReport {
            output: OutputId(output.to_owned()),
            full_rect: Some(full),
            work_area: Some(full),
            gap: 0,
            outer_gap: 0,
            complete: true,
            primary,
        }
    }
    fn fact(window: &str, output: &str, r: Rect, floating: bool) -> WindowFact {
        WindowFact {
            window: WindowId(window.to_owned()),
            output: OutputId(output.to_owned()),
            rect: Some(r),
            floating,
            fit_excluded: floating,
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
                tiler_core::world_report::WorldReport {
                    contract_version: WORLD_CONTRACT_VERSION,
                    owner: owner.clone(),
                    adapter_incarnation: owner.clone(),
                    core_generation: generation.clone(),
                    observation_seq: seq_no,
                    request_seq: seq_no,
                    request_id: format!("req-{seq_no}"),
                    command_seq,
                    command,
                    workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
                    screens: screens.clone(),
                    windows,
                    views: Vec::new(),
                    closed: Vec::new(),
                    host_owned_workspace_domains: false,
                    equal_workspace_count: false,
                    native: Vec::new(),
                    fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
                    migration_source_refill:
                        tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
                    anchor_fallback: tiler_core::admission::AnchorFallback::default(),
                    no_space_result: tiler_core::admission::NoSpaceResult::default(),
                    switch_target: tiler_core::cursor_targeting::SwitchTarget::Cursor,
                    fresh_placement: tiler_core::cursor_targeting::FreshPlacement::CursorOutputMru,
                    focus_follows_cursor: false,
                    cursor_warp: false,
                    cursor: None,
                    cursor_hit: None,
                    default_tiled: true,
                    focused_global: "win-a".to_owned(),
                    inventory_complete: true,
                    gesture_barrier: false,
                    gesture: None,
                    feedback: Vec::new(),
                    monotonic_ms: 0,
                    frames: Vec::new(),
                    member_attempts: Vec::new(),
                    view_attempts: Vec::new(),
                    clear_attempts: Vec::new(),
                }
            };
        let base = match world.deliver_production(&report_with(windows.clone(), None, None)) {
            WorldOutcome::Accepted(reply) => reply.state_revision,
            other => panic!("expected accepted, got {other:?}"),
        };
        // Already-floating movers send (moves require a tiled focused
        // window); tiled movers move. Both adopt once through production.
        let mover = if floating_source {
            WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "up".to_owned(),
                follow: true,
            }
        } else {
            WorldCommand::MoveDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "up".to_owned(),
                same_axis: SameAxisMove::GroupWithNeighbor,
            }
        };
        match world.deliver_production(&report_with(windows.clone(), Some(171), Some(mover))) {
            WorldOutcome::Accepted(reply) => {
                assert_eq!(
                    reply.state_revision,
                    base + 1,
                    "single evaluation adopts once"
                );
                let transfer = reply
                    .transfers
                    .iter()
                    .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a"))
                    .unwrap_or_else(|| panic!("mover transfer: {:?}", reply.transfers));
                assert_eq!(transfer.operation_id, "op-171");
                assert_eq!(transfer.target_output.0, "u2");
            }
            other => panic!("floating={floating_source}: expected adopted accept, got {other:?}"),
        }
    }
}
