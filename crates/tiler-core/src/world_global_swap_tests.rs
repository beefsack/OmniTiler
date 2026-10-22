//! Global-unique nonempty swap preservation: complete logical units move
//! across outputs with Engine sessions rekeyed, retained membership for
//! tiled/float/minimized/fullscreen members, views/history and actuation
//! desires. Empty swaps keep the mapping-only path.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::workspace_scope::WorkspaceMode;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    ScopeAuthority, ScreenReport, ScreenView, TransferKind, WORLD_CONTRACT_VERSION, WindowFact,
    WorldCommand, WorldReport,
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
    }
}

fn float_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        floating: true,
        ..fact(window, output, x)
    }
}

fn fullscreen_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        fullscreen: true,
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
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    views: Vec<(&str, &str)>,
    focused: &str,
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
        workspace_mode: WorkspaceMode::GlobalUnique,
        screens,
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
        default_tiled: true,
        focused_global: focused.to_owned(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: vec![],
        monotonic_ms: 0,
        frames: Vec::new(),
    }
}

fn two_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0, true), screen("out-b", 800, false)]
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn view_of(reply: &crate::world_report::WorldReply, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

fn window_row<'a>(
    reply: &'a crate::world_report::WorldReply,
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
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
    let snap_a = engine
        .engine()
        .session(&src_a)
        .expect("source session a")
        .snapshot();
    let snap_b = engine
        .engine()
        .session(&src_b)
        .expect("source session b")
        .snapshot();
    assert!(snap_a.windows.len() >= 2, "source a tiled: {snap_a:?}");
    assert!(snap_b.windows.len() >= 2, "source b tiled: {snap_b:?}");
    let tree_a = engine
        .engine()
        .session(&src_a)
        .unwrap()
        .tree_for(&src_a)
        .cloned();
    let tree_b = engine
        .engine()
        .session(&src_b)
        .unwrap()
        .tree_for(&src_b)
        .cloned();
    let focus_a = engine.engine().session(&src_a).unwrap().focus();
    let focus_b = engine.engine().session(&src_b).unwrap().focus();

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
                workspace: view_b.clone(),
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
    for window in ["win-a1", "win-b1", "win-af", "win-bf", "win-am", "win-bm"] {
        let _ = window_row(&held, window);
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
            vec![("out-a", view_b.as_str()), ("out-b", view_a.as_str())],
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
    let moved_a = engine.engine().session(&tgt_a).expect("moved session a");
    let moved_b = engine.engine().session(&tgt_b).expect("moved session b");
    let mut pre_a: Vec<String> = snap_a.windows.iter().map(|l| l.window.0.clone()).collect();
    let mut post_a: Vec<String> = moved_a
        .snapshot()
        .windows
        .iter()
        .map(|l| l.window.0.clone())
        .collect();
    pre_a.sort();
    post_a.sort();
    assert_eq!(pre_a, post_a, "tiled membership preserved");
    let mut pre_b: Vec<String> = snap_b.windows.iter().map(|l| l.window.0.clone()).collect();
    let mut post_b: Vec<String> = moved_b
        .snapshot()
        .windows
        .iter()
        .map(|l| l.window.0.clone())
        .collect();
    pre_b.sort();
    post_b.sort();
    assert_eq!(pre_b, post_b, "tiled membership preserved");
    assert_eq!(
        moved_a.tree_for(&tgt_a),
        tree_a.as_ref(),
        "tree/shares preserved"
    );
    assert_eq!(
        moved_b.tree_for(&tgt_b),
        tree_b.as_ref(),
        "tree/shares preserved"
    );
    assert_eq!(
        moved_a.focus().1,
        focus_a.1,
        "remembered focus leaf preserved"
    );
    assert_eq!(
        moved_b.focus().1,
        focus_b.1,
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
            vec![("out-a", view_b.as_str()), ("out-b", view_a.as_str())],
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
        floating: true,
        sticky: true,
        ..fact(window, output, x)
    }
}

#[test]
fn global_unique_swap_keeps_sticky_on_physical_source() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
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
                workspace: view_b.clone(),
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
        &report(
            6,
            6,
            two_screens(),
            arrived_windows,
            vec![("out-a", view_b.as_str()), ("out-b", view_a.as_str())],
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
    reply: &'a crate::world_report::WorldReply,
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, 1, two_screens(), vec![], vec![], "", None, None),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
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
                workspace: view_b.clone(),
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
        &report(
            6,
            6,
            two_screens(),
            arrived_windows,
            vec![("out-a", view_b.as_str()), ("out-b", view_a.as_str())],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
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
                workspace: view_b.clone(),
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
        &report(
            6,
            6,
            two_screens(),
            arrived_windows,
            vec![("out-a", view_b.as_str()), ("out-b", view_a.as_str())],
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
    let rows = sticky_rows(&arrived, "win-as");
    assert_eq!(rows.len(), 1, "arrived sticky: {rows:?}");
    assert_eq!(rows[0].output.0, "out-a");
}
