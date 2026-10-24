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
        native_workspace: None,
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
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
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
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
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
        presentation: crate::world_report::PresentationIngress::disabled(),
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

fn screen_rect(output: &str, full: Rect, work: Rect, primary: bool) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(full),
        work_area: Some(work),
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary,
    }
}

fn fact_at(window: &str, output: &str, rect: Rect) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect: Some(rect),
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
fn report_screens(
    request_seq: u64,
    screens: Vec<ScreenReport>,
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
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens,
        windows,
        views: Vec::new(),
        closed: vec![],
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
        presentation: crate::world_report::PresentationIngress::disabled(),
    }
}

fn migrate_floating_workspace(
    screens: Vec<ScreenReport>,
    members: Vec<WindowFact>,
) -> crate::world_report::WorldReply {
    migrated_session(screens, members).1
}

fn migrated_session(
    screens: Vec<ScreenReport>,
    members: Vec<WindowFact>,
) -> (WorldEngine, crate::world_report::WorldReply, String) {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report_screens(1, screens.clone(), members.clone(), None, None),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    accepted(
        &mut engine,
        &report_screens(
            2,
            screens.clone(),
            members.clone(),
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: view_a.clone(),
                tiled: false,
            }),
        ),
    );
    let migrated = accepted(
        &mut engine,
        &report_screens(
            3,
            screens.clone(),
            members,
            Some(12),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
        ),
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
        screen_rect("out-a", rect(0, 0, 800, 600), rect(0, 0, 800, 600), true),
        screen_rect(
            "out-b",
            rect(800, 0, 400, 300),
            rect(800, 0, 400, 300),
            false,
        ),
    ];
    let members = vec![
        fact_at("w0", "out-a", rect(700, 500, 40, 90)),
        WindowFact {
            floating: true,
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
        screen_rect(
            "out-a",
            rect(-800, 0, 800, 600),
            rect(-800, 0, 800, 600),
            true,
        ),
        screen_rect("out-b", rect(0, 0, 800, 600), rect(0, 0, 800, 600), false),
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
        screen_rect("out-a", rect(0, 0, 800, 600), rect(0, 0, 800, 600), true),
        screen_rect(
            "out-b",
            rect(800, 0, 800, 600),
            rect(800, 0, 800, 600),
            false,
        ),
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
        screen_rect("out-a", rect(0, 0, 800, 600), rect(0, 0, 800, 600), true),
        screen_rect(
            "out-b",
            rect(800, 0, 800, 600),
            rect(800, 0, 800, 600),
            false,
        ),
    ];
    let members = vec![
        fact_at("w0", "out-a", rect(100, 100, 40, 90)),
        fact_at("w1", "out-a", rect(200, 200, 40, 90)),
    ];
    let (mut engine, migrated, _) = migrated_session(screens.clone(), members);
    assert_eq!(
        row_of(&migrated, "w0").effective_frame,
        Some(rect(900, 100, 40, 90)),
        "source-observed member places before the crossing: {:?}",
        row_of(&migrated, "w0")
    );
    // w0 physically crosses to the target with no logical receipt naming
    // the target id, so the host-owned homing stays at the source while
    // the raw fact reads on out-b.
    let mut crossed = report_screens(
        4,
        screens.clone(),
        vec![
            fact_at("w0", "out-b", rect(900, 100, 40, 90)),
            fact_at("w1", "out-a", rect(200, 200, 40, 90)),
        ],
        None,
        None,
    );
    crossed.host_owned_workspace_domains = true;
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
        screen_rect("out-a", rect(0, 0, 800, 600), rect(0, 0, 800, 600), true),
        screen_rect(
            "out-b",
            rect(800, 0, 800, 600),
            rect(800, 0, 800, 600),
            false,
        ),
        screen_rect(
            "out-c",
            rect(1600, 0, 800, 600),
            rect(1600, 0, 800, 600),
            false,
        ),
    ];
    let members = vec![
        fact_at("w0", "out-a", rect(100, 100, 40, 90)),
        fact_at("w1", "out-a", rect(200, 200, 40, 90)),
    ];
    let (mut engine, _, _) = migrated_session(screens.clone(), members);
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
