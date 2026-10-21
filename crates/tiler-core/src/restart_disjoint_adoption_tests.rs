#![forbid(unsafe_code)]
use std::collections::BTreeMap;

use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::engine::Engine;
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::seed::{EngineWindow, seed_session};
use crate::session::OutputDomain;

fn owner() -> OwnerId {
    OwnerId::parse("kwin-plan-adapter").unwrap()
}

fn generation() -> GenerationId {
    GenerationId::parse("plan-1").unwrap()
}

fn domain(output: &str) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.into()),
        workspace: WorkspaceId("ws-1".into()),
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

fn window(id: &str, output: &str, x: i32) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.into()),
        output: OutputId(output.into()),
        workspace: WorkspaceId("ws-1".into()),
        rect: Rect {
            x,
            y: 0,
            w: 600,
            h: 800,
        },
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}

fn seeded_engine() -> Engine {
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    let retained = domain("out-1");
    let old = [
        window("win-old-1", "out-1", 0),
        window("win-old-2", "out-1", 600),
    ];
    engine.store_committed(
        retained.key(),
        seed_session(&owner(), &generation(), 7, &retained, &old).unwrap(),
        0,
    );
    engine
}

fn reconcile(windows: Vec<EngineWindow>) -> CoreEvent {
    let domain = domain("out-1");
    CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: CorrelationId::parse("c-restart").unwrap(),
        revision: 0,
        fingerprint: 7,
        domain_key: domain.key(),
        domain,
        outer_gap: 0,
        focused_window: WindowId("win-z-left".into()),
        windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: vec![],
        closed_windows: vec![],
        command: CoreCommand::Reconcile,
    }
}

#[test]
fn disjoint_reconcile_adopts_like_fresh_startup() {
    let mut engine = seeded_engine();
    // UUID order is opposite observed spatial order.
    let event = reconcile(vec![
        window("win-z-left", "out-1", 0),
        window("win-a-right", "out-1", 600),
    ]);
    match engine.handle(&event) {
        CoreReply::Tiled(plan) => {
            assert_eq!(plan.geometry.len(), 2);
            assert_eq!(plan.geometry[0].window.0, "win-z-left");
            assert_eq!(plan.geometry[0].rect.x, 0);
            assert_eq!(plan.geometry[1].window.0, "win-a-right");
            assert_eq!(plan.geometry[1].rect.x, 600);
        }
        other => panic!("expected fresh strip adoption, got {other:?}"),
    }
    assert_eq!(engine.generation().unwrap().as_str(), "plan-1");
}

#[test]
fn any_shared_id_preserves_retained_topology() {
    let mut engine = seeded_engine();
    let key = domain("out-1").key();
    let original = engine.session(&key).unwrap().snapshot();
    let survivor_leaf = original
        .windows
        .iter()
        .find(|link| link.window.0 == "win-old-2")
        .unwrap()
        .leaf
        .clone();
    let event = reconcile(vec![
        window("win-new", "out-1", 0),
        window("win-old-2", "out-1", 600),
    ]);
    assert!(matches!(engine.handle(&event), CoreReply::Projection(_)));
    let after = engine.session(&key).unwrap().snapshot();
    assert_eq!(
        after
            .windows
            .iter()
            .find(|link| link.window.0 == "win-old-2")
            .unwrap()
            .leaf,
        survivor_leaf
    );
    assert!(
        !after
            .windows
            .iter()
            .any(|link| link.window.0 == "win-old-1")
    );
    assert!(after.windows.iter().any(|link| link.window.0 == "win-new"));
}
