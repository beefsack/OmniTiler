#![forbid(unsafe_code)]
use std::collections::BTreeMap;

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::OutputDomain;
use crate::world_layout::{FreshDomainOutcome, RetainedProjectionOutcome, WorldLayout};

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

fn seeded_layout() -> WorldLayout {
    let mut layout = WorldLayout::new();
    let retained = domain("out-1");
    let old = [
        window("win-old-1", "out-1", 0),
        window("win-old-2", "out-1", 600),
    ];
    layout
        .reconcile_fresh(&retained, 0, &WindowId("win-old-2".into()), &old)
        .expect("seed layout");
    layout
}

enum ReconcileOutcome {
    Fresh(FreshDomainOutcome),
    Retained(RetainedProjectionOutcome),
}

fn reconcile(layout: &mut WorldLayout, windows: &[EngineWindow]) -> ReconcileOutcome {
    let d = domain("out-1");
    let key = d.key();
    let disjoint = !windows.is_empty()
        && layout.contains_domain(&key)
        && !windows.iter().any(|w| {
            layout.is_tiled_window(&key, &w.window)
                || layout.is_exception(&key, &w.window)
                || layout.has_minimized_slot(&key, &w.window)
        });
    if disjoint {
        layout.drop_domain(&key);
    }
    let focus = WindowId("win-z-left".into());
    layout
        .converge_retained_observation(&key, windows, &focus, &[])
        .expect("observation convergence");
    match layout
        .reconcile_retained(&key, d.bounds, d.gap, 0, &focus, windows, &[])
        .expect("retained reconcile")
    {
        Some(proj) => ReconcileOutcome::Retained(proj),
        None => ReconcileOutcome::Fresh(
            layout
                .reconcile_fresh(&d, 0, &focus, windows)
                .expect("fresh reconcile"),
        ),
    }
}

#[test]
fn disjoint_reconcile_adopts_like_fresh_startup() {
    let mut layout = seeded_layout();
    // UUID order is opposite observed spatial order.
    let windows = vec![
        window("win-z-left", "out-1", 0),
        window("win-a-right", "out-1", 600),
    ];
    match reconcile(&mut layout, &windows) {
        ReconcileOutcome::Fresh(outcome) => {
            let geom = outcome.projection.geometry;
            assert_eq!(geom.len(), 2);
            assert_eq!(geom[0].window.0, "win-z-left");
            assert_eq!(geom[0].rect.x, 0);
            assert_eq!(geom[1].window.0, "win-a-right");
            assert_eq!(geom[1].rect.x, 600);
        }
        ReconcileOutcome::Retained(other) => {
            panic!("expected fresh strip adoption, got retained {other:?}")
        }
    }
}

#[test]
fn any_shared_id_preserves_retained_topology() {
    let mut layout = seeded_layout();
    let key = domain("out-1").key();
    let survivor_leaf = layout
        .bindings(&key)
        .find(|(w, _)| w.0 == "win-old-2")
        .unwrap()
        .1
        .clone();
    let windows = vec![
        window("win-new", "out-1", 0),
        window("win-old-2", "out-1", 600),
    ];
    assert!(matches!(
        reconcile(&mut layout, &windows),
        ReconcileOutcome::Retained(_)
    ));
    assert_eq!(
        layout
            .bindings(&key)
            .find(|(w, _)| w.0 == "win-old-2")
            .unwrap()
            .1,
        &survivor_leaf
    );
    assert!(!layout.bindings(&key).any(|(w, _)| w.0 == "win-old-1"));
    assert!(layout.bindings(&key).any(|(w, _)| w.0 == "win-new"));
}
