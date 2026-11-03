//! Floating membership retention: toggling a workspace to floating
//! releases the Engine session, but the core membership owner keeps every
//! member homed. Switching the view then repeating the unchanged
//! observation never rehomes floating members or authorizes frame writes;
//! re-tiling restores float intent.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    CommandRejectKind, CommandResult, ScopeAuthority, ScreenReport, ScreenView,
    WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
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

fn float_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        transient: Some(true),
        ..fact(window, output, x)
    }
}

fn fixed_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        hints: Some(WindowSizeHints {
            min_w: Some(40),
            min_h: Some(90),
            max_w: Some(40),
            max_h: Some(90),
        }),
        ..fact(window, output, x)
    }
}

fn minimized_fact(window: &str, output: &str) -> WindowFact {
    WindowFact {
        minimized: Some(true),
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
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        screens: vec![screen("out-a", 0), screen("out-b", 800)],
        windows,
        views: views
            .into_iter()
            .map(|(output, view)| ScreenView {
                output: OutputId(output.to_owned()),
                view: view.to_owned(),
            })
            .collect(),
        closed: vec![],
        native: vec![],
        cursor: None,
        cursor_hit: None,
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        held_screens: Vec::new(),
        held_windows: Vec::new(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: crate::world_report::PresentationIngress::disabled(),

        intent_records: Vec::new(),
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

#[test]
fn floating_toggle_keeps_membership_across_view_switch_and_unchanged_report() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
    let float_key = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
    };
    assert!(
        engine.engine().session(&float_key).is_none(),
        "toggle releases the layout session (old homing source is empty)"
    );
    let scope = toggled
        .scopes
        .iter()
        .find(|status| {
            status
                .scope
                .covers_workspace(&OutputId("out-a".into()), &view_a)
        })
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
        .find(|status| {
            status
                .scope
                .covers_workspace(&OutputId("out-a".into()), &view_a)
        })
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

fn screen_rect(output: &str, full: Rect, work: Rect) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(full),
        work_area: Some(work),
        complete: true,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    }
}

fn fact_at(window: &str, output: &str, rect: Rect) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId(output.to_owned())),
        rect: Some(rect),
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

#[allow(clippy::too_many_arguments)]
fn report_screens(
    request_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        screens,
        windows,
        views: Vec::new(),
        closed: vec![],
        native: vec![],
        cursor: None,
        cursor_hit: None,
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        held_screens: Vec::new(),
        held_windows: Vec::new(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        creation_attempts: Vec::new(),
        presentation: crate::world_report::PresentationIngress::disabled(),

        intent_records: Vec::new(),
    }
}

fn migrate_floating_workspace(
    screens: Vec<ScreenReport>,
    members: Vec<WindowFact>,
) -> crate::world_report::WorldReply {
    migrated_session(screens, members, false).1
}

fn migrated_session(
    screens: Vec<ScreenReport>,
    members: Vec<WindowFact>,
    host_owned: bool,
) -> (WorldEngine, crate::world_report::WorldReply, String) {
    fn owned_report(
        seq: u64,
        screens: Vec<ScreenReport>,
        mut members: Vec<WindowFact>,
        command_seq: Option<u64>,
        command: Option<WorldCommand>,
    ) -> WorldReport {
        use crate::world_report::{NativeObservation, NativeOccupancy};
        for fact in &mut members {
            fact.native_workspace = crate::world_report::NativeWorkspaceFact::Value(format!(
                "native-{}-0",
                fact.output
                    .as_ref()
                    .expect("fact output")
                    .0
                    .trim_start_matches("out-")
            ));
        }
        let mut report = report_screens(seq, screens, members, command_seq, command);
        report.native = ["out-a", "out-b"]
            .into_iter()
            .map(|output| NativeObservation {
                output: OutputId(output.to_owned()),
                native_order: vec![format!("native-{}-0", output.trim_start_matches("out-"))],
                current_native: Some(format!("native-{}-0", output.trim_start_matches("out-"))),
                occupancy: vec![NativeOccupancy {
                    native: format!("native-{}-0", output.trim_start_matches("out-")),
                    count: None,
                }],
            })
            .collect();
        report
    }

    let mut engine = if host_owned {
        // Host-owned receipts with native-supported facts: the oracle
        // facts carry non-null native `maximized`, which needs
        // `caps.maximize == true` to stay valid (unsupported caps would
        // grade every row to a narrow conflicting hold). The
        // ANY-non-null unsupported grading itself is unchanged.
        WorldEngine::new_test_bound_with_capabilities(
            "gen-1",
            "adapter-1",
            crate::connection::HostCapabilities {
                host_owned: true,
                native_sticky: true,
                maximize: true,
                ..crate::connection::HostCapabilities::default()
            },
        )
        .expect("generation")
    } else {
        WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
    };
    let first = accepted(
        &mut engine,
        &if host_owned {
            owned_report(1, screens.clone(), members.clone(), None, None)
        } else {
            report_screens(1, screens.clone(), members.clone(), None, None)
        },
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    accepted(
        &mut engine,
        &if host_owned {
            owned_report(
                2,
                screens.clone(),
                members.clone(),
                Some(11),
                Some(WorldCommand::SetWorkspaceTiling {
                    output: OutputId("out-a".to_owned()),
                    workspace: view_a.clone(),
                    tiled: false,
                }),
            )
        } else {
            report_screens(
                2,
                screens.clone(),
                members.clone(),
                Some(11),
                Some(WorldCommand::SetWorkspaceTiling {
                    output: OutputId("out-a".to_owned()),
                    workspace: view_a.clone(),
                    tiled: false,
                }),
            )
        },
    );
    let migrated = accepted(
        &mut engine,
        &if host_owned {
            owned_report(
                3,
                screens.clone(),
                members,
                Some(12),
                Some(WorldCommand::MigrateWorkspace {
                    source_output: OutputId("out-a".to_owned()),
                    target_output: OutputId("out-b".to_owned()),
                    workspace: view_a.clone(),
                }),
            )
        } else {
            report_screens(
                3,
                screens.clone(),
                members,
                Some(12),
                Some(WorldCommand::MigrateWorkspace {
                    source_output: OutputId("out-a".to_owned()),
                    target_output: OutputId("out-b".to_owned()),
                    workspace: view_a.clone(),
                }),
            )
        },
    );
    (engine, migrated, view_a)
}

fn row_of<'a>(
    reply: &'a crate::world_report::WorldReply,
    window: &str,
) -> &'a crate::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing row {window}"))
}

#[test]
fn floating_source_migration_asymmetric_constrains_placement_to_destination() {
    // A FULL-delta translation of the source-edge member lands off the
    // small destination (1500,500 past the 1200px edge), so the core must
    // constrain the portable placement into the destination work area
    // instead of stranding the member off-screen. Size is preserved.
    let screens = vec![
        screen_rect("out-a", rect(0, 0, 800, 600), rect(0, 0, 800, 600)),
        screen_rect("out-b", rect(800, 0, 400, 300), rect(800, 0, 400, 300)),
    ];
    let members = vec![
        fact_at("w0", "out-a", rect(700, 500, 40, 90)),
        WindowFact {
            transient: Some(true),
            ..fact_at("wf", "out-a", rect(100, 100, 40, 90))
        },
    ];
    let reply = migrate_floating_workspace(screens, members);
    let row = row_of(&reply, "w0");
    assert_eq!(row.output.0.as_str(), "out-b", "{row:?}");
    assert_eq!(
        row.membership_authority,
        ScopeAuthority::Controlled,
        "{row:?}"
    );
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched, "{row:?}");
    assert!(row.allocation.is_none(), "{row:?}");
    assert_eq!(
        row.effective_frame,
        Some(rect(1160, 210, 40, 90)),
        "translated then clamped into the destination work area: {row:?}"
    );
    // Float members keep membership-only behavior: no placement, no frame
    // writes, membership still desired at the target.
    let float_row = row_of(&reply, "wf");
    assert_eq!(float_row.output.0.as_str(), "out-b", "{float_row:?}");
    assert!(float_row.allocation.is_none(), "{float_row:?}");
    assert!(float_row.effective_frame.is_none(), "{float_row:?}");
    assert_eq!(
        float_row.membership_authority,
        ScopeAuthority::Controlled,
        "{float_row:?}"
    );
}

#[test]
fn floating_source_migration_negative_origin_preserves_relative_placement() {
    // Negative-origin source: the FULL-delta translation lands inside the
    // destination, so the core preserves the relative placement exactly
    // instead of clamping to the destination origin.
    let screens = vec![
        screen_rect("out-a", rect(-800, 0, 800, 600), rect(-800, 0, 800, 600)),
        screen_rect("out-b", rect(0, 0, 800, 600), rect(0, 0, 800, 600)),
    ];
    let members = vec![fact_at("w0", "out-a", rect(-100, 100, 40, 90))];
    let reply = migrate_floating_workspace(screens, members);
    let row = row_of(&reply, "w0");
    assert_eq!(row.output.0.as_str(), "out-b", "{row:?}");
    assert_eq!(
        row.effective_frame,
        Some(rect(700, 100, 40, 90)),
        "FULL-delta translation preserved where valid: {row:?}"
    );
}

#[test]
fn floating_source_migration_minimized_carries_no_placement() {
    // Frame-free members (minimized, no readable frame) ride minimized
    // authority at the target homing with no placement: the reveal writes
    // the frame, never the membership move.
    let screens = vec![
        screen_rect("out-a", rect(0, 0, 800, 600), rect(0, 0, 800, 600)),
        screen_rect("out-b", rect(800, 0, 800, 600), rect(800, 0, 800, 600)),
    ];
    let members = vec![
        fact_at("w0", "out-a", rect(100, 100, 40, 90)),
        minimized_fact("wm", "out-a"),
    ];
    let reply = migrate_floating_workspace(screens, members);
    let row = row_of(&reply, "wm");
    assert_eq!(
        row.output.0.as_str(),
        "out-b",
        "minimized member rides the target homing: {row:?}"
    );
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(crate::world_report::HoldReason::Minimized),
        "{row:?}"
    );
    assert!(row.effective_frame.is_none(), "{row:?}");
}

#[test]
fn sessionless_target_observed_member_takes_no_retranslated_placement() {
    // Host-owned receipts hold a member at the source homing while its
    // frame already reads on the target: the portable placement grounds on
    // the RAW observed screen, never the logical homing, so an
    // already-target frame takes no placement instead of a second
    // FULL-delta translation. At-most-once/partial-arrival behavior is
    // unchanged: the migration stays retained and the member rides one
    // membership-only row at the target homing.
    let screens = vec![
        screen_rect("out-a", rect(0, 0, 800, 600), rect(0, 0, 800, 600)),
        screen_rect("out-b", rect(800, 0, 800, 600), rect(800, 0, 800, 600)),
    ];
    let members = vec![
        fact_at("w0", "out-a", rect(100, 100, 40, 90)),
        fact_at("w1", "out-a", rect(200, 200, 40, 90)),
    ];
    let (mut engine, migrated, _) = migrated_session(screens.clone(), members, true);
    assert_eq!(
        row_of(&migrated, "w0").effective_frame,
        Some(rect(900, 100, 40, 90)),
        "source-observed member places before the crossing: {:?}",
        row_of(&migrated, "w0")
    );
    // w0 physically crosses to the target with no logical receipt naming
    // the target id, so the host-owned homing stays at the source while
    // the raw fact reads on out-b.
    let crossed = report_screens(
        4,
        screens.clone(),
        vec![
            fact_at("w0", "out-b", rect(900, 100, 40, 90)),
            fact_at("w1", "out-a", rect(200, 200, 40, 90)),
        ],
        None,
        None,
    );
    let held = accepted(&mut engine, &crossed);
    let row = row_of(&held, "w0");
    assert_eq!(
        row.output.0.as_str(),
        "out-b",
        "pending member still desires the target homing: {row:?}"
    );
    assert!(
        row.effective_frame.is_none(),
        "already-target frame never re-translates (a homing-grounded pass would emit (1560,100,40,90)): {row:?}"
    );
    assert!(row.allocation.is_none(), "{row:?}");
    assert_eq!(
        row.membership_authority,
        ScopeAuthority::Controlled,
        "{row:?}"
    );
    assert!(
        held.transfers
            .iter()
            .any(|t| matches!(t.kind, crate::world_report::TransferKind::Migration)),
        "partial arrival holds the migration: {:?}",
        held.transfers
    );
    // Control: the source-observed member keeps its source-grounded
    // placement on the same reply.
    assert_eq!(
        row_of(&held, "w1").effective_frame,
        Some(rect(1000, 200, 40, 90)),
        "source-observed member still places: {:?}",
        row_of(&held, "w1")
    );
}

#[test]
fn sessionless_third_screen_observation_adopts_without_placement() {
    // A pending member observed on a third screen is newer external input,
    // never fought: settlement drops it from the flight and the sweep
    // adopts it there with a real allocation, while the migration stays
    // retained for the remaining member. No portable placement is invented
    // for the adopted member (the raw observation already grounds it).
    let screens = vec![
        screen_rect("out-a", rect(0, 0, 800, 600), rect(0, 0, 800, 600)),
        screen_rect("out-b", rect(800, 0, 800, 600), rect(800, 0, 800, 600)),
        screen_rect("out-c", rect(1600, 0, 800, 600), rect(1600, 0, 800, 600)),
    ];
    let members = vec![
        fact_at("w0", "out-a", rect(100, 100, 40, 90)),
        fact_at("w1", "out-a", rect(200, 200, 40, 90)),
    ];
    let (mut engine, _, _) = migrated_session(screens.clone(), members, false);
    let drifted = accepted(
        &mut engine,
        &report_screens(
            4,
            screens.clone(),
            vec![
                fact_at("w0", "out-c", rect(1650, 100, 40, 90)),
                fact_at("w1", "out-a", rect(200, 200, 40, 90)),
            ],
            None,
            None,
        ),
    );
    let row = row_of(&drifted, "w0");
    assert_eq!(
        row.output.0.as_str(),
        "out-c",
        "externally moved member adopts the observed screen: {row:?}"
    );
    assert!(
        row.allocation.is_some(),
        "adoption carries a real sweep allocation, never a stranded hold: {row:?}"
    );
    assert_eq!(
        row.membership_authority,
        ScopeAuthority::Controlled,
        "{row:?}"
    );
    assert!(
        drifted
            .transfers
            .iter()
            .any(|t| matches!(t.kind, crate::world_report::TransferKind::Migration)),
        "migration stays retained for the remaining member: {:?}",
        drifted.transfers
    );
    assert_eq!(
        row_of(&drifted, "w1").effective_frame,
        Some(rect(1000, 200, 40, 90)),
        "remaining member still places: {:?}",
        row_of(&drifted, "w1")
    );
}

fn dialog_hints_null(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        window_type: Some(crate::world_report::WindowType::Dialog),
        hints: None,
        ..fact(window, output, x)
    }
}

fn ordinary_hints_null(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        hints: None,
        ..fact(window, output, x)
    }
}

#[test]
fn classifier_known_float_survives_unreadable_hints_ordinary_holds() {
    use crate::size_hints::FixedSizePredicate;
    use crate::window_classify::{ClassifyInput, classify_window};
    let unavailable: &[crate::connection::FactName] = &[];
    let dialog = dialog_hints_null("win-d", "out-a", 0);
    let classified = classify_window(ClassifyInput {
        fact: &dialog,
        unavailable,
        fixed_predicate: FixedSizePredicate::BothAxes,
        maximize_supported: true,
        native_sticky_supported: true,
        retained_tiled: false,
        automatic_fixed: false,
        tile_override: false,
        explicit_float: false,
        semantic_sticky: false,
        own_process_id: Some("test-own-pid"),
    });
    assert!(
        !classified.hold,
        "known dialog keeps membership: {classified:?}"
    );
    assert!(classified.floating, "{classified:?}");
    assert!(!classified.fit_excluded, "{classified:?}");
    assert!(!classified.excluded, "{classified:?}");

    let modal = WindowFact {
        modal: Some(true),
        hints: None,
        ..fact("win-m", "out-a", 0)
    };
    let classified = classify_window(ClassifyInput {
        fact: &modal,
        unavailable,
        fixed_predicate: FixedSizePredicate::BothAxes,
        maximize_supported: true,
        native_sticky_supported: true,
        retained_tiled: false,
        automatic_fixed: false,
        tile_override: false,
        explicit_float: false,
        semantic_sticky: false,
        own_process_id: Some("test-own-pid"),
    });
    assert!(!classified.hold, "{classified:?}");
    assert!(classified.floating, "{classified:?}");

    // Sheet kind, transient-only (modal null), and retained-tile dialog
    // all keep proven float membership without readable hints.
    let sheet = WindowFact {
        window_type: Some(crate::world_report::WindowType::Sheet),
        hints: None,
        ..fact("win-s", "out-a", 0)
    };
    let transient_only = WindowFact {
        transient: Some(true),
        modal: None,
        hints: None,
        ..fact("win-to", "out-a", 0)
    };
    for (name, probe) in [("sheet", &sheet), ("transient-only", &transient_only)] {
        let classified = classify_window(ClassifyInput {
            fact: probe,
            unavailable,
            fixed_predicate: FixedSizePredicate::BothAxes,
            maximize_supported: true,
            native_sticky_supported: true,
            retained_tiled: false,
            automatic_fixed: false,
            tile_override: false,
            explicit_float: false,
            semantic_sticky: false,
            own_process_id: Some("test-own-pid"),
        });
        assert!(!classified.hold, "{name}: {classified:?}");
        assert!(classified.floating, "{name}: {classified:?}");
    }
    let classified = classify_window(ClassifyInput {
        fact: &dialog,
        unavailable,
        fixed_predicate: FixedSizePredicate::BothAxes,
        maximize_supported: true,
        native_sticky_supported: true,
        retained_tiled: true,
        automatic_fixed: false,
        tile_override: false,
        explicit_float: false,
        semantic_sticky: false,
        own_process_id: Some("test-own-pid"),
    });
    assert!(!classified.hold, "retained-tile dialog: {classified:?}");
    assert!(classified.floating, "retained-tile dialog: {classified:?}");

    // Retained explicit float survives unreadable hints.
    let ordinary = ordinary_hints_null("win-e", "out-a", 0);
    let classified = classify_window(ClassifyInput {
        fact: &ordinary,
        unavailable,
        fixed_predicate: FixedSizePredicate::BothAxes,
        maximize_supported: true,
        native_sticky_supported: true,
        retained_tiled: false,
        automatic_fixed: false,
        tile_override: false,
        explicit_float: true,
        semantic_sticky: false,
        own_process_id: Some("test-own-pid"),
    });
    assert!(!classified.hold, "{classified:?}");
    assert!(classified.floating, "{classified:?}");

    // Retained automatic-fixed latch survives unreadable hints.
    let classified = classify_window(ClassifyInput {
        fact: &ordinary,
        unavailable,
        fixed_predicate: FixedSizePredicate::BothAxes,
        maximize_supported: true,
        native_sticky_supported: true,
        retained_tiled: false,
        automatic_fixed: true,
        tile_override: false,
        explicit_float: false,
        semantic_sticky: false,
        own_process_id: Some("test-own-pid"),
    });
    assert!(!classified.hold, "{classified:?}");
    assert!(classified.floating, "{classified:?}");

    // Unknown ordinary fresh admission still holds.
    let classified = classify_window(ClassifyInput {
        fact: &ordinary,
        unavailable,
        fixed_predicate: FixedSizePredicate::BothAxes,
        maximize_supported: true,
        native_sticky_supported: true,
        retained_tiled: false,
        automatic_fixed: false,
        tile_override: false,
        explicit_float: false,
        semantic_sticky: false,
        own_process_id: Some("test-own-pid"),
    });
    assert!(
        classified.hold,
        "ordinary hints-null must hold: {classified:?}"
    );
    assert!(!classified.floating, "{classified:?}");

    // Tile override with unreadable hints still holds (no invented minima).
    let classified = classify_window(ClassifyInput {
        fact: &ordinary,
        unavailable,
        fixed_predicate: FixedSizePredicate::BothAxes,
        maximize_supported: true,
        native_sticky_supported: true,
        retained_tiled: false,
        automatic_fixed: false,
        tile_override: true,
        explicit_float: false,
        semantic_sticky: false,
        own_process_id: Some("test-own-pid"),
    });
    assert!(classified.hold, "{classified:?}");
}

#[test]
fn dialog_hints_null_stays_float_member_without_frame_write() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                dialog_hints_null("win-d", "out-a", 0),
                fact("win-t", "out-b", 800),
            ],
            vec![],
            None,
            None,
        ),
    );
    // Known float membership: staged fact keeps raw None, derived floats.
    assert!(
        engine
            .staged
            .facts
            .get(&WindowId("win-d".to_owned()))
            .is_some_and(|fact| fact.hints.is_none()),
        "raw hints stay None, never defaulted"
    );
    let derived = engine.derived_for(&WindowId("win-d".to_owned()));
    assert!(!derived.hold, "{derived:?}");
    assert!(derived.floating, "{derived:?}");
    assert!(
        engine.members.contains_key(&WindowId("win-d".to_owned())),
        "float keeps membership"
    );
    // Healthy sibling scope stays controlled with a live row.
    let healthy = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-t")
        .expect("healthy row");
    assert_eq!(healthy.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(healthy.frame_authority, ScopeAuthority::Controlled);
    assert!(healthy.allocation.is_some(), "{healthy:?}");
    // Float row carries no constrained write or fake hint target.
    let row = row_of(&reply, "win-d");
    assert!(row.floating, "{row:?}");
    assert!(row.allocation.is_none(), "{row:?}");
    assert!(row.frame_operation.is_none(), "{row:?}");
    assert!(row.clear_operation.is_none(), "{row:?}");
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched, "{row:?}");
    // Scope holding the float stays controlled (no unreadable hold).
    assert!(
        reply.scopes.iter().any(|status| {
            status.authority == ScopeAuthority::Controlled
                && status
                    .scope
                    .covers_workspace(&OutputId("out-a".to_owned()), &row.workspace)
        }),
        "float scope stays controlled: {:?}",
        reply.scopes
    );
    assert!(reply.effects.is_empty(), "{:?}", reply.effects);

    // Readable hints then unreadable again: no home/authority churn.
    let home = engine
        .members
        .get(&WindowId("win-d".to_owned()))
        .expect("member")
        .home
        .clone();
    let readable = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                WindowFact {
                    hints: Some(WindowSizeHints::none()),
                    ..dialog_hints_null("win-d", "out-a", 0)
                },
                fact("win-t", "out-b", 800),
            ],
            vec![],
            None,
            None,
        ),
    );
    let readable_row = readable
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-d")
        .expect("readable row");
    assert!(readable_row.floating, "{readable_row:?}");
    let back = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                dialog_hints_null("win-d", "out-a", 0),
                fact("win-t", "out-b", 800),
            ],
            vec![],
            None,
            None,
        ),
    );
    assert_eq!(
        engine
            .members
            .get(&WindowId("win-d".to_owned()))
            .expect("member")
            .home,
        home
    );
    let back_row = back
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-d")
        .expect("back row");
    assert!(back_row.floating, "{back_row:?}");
    assert!(back_row.allocation.is_none(), "{back_row:?}");
    assert!(back_row.frame_operation.is_none(), "{back_row:?}");
    assert_eq!(back_row.workspace, readable_row.workspace, "{back_row:?}");
    assert!(back.effects.is_empty(), "{:?}", back.effects);
    assert!(back.transfers.is_empty(), "{:?}", back.transfers);

    // Dependent toggle stays fail-closed without replacing nulls.
    match engine.handle_report(&report(
        4,
        4,
        vec![
            dialog_hints_null("win-d", "out-a", 0),
            fact("win-t", "out-b", 800),
        ],
        vec![],
        Some(11),
        Some(WorldCommand::Operation(
            crate::boundary::CoreCommand::ToggleFloat {
                window: "win-d".to_owned(),
                float_rect: None,
            },
        )),
    )) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "hints-null toggle must refuse"
        ),
        other => panic!("hints-null toggle must refuse, got {other:?}"),
    }
    assert!(
        engine
            .staged
            .facts
            .get(&WindowId("win-d".to_owned()))
            .is_some_and(|fact| fact.hints.is_none()),
        "toggle refusal never fabricates hints"
    );
}

#[test]
fn ordinary_hints_null_fresh_admission_stays_held() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                ordinary_hints_null("win-u", "out-a", 0),
                fact("win-t", "out-b", 800),
            ],
            vec![],
            None,
            None,
        ),
    );
    // Classification hold: no usable membership row, never a fabricated
    // controlled float/tile.
    let derived = engine.derived_for(&WindowId("win-u".to_owned()));
    assert!(derived.hold, "{derived:?}");
    assert!(!derived.floating, "{derived:?}");
    assert!(
        !reply.windows.iter().any(|row| row.lifetime.0 == "win-u"),
        "unknown ordinary publishes no row: {:?}",
        reply.windows
    );
    let healthy = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-t")
        .expect("healthy row");
    assert_eq!(healthy.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(healthy.frame_authority, ScopeAuthority::Controlled);
}

#[test]
fn hints_null_float_move_refuses_without_side_effects() {
    use crate::directional::SameAxisMove;
    use crate::world_directional::DirectionalOutcome;
    use crate::world_report::CommandResult;
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    // Same-output healthy tiled sibling proves no collateral.
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                dialog_hints_null("win-d", "out-a", 0),
                fact("win-t", "out-a", 100),
            ],
            vec![],
            None,
            None,
        ),
    );
    assert!(
        engine
            .staged
            .facts
            .get(&WindowId("win-d".to_owned()))
            .is_some_and(|fact| fact.hints.is_none()),
        "raw hints stay None"
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                dialog_hints_null("win-d", "out-a", 0),
                fact("win-t", "out-a", 100),
            ],
            vec![],
            Some(11),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-d".to_owned()),
                direction: "right".to_owned(),
                same_axis: SameAxisMove::default(),
            }),
        ),
    );
    match &reply.command_result {
        Some(CommandResult::Directional { outcome, .. }) => assert_eq!(
            *outcome,
            DirectionalOutcome::MoveFloatRefusedConstraints,
            "unreadable hints refuse: {reply:?}"
        ),
        other => panic!("expected directional refusal, got {other:?}"),
    }
    assert!(
        engine.retained_floats.is_empty(),
        "no placement: {:?}",
        engine.retained_floats
    );
    assert!(
        engine.retained_clears.is_empty(),
        "no clear kickoff: {:?}",
        engine.retained_clears
    );
    let row = row_of(&reply, "win-d");
    assert!(row.frame_operation.is_none(), "{row:?}");
    assert!(row.clear_operation.is_none(), "{row:?}");
    let healthy = row_of(&reply, "win-t");
    assert_eq!(
        healthy.membership_authority,
        ScopeAuthority::Controlled,
        "{healthy:?}"
    );
    assert_eq!(
        healthy.frame_authority,
        ScopeAuthority::Controlled,
        "{healthy:?}"
    );
    assert!(healthy.allocation.is_some(), "{healthy:?}");

    // Maximized hints-null float must refuse the same way, never bind a
    // clear the continuation could not authorize.
    let mut cleared_engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized_null = WindowFact {
        maximized: Some(true),
        ..dialog_hints_null("win-m", "out-a", 0)
    };
    accepted(
        &mut cleared_engine,
        &report(
            1,
            1,
            vec![maximized_null.clone(), fact("win-t", "out-a", 100)],
            vec![],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut cleared_engine,
        &report(
            2,
            2,
            vec![maximized_null, fact("win-t", "out-a", 100)],
            vec![],
            Some(12),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-m".to_owned()),
                direction: "right".to_owned(),
                same_axis: SameAxisMove::default(),
            }),
        ),
    );
    match &reply.command_result {
        Some(CommandResult::Directional { outcome, .. }) => assert_eq!(
            *outcome,
            DirectionalOutcome::MoveFloatRefusedConstraints,
            "maximized unreadable refuses, never NeedClear: {reply:?}"
        ),
        other => panic!("expected directional refusal, got {other:?}"),
    }
    assert!(
        cleared_engine.retained_clears.is_empty(),
        "no clear kickoff: {:?}",
        cleared_engine.retained_clears
    );
    assert!(
        cleared_engine.retained_floats.is_empty(),
        "{:?}",
        cleared_engine.retained_floats
    );

    // Readable unconstrained recovers: the same half-snap authorizes.
    let readable = WindowFact {
        hints: Some(WindowSizeHints::none()),
        ..dialog_hints_null("win-d", "out-a", 0)
    };
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![readable, fact("win-t", "out-a", 100)],
            vec![],
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-d".to_owned()),
                direction: "right".to_owned(),
                same_axis: SameAxisMove::default(),
            }),
        ),
    );
    match &reply.command_result {
        Some(CommandResult::Directional { outcome, .. }) => assert_eq!(
            *outcome,
            DirectionalOutcome::MoveFloatApplied,
            "readable recovery: {reply:?}"
        ),
        other => panic!("expected applied, got {other:?}"),
    }
    assert_eq!(
        engine.retained_floats.len(),
        1,
        "{:?}",
        engine.retained_floats
    );
    assert_eq!(
        row_of(&reply, "win-d").frame_operation.as_deref(),
        Some("op-13")
    );

    // True constraining hints refuse the same way (consistency probe).
    let mut capped_engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let capped = WindowFact {
        hints: Some(WindowSizeHints {
            min_w: Some(10_000),
            min_h: None,
            max_w: None,
            max_h: None,
        }),
        ..dialog_hints_null("win-d", "out-a", 0)
    };
    accepted(
        &mut capped_engine,
        &report(
            1,
            1,
            vec![capped.clone(), fact("win-t", "out-a", 100)],
            vec![],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut capped_engine,
        &report(
            2,
            2,
            vec![capped, fact("win-t", "out-a", 100)],
            vec![],
            Some(11),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-d".to_owned()),
                direction: "right".to_owned(),
                same_axis: SameAxisMove::default(),
            }),
        ),
    );
    match &reply.command_result {
        Some(CommandResult::Directional { outcome, .. }) => assert_eq!(
            *outcome,
            DirectionalOutcome::MoveFloatRefusedConstraints,
            "constraining hints refuse: {reply:?}"
        ),
        other => panic!("expected constraints refusal, got {other:?}"),
    }
    assert!(
        capped_engine.retained_floats.is_empty(),
        "{:?}",
        capped_engine.retained_floats
    );
}

#[test]
fn retained_explicit_automatic_null_cycle_keeps_home() {
    // Automatic fixed float: readable fixed -> unreadable -> readable keeps
    // home/floating with no minted ops.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                fixed_fact("win-a", "out-a", 200),
                fact("win-t", "out-a", 100),
            ],
            vec![],
            None,
            None,
        ),
    );
    assert!(
        row_of(&reply, "win-a").floating,
        "{:?}",
        row_of(&reply, "win-a")
    );
    let home = engine
        .members
        .get(&WindowId("win-a".to_owned()))
        .expect("member")
        .home
        .clone();
    let nulled = WindowFact {
        hints: None,
        ..fixed_fact("win-a", "out-a", 200)
    };
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![nulled.clone(), fact("win-t", "out-a", 100)],
            vec![],
            None,
            None,
        ),
    );
    assert!(
        row_of(&reply, "win-a").floating,
        "latch keeps float: {reply:?}"
    );
    assert!(
        engine.derived_for(&WindowId("win-a".to_owned())).fixed_auto,
        "automatic latch retained while null"
    );
    assert_eq!(
        engine
            .members
            .get(&WindowId("win-a".to_owned()))
            .expect("member")
            .home,
        home
    );
    assert!(
        row_of(&reply, "win-a").frame_operation.is_none(),
        "{:?}",
        row_of(&reply, "win-a")
    );
    assert!(reply.effects.is_empty(), "{:?}", reply.effects);
    assert!(reply.transfers.is_empty(), "{:?}", reply.transfers);
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                fixed_fact("win-a", "out-a", 200),
                fact("win-t", "out-a", 100),
            ],
            vec![],
            None,
            None,
        ),
    );
    assert!(row_of(&reply, "win-a").floating, "{reply:?}");
    assert!(
        engine.derived_for(&WindowId("win-a".to_owned())).fixed_auto,
        "automatic origin retained after recovery"
    );
    assert_eq!(
        engine
            .members
            .get(&WindowId("win-a".to_owned()))
            .expect("member")
            .home,
        home
    );
    assert!(reply.effects.is_empty(), "{:?}", reply.effects);
    assert!(reply.transfers.is_empty(), "{:?}", reply.transfers);

    // Explicit float via ToggleFloat keeps the same stability.
    let mut explicit = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut explicit,
        &report(
            1,
            1,
            vec![fact("win-e", "out-a", 0), fact("win-t", "out-a", 100)],
            vec![],
            None,
            None,
        ),
    );
    let floated = accepted(
        &mut explicit,
        &report(
            2,
            2,
            vec![fact("win-e", "out-a", 0), fact("win-t", "out-a", 100)],
            vec![],
            Some(11),
            Some(WorldCommand::Operation(
                crate::boundary::CoreCommand::ToggleFloat {
                    window: "win-e".to_owned(),
                    float_rect: None,
                },
            )),
        ),
    );
    assert!(row_of(&floated, "win-e").floating, "{floated:?}");
    let home = explicit
        .members
        .get(&WindowId("win-e".to_owned()))
        .expect("member")
        .home
        .clone();
    let nulled = WindowFact {
        hints: None,
        ..fact("win-e", "out-a", 0)
    };
    let reply = accepted(
        &mut explicit,
        &report(
            3,
            3,
            vec![nulled, fact("win-t", "out-a", 100)],
            vec![],
            None,
            None,
        ),
    );
    assert!(
        row_of(&reply, "win-e").floating,
        "explicit survives null: {reply:?}"
    );
    assert_eq!(
        explicit
            .members
            .get(&WindowId("win-e".to_owned()))
            .expect("member")
            .home,
        home
    );
    assert!(
        !explicit.derived_for(&WindowId("win-e".to_owned())).hold,
        "{:?}",
        explicit.derived_for(&WindowId("win-e".to_owned()))
    );
    assert!(
        row_of(&reply, "win-e").frame_operation.is_none(),
        "withhold while null: {:?}",
        row_of(&reply, "win-e")
    );
    assert!(reply.effects.is_empty(), "{:?}", reply.effects);
    assert!(reply.transfers.is_empty(), "{:?}", reply.transfers);
    // Readable recovery: same home, still floating, same ToggleFloat
    // op resumes (never reminted).
    let reply = accepted(
        &mut explicit,
        &report(
            4,
            4,
            vec![fact("win-e", "out-a", 0), fact("win-t", "out-a", 100)],
            vec![],
            None,
            None,
        ),
    );
    let row = row_of(&reply, "win-e");
    assert!(row.floating, "{row:?}");
    assert_eq!(row.frame_operation.as_deref(), Some("op-11"), "{row:?}");
    assert!(row.clear_operation.is_none(), "{row:?}");
    assert_eq!(
        explicit
            .members
            .get(&WindowId("win-e".to_owned()))
            .expect("member")
            .home,
        home
    );
    assert!(reply.effects.is_empty(), "{:?}", reply.effects);
    assert!(reply.transfers.is_empty(), "{:?}", reply.transfers);
}

#[test]
fn retained_float_tokens_withheld_while_hints_unreadable() {
    use crate::directional::SameAxisMove;
    use crate::world_directional::DirectionalOutcome;
    use crate::world_report::{CommandResult, FrameSetter};
    // Normal float: readable Move authorizes, null withholds, readable reuses same op.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let readable = WindowFact {
        hints: Some(WindowSizeHints::none()),
        ..dialog_hints_null("win-d", "out-a", 0)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![readable.clone(), fact("win-t", "out-a", 100)],
            vec![],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![readable.clone(), fact("win-t", "out-a", 100)],
            vec![],
            Some(11),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-d".to_owned()),
                direction: "right".to_owned(),
                same_axis: SameAxisMove::default(),
            }),
        ),
    );
    match &reply.command_result {
        Some(CommandResult::Directional { outcome, .. }) => {
            assert_eq!(*outcome, DirectionalOutcome::MoveFloatApplied, "{reply:?}")
        }
        other => panic!("expected applied, got {other:?}"),
    }
    assert_eq!(
        row_of(&reply, "win-d").frame_operation.as_deref(),
        Some("op-11")
    );
    let home = engine
        .members
        .get(&WindowId("win-d".to_owned()))
        .expect("member")
        .home
        .clone();
    // Commandless null with the unmatched original rect: membership stays,
    // the write token is withheld, the binding survives.
    let nulled = WindowFact {
        hints: None,
        ..dialog_hints_null("win-d", "out-a", 0)
    };
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![nulled, fact("win-t", "out-a", 100)],
            vec![],
            None,
            None,
        ),
    );
    assert_eq!(
        engine
            .members
            .get(&WindowId("win-d".to_owned()))
            .expect("member")
            .home,
        home
    );
    let row = row_of(&reply, "win-d");
    assert!(row.floating, "{row:?}");
    assert!(
        row.frame_operation.is_none(),
        "withhold while null: {row:?}"
    );
    assert!(row.clear_operation.is_none(), "{row:?}");
    assert_eq!(
        engine.retained_floats.len(),
        1,
        "{:?}",
        engine.retained_floats
    );
    assert_eq!(engine.retained_floats[0].operation_id, "op-11");
    let healthy = row_of(&reply, "win-t");
    assert_eq!(
        healthy.membership_authority,
        ScopeAuthority::Controlled,
        "{healthy:?}"
    );
    assert_eq!(
        healthy.frame_authority,
        ScopeAuthority::Controlled,
        "{healthy:?}"
    );
    // Readable recovery reuses the same op, never remints.
    let reply = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![readable, fact("win-t", "out-a", 100)],
            vec![],
            None,
            None,
        ),
    );
    assert_eq!(
        row_of(&reply, "win-d").frame_operation.as_deref(),
        Some("op-11"),
        "same op resumes: {reply:?}"
    );
    assert_eq!(
        engine.retained_floats.len(),
        1,
        "{:?}",
        engine.retained_floats
    );
    assert_eq!(engine.retained_floats[0].operation_id, "op-11");

    // Maximized float parallel: NeedClear binds, null withholds, readable resumes same op.
    let mut max_engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let max_readable = WindowFact {
        maximized: Some(true),
        hints: Some(WindowSizeHints::none()),
        ..dialog_hints_null("win-m", "out-a", 0)
    };
    accepted(
        &mut max_engine,
        &report(
            1,
            1,
            vec![max_readable.clone(), fact("win-t", "out-a", 100)],
            vec![],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut max_engine,
        &report(
            2,
            2,
            vec![max_readable.clone(), fact("win-t", "out-a", 100)],
            vec![],
            Some(12),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-m".to_owned()),
                direction: "right".to_owned(),
                same_axis: SameAxisMove::default(),
            }),
        ),
    );
    match &reply.command_result {
        Some(CommandResult::Directional { outcome, .. }) => {
            assert_eq!(*outcome, DirectionalOutcome::NeedClear, "{reply:?}")
        }
        other => panic!("expected NeedClear, got {other:?}"),
    }
    assert_eq!(
        row_of(&reply, "win-m").clear_operation.as_deref(),
        Some("op-12")
    );
    let max_nulled = WindowFact {
        maximized: Some(true),
        hints: None,
        ..dialog_hints_null("win-m", "out-a", 0)
    };
    let reply = accepted(
        &mut max_engine,
        &report(
            3,
            3,
            vec![max_nulled.clone(), fact("win-t", "out-a", 100)],
            vec![],
            None,
            None,
        ),
    );
    let row = row_of(&reply, "win-m");
    assert!(
        row.clear_operation.is_none(),
        "withhold clear while null: {row:?}"
    );
    assert!(row.frame_operation.is_none(), "{row:?}");
    assert_eq!(
        max_engine.retained_clears.len(),
        1,
        "{:?}",
        max_engine.retained_clears
    );
    assert_eq!(max_engine.retained_clears[0].operation_id, "op-12");
    assert!(max_engine.retained_clears[0].pending.is_none());
    let reply = accepted(
        &mut max_engine,
        &report(
            4,
            4,
            vec![max_readable.clone(), fact("win-t", "out-a", 100)],
            vec![],
            None,
            None,
        ),
    );
    assert_eq!(
        row_of(&reply, "win-m").clear_operation.as_deref(),
        Some("op-12"),
        "same clear op resumes: {reply:?}"
    );
    // A spent attempt stays spent across a null gap: never republished.
    let mut spent = report(
        5,
        5,
        vec![max_readable, fact("win-t", "out-a", 100)],
        vec![],
        None,
        None,
    );
    spent.clear_attempts = vec![crate::world_report::ClearAttempt {
        operation_id: "op-12".to_owned(),
        window: WindowId("win-m".to_owned()),
        setter: FrameSetter::Accepted,
        pre_maximized: Some(true),
        post_maximized: Some(true),
        attempt_ms: 7,
    }];
    let reply = accepted(&mut max_engine, &spent);
    assert!(
        row_of(&reply, "win-m").clear_operation.is_none(),
        "{:?}",
        row_of(&reply, "win-m")
    );
    assert!(
        max_engine.retained_clears[0].pending.is_some(),
        "attempt staged: {:?}",
        max_engine.retained_clears
    );
    let reply = accepted(
        &mut max_engine,
        &report(
            6,
            6,
            vec![max_nulled, fact("win-t", "out-a", 100)],
            vec![],
            None,
            None,
        ),
    );
    assert!(
        row_of(&reply, "win-m").clear_operation.is_none(),
        "{:?}",
        row_of(&reply, "win-m")
    );
}
