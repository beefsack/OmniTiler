//! Host-owned native adoption tests (Stage 1): fresh generation builds
//! each display's logical workspaces from observed native desktops in
//! native order, preserves membership with no moves at launch, keeps
//! startup size/position inference, then adopts outside moves and
//! outside-created desktops. Unknown/partial/missing evidence holds;
//! duplicates refuse; native-free behavior unchanged (covered by the
//! existing suite).

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    HoldReason, NativeObservation, ScopeAuthority, ScreenReport, TransferKind,
    WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
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

// Opaque native ids: UUID-like desktop ids and numeric Space ids.
const N1: &str = "3f9a8c1e-1111-4b2a-9c3d-0000000000a1";
const N2: &str = "3f9a8c1e-2222-4b2a-9c3d-0000000000a2";
const N3: &str = "7";
const N4: &str = "12";

fn fact_native(window: &str, output: &str, frame: Rect, native: Option<&str>) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect: Some(frame),
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
        native_workspace: native.map(str::to_owned),
    }
}

fn minimized_native(window: &str, output: &str, native: Option<&str>) -> WindowFact {
    WindowFact {
        rect: None,
        minimized: true,
        ..fact_native(window, output, rect(0, 0, 40, 90), native)
    }
}

fn floating_native(window: &str, output: &str, frame: Rect, native: Option<&str>) -> WindowFact {
    WindowFact {
        floating: true,
        fit_excluded: true,
        ..fact_native(window, output, frame, native)
    }
}

fn fullscreen_native(window: &str, output: &str, frame: Rect, native: Option<&str>) -> WindowFact {
    WindowFact {
        fullscreen: true,
        fit_excluded: true,
        ..fact_native(window, output, frame, native)
    }
}

/// Authority of one `output/workspace` scope row, if published.
fn scope_authority(
    reply: &tiler_core::world_report::WorldReply,
    output: &str,
    workspace: &str,
) -> Option<ScopeAuthority> {
    reply
        .scopes
        .iter()
        .find(|status| status.scope == format!("{output}/{workspace}"))
        .map(|status| status.authority)
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    native: Vec<NativeObservation>,
    mode: WorkspaceMode,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    inventory_complete: bool,
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
        workspace_mode: mode,
        screens,
        windows: windows.clone(),
        views: vec![],
        closed: vec![],
        host_owned_workspace_domains: true,
        equal_workspace_count: false,
        birth_clear: false,
        native,
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        born_maximized: tiler_core::born_maximized::BornMaximized::default(),
        default_tiled: true,
        focused_global: windows
            .first()
            .map(|f| f.window.0.clone())
            .unwrap_or_default(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        inventory_complete,
        gesture_barrier: false,
        gesture: None,
        switch_target: tiler_core::cursor_targeting::SwitchTarget::default(),
        fresh_placement: tiler_core::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
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

fn native_obs(output: &str, order: &[&str], current: &str) -> NativeObservation {
    NativeObservation {
        output: OutputId(output.to_owned()),
        native_order: order.iter().map(|s| (*s).to_owned()).collect(),
        current_native: Some(current.to_owned()),
    }
}

fn two_native_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0, true)]
}

fn bound_ids(reply: &tiler_core::world_report::WorldReply, output: &str) -> Vec<(String, String)> {
    reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output && row.native_binding.is_some())
        .map(|row| (row.id.clone(), row.native_binding.clone().expect("bound")))
        .collect()
}

#[test]
fn four_native_bound_plus_ordinary_spare_no_moves_at_launch() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    // Asymmetric native order (not layout, not sorted) with two windows
    // sharing one native desktop, so the startup size/position inference
    // must prove a retained split: side-by-side frames on N1 plus members
    // on every other desktop, including minimized, floating, and
    // fullscreen members on noncurrent desktops.
    let windows = vec![
        fact_native("win-b", "out-a", rect(300, 0, 500, 560), Some(N1)),
        fact_native("win-a", "out-a", rect(0, 0, 300, 560), Some(N1)),
        fullscreen_native("win-e", "out-a", rect(0, 0, 800, 560), Some(N2)),
        minimized_native("win-c", "out-a", Some(N3)),
        floating_native("win-d", "out-a", rect(500, 100, 200, 150), Some(N4)),
    ];
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_native_screens(),
            windows,
            vec![native_obs("out-a", &[N3, N1, N4, N2], N1)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    assert!(reply.ready);
    // Four bound in observed native order plus the unchanged ordinary
    // trailing spare.
    let bound = bound_ids(&reply, "out-a");
    assert_eq!(
        bound.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>(),
        vec![N3, N1, N4, N2],
        "native order preserved: {reply:?}"
    );
    let total: Vec<_> = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .collect();
    assert_eq!(total.len(), 5, "four bound plus spare: {total:?}");
    assert!(
        total.last().is_some_and(|row| row.native_binding.is_none()),
        "spare stays unbound: {total:?}"
    );
    let logical = |native: &str| {
        bound
            .iter()
            .find(|(_, n)| n == native)
            .map(|(id, _)| id.clone())
            .expect("bound id")
    };
    // No launch moves, effects, or command outcomes.
    assert!(
        reply.transfers.is_empty(),
        "no member moves at launch: {:?}",
        reply.transfers
    );
    assert!(reply.effects.is_empty(), "no launch effects");
    assert!(reply.command_result.is_none(), "no launch result");
    // The N1 pair shares one logical workspace with a real tiled split:
    // both allocations present, x-ordered, non-overlapping, controlled.
    let row_a = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("row a");
    let row_b = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .expect("row b");
    assert_eq!(row_a.workspace, logical(N1));
    assert_eq!(row_b.workspace, logical(N1));
    let (alloc_a, alloc_b) = (row_a.allocation, row_b.allocation);
    let (alloc_a, alloc_b) = (alloc_a.expect("split a"), alloc_b.expect("split b"));
    assert_eq!(alloc_a, rect(0, 0, 300, 560), "infer position and share");
    assert_eq!(
        alloc_b,
        rect(300, 0, 500, 560),
        "not input order or equal split"
    );
    for row in [row_a, row_b] {
        assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
        assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    }
    // Fullscreen member on noncurrent N2 homes its mapping. A fresh
    // fullscreen-only seed sweeps to the existing Unreadable fence (same
    // as native-free: no allocation, no writes blessed); the homing is
    // still the adopted N2 logical, never collapsed elsewhere.
    let row_e = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-e")
        .expect("row e");
    assert_eq!(row_e.workspace, logical(N2));
    assert_eq!(
        row_e.frame_authority,
        ScopeAuthority::Hold(HoldReason::Unreadable)
    );
    assert_eq!(
        row_e.membership_authority,
        ScopeAuthority::Hold(HoldReason::Unreadable)
    );
    let row_c = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-c")
        .expect("row c");
    assert_eq!(row_c.workspace, logical(N3));
    assert_eq!(
        row_c.frame_authority,
        ScopeAuthority::Hold(HoldReason::Minimized)
    );
    assert_eq!(row_c.membership_authority, ScopeAuthority::Controlled);
    let row_d = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-d")
        .expect("row d");
    assert_eq!(row_d.workspace, logical(N4));
    assert!(row_d.floating);
    assert_eq!(row_d.frame_authority, ScopeAuthority::Untouched);
    // Known-evidence scopes publish full authority; the fresh
    // fullscreen-only N2 scope carries the existing sweep fence.
    for native in [N3, N1, N4] {
        assert_eq!(
            scope_authority(&reply, "out-a", &logical(native)),
            Some(ScopeAuthority::Controlled),
            "known scope stays controlled"
        );
    }
    assert_eq!(
        scope_authority(&reply, "out-a", &logical(N2)),
        Some(ScopeAuthority::Hold(HoldReason::Unreadable)),
        "fresh fullscreen-only seed keeps the existing sweep fence"
    );
    // Repeat stability: same ids, same order, same homing, same split.
    let again = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_native_screens(),
            vec![
                fact_native("win-b", "out-a", rect(300, 0, 500, 560), Some(N1)),
                fact_native("win-a", "out-a", rect(0, 0, 300, 560), Some(N1)),
                fullscreen_native("win-e", "out-a", rect(0, 0, 800, 560), Some(N2)),
                minimized_native("win-c", "out-a", Some(N3)),
                floating_native("win-d", "out-a", rect(500, 100, 200, 150), Some(N4)),
            ],
            vec![native_obs("out-a", &[N3, N1, N4, N2], N1)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    let bound_again = bound_ids(&again, "out-a");
    assert_eq!(bound, bound_again, "stable across repeats");
    assert!(again.transfers.is_empty());
    assert!(again.effects.is_empty());
    let again_a = again
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("row a");
    let again_b = again
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .expect("row b");
    assert_eq!(again_a.allocation, row_a.allocation, "split retained");
    assert_eq!(again_b.allocation, row_b.allocation, "split retained");
}

#[test]
fn incarnation_reset_readopts_cleanly() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_native_screens(),
            vec![fact_native(
                "win-a",
                "out-a",
                rect(0, 0, 400, 560),
                Some(N1),
            )],
            vec![native_obs("out-a", &[N1, N2], N1)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    assert_eq!(bound_ids(&first, "out-a").len(), 2);
    // New adapter incarnation resets generation state; re-adoption binds
    // the same native set again with no carried flights.
    let mut second = report(
        1,
        1,
        two_native_screens(),
        vec![fact_native(
            "win-a",
            "out-a",
            rect(0, 0, 400, 560),
            Some(N1),
        )],
        vec![native_obs("out-a", &[N1, N2], N1)],
        WorkspaceMode::PerOutputLocal,
        None,
        None,
        true,
    );
    second.adapter_incarnation = "adapter-2".to_owned();
    second.request_id = "req-1b".to_owned();
    let reply = accepted(&mut engine, &second);
    assert_eq!(bound_ids(&reply, "out-a").len(), 2);
    assert!(reply.transfers.is_empty());
}

#[test]
fn outside_move_and_create_are_adopted() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_native_screens(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], N1)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    // Outside host move: win-a now observed on N2.
    let moved = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_native_screens(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N2)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], N2)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    assert!(moved.transfers.is_empty(), "outside move needs no flight");
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&moved, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    for window in ["win-a", "win-b"] {
        let row = moved
            .windows
            .iter()
            .find(|row| row.lifetime.0 == window)
            .expect("row");
        assert_eq!(row.workspace, bound[N2], "outside move adopted");
    }
    // Outside-created desktop N5 with a member becomes a bound workspace.
    const N5: &str = "9f1c-0005-space";
    let created = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_native_screens(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N2)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
                fact_native("win-c", "out-a", rect(0, 0, 800, 560), Some(N5)),
            ],
            vec![native_obs("out-a", &[N1, N2, N5], N2)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    let bound = bound_ids(&created, "out-a");
    assert_eq!(
        bound.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>(),
        vec![N1, N2, N5],
        "outside id bound at native-order position"
    );
    // Empty native-bound desktops keep identity: N1 is empty but stays.
    assert!(bound.iter().any(|(_, n)| n == N1));
}

#[test]
fn unknown_partial_missing_hold_and_duplicates_refuse() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_native_screens(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], N1)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    // Unknown native membership holds at the retained home (no collapse).
    let held = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_native_screens(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), None),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], N2)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    let row = held
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("row");
    assert_eq!(row.workspace, bound[N1], "unknown holds, never collapses");
    // The fenced scope withholds write authority through the existing
    // Unreadable lane; the known scope stays controlled.
    assert_eq!(
        scope_authority(&held, "out-a", &bound[N1]),
        Some(ScopeAuthority::Hold(HoldReason::Unreadable)),
        "unknown fences its scope, no writes blessed"
    );
    assert_eq!(
        scope_authority(&held, "out-a", &bound[N2]),
        Some(ScopeAuthority::Controlled)
    );
    // Host-owned mode is native-only: the adapter logical receipt is
    // never consulted, so a conflicting observed workspace naming the
    // known N2 workspace loses to the native N1 evidence (native wins,
    // no fence). Only unknown/unmapped native evidence holds.
    let mut conflict = fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1));
    conflict.observed_workspace = Some(bound[N2].clone());
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_native_screens(),
            vec![
                conflict,
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2], N2)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    let row = held
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("row");
    assert_eq!(
        row.workspace, bound[N1],
        "native wins over the ignored receipt"
    );
    assert_eq!(
        scope_authority(&held, "out-a", &bound[N1]),
        Some(ScopeAuthority::Controlled),
        "native-only ignores the conflicting receipt, no fence"
    );
    // Partial report (incomplete inventory) with a new native id creates
    // nothing.
    let partial = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_native_screens(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
            ],
            vec![native_obs("out-a", &[N1, N2, "new-partial"], N1)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            false,
        ),
    );
    assert_eq!(
        bound_ids(&partial, "out-a").len(),
        2,
        "partial creates nothing"
    );
    // Partial reports hold every host-owned domain: the inventory gap
    // is unreadable even when the staged evidence is known.
    let partial_bound: std::collections::BTreeMap<String, String> = bound_ids(&partial, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    for native in [N1, N2] {
        assert_eq!(
            scope_authority(&partial, "out-a", &partial_bound[native]),
            Some(ScopeAuthority::Hold(HoldReason::IncompleteInventory)),
            "partial inventory gap holds known scopes"
        );
    }
    assert!(partial.transfers.is_empty());
    // Missing native row deletes nothing.
    let mut missing = report(
        5,
        5,
        two_native_screens(),
        vec![
            fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
            fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
        ],
        vec![],
        WorkspaceMode::PerOutputLocal,
        None,
        None,
        true,
    );
    // Without the capability, native per-window rows would refuse; keep
    // the capability but omit the native list: bindings persist.
    let held = accepted(&mut engine, &missing);
    assert_eq!(
        bound_ids(&held, "out-a").len(),
        2,
        "absent rows never delete"
    );
    let _ = &mut missing;

    // Duplicates refuse: within-output repeat, and contradictory rows.
    let mut dup = report(
        6,
        6,
        two_native_screens(),
        vec![],
        vec![native_obs("out-a", &[N1, N1], N1)],
        WorkspaceMode::PerOutputLocal,
        None,
        None,
        true,
    );
    match engine.handle_report(&dup) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "snapshot-invalid"),
        other => panic!("within-output duplicate must refuse, got {other:?}"),
    }
    dup.native = vec![
        native_obs("out-a", &[N1], N1),
        native_obs("out-a", &[N2], N2),
    ];
    match engine.handle_report(&dup) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "snapshot-invalid"),
        other => panic!("contradictory duplicate rows must refuse, got {other:?}"),
    }
}

#[test]
fn separate_outputs_share_ids_independently_and_shared_unions() {
    // KDE-style: the same global native ids on two outputs map
    // independently per output in separate-view modes.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = vec![screen("out-a", 0, true), screen("out-b", 800, false)];
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-b", rect(800, 0, 400, 560), Some(N1)),
            ],
            vec![
                native_obs("out-a", &[N1, N2], N1),
                native_obs("out-b", &[N1, N2], N1),
            ],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    let a: std::collections::BTreeMap<String, String> = bound_ids(&reply, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let b: std::collections::BTreeMap<String, String> = bound_ids(&reply, "out-b")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert_ne!(a[N1], b[N1], "same native id maps independently");
    // macOS-style differing lists on a fresh engine stay coherent.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-b", rect(800, 0, 400, 560), Some(N3)),
            ],
            vec![
                native_obs("out-a", &[N1, N2], N1),
                native_obs("out-b", &[N3], N3),
            ],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    assert_eq!(bound_ids(&reply, "out-a").len(), 2);
    assert_eq!(bound_ids(&reply, "out-b").len(), 1);

    // Shared linked mode: one coherent ring union without dropping members.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-b", rect(800, 0, 400, 560), Some(N3)),
            ],
            vec![
                native_obs("out-a", &[N1, N2], N1),
                native_obs("out-b", &[N3], N3),
            ],
            WorkspaceMode::Shared,
            None,
            None,
            true,
        ),
    );
    let ring_a: Vec<String> = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let ring_b: Vec<String> = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-b")
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(ring_a, ring_b, "shared uses one coherent ring");
    assert!(
        reply.transfers.is_empty(),
        "no drops: {:?}",
        reply.transfers
    );
    // Both members survive on the union ring.
    let homes: Vec<String> = reply
        .windows
        .iter()
        .map(|row| row.workspace.clone())
        .collect();
    assert_eq!(homes.len(), 2);
    assert!(homes.iter().all(|home| ring_a.contains(home)));
    // No cross-output fabrication: a logical adopted from out-a's native
    // publishes no binding on out-b rows that never observed it.
    let logical_n1 = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some(N1))
        .map(|row| row.id.clone())
        .expect("bound N1");
    let mirrored_b = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-b" && row.id == logical_n1)
        .expect("mirrored row");
    assert_eq!(
        mirrored_b.native_binding, None,
        "unobserved pairs publish no binding"
    );
}

#[test]
fn shared_late_pair_binds_existing_logical_at_union_position() {
    let screens = vec![screen("out-a", 0, true), screen("out-b", 800, false)];
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-b", rect(800, 0, 400, 560), Some(N3)),
            ],
            vec![
                native_obs("out-a", &[N1, N2], N1),
                native_obs("out-b", &[N3], N3),
            ],
            WorkspaceMode::Shared,
            None,
            None,
            true,
        ),
    );
    let logical_n1 = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some(N1))
        .map(|row| row.id.clone())
        .expect("bound N1");
    // out-b's list extends with out-a's N1 and its window moves there:
    // the late pair binds the existing shared logical, no duplicate.
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-c", "out-b", rect(800, 0, 400, 560), Some(N1)),
            ],
            vec![
                native_obs("out-a", &[N1, N2], N1),
                native_obs("out-b", &[N3, N1], N3),
            ],
            WorkspaceMode::Shared,
            None,
            None,
            true,
        ),
    );
    let ring_a: Vec<String> = second
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let ring_b: Vec<String> = second
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-b")
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(ring_a, ring_b, "rings stay coherent");
    let row_c = second
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-c")
        .expect("row c");
    assert_eq!(
        row_c.workspace, logical_n1,
        "late pair homes the shared logical"
    );
    let bound_b: Vec<(String, String)> = bound_ids(&second, "out-b");
    assert!(
        bound_b.iter().any(|(id, n)| id == &logical_n1 && n == N1),
        "late pair published: {bound_b:?}"
    );
    // Union-order position: a genuinely new shared id inserts at its
    // union slot, not appended. N4 is new on out-a after N1.
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-c", "out-b", rect(800, 0, 400, 560), Some(N1)),
                fact_native("win-d", "out-a", rect(0, 0, 800, 560), Some(N4)),
            ],
            vec![
                native_obs("out-a", &[N1, N4, N2], N1),
                native_obs("out-b", &[N3, N1], N3),
            ],
            WorkspaceMode::Shared,
            None,
            None,
            true,
        ),
    );
    let ring: Vec<String> = third
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let pos_n4 = ring.iter().position(|id| {
        third.workspaces.iter().any(|row| {
            row.output.0 == "out-a" && &row.id == id && row.native_binding.as_deref() == Some(N4)
        })
    });
    let pos_n2 = ring.iter().position(|id| {
        third.workspaces.iter().any(|row| {
            row.output.0 == "out-a" && &row.id == id && row.native_binding.as_deref() == Some(N2)
        })
    });
    match (pos_n4, pos_n2) {
        (Some(n4), Some(n2)) => assert!(n4 < n2, "union-order insert: {ring:?}"),
        other => panic!("both bound expected: {other:?} in {ring:?}"),
    }
}

#[test]
fn migrate_workspace_publishes_no_destination_binding() {
    // A logical workspace migrated onto an output that never observed
    // its native id publishes no binding there: bindings are observed
    // pairs, never fabricated. The global map entry is retained (no
    // redesign of migration/hotplug ownership here).
    let screens = vec![screen("out-a", 0, true), screen("out-b", 800, false)];
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![fact_native(
                "win-a",
                "out-a",
                rect(0, 0, 400, 560),
                Some(N1),
            )],
            vec![
                native_obs("out-a", &[N1, N2], N1),
                native_obs("out-b", &[N3], N3),
            ],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    let migrated = accepted(
        &mut engine,
        &report(
            2,
            2,
            screens.clone(),
            vec![fact_native(
                "win-a",
                "out-a",
                rect(0, 0, 400, 560),
                Some(N1),
            )],
            vec![
                native_obs("out-a", &[N1, N2], N1),
                native_obs("out-b", &[N3], N3),
            ],
            WorkspaceMode::PerOutputLocal,
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            true,
        ),
    );
    let on_b: Vec<_> = migrated
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-b" && row.id == view_a)
        .collect();
    assert_eq!(on_b.len(), 1, "migrated id now on out-b");
    assert_eq!(
        on_b[0].native_binding, None,
        "no fabricated destination binding"
    );
    // Source output no longer rings the migrated id.
    assert!(
        !migrated
            .workspaces
            .iter()
            .any(|row| row.output.0 == "out-a" && row.id == view_a),
        "migration moved the logical id"
    );
}

#[test]
fn incomplete_then_complete_adopts_without_blessing_partial() {
    // First report incomplete: no bindings created, no member rows
    // published, no moves blessed. The complete follow-up adopts fully.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
        fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N2)),
    ];
    let partial = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_native_screens(),
            windows.clone(),
            vec![native_obs("out-a", &[N1, N2], N1)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            false,
        ),
    );
    assert!(
        bound_ids(&partial, "out-a").is_empty(),
        "incomplete creates no bindings"
    );
    assert!(
        partial.windows.is_empty(),
        "incomplete publishes no unproven homing"
    );
    assert!(partial.transfers.is_empty());
    let full = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_native_screens(),
            windows,
            vec![native_obs("out-a", &[N1, N2], N1)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    let bound = bound_ids(&full, "out-a");
    assert_eq!(
        bound.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>(),
        vec![N1, N2],
        "complete follow-up adopts"
    );
    assert_eq!(full.windows.len(), 2, "members homed on proof");
    assert!(full.transfers.is_empty());
}

#[test]
fn unknown_minimized_newcomer_waits_for_native_membership() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut input = report(
        1,
        1,
        two_native_screens(),
        vec![minimized_native("win-minimized", "out-a", None)],
        vec![native_obs("out-a", &[N1, N2], N1)],
        WorkspaceMode::PerOutputLocal,
        None,
        None,
        true,
    );
    let unknown = accepted(&mut engine, &input);
    assert!(
        unknown.windows.is_empty(),
        "unknown membership authorizes no row"
    );
    input.request_seq = 2;
    input.observation_seq = 2;
    input.request_id = "req-2".to_owned();
    input.windows[0].native_workspace = Some(N2.to_owned());
    let known = accepted(&mut engine, &input);
    let expected = bound_ids(&known, "out-a")
        .into_iter()
        .find(|(_, native)| native == N2)
        .expect("native binding")
        .0;
    let row = known
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-minimized")
        .expect("known minimized member");
    assert_eq!(row.workspace, expected);
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(HoldReason::Minimized)
    );
    assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    assert!(known.transfers.is_empty());
}

#[test]
fn missing_row_start_never_adopts_current_view() {
    // Capability on, first report carries per-window native evidence but
    // no native list: the unknown native membership must not silently
    // adopt the current logical view. The listed follow-up adopts.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
        fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N1)),
    ];
    let unlisted = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_native_screens(),
            windows.clone(),
            vec![],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    assert!(
        bound_ids(&unlisted, "out-a").is_empty(),
        "no list maps nothing"
    );
    assert!(
        unlisted.windows.is_empty(),
        "unlisted native evidence never adopts the current view"
    );
    assert!(unlisted.transfers.is_empty());
    let listed = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_native_screens(),
            windows,
            vec![native_obs("out-a", &[N1, N2], N1)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    let bound = bound_ids(&listed, "out-a");
    assert_eq!(bound.len(), 2, "listed follow-up adopts");
    let home = listed
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("row")
        .workspace
        .clone();
    assert_eq!(
        home,
        bound
            .iter()
            .find(|(_, n)| n == N1)
            .map(|(id, _)| id.clone())
            .expect("N1"),
        "listed homing follows the mapping"
    );
}

#[test]
fn shared_incompatible_orders_hold_without_adoption() {
    // Fresh A[N1,N2], B[N3,N1]: the first-appearance union [N1,N2,N3]
    // reverses B's reported order, so no single ring can preserve every
    // display's order. The core adopts nothing (no merge policy): no
    // bindings, no member rows, no effects, no transfers; scope, output,
    // and workspace rows hold through the existing fences. A consistent
    // follow-up adopts normally.
    let screens = vec![screen("out-a", 0, true), screen("out-b", 800, false)];
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let conflicted = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-b", rect(800, 0, 400, 560), Some(N3)),
            ],
            vec![
                native_obs("out-a", &[N1, N2], N1),
                native_obs("out-b", &[N3, N1], N3),
            ],
            WorkspaceMode::Shared,
            None,
            None,
            true,
        ),
    );
    assert!(conflicted.ready);
    assert!(
        bound_ids(&conflicted, "out-a").is_empty() && bound_ids(&conflicted, "out-b").is_empty(),
        "ambiguous projection binds nothing"
    );
    assert!(
        conflicted.windows.is_empty(),
        "no invented membership while ambiguous"
    );
    assert!(conflicted.transfers.is_empty());
    assert!(conflicted.effects.is_empty());
    // Every published scope row holds; outputs hold; workspace rows hold
    // with no fabricated bindings.
    assert!(!conflicted.scopes.is_empty(), "held rows still publish");
    for status in &conflicted.scopes {
        assert_eq!(
            status.authority,
            ScopeAuthority::Hold(HoldReason::Unreadable),
            "scope holds: {}",
            status.scope
        );
    }
    for row in &conflicted.workspaces {
        assert_eq!(row.native_binding, None);
        assert_eq!(
            row.hold,
            Some(HoldReason::Unreadable),
            "workspace holds: {}",
            row.id
        );
    }
    for output in &conflicted.outputs {
        assert!(output.hold.is_some(), "output holds: {}", output.output.0);
    }
    // Consistent follow-up (identical lists, the normal KDE shared shape)
    // recovers: initial adoption runs instead of stranding singletons.
    let adopted = accepted(
        &mut engine,
        &report(
            2,
            2,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-b", rect(800, 0, 400, 560), Some(N2)),
            ],
            vec![
                native_obs("out-a", &[N1, N2], N1),
                native_obs("out-b", &[N1, N2], N1),
            ],
            WorkspaceMode::Shared,
            None,
            None,
            true,
        ),
    );
    let bound_a = bound_ids(&adopted, "out-a");
    let bound_b = bound_ids(&adopted, "out-b");
    assert_eq!(bound_a.len(), 2, "follow-up adopts: {bound_a:?}");
    assert_eq!(bound_b.len(), 2, "follow-up adopts: {bound_b:?}");
    assert_eq!(adopted.windows.len(), 2, "members homed on proof");
    let ring_a: Vec<String> = adopted
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let ring_b: Vec<String> = adopted
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-b")
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(ring_a, ring_b, "one coherent ring");
    for row in &adopted.windows {
        assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    }
    assert!(adopted.transfers.is_empty());
}

#[test]
fn shared_missing_native_inventory_retains_unbound_rings() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut input = report(
        1,
        1,
        two_native_screens(),
        vec![],
        vec![],
        WorkspaceMode::Shared,
        None,
        None,
        true,
    );
    let first = accepted(&mut engine, &input);
    assert_eq!(first.workspaces.len(), 2);
    assert!(
        first
            .workspaces
            .iter()
            .all(|row| row.native_binding.is_none())
    );
    input.request_seq = 2;
    input.observation_seq = 2;
    input.request_id = "req-2".to_owned();
    input.inventory_complete = false;
    let partial = accepted(&mut engine, &input);
    assert_eq!(first.workspaces, partial.workspaces);
}

#[test]
fn pending_transfer_holds_on_unknown_native() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_native_screens(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N1)),
            ],
            vec![native_obs("out-a", &[N1, N2], N1)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    let target = bound_ids(&first, "out-a")
        .into_iter()
        .find(|(_, n)| n == N2)
        .map(|(id, _)| id)
        .expect("target");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            two_native_screens(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(N1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N1)),
            ],
            vec![native_obs("out-a", &[N1, N2], N1)],
            WorkspaceMode::PerOutputLocal,
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target,
                follow: false,
            }),
            true,
        ),
    );
    // Same-screen raw with unknown native evidence: the flight holds at
    // the source, never flaps to the view.
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_native_screens(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), None),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(N1)),
            ],
            vec![native_obs("out-a", &[N1, N2], N1)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        held.transfers.len(),
        1,
        "unknown holds the flight: {:?}",
        held.transfers
    );
    assert!(
        held.transfers
            .iter()
            .all(|t| matches!(t.kind, TransferKind::Window(_))),
        "no flap: {:?}",
        held.transfers
    );
    // The retained scope withholds write authority while native proof is
    // unknown; the retained home is kept, not blessed.
    let source = engine
        .adapter_homing(&WindowId("win-a".to_owned()))
        .expect("retained home")
        .1;
    assert_eq!(
        scope_authority(&held, "out-a", &source),
        Some(ScopeAuthority::Hold(HoldReason::Unreadable)),
        "unknown flight fences its scope"
    );
    let arrived = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_native_screens(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 800, 560), Some(N2)),
                fact_native("win-b", "out-a", rect(0, 0, 800, 560), Some(N1)),
            ],
            vec![native_obs("out-a", &[N1, N2], N1)],
            WorkspaceMode::PerOutputLocal,
            None,
            None,
            true,
        ),
    );
    assert!(arrived.transfers.is_empty(), "exact native receipt settles");
    let mover = arrived
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("arrived member");
    assert_eq!(mover.workspace, source);
    assert_eq!(mover.membership_authority, ScopeAuthority::Controlled);
}

#[test]
fn equal_counts_pad_neutrally_over_native_rings() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = vec![screen("out-a", 0, true), screen("out-b", 800, false)];
    let mut req = report(
        1,
        1,
        screens,
        vec![fact_native(
            "win-a",
            "out-a",
            rect(0, 0, 400, 560),
            Some(N1),
        )],
        vec![
            native_obs("out-a", &[N1, N2], N1),
            native_obs("out-b", &[N1], N1),
        ],
        WorkspaceMode::PerOutputLocal,
        None,
        None,
        true,
    );
    req.equal_workspace_count = true;
    let reply = accepted(&mut engine, &req);
    let len_a = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .count();
    let len_b = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-b")
        .count();
    assert_eq!(len_a, len_b, "equal counts pad neutrally");
    // Native bindings survive padding.
    assert!(bound_ids(&reply, "out-a").iter().any(|(_, n)| n == N1));
}
