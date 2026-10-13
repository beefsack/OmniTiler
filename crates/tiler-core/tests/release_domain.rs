//! Explicit domain release: drop one domain, then fresh-adopt its current
//! geometry while an independent domain is unaffected.

use std::collections::BTreeMap;

use tiler_core::boundary::{CoreCommand, CoreEvent, CoreReply};
use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::engine::Engine;
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;

fn owner() -> OwnerId {
    OwnerId::parse("owner-1").unwrap()
}

fn generation() -> GenerationId {
    GenerationId::parse("gen-1").unwrap()
}

fn domain(output: &str, workspace: &str) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.into()),
        workspace: WorkspaceId(workspace.into()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}

fn window(id: &str, output: &str, workspace: &str, x: i32, w: i32) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.into()),
        output: OutputId(output.into()),
        workspace: WorkspaceId(workspace.into()),
        rect: Rect { x, y: 0, w, h: 800 },
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    }
}

fn event(
    domain: &OutputDomain,
    windows: Vec<EngineWindow>,
    focused: &str,
    correlation: &str,
    command: CoreCommand,
) -> CoreEvent {
    CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: CorrelationId::parse(correlation).unwrap(),
        revision: 0,
        fingerprint: 7,
        domain_key: domain.key(),
        domain: domain.clone(),
        outer_gap: 0,
        focused_window: WindowId(focused.into()),
        windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: vec![],
        closed_windows: vec![],
        command,
    }
}

fn reconcile(
    domain: &OutputDomain,
    windows: Vec<EngineWindow>,
    focused: &str,
    correlation: &str,
) -> CoreEvent {
    event(
        domain,
        windows,
        focused,
        correlation,
        CoreCommand::Reconcile,
    )
}

fn release(
    domain: &OutputDomain,
    windows: Vec<EngineWindow>,
    focused: &str,
    correlation: &str,
) -> CoreEvent {
    event(
        domain,
        windows,
        focused,
        correlation,
        CoreCommand::ReleaseDomain,
    )
}

fn tiled_geometry(reply: CoreReply) -> Vec<(String, Rect)> {
    match reply {
        CoreReply::Tiled(plan) => plan
            .geometry
            .iter()
            .map(|g| (g.window.0.clone(), g.rect))
            .collect(),
        other => panic!("expected tiled plan, got {other:?}"),
    }
}

#[test]
fn release_then_fresh_reconcile_readopts_current_geometry() {
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    let home = domain("out-1", "ws-1");
    let current = vec![
        window("win-a1", "out-1", "ws-1", 0, 600),
        window("win-a2", "out-1", "ws-1", 600, 600),
    ];
    let planned =
        tiled_geometry(engine.handle(&reconcile(&home, current.clone(), "win-a2", "rel-seed-1")));
    assert_eq!(planned.len(), 2);
    assert!(engine.contains(&home.key()));

    match engine.handle(&release(&home, current.clone(), "win-a2", "rel-drop-1")) {
        CoreReply::Released => {}
        other => panic!("expected release, got {other:?}"),
    }
    assert!(!engine.contains(&home.key()));

    // Repeat release is idempotent: still released, still absent.
    match engine.handle(&release(&home, current.clone(), "win-a2", "rel-drop-2")) {
        CoreReply::Released => {}
        other => panic!("expected idempotent release, got {other:?}"),
    }
    assert!(!engine.contains(&home.key()));

    // Ordinary fresh reconcile re-adopts the carried current geometry
    // through the existing fit instead of the seed spiral.
    let mut readapted =
        tiled_geometry(engine.handle(&reconcile(&home, current.clone(), "win-a2", "rel-fresh-1")));
    readapted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut expected = planned;
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(readapted, expected);
    assert!(engine.contains(&home.key()));
}

#[test]
fn release_leaves_independent_domain_untouched() {
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    let home = domain("out-1", "ws-1");
    let away = domain("out-2", "ws-1");
    let home_windows = vec![
        window("win-a1", "out-1", "ws-1", 0, 600),
        window("win-a2", "out-1", "ws-1", 600, 600),
    ];
    let away_windows = vec![window("win-b1", "out-2", "ws-1", 0, 1200)];
    tiled_geometry(engine.handle(&reconcile(
        &home,
        home_windows.clone(),
        "win-a2",
        "rel-two-1",
    )));
    tiled_geometry(engine.handle(&reconcile(
        &away,
        away_windows.clone(),
        "win-b1",
        "rel-two-2",
    )));

    let before = match engine.handle(&reconcile(
        &away,
        away_windows.clone(),
        "win-b1",
        "rel-two-3",
    )) {
        CoreReply::Projection(plan) => plan.geometry,
        other => panic!("expected retained projection for untouched domain, got {other:?}"),
    };

    match engine.handle(&release(&home, home_windows.clone(), "win-a2", "rel-two-4")) {
        CoreReply::Released => {}
        other => panic!("expected release, got {other:?}"),
    }
    assert!(!engine.contains(&home.key()));
    assert!(engine.contains(&away.key()));

    match engine.handle(&reconcile(
        &away,
        away_windows.clone(),
        "win-b1",
        "rel-two-5",
    )) {
        CoreReply::Projection(plan) => assert_eq!(plan.geometry, before),
        other => panic!("independent domain must keep projecting, got {other:?}"),
    }

    // The released domain fresh-adopts on its next ordinary reconcile.
    let readapted = tiled_geometry(engine.handle(&reconcile(
        &home,
        home_windows.clone(),
        "win-a2",
        "rel-two-6",
    )));
    assert_eq!(readapted.len(), 2);
    assert!(engine.contains(&home.key()));
    assert!(engine.contains(&away.key()));
}
