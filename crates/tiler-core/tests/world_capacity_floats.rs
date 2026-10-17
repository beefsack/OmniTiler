//! Capacity-float ordinary-float echo at the full-world boundary (D4/D5).
//!
//! The session/engine admission units own tile/float classification; this
//! file owns what the adapter sees: every controlled exception row carries
//! explicit `floating` plus the diagnostic `capacity` origin, tiled rows
//! carry neither, and unclassified protected/held/overlay/minimized rows
//! never infer float from a missing allocation. Fresh frames are
//! preserved, evictions keep the last live frame, growth/close/gap edits
//! never auto-retile, restart/minimize/restore/disable-enable keep float
//! status without minting tile slots, and an explicit tile retry reports
//! no-space without a false tile override (success clears it).

use tiler_core::boundary::CoreCommand;
use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    DomainReport, FloatOrigin, OutputTopology, ScopeAuthority, WORLD_CONTRACT_VERSION,
    WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn win(id: &str, floating: bool, x: i32) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId("out".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
        rect: rect(x, 0, 50, 50),
        floating,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn id(n: usize) -> String {
    format!("win-{n:06}")
}

/// 10x10 gap 8 exhausts after four tiles (session recipe): six rows seed a
/// mixed roster with two capacity floats in one atomic commit.
fn base_report(request_seq: u64, observation_seq: u64) -> WorldReport {
    let windows: Vec<EngineWindow> = (0..6)
        .map(|i| win(&id(i), false, (i as i32) * 10))
        .collect();
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq: None,
        command: None,
        workspace_mode: WorkspaceMode::PerOutputLocal,
        topology: vec![OutputTopology {
            output: OutputId("out".to_owned()),
            native_order: vec!["ws".to_owned()],
            current_native: "ws".to_owned(),
            complete: true,
            geometry: None,
            primary: true,
        }],
        bindings: Vec::new(),
        domains: vec![DomainReport {
            domain: OutputDomain {
                id: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
                bounds: rect(0, 0, 10, 10),
                gap: 8,
                adjacent: Default::default(),
            },
            outer_gap: 0,
            focused: WindowId(id(0)),
            windows,
            closed: Vec::new(),
            readable: true,
            hold: None,
        }],
        minimized: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: String::new(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: Vec::new(),
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

/// Follow-up reports name a tiled window as observed focus: a fresh
/// capacity seed leaves session focus none (focus unit owns fresh-float
/// focus), and reconcile refuses focus-none against a floated observed
/// focus. Naming a tile keeps the echo path under test.
fn with_tiled_focus(report: &mut WorldReport) {
    report.domains[0].focused = WindowId(id(1));
}

fn row_of<'a>(
    reply: &'a tiler_core::world_report::WorldReply,
    id: &str,
) -> &'a tiler_core::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == id)
        .unwrap_or_else(|| panic!("expected row for {id}"))
}

fn float_ids(reply: &tiler_core::world_report::WorldReply) -> Vec<String> {
    let mut ids: Vec<String> = reply
        .windows
        .iter()
        .filter(|w| w.floating)
        .map(|w| w.lifetime.0.clone())
        .collect();
    ids.sort();
    ids
}

#[test]
fn mixed_seed_publishes_explicit_float_rows_with_capacity_origin() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = accepted(&mut engine, &base_report(1, 1));
    assert_eq!(reply.windows.len(), 6, "full roster publishes");
    let tiled: Vec<_> = reply
        .windows
        .iter()
        .filter(|w| w.allocation.is_some())
        .collect();
    assert_eq!(tiled.len(), 4, "four tiles project");
    for row in &tiled {
        assert!(!row.floating, "tiled rows carry no float marker");
        assert_eq!(row.float_origin, None, "tiled rows carry no origin");
        assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    }
    let floats = float_ids(&reply);
    assert_eq!(floats.len(), 2, "two capacity floats echo");
    for id in &floats {
        let row = row_of(&reply, id);
        assert_eq!(row.allocation, None, "floats take no tile slot");
        assert_eq!(
            row.frame_authority,
            ScopeAuthority::Untouched,
            "float frames stay native-owned"
        );
        assert_eq!(
            row.float_origin,
            Some(FloatOrigin::Capacity),
            "diagnostic origin rides the echo"
        );
    }
    // The wire carries the explicit classification, bounded by the estimate.
    let encoded = tiler_core::world_report::encode_reply_canonical(&reply);
    assert_eq!(encoded.matches("\"floating\":true").count(), 2);
    assert_eq!(encoded.matches("\"float_origin\":\"capacity\"").count(), 2);
    assert!(
        tiler_core::world_report::reply_estimate(&reply) >= encoded.len(),
        "estimator bounds the classified reply"
    );
}

#[test]
fn normalized_echo_growth_and_close_never_retile() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let first = accepted(&mut engine, &base_report(1, 1));
    let floats = float_ids(&first);
    assert_eq!(floats.len(), 2);
    // Normalized floating:true echo replays with no re-tile.
    let mut echo = base_report(2, 2);
    for w in &mut echo.domains[0].windows {
        w.floating = floats.contains(&w.window.0);
    }
    with_tiled_focus(&mut echo);
    let reply = accepted(&mut engine, &echo);
    assert_eq!(float_ids(&reply), floats, "echo persists without retile");
    assert!(
        reply
            .windows
            .iter()
            .filter(|w| w.floating)
            .all(|w| w.allocation.is_none() && w.float_origin == Some(FloatOrigin::Capacity)),
        "origin survives the echo"
    );
    // Growth alone never tiles: the same echo on 800x600 stays floated.
    let mut grown = base_report(3, 3);
    grown.domains[0].domain.bounds = rect(0, 0, 800, 600);
    grown.domains[0].domain.gap = 8;
    for w in &mut grown.domains[0].windows {
        w.floating = floats.contains(&w.window.0);
    }
    with_tiled_focus(&mut grown);
    let reply = accepted(&mut engine, &grown);
    assert_eq!(float_ids(&reply), floats, "growth never auto-retiles");
    // Closing a tile never retiles the surviving floats either.
    let mut closed = base_report(4, 4);
    closed.domains[0].domain.bounds = rect(0, 0, 800, 600);
    closed.domains[0].windows.retain(|w| w.window.0 != id(1));
    for w in &mut closed.domains[0].windows {
        w.floating = floats.contains(&w.window.0);
    }
    with_tiled_focus(&mut closed);
    closed.domains[0].closed = vec![WindowId(id(1))];
    let reply = accepted(&mut engine, &closed);
    assert_eq!(float_ids(&reply), floats, "close never auto-retiles");
    assert!(
        reply.windows.iter().all(|w| w.lifetime.0 != id(1)),
        "closed tile leaves"
    );
}

#[test]
fn explicit_retry_reports_no_space_then_clears_on_success() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let first = accepted(&mut engine, &base_report(1, 1));
    let floats = float_ids(&first);
    let retry = floats[0].clone();
    // Explicit unfloat while still exhausted: stays floated, reports
    // no-space, records no tile override.
    let mut attempt = base_report(2, 2);
    for w in &mut attempt.domains[0].windows {
        w.floating = floats.contains(&w.window.0);
    }
    with_tiled_focus(&mut attempt);
    attempt.command_seq = Some(1);
    attempt.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: retry.clone(),
        float_rect: None,
    }));
    let reply = accepted(&mut engine, &attempt);
    match &reply.command_result {
        Some(tiler_core::world_report::CommandResult::Float {
            window,
            rect,
            no_space,
        }) => {
            assert_eq!(window.0, retry);
            assert_eq!(*rect, None);
            assert!(*no_space, "exhausted retry reports no-space");
        }
        other => panic!("expected float result, got {other:?}"),
    }
    let row = row_of(&reply, &retry);
    assert!(row.floating, "failed retry stays floated");
    assert_eq!(row.float_origin, Some(FloatOrigin::Capacity));
    // The wire marks the retry distinctly from a successful unfloat.
    let encoded = tiler_core::world_report::encode_reply_canonical(&reply);
    assert!(
        encoded.contains("\"noSpace\":true"),
        "no-space retry carries the marker"
    );
    assert!(
        tiler_core::world_report::reply_estimate(&reply) >= encoded.len(),
        "estimator bounds the no-space reply"
    );
    // A close frees room; the retry then tiles and clears the marker.
    let mut freed = base_report(3, 3);
    freed.domains[0].windows.retain(|w| w.window.0 != id(1));
    for w in &mut freed.domains[0].windows {
        w.floating = floats.contains(&w.window.0);
    }
    with_tiled_focus(&mut freed);
    freed.domains[0].closed = vec![WindowId(id(1))];
    freed.command_seq = Some(2);
    freed.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: retry.clone(),
        float_rect: None,
    }));
    let reply = accepted(&mut engine, &freed);
    match &reply.command_result {
        Some(tiler_core::world_report::CommandResult::Float {
            window,
            rect,
            no_space,
        }) => {
            assert_eq!(window.0, retry);
            assert_eq!(*rect, None);
            assert!(!no_space, "successful tile reports room");
        }
        other => panic!("expected float result, got {other:?}"),
    }
    let row = row_of(&reply, &retry);
    assert!(!row.floating, "successful retry tiles");
    assert_eq!(row.float_origin, None, "tile clears the origin");
    assert!(row.allocation.is_some(), "tile takes a slot");
    // The wire distinguishes the two outcomes.
    let encoded = tiler_core::world_report::encode_reply_canonical(&reply);
    assert!(
        !encoded.contains("\"noSpace\":true"),
        "success carries no marker"
    );
}

#[test]
fn minimized_float_restores_without_tile_slot() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let first = accepted(&mut engine, &base_report(1, 1));
    let floats = float_ids(&first);
    let target = floats[0].clone();
    // Minimizing the float publishes an explicit minimized hold row that
    // never reads as a float, with no managed geometry.
    let mut minimized = base_report(2, 2);
    minimized.domains[0]
        .windows
        .retain(|w| w.window.0 != target);
    for w in &mut minimized.domains[0].windows {
        w.floating = floats.contains(&w.window.0);
    }
    with_tiled_focus(&mut minimized);
    minimized.minimized = vec![tiler_core::world_report::MinimizedEntry {
        window: WindowId(target.clone()),
        output: OutputId("out".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    }];
    let reply = accepted(&mut engine, &minimized);
    let row = row_of(&reply, &target);
    assert!(!row.floating, "minimized rows never infer float");
    assert_eq!(row.allocation, None);
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(tiler_core::world_report::HoldReason::Minimized)
    );
    // Restore with a floating echo returns the float with no tile slot.
    // The diagnostic origin normalizes to ordinary across the minimize
    // roundtrip; float status (never a D23 tiled slot) is what persists.
    let mut restored = base_report(3, 3);
    for w in &mut restored.domains[0].windows {
        w.floating = w.window.0 == target || floats.contains(&w.window.0);
    }
    with_tiled_focus(&mut restored);
    let reply = accepted(&mut engine, &restored);
    let row = row_of(&reply, &target);
    assert!(row.floating, "restore keeps float status");
    assert_eq!(row.allocation, None, "restore mints no tile slot");
}

#[test]
fn held_scope_never_relabels_excluded_clients() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(&mut engine, &base_report(1, 1));
    // An unreadable scope holds every lifetime explicitly: overlays and
    // excluded members keep Hold authority and never read as floats.
    let mut held = base_report(2, 2);
    held.domains[0].readable = false;
    held.domains[0].hold = Some(tiler_core::world_report::HoldReason::Unreadable);
    for w in &mut held.domains[0].windows {
        w.fullscreen = w.window.0 == id(1);
        w.fit_excluded = w.window.0 == id(1);
    }
    with_tiled_focus(&mut held);
    with_tiled_focus(&mut held);
    let reply = accepted(&mut engine, &held);
    assert!(
        reply.windows.iter().all(|w| !w.floating),
        "held rows never infer float"
    );
    assert!(
        reply
            .windows
            .iter()
            .all(|w| w.float_origin.is_none() && w.allocation.is_none()),
        "held rows carry no origin and no slot"
    );
    let overlay = row_of(&reply, &id(1));
    assert!(
        matches!(
            overlay.frame_authority,
            ScopeAuthority::Hold(tiler_core::world_report::HoldReason::Unreadable)
        ),
        "overlay keeps its hold"
    );
}

#[test]
fn disable_enable_keeps_floats_without_tile_slots() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let first = accepted(&mut engine, &base_report(1, 1));
    let floats = float_ids(&first);
    assert_eq!(floats.len(), 2);
    // Disable releases the retained session; the scope goes floating with
    // explicit untouched rows and no frame writes.
    let mut disable = base_report(2, 2);
    for w in &mut disable.domains[0].windows {
        w.floating = floats.contains(&w.window.0);
    }
    with_tiled_focus(&mut disable);
    disable.command_seq = Some(1);
    disable.command = Some(WorldCommand::SetWorkspaceTiling {
        output: OutputId("out".to_owned()),
        workspace: "ws".to_owned(),
        tiled: false,
    });
    let reply = accepted(&mut engine, &disable);
    assert!(
        reply.windows.iter().all(|w| w.allocation.is_none()),
        "disabled scope writes no tile frames"
    );
    // Re-enable re-adopts through the fresh route: the saved floats echo
    // back as floats with no tile slots.
    let mut enable = base_report(3, 3);
    for w in &mut enable.domains[0].windows {
        w.floating = floats.contains(&w.window.0);
    }
    with_tiled_focus(&mut enable);
    enable.command_seq = Some(2);
    enable.command = Some(WorldCommand::SetWorkspaceTiling {
        output: OutputId("out".to_owned()),
        workspace: "ws".to_owned(),
        tiled: true,
    });
    let reply = accepted(&mut engine, &enable);
    assert_eq!(float_ids(&reply), floats, "enable keeps saved floats");
    assert!(
        reply
            .windows
            .iter()
            .filter(|w| w.floating)
            .all(|w| w.allocation.is_none()),
        "re-adopted floats take no tile slots"
    );
}
