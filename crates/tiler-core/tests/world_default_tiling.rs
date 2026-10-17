//! Saved-default tiling seed parity (legacy `defaultTiled`): the reported
//! saved default seeds newly observed native workspaces (startup seeds
//! all; live changes seed only subsequently observed ones), while manual
//! per-workspace toggles never reseed. Full reports through the production
//! transition, never constructor asserts.

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    DomainReport, OutputTopology, ScopeAuthority, TilingMode, WORLD_CONTRACT_VERSION, WorldCommand,
    WorldReport,
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

fn domain(ws: &str, windows: Vec<EngineWindow>) -> DomainReport {
    DomainReport {
        domain: OutputDomain {
            id: OutputId("out".to_owned()),
            workspace: WorkspaceId(ws.to_owned()),
            bounds: rect(0, 0, 800, 600),
            gap: 8,
            adjacent: BTreeMap::new(),
        },
        outer_gap: 4,
        focused: windows
            .first()
            .map(|w| w.window.clone())
            .unwrap_or(WindowId(String::new())),
        windows,
        closed: Vec::new(),
        readable: true,
        hold: None,
    }
}

fn report(
    request_seq: u64,
    observation_seq: u64,
    default_tiled: bool,
    natives: &[&str],
    current: &str,
    domains: Vec<DomainReport>,
) -> WorldReport {
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
            native_order: natives.iter().map(|s| (*s).to_owned()).collect(),
            current_native: current.to_owned(),
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
        default_tiled,
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

fn workspace_mode_of(reply: &tiler_core::world_report::WorldReply, id: &str) -> TilingMode {
    reply
        .workspaces
        .iter()
        .find(|w| w.id == id)
        .unwrap_or_else(|| panic!("workspace row for {id}"))
        .tiling_mode
}

fn scope_of<'a>(
    reply: &'a tiler_core::world_report::WorldReply,
    scope: &str,
) -> &'a tiler_core::world_report::ScopeStatus {
    reply
        .scopes
        .iter()
        .find(|s| s.scope == scope)
        .unwrap_or_else(|| panic!("scope row for {scope}"))
}

#[test]
fn startup_false_floats_seed_with_no_frame_writes() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    // Saved default false at startup: ws1 seeds floating; the tiled window
    // keeps its native frame because the reply authorizes no frame write.
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            false,
            &["ws1"],
            "ws1",
            vec![domain("ws1", vec![win("win-a", "ws1", 0)])],
        ),
    );
    assert_eq!(workspace_mode_of(&reply, "ws1"), TilingMode::Floating);
    assert_eq!(
        scope_of(&reply, "out/ws1").authority,
        ScopeAuthority::Untouched
    );
    assert!(
        reply
            .windows
            .iter()
            .all(|w| !matches!(w.frame_authority, ScopeAuthority::Controlled)),
        "startup false writes no frames; native frames unchanged"
    );
}

#[test]
fn live_default_change_seeds_only_new_workspaces() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            true,
            &["ws1"],
            "ws1",
            vec![domain("ws1", vec![win("win-a", "ws1", 0)])],
        ),
    );
    assert_eq!(workspace_mode_of(&first, "ws1"), TilingMode::Tiled);
    // Live flip to false plus a new native ws2: only ws2 seeds floating;
    // ws1 keeps its tiled seed (no override reset).
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            false,
            &["ws1", "ws2"],
            "ws1",
            vec![
                domain("ws1", vec![win("win-a", "ws1", 0)]),
                domain("ws2", Vec::new()),
            ],
        ),
    );
    assert_eq!(workspace_mode_of(&second, "ws1"), TilingMode::Tiled);
    assert_eq!(workspace_mode_of(&second, "ws2"), TilingMode::Floating);
}

#[test]
fn manual_override_survives_later_default_changes() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            false,
            &["ws1"],
            "ws1",
            vec![domain("ws1", vec![win("win-a", "ws1", 0)])],
        ),
    );
    // Manual retile of the seeded-floating ws1.
    let mut toggle = report(
        2,
        2,
        false,
        &["ws1"],
        "ws1",
        vec![domain("ws1", vec![win("win-a", "ws1", 0)])],
    );
    toggle.command_seq = Some(1);
    toggle.command = Some(WorldCommand::SetWorkspaceTiling {
        output: OutputId("out".to_owned()),
        workspace: "ws1".to_owned(),
        tiled: true,
    });
    let toggled = accepted(&mut engine, &toggle);
    assert_eq!(workspace_mode_of(&toggled, "ws1"), TilingMode::Tiled);
    // Saved default later flips true with a new ws2: the manual ws1
    // override is retained and ws2 seeds from the then-current default.
    let later = accepted(
        &mut engine,
        &report(
            3,
            3,
            true,
            &["ws1", "ws2"],
            "ws1",
            vec![
                domain("ws1", vec![win("win-a", "ws1", 0)]),
                domain("ws2", Vec::new()),
            ],
        ),
    );
    assert_eq!(workspace_mode_of(&later, "ws1"), TilingMode::Tiled);
    assert_eq!(workspace_mode_of(&later, "ws2"), TilingMode::Tiled);
    // And a manual float override on ws2 is not reset by a default flip
    // back to false either.
    let mut float_it = report(
        4,
        4,
        true,
        &["ws1", "ws2"],
        "ws1",
        vec![
            domain("ws1", vec![win("win-a", "ws1", 0)]),
            domain("ws2", Vec::new()),
        ],
    );
    float_it.command_seq = Some(2);
    float_it.command = Some(WorldCommand::SetWorkspaceTiling {
        output: OutputId("out".to_owned()),
        workspace: "ws2".to_owned(),
        tiled: false,
    });
    let floated = accepted(&mut engine, &float_it);
    assert_eq!(workspace_mode_of(&floated, "ws2"), TilingMode::Floating);
    let last = accepted(
        &mut engine,
        &report(
            5,
            5,
            false,
            &["ws1", "ws2"],
            "ws1",
            vec![
                domain("ws1", vec![win("win-a", "ws1", 0)]),
                domain("ws2", Vec::new()),
            ],
        ),
    );
    assert_eq!(workspace_mode_of(&last, "ws1"), TilingMode::Tiled);
    assert_eq!(workspace_mode_of(&last, "ws2"), TilingMode::Floating);
}
