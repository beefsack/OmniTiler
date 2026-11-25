//! Global-unique nonempty swap preservation: complete logical units move
//! across outputs with Engine sessions rekeyed, retained membership for
//! tiled/float/minimized/fullscreen members, views/history and actuation
//! desires. Empty swaps keep the mapping-only path.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    CommandResult, PresentationIngressV4, PresentationRowV4, ScopeAuthority, ScreenReport,
    TransferKind, WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand, WorldReportV4,
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
    }
}

fn float_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        transient: Some(true),
        ..fact(window, output, x)
    }
}

fn fullscreen_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        fullscreen: Some(true),
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
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    views: Vec<(&str, &str)>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReportV4 {
    // v4 carries no view-echo lane; arrival/held echoes here were inert
    // (command-driven swap + member facts carry the proof), so the v4
    // port passes an empty lane.
    assert!(
        views.is_empty(),
        "v4 has no views lane: {} echo(s) cannot convert",
        views.len()
    );
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
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
        focus_changes: None,
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn two_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0), screen("out-b", 800)]
}

fn test_issue(report: &WorldReportV4) -> crate::world_engine::EventIssue {
    crate::world_engine::EventIssue {
        owner: crate::ids::OwnerId::parse("adapter-1").expect("owner"),
        correlation: crate::ids::CorrelationId::parse(&report.request_id).expect("correlation"),
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> crate::world_report::WorldReplyV4 {
    match engine.handle_report(report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn global_unique_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            // Fixtures carry native sticky and maximize facts (including
            // sticky floats); the swap lane under test is the native route.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![(
            "mode".to_owned(),
            crate::connection::RawSettingValue::String("global-unique".to_owned()),
        )],
    )
    .expect("generation")
}

fn view_of(reply: &crate::world_report::WorldReplyV4, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

fn window_row<'a>(
    reply: &'a crate::world_report::WorldReplyV4,
    window: &str,
) -> &'a crate::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing window row {window}"))
}

#[test]
fn global_unique_swap_preserves_nonempty_units() {
    let mut engine = global_unique_engine();
    let first = accepted(
        &mut engine,
        &report(1, 1, two_screens(), vec![], vec![], "", None, None),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    let view_b = view_of(&first, "out-b").expect("view b");
    assert_ne!(view_a, view_b);

    // Three tiled per side (unequal shares via nesting) plus float,
    // fullscreen, and minimized members. Remembered focus per side via
    // sequential global focus.
    let windows = vec![
        fact("win-a1", "out-a", 0),
        fact("win-a2", "out-a", 100),
        fact("win-a3", "out-a", 200),
        float_fact("win-af", "out-a", 300),
        fullscreen_fact("win-as", "out-a", 400),
        minimized_fact("win-am", "out-a"),
        fact("win-b1", "out-b", 800),
        fact("win-b2", "out-b", 900),
        fact("win-b3", "out-b", 1000),
        float_fact("win-bf", "out-b", 1100),
        fullscreen_fact("win-bs", "out-b", 1200),
        minimized_fact("win-bm", "out-b"),
    ];
    accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            windows.clone(),
            vec![],
            "win-a2",
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            windows.clone(),
            vec![],
            "win-b3",
            None,
            None,
        ),
    );
    let calm = accepted(
        &mut engine,
        &report(4, 4, two_screens(), windows.clone(), vec![], "", None, None),
    );
    assert_eq!(view_of(&calm, "out-a").as_deref(), Some(view_a.as_str()));
    assert_eq!(view_of(&calm, "out-b").as_deref(), Some(view_b.as_str()));

    // Pre-swap Engine snapshots for modulo-output comparison.
    let src_a = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
    };
    let src_b = crate::session::DomainKey {
        output: OutputId("out-b".to_owned()),
        workspace: crate::directional::WorkspaceId(view_b.clone()),
    };
    let snap_a: Vec<String> = engine
        .world
        .layout()
        .tiled_windows(&src_a)
        .map(|window| window.0.clone())
        .collect();
    let snap_b: Vec<String> = engine
        .world
        .layout()
        .tiled_windows(&src_b)
        .map(|window| window.0.clone())
        .collect();
    assert!(snap_a.len() >= 2, "source a tiled: {snap_a:?}");
    assert!(snap_b.len() >= 2, "source b tiled: {snap_b:?}");
    let tree_a = engine.world.layout().tree(&src_a).cloned();
    let tree_b = engine.world.layout().tree(&src_b).cloned();
    let focus_a = engine.world.layout().focus_leaf(&src_a).cloned();
    let focus_b = engine.world.layout().focus_leaf(&src_b).cloned();

    // Swap: select out-b's workspace on out-a.
    let swapped = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            windows.clone(),
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(view_b.clone()),
            }),
        ),
    );
    assert_eq!(view_of(&swapped, "out-a").as_deref(), Some(view_b.as_str()));
    assert_eq!(view_of(&swapped, "out-b").as_deref(), Some(view_a.as_str()));
    // Both sides retain migration flights for actuation settlement.
    let mut ops: Vec<String> = swapped
        .transfers
        .iter()
        .filter(|t| matches!(t.kind, TransferKind::Migration))
        .map(|t| t.operation_id.clone())
        .collect();
    ops.sort();
    assert_eq!(
        ops,
        vec!["op-11-swap-a".to_owned(), "op-11-swap-b".to_owned()]
    );
    // Desired members ride the moved outputs before arrival: tiled via
    // Controlled membership-only rows, floats/minimized/fullscreen via
    // session rows homed at the targets.
    for window in ["win-a1", "win-a2", "win-a3"] {
        let row = window_row(&swapped, window);
        assert_eq!(row.output.0, "out-b", "{window}: {row:?}");
        assert_eq!(row.workspace, view_a, "{window}: {row:?}");
        assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    }
    for window in ["win-b1", "win-b2", "win-b3"] {
        let row = window_row(&swapped, window);
        assert_eq!(row.output.0, "out-a", "{window}: {row:?}");
        assert_eq!(row.workspace, view_b, "{window}: {row:?}");
        assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    }
    for (window, output, workspace) in [
        ("win-af", "out-b", view_a.as_str()),
        ("win-bf", "out-a", view_b.as_str()),
    ] {
        let row = window_row(&swapped, window);
        assert_eq!(row.output.0, output, "{window}: {row:?}");
        assert_eq!(row.workspace, workspace, "{window}: {row:?}");
        assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
        assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    }
    for (window, output, workspace) in [
        ("win-am", "out-b", view_a.as_str()),
        ("win-bm", "out-a", view_b.as_str()),
    ] {
        let row = window_row(&swapped, window);
        assert_eq!(row.output.0, output, "{window}: {row:?}");
        assert_eq!(row.workspace, workspace, "{window}: {row:?}");
        assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    }

    // Unchanged report: nothing disappears or flaps, flights hold.
    let held = accepted(
        &mut engine,
        &report(6, 6, two_screens(), windows.clone(), vec![], "", None, None),
    );
    assert_eq!(view_of(&held, "out-a").as_deref(), Some(view_b.as_str()));
    assert_eq!(view_of(&held, "out-b").as_deref(), Some(view_a.as_str()));
    let mut held_ops: Vec<String> = held
        .transfers
        .iter()
        .filter(|t| matches!(t.kind, TransferKind::Migration))
        .map(|t| t.operation_id.clone())
        .collect();
    held_ops.sort();
    assert_eq!(
        held_ops, ops,
        "unchanged holds without flap: {:?}",
        held.transfers
    );
    for window in ["win-a1", "win-b1", "win-am", "win-bm"] {
        let _ = window_row(&held, window);
    }
    for (window, output, workspace) in [
        ("win-af", "out-b", view_a.as_str()),
        ("win-bf", "out-a", view_b.as_str()),
    ] {
        let row = window_row(&held, window);
        assert_eq!(row.output.0, output, "{window}: {row:?}");
        assert_eq!(row.workspace, workspace, "{window}: {row:?}");
        assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
        assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    }

    // Arrival: every member observed on its moved output with view echoes.
    let arrived_windows = vec![
        fact("win-a1", "out-b", 800),
        fact("win-a2", "out-b", 900),
        fact("win-a3", "out-b", 1000),
        float_fact("win-af", "out-b", 1100),
        fullscreen_fact("win-as", "out-b", 1200),
        minimized_fact("win-am", "out-b"),
        fact("win-b1", "out-a", 0),
        fact("win-b2", "out-a", 100),
        fact("win-b3", "out-a", 200),
        float_fact("win-bf", "out-a", 300),
        fullscreen_fact("win-bs", "out-a", 400),
        minimized_fact("win-bm", "out-a"),
    ];
    let arrived = accepted(
        &mut engine,
        &report(
            7,
            7,
            two_screens(),
            arrived_windows.clone(),
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        arrived.transfers.is_empty(),
        "arrival settles: {:?}",
        arrived.transfers
    );
    assert_eq!(view_of(&arrived, "out-a").as_deref(), Some(view_b.as_str()));
    assert_eq!(view_of(&arrived, "out-b").as_deref(), Some(view_a.as_str()));

    // Session snapshots compare modulo output homing: same window sets,
    // trees, and focus leaves ride the rekey to the swapped outputs.
    let tgt_a = crate::session::DomainKey {
        output: OutputId("out-b".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
    };
    let tgt_b = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_b.clone()),
    };
    let mut pre_a: Vec<String> = snap_a;
    let mut post_a: Vec<String> = engine
        .world
        .layout()
        .tiled_windows(&tgt_a)
        .map(|window| window.0.clone())
        .collect();
    pre_a.sort();
    post_a.sort();
    assert_eq!(pre_a, post_a, "tiled membership preserved");
    let mut pre_b: Vec<String> = snap_b;
    let mut post_b: Vec<String> = engine
        .world
        .layout()
        .tiled_windows(&tgt_b)
        .map(|window| window.0.clone())
        .collect();
    pre_b.sort();
    post_b.sort();
    assert_eq!(pre_b, post_b, "tiled membership preserved");
    assert_eq!(
        engine.world.layout().tree(&tgt_a),
        tree_a.as_ref(),
        "tree/shares preserved"
    );
    assert_eq!(
        engine.world.layout().tree(&tgt_b),
        tree_b.as_ref(),
        "tree/shares preserved"
    );
    assert_eq!(
        engine.world.layout().focus_leaf(&tgt_a),
        focus_a.as_ref(),
        "remembered focus leaf preserved"
    );
    assert_eq!(
        engine.world.layout().focus_leaf(&tgt_b),
        focus_b.as_ref(),
        "remembered focus leaf preserved"
    );
    // Previous-toggle stays parked: swapped-out ids left their scopes.
    let parked = accepted(
        &mut engine,
        &report(
            8,
            8,
            two_screens(),
            arrived_windows,
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectPrevious {
                output: Some(OutputId("out-a".to_owned())),
            }),
        ),
    );
    assert_eq!(view_of(&parked, "out-a").as_deref(), Some(view_b.as_str()));
}

fn sticky_float(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        sticky: Some(true),
        transient: Some(true),
        ..fact(window, output, x)
    }
}

#[test]
fn global_unique_swap_keeps_sticky_on_physical_source() {
    let mut engine = global_unique_engine();
    let first = accepted(
        &mut engine,
        &report(1, 1, two_screens(), vec![], vec![], "", None, None),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    let view_b = view_of(&first, "out-b").expect("view b");
    assert_ne!(view_a, view_b);
    // Tiled pairs plus one sticky float per output.
    let windows = vec![
        fact("win-a1", "out-a", 0),
        fact("win-a2", "out-a", 100),
        sticky_float("win-as", "out-a", 300),
        fact("win-b1", "out-b", 800),
        fact("win-b2", "out-b", 900),
        sticky_float("win-bs", "out-b", 1100),
    ];
    accepted(
        &mut engine,
        &report(2, 2, two_screens(), windows.clone(), vec![], "", None, None),
    );
    accepted(
        &mut engine,
        &report(3, 3, two_screens(), windows.clone(), vec![], "", None, None),
    );
    let swapped = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            windows.clone(),
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(view_b.clone()),
            }),
        ),
    );
    assert_eq!(view_of(&swapped, "out-a").as_deref(), Some(view_b.as_str()));
    assert_eq!(view_of(&swapped, "out-b").as_deref(), Some(view_a.as_str()));
    // Nonempty swap retains both migration flights.
    let mut ops: Vec<String> = swapped
        .transfers
        .iter()
        .filter(|t| matches!(t.kind, TransferKind::Migration))
        .map(|t| t.operation_id.clone())
        .collect();
    ops.sort();
    assert_eq!(
        ops,
        vec!["op-11-swap-a".to_owned(), "op-11-swap-b".to_owned()]
    );
    // Core members skip sticky: tiled ride Controlled at the targets.
    for (window, output, workspace) in [
        ("win-a1", "out-b", view_a.as_str()),
        ("win-a2", "out-b", view_a.as_str()),
        ("win-b1", "out-a", view_b.as_str()),
        ("win-b2", "out-a", view_b.as_str()),
    ] {
        let row = window_row(&swapped, window);
        assert_eq!(row.output.0, output, "{window}: {row:?}");
        assert_eq!(row.workspace, workspace, "{window}: {row:?}");
        assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    }
    // Session sticky stays on the physical source: exactly one row per
    // sticky on its original output, homed via the source effective view,
    // with no frame writes.
    for (window, output) in [("win-as", "out-a"), ("win-bs", "out-b")] {
        let rows: Vec<_> = swapped
            .windows
            .iter()
            .filter(|row| row.lifetime.0 == window)
            .collect();
        assert_eq!(rows.len(), 1, "{window} single row: {rows:?}");
        assert_eq!(rows[0].output.0, output, "{window}: {rows:?}");
        assert!(
            matches!(rows[0].frame_authority, ScopeAuthority::Untouched),
            "{window} no frame write: {rows:?}"
        );
    }
    // Unchanged report: no flap, flights hold, sticky rows unchanged.
    let held = accepted(
        &mut engine,
        &report(5, 5, two_screens(), windows.clone(), vec![], "", None, None),
    );
    for (window, output) in [("win-as", "out-a"), ("win-bs", "out-b")] {
        let rows: Vec<_> = held
            .windows
            .iter()
            .filter(|row| row.lifetime.0 == window)
            .collect();
        assert_eq!(rows.len(), 1, "unchanged {window}: {rows:?}");
        assert_eq!(rows[0].output.0, output);
        assert!(matches!(rows[0].frame_authority, ScopeAuthority::Untouched));
    }
    assert_eq!(
        held.transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        2,
        "unchanged holds: {:?}",
        held.transfers
    );
    // Arrival settles with sticky still at the physical sources.
    let arrived_windows = vec![
        fact("win-a1", "out-b", 800),
        fact("win-a2", "out-b", 900),
        sticky_float("win-as", "out-a", 0),
        fact("win-b1", "out-a", 0),
        fact("win-b2", "out-a", 100),
        sticky_float("win-bs", "out-b", 800),
    ];
    let arrived = accepted(
        &mut engine,
        &report(6, 6, two_screens(), arrived_windows, vec![], "", None, None),
    );
    assert!(
        arrived.transfers.is_empty(),
        "arrival settles: {:?}",
        arrived.transfers
    );
    for (window, output) in [("win-as", "out-a"), ("win-bs", "out-b")] {
        let rows: Vec<_> = arrived
            .windows
            .iter()
            .filter(|row| row.lifetime.0 == window)
            .collect();
        assert_eq!(rows.len(), 1, "arrived {window}: {rows:?}");
        assert_eq!(rows[0].output.0, output);
    }
}

fn sticky_rows<'a>(
    reply: &'a crate::world_report::WorldReplyV4,
    window: &str,
) -> Vec<&'a crate::world_report::WindowDesire> {
    reply
        .windows
        .iter()
        .filter(|row| row.lifetime.0 == window)
        .collect()
}

#[test]
fn global_unique_swap_sticky_only_side_keeps_both_rows() {
    // A carries tiled plus a sticky float; B carries only a sticky float
    // (rootless empty domain after stripping). Neither side may refuse,
    // and both physical outputs retain exactly one sticky row.
    let mut engine = global_unique_engine();
    let first = accepted(
        &mut engine,
        &report(1, 1, two_screens(), vec![], vec![], "", None, None),
    );
    let _view_a = view_of(&first, "out-a").expect("view a");
    let view_b = view_of(&first, "out-b").expect("view b");
    let windows = vec![
        fact("win-a1", "out-a", 0),
        sticky_float("win-as", "out-a", 300),
        sticky_float("win-bs", "out-b", 800),
    ];
    accepted(
        &mut engine,
        &report(2, 2, two_screens(), windows.clone(), vec![], "", None, None),
    );
    accepted(
        &mut engine,
        &report(3, 3, two_screens(), windows.clone(), vec![], "", None, None),
    );
    let swapped = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            windows.clone(),
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(view_b.clone()),
            }),
        ),
    );
    for (window, output) in [("win-as", "out-a"), ("win-bs", "out-b")] {
        let rows = sticky_rows(&swapped, window);
        assert_eq!(rows.len(), 1, "swapped {window}: {rows:?}");
        assert_eq!(rows[0].output.0, output, "{window}: {rows:?}");
        assert!(matches!(rows[0].frame_authority, ScopeAuthority::Untouched));
    }
    let held = accepted(
        &mut engine,
        &report(5, 5, two_screens(), windows.clone(), vec![], "", None, None),
    );
    for (window, output) in [("win-as", "out-a"), ("win-bs", "out-b")] {
        let rows = sticky_rows(&held, window);
        assert_eq!(rows.len(), 1, "unchanged {window}: {rows:?}");
        assert_eq!(rows[0].output.0, output);
    }
    let arrived_windows = vec![
        fact("win-a1", "out-b", 800),
        sticky_float("win-as", "out-a", 0),
        sticky_float("win-bs", "out-b", 800),
    ];
    let arrived = accepted(
        &mut engine,
        &report(6, 6, two_screens(), arrived_windows, vec![], "", None, None),
    );
    assert!(
        arrived.transfers.is_empty(),
        "arrival settles: {:?}",
        arrived.transfers
    );
    for (window, output) in [("win-as", "out-a"), ("win-bs", "out-b")] {
        let rows = sticky_rows(&arrived, window);
        assert_eq!(rows.len(), 1, "arrived {window}: {rows:?}");
        assert_eq!(rows[0].output.0, output);
    }
}

#[test]
fn global_unique_swap_empty_side_keeps_sticky_row() {
    // B is genuinely empty (no session at all); A carries tiled plus a
    // sticky float. The command stays accepted and the sticky rides a
    // stay derived from its own actual source session.
    let mut engine = global_unique_engine();
    let first = accepted(
        &mut engine,
        &report(1, 1, two_screens(), vec![], vec![], "", None, None),
    );
    let _view_a = view_of(&first, "out-a").expect("view a");
    let view_b = view_of(&first, "out-b").expect("view b");
    let windows = vec![
        fact("win-a1", "out-a", 0),
        sticky_float("win-as", "out-a", 300),
    ];
    accepted(
        &mut engine,
        &report(2, 2, two_screens(), windows.clone(), vec![], "", None, None),
    );
    accepted(
        &mut engine,
        &report(3, 3, two_screens(), windows.clone(), vec![], "", None, None),
    );
    let swapped = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            windows.clone(),
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(view_b.clone()),
            }),
        ),
    );
    let tiled: Vec<_> = swapped
        .windows
        .iter()
        .filter(|row| row.lifetime.0 == "win-a1")
        .collect();
    assert_eq!(tiled.len(), 1, "tiled row: {tiled:?}");
    assert_eq!(tiled[0].output.0, "out-b");
    let rows = sticky_rows(&swapped, "win-as");
    assert_eq!(rows.len(), 1, "swapped sticky: {rows:?}");
    assert_eq!(rows[0].output.0, "out-a", "sticky: {rows:?}");
    assert!(matches!(rows[0].frame_authority, ScopeAuthority::Untouched));
    let held = accepted(
        &mut engine,
        &report(5, 5, two_screens(), windows.clone(), vec![], "", None, None),
    );
    let rows = sticky_rows(&held, "win-as");
    assert_eq!(rows.len(), 1, "unchanged sticky: {rows:?}");
    assert_eq!(rows[0].output.0, "out-a");
    let arrived_windows = vec![
        fact("win-a1", "out-b", 800),
        sticky_float("win-as", "out-a", 0),
    ];
    let arrived = accepted(
        &mut engine,
        &report(6, 6, two_screens(), arrived_windows, vec![], "", None, None),
    );
    assert!(
        arrived.transfers.is_empty(),
        "arrival settles: {:?}",
        arrived.transfers
    );
    let rows = sticky_rows(&arrived, "win-as");
    assert_eq!(rows.len(), 1, "arrived sticky: {rows:?}");
    assert_eq!(rows[0].output.0, "out-a");
}

#[test]
fn global_unique_swap_carries_floating_side_mode_and_flights() {
    // Swapping with a floating side carries the floating mode to the new
    // output on the commit reply itself: the moved floating scope
    // publishes Untouched (never a tiled scope), the tiled side stays
    // Controlled, and both sides retain their migration flights for
    // actuation settlement. This locks the seam precondition for F2:
    // floating-target migration members must actuate rather than stall.
    let mut engine = global_unique_engine();
    let first = accepted(
        &mut engine,
        &report(1, 1, two_screens(), vec![], vec![], "", None, None),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    let view_b = view_of(&first, "out-b").expect("view b");
    assert_ne!(view_a, view_b);
    let windows = vec![fact("win-a1", "out-a", 0), fact("win-b1", "out-b", 800)];
    accepted(
        &mut engine,
        &report(2, 2, two_screens(), windows.clone(), vec![], "", None, None),
    );
    accepted(
        &mut engine,
        &report(3, 3, two_screens(), windows.clone(), vec![], "", None, None),
    );
    // Float the out-a side, then swap by selecting out-b's workspace.
    accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            windows.clone(),
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(view_a.clone()),
                tiled: false,
            }),
        ),
    );
    let swapped = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            windows.clone(),
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(view_b.clone()),
            }),
        ),
    );
    assert_eq!(view_of(&swapped, "out-a").as_deref(), Some(view_b.as_str()));
    assert_eq!(view_of(&swapped, "out-b").as_deref(), Some(view_a.as_str()));
    // The floating mode followed its id across the swap.
    let moved = swapped
        .workspaces
        .iter()
        .find(|row| row.id == view_a)
        .expect("moved floating row");
    assert_eq!(moved.output.0, "out-b");
    assert_eq!(
        moved.tiling_mode,
        crate::world_report::TilingMode::Floating,
        "floating mode follows the id: {moved:?}"
    );
    let scope_status = |output: &str, workspace: &str| {
        swapped
            .scopes
            .iter()
            .find(|status| {
                status
                    .scope
                    .covers_workspace(&OutputId(output.to_owned()), workspace)
            })
            .unwrap_or_else(|| panic!("missing scope {output}/{workspace}"))
            .authority
    };
    assert_eq!(scope_status("out-b", &view_a), ScopeAuthority::Untouched);
    assert_eq!(scope_status("out-a", &view_b), ScopeAuthority::Controlled);
    // Both sides retain migration flights for actuation settlement.
    let mut ops: Vec<String> = swapped
        .transfers
        .iter()
        .filter(|t| matches!(t.kind, TransferKind::Migration))
        .map(|t| t.operation_id.clone())
        .collect();
    ops.sort();
    assert_eq!(
        ops,
        vec!["op-12-swap-a".to_owned(), "op-12-swap-b".to_owned()]
    );
    for (window, output, workspace) in [
        ("win-a1", "out-b", view_a.as_str()),
        ("win-b1", "out-a", view_b.as_str()),
    ] {
        let row = window_row(&swapped, window);
        assert_eq!(row.output.0, output, "{window}: {row:?}");
        assert_eq!(row.workspace, workspace, "{window}: {row:?}");
        assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    }
}

#[test]
fn global_unique_swap_failed_stay_keeps_world_unchanged() {
    use std::collections::BTreeSet;

    use crate::world_commands::CommandApplied;
    use crate::world_engine::{KIND_MIGRATION, MSG_MIGRATION};

    // A carries tiled plus a sticky float; B is genuinely empty (no
    // domain), so the B-side sticky stay derives from the A source.
    let mut engine = global_unique_engine();
    let first = accepted(
        &mut engine,
        &report(1, 1, two_screens(), vec![], vec![], "", None, None),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    let view_b = view_of(&first, "out-b").expect("view b");
    let windows = vec![
        fact("win-a1", "out-a", 0),
        sticky_float("win-as", "out-a", 300),
    ];
    accepted(
        &mut engine,
        &report(2, 2, two_screens(), windows.clone(), vec![], "", None, None),
    );
    // Stage an unknown-sticky fact on win-a1 so the swap refuses atomically
    // before any world/layout commit.
    let mut unknown_sticky = fact("win-a1", "out-a", 0);
    unknown_sticky.sticky = None;
    accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![unknown_sticky, sticky_float("win-as", "out-a", 300)],
            vec![],
            "",
            None,
            None,
        ),
    );
    let src_a = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
    };
    let src_b = crate::session::DomainKey {
        output: OutputId("out-b".to_owned()),
        workspace: crate::directional::WorkspaceId(view_b.clone()),
    };
    let tgt_a = crate::session::DomainKey {
        output: OutputId("out-b".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
    };
    let tgt_b = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_b.clone()),
    };
    assert!(!engine.world.layout().contains_domain(&src_b));
    let before_tiled: Vec<_> = engine
        .world
        .layout()
        .tiled_windows(&src_a)
        .cloned()
        .collect();
    let before_exceptions: Vec<_> = engine
        .world
        .layout()
        .exception_windows_with_flags(&src_a)
        .collect();
    let before_tree = engine.world.layout().tree(&src_a).cloned();
    let before_focus = engine.world.layout().focus_leaf(&src_a).cloned();
    let before_rev = engine.world.layout().accepted_revision(&src_a);

    // Snapshot world state before the refused swap.
    let mapping = engine.world.workspace.mapping.clone();
    let views = engine.world.workspace.views.clone();
    let history = engine.world.workspace.history.clone();
    let members = engine.world.workspace.members.clone();
    let tiling = engine.world.workspace.tiling.clone();
    let floating = engine.world.floating_scopes.clone();
    let retained = engine.world.settlement.retained.clone();

    let swap_report = report(
        4,
        4,
        two_screens(),
        windows.clone(),
        vec![],
        "",
        Some(12),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: crate::directional::WorkspaceId(view_b.clone()),
        }),
    );
    let held = BTreeSet::new();
    match engine.tx(|tx| {
        tx.execute_command(
            &test_issue(&swap_report),
            12,
            swap_report.command.as_ref().expect("command"),
            &held,
            None,
            None,
        )
    }) {
        CommandApplied::Rejected { kind, message } => {
            assert_eq!(kind, KIND_MIGRATION);
            assert_eq!(message, MSG_MIGRATION);
        }
        CommandApplied::Effect(_) | CommandApplied::NoEffect => {
            panic!("refused swap must reject");
        }
    }
    // The refusal leaves mapping/views/history, layout domains, members, and
    // retained flights exactly as before: no partial commit.
    assert_eq!(engine.world.workspace.mapping, mapping);
    assert_eq!(engine.world.workspace.views, views);
    assert_eq!(engine.world.workspace.history, history);
    assert_eq!(engine.world.workspace.members, members);
    assert_eq!(engine.world.workspace.tiling, tiling);
    assert_eq!(engine.world.floating_scopes, floating);
    assert_eq!(engine.world.settlement.retained, retained);
    assert!(!engine.world.layout().contains_domain(&tgt_a));
    assert!(!engine.world.layout().contains_domain(&tgt_b));
    assert!(!engine.world.layout().contains_domain(&src_b));
    assert_eq!(
        engine
            .world
            .layout()
            .tiled_windows(&src_a)
            .cloned()
            .collect::<Vec<_>>(),
        before_tiled
    );
    assert_eq!(
        engine
            .world
            .layout()
            .exception_windows_with_flags(&src_a)
            .collect::<Vec<_>>(),
        before_exceptions
    );
    assert_eq!(engine.world.layout().tree(&src_a).cloned(), before_tree);
    assert_eq!(
        engine.world.layout().focus_leaf(&src_a).cloned(),
        before_focus
    );
    assert_eq!(engine.world.layout().accepted_revision(&src_a), before_rev);

    // Restore proven sticky=false on win-a1: the successful sessionless-B
    // stay path works through the public boundary.
    accepted(
        &mut engine,
        &report(4, 4, two_screens(), windows.clone(), vec![], "", None, None),
    );
    let swapped = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            windows.clone(),
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(view_b.clone()),
            }),
        ),
    );
    assert_eq!(view_of(&swapped, "out-a").as_deref(), Some(view_b.as_str()));
    assert_eq!(view_of(&swapped, "out-b").as_deref(), Some(view_a.as_str()));
    let tiled: Vec<_> = swapped
        .windows
        .iter()
        .filter(|row| row.lifetime.0 == "win-a1")
        .collect();
    assert_eq!(tiled.len(), 1, "tiled row: {tiled:?}");
    assert_eq!(tiled[0].output.0, "out-b");
    assert_eq!(tiled[0].workspace, view_a);
    assert_eq!(tiled[0].membership_authority, ScopeAuthority::Controlled);
    let sticky = sticky_rows(&swapped, "win-as");
    assert_eq!(sticky.len(), 1, "swapped sticky: {sticky:?}");
    assert_eq!(sticky[0].output.0, "out-a");
    assert!(matches!(
        sticky[0].frame_authority,
        ScopeAuthority::Untouched
    ));
    let arrived = accepted(
        &mut engine,
        &report(
            6,
            6,
            two_screens(),
            vec![
                fact("win-a1", "out-b", 800),
                sticky_float("win-as", "out-a", 0),
            ],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        arrived.transfers.is_empty(),
        "arrival settles: {:?}",
        arrived.transfers
    );
    let sticky = sticky_rows(&arrived, "win-as");
    assert_eq!(sticky.len(), 1, "arrived sticky: {sticky:?}");
    assert_eq!(sticky[0].output.0, "out-a");
}

#[test]
fn global_unique_swap_occupied_current_emits_destination_remembered_focus() {
    // R-WS-09 over the global-unique swap path (occupied <-> occupied):
    // stale pre-command report, absent view echoes. The requested output
    // lands on the occupied target, so the reply carries one explicit
    // focus effect to that destination's remembered leaf.
    let mut engine = global_unique_engine();
    let first = accepted(
        &mut engine,
        &report(1, 1, two_screens(), vec![], vec![], "", None, None),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    let view_b = view_of(&first, "out-b").expect("view b");
    assert_ne!(view_a, view_b);
    let windows = vec![
        fact("win-a1", "out-a", 0),
        fact("win-a2", "out-a", 100),
        fact("win-b1", "out-b", 800),
        fact("win-b2", "out-b", 900),
    ];
    accepted(
        &mut engine,
        &report(2, 2, two_screens(), windows.clone(), vec![], "", None, None),
    );
    accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            windows.clone(),
            vec![],
            "win-a2",
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            windows.clone(),
            vec![],
            "win-b1",
            None,
            None,
        ),
    );
    let swapped = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            windows.clone(),
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(view_b.clone()),
            }),
        ),
    );
    assert_eq!(view_of(&swapped, "out-a").as_deref(), Some(view_b.as_str()));
    assert_eq!(view_of(&swapped, "out-b").as_deref(), Some(view_a.as_str()));
    let request = swapped
        .focus_request
        .as_ref()
        .expect("retained focus request");
    assert_eq!(request.target.0, "win-b1");
    assert_eq!(request.id, format!("focus-{}", swapped.state_revision));
    assert_eq!(request.issued_revision, swapped.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(11));
}

#[test]
fn global_unique_swap_empty_current_emits_destination_remembered_focus() {
    // Empty current (out-a) swaps with the occupied other side: same stale
    // pre-command shape, absent echoes. The requested output lands on the
    // occupied target, so its remembered leaf focuses.
    let mut engine = global_unique_engine();
    let first = accepted(
        &mut engine,
        &report(1, 1, two_screens(), vec![], vec![], "", None, None),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    let view_b = view_of(&first, "out-b").expect("view b");
    assert_ne!(view_a, view_b);
    let windows = vec![fact("win-b1", "out-b", 800), fact("win-b2", "out-b", 900)];
    accepted(
        &mut engine,
        &report(2, 2, two_screens(), windows.clone(), vec![], "", None, None),
    );
    accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            windows.clone(),
            vec![],
            "win-b1",
            None,
            None,
        ),
    );
    let swapped = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            windows.clone(),
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(view_b.clone()),
            }),
        ),
    );
    assert_eq!(view_of(&swapped, "out-a").as_deref(), Some(view_b.as_str()));
    assert_eq!(view_of(&swapped, "out-b").as_deref(), Some(view_a.as_str()));
    let request = swapped
        .focus_request
        .as_ref()
        .expect("retained focus request");
    assert_eq!(request.target.0, "win-b1");
    assert_eq!(request.id, format!("focus-{}", swapped.state_revision));
    assert_eq!(request.issued_revision, swapped.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(11));
}

#[test]
fn global_unique_swap_to_empty_stays_quiet() {
    // No-memory control: the requested output lands on the empty side, so
    // no remembered window exists and the swap stays quiet.
    let mut engine = global_unique_engine();
    let first = accepted(
        &mut engine,
        &report(1, 1, two_screens(), vec![], vec![], "", None, None),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    let view_b = view_of(&first, "out-b").expect("view b");
    assert_ne!(view_a, view_b);
    let windows = vec![fact("win-a1", "out-a", 0)];
    accepted(
        &mut engine,
        &report(2, 2, two_screens(), windows.clone(), vec![], "", None, None),
    );
    accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            windows.clone(),
            vec![],
            "win-a1",
            None,
            None,
        ),
    );
    let swapped = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            windows.clone(),
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(view_b.clone()),
            }),
        ),
    );
    assert_eq!(view_of(&swapped, "out-a").as_deref(), Some(view_b.as_str()));
    assert_eq!(view_of(&swapped, "out-b").as_deref(), Some(view_a.as_str()));
}

#[test]
fn global_unique_swap_minimized_incoming_and_outgoing_rows_stay_unique() {
    // Incoming minimized on out-b (b1 minimized, b2 focused tiled) swapped
    // onto out-a (a1, a2 tiled): B2 keeps controlled focus at A/ws-2, B1
    // stays genuinely Minimized at A/ws-2, and A1/A2 move to controlled B/ws-1
    // with allocations, with exactly one row per window.
    let mut engine = global_unique_engine();
    let base = accepted(
        &mut engine,
        &report(1, 1, two_screens(), vec![], vec![], "", None, None),
    );
    let view_a = view_of(&base, "out-a").expect("out-a view");
    let view_b = view_of(&base, "out-b").expect("out-b view");
    assert_ne!(view_a, view_b);

    let windows = vec![
        fact("win-a1", "out-a", 0),
        fact("win-a2", "out-a", 400),
        minimized_fact("win-b1", "out-b"),
        fact("win-b2", "out-b", 800),
    ];
    accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            windows.clone(),
            vec![],
            "win-b2",
            None,
            None,
        ),
    );

    let swapped = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            windows,
            vec![],
            "win-b2",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(view_b.clone()),
            }),
        ),
    );
    assert_eq!(view_of(&swapped, "out-a").as_deref(), Some(view_b.as_str()));
    assert_eq!(view_of(&swapped, "out-b").as_deref(), Some(view_a.as_str()));
    assert_eq!(
        swapped.windows.len(),
        4,
        "exactly one row per window: {:?}",
        swapped.windows
    );
    for name in ["win-a1", "win-a2"] {
        let row = window_row(&swapped, name);
        assert_eq!(
            row.membership_authority,
            ScopeAuthority::Controlled,
            "{name}: {row:?}"
        );
        assert_eq!(row.output.0, "out-b", "{name}: {row:?}");
        assert_eq!(row.workspace, view_a, "{name}: {row:?}");
        assert!(
            row.allocation.is_some(),
            "{name} must have allocation: {row:?}"
        );
    }
    let b1 = window_row(&swapped, "win-b1");
    assert_eq!(
        b1.membership_authority,
        ScopeAuthority::Controlled,
        "win-b1: {b1:?}"
    );
    assert_eq!(
        b1.frame_authority,
        ScopeAuthority::Hold(crate::world_report::HoldReason::Minimized),
        "win-b1: {b1:?}"
    );
    assert_eq!(b1.output.0, "out-a", "win-b1: {b1:?}");
    assert_eq!(b1.workspace, view_b, "win-b1: {b1:?}");
    assert!(
        b1.allocation.is_none(),
        "win-b1 minimized has no allocation: {b1:?}"
    );
    let b2 = window_row(&swapped, "win-b2");
    assert_eq!(
        b2.membership_authority,
        ScopeAuthority::Controlled,
        "win-b2: {b2:?}"
    );
    assert_eq!(
        b2.frame_authority,
        ScopeAuthority::Controlled,
        "win-b2: {b2:?}"
    );
    assert_eq!(b2.output.0, "out-a", "win-b2: {b2:?}");
    assert_eq!(b2.workspace, view_b, "win-b2: {b2:?}");
    assert!(
        b2.allocation.is_some(),
        "win-b2 must have allocation: {b2:?}"
    );
    assert_eq!(
        swapped.focus_request.as_ref().map(|r| r.target.0.as_str()),
        Some("win-b2"),
        "B2 keeps controlled focus at A/ws-2: {:?}",
        swapped.focus_request
    );

    // Outgoing minimized on out-a (a1 minimized, a2 tiled) swapped onto
    // out-b (b1, b2 tiled): A1 stays genuinely Minimized at B/ws-1, A2
    // stays Controlled at B/ws-1 with allocation, B1/B2 move to A/ws-2.
    let mut out_engine = global_unique_engine();
    let out_base = accepted(
        &mut out_engine,
        &report(1, 1, two_screens(), vec![], vec![], "", None, None),
    );
    let ov_a = view_of(&out_base, "out-a").expect("out-a view");
    let ov_b = view_of(&out_base, "out-b").expect("out-b view");
    let out_windows = vec![
        minimized_fact("win-a1", "out-a"),
        fact("win-a2", "out-a", 0),
        fact("win-b1", "out-b", 800),
        fact("win-b2", "out-b", 1200),
    ];
    accepted(
        &mut out_engine,
        &report(
            2,
            2,
            two_screens(),
            out_windows.clone(),
            vec![],
            "win-a2",
            None,
            None,
        ),
    );
    let out_swapped = accepted(
        &mut out_engine,
        &report(
            3,
            3,
            two_screens(),
            out_windows,
            vec![],
            "win-a2",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(ov_b.clone()),
            }),
        ),
    );
    assert_eq!(
        out_swapped.windows.len(),
        4,
        "exactly one row per window: {:?}",
        out_swapped.windows
    );
    let oa1 = window_row(&out_swapped, "win-a1");
    assert_eq!(
        oa1.membership_authority,
        ScopeAuthority::Controlled,
        "oa1: {oa1:?}"
    );
    assert_eq!(
        oa1.frame_authority,
        ScopeAuthority::Hold(crate::world_report::HoldReason::Minimized),
        "oa1: {oa1:?}"
    );
    assert_eq!(oa1.output.0, "out-b", "oa1: {oa1:?}");
    assert_eq!(oa1.workspace, ov_a, "oa1: {oa1:?}");
    assert!(oa1.allocation.is_none(), "oa1: {oa1:?}");
    let oa2 = window_row(&out_swapped, "win-a2");
    assert_eq!(
        oa2.membership_authority,
        ScopeAuthority::Controlled,
        "oa2: {oa2:?}"
    );
    assert_eq!(
        oa2.frame_authority,
        ScopeAuthority::Controlled,
        "oa2: {oa2:?}"
    );
    assert_eq!(oa2.output.0, "out-b", "oa2: {oa2:?}");
    assert_eq!(oa2.workspace, ov_a, "oa2: {oa2:?}");
    assert!(oa2.allocation.is_some(), "oa2: {oa2:?}");
    for name in ["win-b1", "win-b2"] {
        let row = window_row(&out_swapped, name);
        assert_eq!(
            row.membership_authority,
            ScopeAuthority::Controlled,
            "{name}: {row:?}"
        );
        assert_eq!(row.output.0, "out-a", "{name}: {row:?}");
        assert_eq!(row.workspace, ov_b, "{name}: {row:?}");
        assert!(row.allocation.is_some(), "{name}: {row:?}");
    }
}

fn composed_sticky_fact(window: &str, output: &str, x: i32) -> WindowFact {
    // Composed-sticky hosts carry no native sticky fact: the raw row stays
    // null and only core's retained intent proves the lifetime.
    let mut row = fact(window, output, x);
    row.sticky = None;
    row
}

fn composed_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            native_sticky: false,
            maximize: true,
            fullscreen: true,
            ..Default::default()
        },
    )
    .expect("generation")
}

fn composed_global_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            native_sticky: false,
            maximize: true,
            fullscreen: true,
            ..Default::default()
        },
        vec![(
            "mode".to_owned(),
            crate::connection::RawSettingValue::String("global-unique".to_owned()),
        )],
    )
    .expect("generation")
}

#[test]
fn composed_sticky_global_unique_swap_keeps_only_intent_sticky_windows() {
    let mut engine = composed_global_engine();
    let first = accepted(
        &mut engine,
        &report(1, 1, two_screens(), vec![], vec![], "", None, None),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    let view_b = view_of(&first, "out-b").expect("view b");
    assert_ne!(view_a, view_b);

    let sticky = WindowId("win-sticky".to_owned());
    let plain = WindowId("win-plain".to_owned());
    let windows = vec![
        composed_sticky_fact("win-sticky", "out-b", 800),
        composed_sticky_fact("win-plain", "out-b", 900),
    ];
    // Retained intent before staging proves the composed sticky lifetime;
    // the raw rows stay null, so a raw-fact filter would mark every window
    // sticky here.
    engine
        .world
        .intent
        .restored_sticky
        .insert(sticky.clone(), false);
    accepted(
        &mut engine,
        &report(2, 2, two_screens(), windows.clone(), vec![], "", None, None),
    );
    assert_eq!(
        engine
            .world
            .staged
            .facts
            .get(&sticky)
            .and_then(|fact| fact.sticky),
        None,
        "composed route carries no native sticky fact",
    );
    assert!(engine.tx_ro(|tx| tx.derived_for(&sticky).sticky));
    assert!(!engine.tx_ro(|tx| tx.derived_for(&plain).sticky));

    // Swap the two views by selecting B on out-a. The composed sticky
    // window stays physical on out-b under the workspace arriving there;
    // the plain window moves to out-a. A raw-fact sticky set would treat
    // the plain window as sticky too and keep it on out-b.
    let swapped = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            windows,
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(view_b.clone()),
            }),
        ),
    );
    assert_eq!(view_of(&swapped, "out-a").as_deref(), Some(view_b.as_str()));
    assert_eq!(view_of(&swapped, "out-b").as_deref(), Some(view_a.as_str()));
    let plain_row = window_row(&swapped, "win-plain");
    assert_eq!(
        (plain_row.output.0.as_str(), plain_row.workspace.as_str()),
        ("out-a", view_b.as_str()),
        "composed non-sticky member swaps: {plain_row:?}",
    );
    let sticky_row = window_row(&swapped, "win-sticky");
    assert_eq!(
        (sticky_row.output.0.as_str(), sticky_row.workspace.as_str()),
        ("out-b", view_a.as_str()),
        "composed sticky window stays physical: {sticky_row:?}",
    );
}

#[test]
fn composed_sticky_hidden_lane_migration_excludes_sticky_and_carries_plain() {
    let mut engine = composed_engine();
    let sticky = WindowId("win-sticky".to_owned());
    let plain = WindowId("win-plain".to_owned());
    engine
        .world
        .intent
        .restored_sticky
        .insert(sticky.clone(), false);
    let mut tiled = fact("win-tiled", "out-a", 0);
    tiled.sticky = None;
    let mut sticky_hidden = minimized_fact("win-sticky", "out-a");
    sticky_hidden.sticky = None;
    let mut plain_hidden = minimized_fact("win-plain", "out-a");
    plain_hidden.sticky = None;
    let windows = vec![tiled, sticky_hidden, plain_hidden];
    let first = accepted(
        &mut engine,
        &report(1, 1, two_screens(), windows.clone(), vec![], "", None, None),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    assert!(engine.tx_ro(|tx| tx.derived_for(&sticky).sticky));
    assert!(!engine.tx_ro(|tx| tx.derived_for(&plain).sticky));
    assert!(engine.tx_ro(|tx| tx.derived_for(&sticky).hidden));
    assert!(engine.tx_ro(|tx| tx.derived_for(&plain).hidden));

    let migration = report(
        2,
        2,
        two_screens(),
        windows,
        vec![],
        "",
        Some(11),
        Some(WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: crate::directional::WorkspaceId(view_a.clone()),
        }),
    );
    let settlements = match engine.handle_report(&migration, &[]) {
        (WorldOutcome::Accepted(_), settlements) => settlements,
        (other, _) => panic!("expected accepted migration, got {other:?}"),
    };
    let Some(CommandResult::Migration { members, .. }) = settlements
        .first()
        .and_then(|settlement| settlement.result.clone())
    else {
        panic!("expected migration provenance, got {settlements:?}");
    };
    assert!(
        members.contains(&WindowId("win-tiled".to_owned())),
        "tiled member carried: {members:?}",
    );
    assert!(
        members.contains(&plain),
        "composed non-sticky hidden-lane lifetime is carried: {members:?}",
    );
    assert!(
        !members.contains(&sticky),
        "composed sticky hidden-lane lifetime stays physical: {members:?}",
    );
}
