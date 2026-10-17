//! Born-fullscreen exact lifetime hold (D5) through the world route:
//! a first-seen fullscreen window holds slotless (no reserved tile, no
//! frame/focus/stack/membership side effects) until its first exit, which
//! classifies fresh; a later fullscreen keeps its reserved tiled
//! allocation while the native overlay shows. Full reports on the
//! production transition.

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    DomainReport, OutputTopology, ScopeAuthority, WORLD_CONTRACT_VERSION, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn win(id: &str, ws: &str, x: i32) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId("out".to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        rect: rect(x, 0, 200, 200),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

/// First-seen fullscreen exactly as world-observe normalizes it: native
/// overlay flags with no synthetic float.
fn born_fullscreen(id: &str, ws: &str, x: i32) -> EngineWindow {
    let mut w = win(id, ws, x);
    w.fullscreen = true;
    w.fit_excluded = true;
    w
}

fn domain(ws: &str, windows: Vec<EngineWindow>, focused: &str) -> DomainReport {
    DomainReport {
        domain: OutputDomain {
            id: OutputId("out".to_owned()),
            workspace: WorkspaceId(ws.to_owned()),
            bounds: rect(0, 0, 800, 600),
            gap: 8,
            adjacent: BTreeMap::new(),
        },
        outer_gap: 4,
        focused: WindowId(focused.to_owned()),
        windows,
        closed: Vec::new(),
        readable: true,
        hold: None,
    }
}

fn report(request_seq: u64, observation_seq: u64, domains: Vec<DomainReport>) -> WorldReport {
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
            native_order: vec!["ws1".to_owned()],
            current_native: "ws1".to_owned(),
            complete: true,
            geometry: None,
            primary: false,
        }],
        bindings: Vec::new(),
        domains,
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

#[test]
fn born_fullscreen_holds_slotless_until_first_exit() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    // First sighting: a born-fullscreen game plus one ordinary tiled
    // window. The game holds slotless: no tile reserved, no writes, no
    // focus/stack/membership side effects.
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![domain(
                "ws1",
                vec![
                    win("win-a", "ws1", 0),
                    born_fullscreen("win-game", "ws1", 210),
                ],
                "win-a",
            )],
        ),
    );
    let held = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-game")
        .expect("born-fullscreen row");
    assert_eq!(format!("{}/{}", held.output.0, held.workspace), "out/ws1");
    assert!(
        held.allocation.is_none(),
        "no reserved tile for born fullscreen"
    );
    assert_eq!(held.frame_authority, ScopeAuthority::Untouched);
    assert_eq!(held.membership_authority, ScopeAuthority::Untouched);
    assert!(reply.effects.is_empty(), "no auto focus");
    assert!(reply.transfers.is_empty(), "no auto membership");
    // The ordinary member still tiles around the hold.
    let tiled = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .expect("tiled row");
    assert!(tiled.allocation.is_some());
    // First exit (same id, normal flags) classifies fresh: the game
    // admits tiled with retained geometry policy, still no side effects.
    let exit = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![domain(
                "ws1",
                vec![win("win-a", "ws1", 0), win("win-game", "ws1", 210)],
                "win-game",
            )],
        ),
    );
    let admitted = exit
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-game")
        .expect("released row");
    assert_eq!(admitted.membership_authority, ScopeAuthority::Controlled);
    assert!(admitted.allocation.is_some(), "first exit tiles fresh");
    assert!(exit.effects.is_empty());
    assert!(exit.transfers.is_empty());
}

#[test]
fn later_fullscreen_keeps_reserved_allocation() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    // Ordinary tiled life first.
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![domain("ws1", vec![win("win-a", "ws1", 0)], "win-a")],
        ),
    );
    // Later fullscreen: the tile stays reserved under the native overlay
    // with no command side effects. The retained geometry still projects
    // (reserved allocation); frame writes are fenced adapter-side by the
    // observed overlay guard, exactly like the legacy never-actuated slot.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![domain(
                "ws1",
                vec![born_fullscreen("win-a", "ws1", 0)],
                "win-a",
            )],
        ),
    );
    let overlay = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-a")
        .expect("overlay row");
    assert!(
        overlay.allocation.is_some(),
        "later fullscreen keeps its reserved tile"
    );
    assert_eq!(
        overlay.effective_frame, None,
        "no effective frame on fullscreen overlays"
    );
    assert_eq!(
        overlay.frame_authority,
        ScopeAuthority::Untouched,
        "no automatic frame authority on fullscreen overlays"
    );
    assert_eq!(overlay.membership_authority, ScopeAuthority::Controlled);
    assert!(reply.effects.is_empty());
    assert!(reply.transfers.is_empty());
}

#[test]
fn born_fullscreen_id_reuse_after_close_admits_fresh() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![domain(
                "ws1",
                vec![
                    win("win-a", "ws1", 0),
                    born_fullscreen("win-game", "ws1", 210),
                ],
                "win-a",
            )],
        ),
    );
    // Explicit close of the game, then the same id returns as an
    // ordinary window (new native lifetime): fresh tiled admission, no
    // stale hold inherited.
    let mut closed = report(
        2,
        2,
        vec![domain("ws1", vec![win("win-a", "ws1", 0)], "win-a")],
    );
    closed.domains[0].closed = vec![WindowId("win-game".to_owned())];
    accepted(&mut engine, &closed);
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![domain(
                "ws1",
                vec![win("win-a", "ws1", 0), win("win-game", "ws1", 210)],
                "win-a",
            )],
        ),
    );
    let fresh = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-game")
        .expect("reused-id row");
    assert_eq!(fresh.membership_authority, ScopeAuthority::Controlled);
    assert!(fresh.allocation.is_some());
}

#[test]
fn born_fullscreen_id_reuse_without_close_heals_on_normal_sighting() {
    // Same native id observed normal after the held game vanishes with no
    // explicit close (new native lifetime reusing the id): the stale hold
    // converts to a fresh tiled admission instead of inheriting it.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![domain(
                "ws1",
                vec![
                    win("win-a", "ws1", 0),
                    born_fullscreen("win-game", "ws1", 210),
                ],
                "win-a",
            )],
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![domain(
                "ws1",
                vec![win("win-a", "ws1", 0), win("win-game", "ws1", 210)],
                "win-a",
            )],
        ),
    );
    let healed = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "win-game")
        .expect("reused-id row");
    assert_eq!(healed.membership_authority, ScopeAuthority::Controlled);
    assert!(healed.allocation.is_some());
    assert!(reply.effects.is_empty());
    assert!(reply.transfers.is_empty());
}
