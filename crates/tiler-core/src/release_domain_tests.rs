#![forbid(unsafe_code)]
//! Explicit domain release: drop one domain, then fresh-adopt its current
//! geometry while an independent domain is unaffected.

use std::collections::BTreeMap;

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::OutputDomain;
use crate::world_layout::{RetainedProjectionOutcome, WorldLayout};

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
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}

enum ReconcileKind {
    Fresh(RetainedProjectionOutcome),
    Retained(RetainedProjectionOutcome),
}

fn reconcile(
    layout: &mut WorldLayout,
    domain: &OutputDomain,
    windows: &[EngineWindow],
    focused: &str,
) -> ReconcileKind {
    let key = domain.key();
    let focus = WindowId(focused.into());
    match layout
        .reconcile_retained(&key, domain.bounds, domain.gap, 0, &focus, windows, &[])
        .expect("retained reconcile succeeds")
    {
        Some(proj) => ReconcileKind::Retained(proj),
        None => ReconcileKind::Fresh(
            layout
                .reconcile_fresh(domain, 0, &focus, windows)
                .expect("fresh reconcile succeeds")
                .projection,
        ),
    }
}

fn tiled_geometry(proj: RetainedProjectionOutcome) -> Vec<(String, Rect)> {
    proj.geometry
        .into_iter()
        .map(|g| (g.window.0, g.rect))
        .collect()
}

#[test]
fn release_then_fresh_reconcile_readopts_current_geometry() {
    let mut layout = WorldLayout::new();
    let home = domain("out-1", "ws-1");
    let current = vec![
        window("win-a1", "out-1", "ws-1", 0, 600),
        window("win-a2", "out-1", "ws-1", 600, 600),
    ];
    let planned = match reconcile(&mut layout, &home, &current, "win-a2") {
        ReconcileKind::Fresh(proj) => tiled_geometry(proj),
        other => panic!(
            "expected fresh adoption, got retained {:?}",
            match other {
                ReconcileKind::Retained(p) => p,
                ReconcileKind::Fresh(_) => unreachable!(),
            }
        ),
    };
    assert_eq!(planned.len(), 2);
    assert!(layout.contains_domain(&home.key()));

    layout.drop_domain(&home.key());
    assert!(!layout.contains_domain(&home.key()));

    // Repeat release is idempotent: still released, still absent.
    layout.drop_domain(&home.key());
    assert!(!layout.contains_domain(&home.key()));

    // Ordinary fresh reconcile re-adopts the carried current geometry
    // through the existing fit instead of the seed spiral.
    let mut readapted = match reconcile(&mut layout, &home, &current, "win-a2") {
        ReconcileKind::Fresh(proj) => tiled_geometry(proj),
        ReconcileKind::Retained(p) => panic!("expected fresh adoption, got retained {p:?}"),
    };
    readapted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut expected = planned;
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(readapted, expected);
    assert!(layout.contains_domain(&home.key()));
}

#[test]
fn release_leaves_independent_domain_untouched() {
    let mut layout = WorldLayout::new();
    let home = domain("out-1", "ws-1");
    let away = domain("out-2", "ws-1");
    let home_windows = vec![
        window("win-a1", "out-1", "ws-1", 0, 600),
        window("win-a2", "out-1", "ws-1", 600, 600),
    ];
    let away_windows = vec![window("win-b1", "out-2", "ws-1", 0, 1200)];
    reconcile(&mut layout, &home, &home_windows, "win-a2");
    reconcile(&mut layout, &away, &away_windows, "win-b1");

    let before = match reconcile(&mut layout, &away, &away_windows, "win-b1") {
        ReconcileKind::Retained(plan) => plan.geometry,
        ReconcileKind::Fresh(p) => {
            panic!("expected retained projection for untouched domain, got fresh {p:?}")
        }
    };

    layout.drop_domain(&home.key());
    assert!(!layout.contains_domain(&home.key()));
    assert!(layout.contains_domain(&away.key()));

    match reconcile(&mut layout, &away, &away_windows, "win-b1") {
        ReconcileKind::Retained(plan) => assert_eq!(plan.geometry, before),
        ReconcileKind::Fresh(p) => {
            panic!("independent domain must keep projecting, got fresh {p:?}")
        }
    }

    // The released domain fresh-adopts on its next ordinary reconcile.
    let readapted = match reconcile(&mut layout, &home, &home_windows, "win-a2") {
        ReconcileKind::Fresh(proj) => tiled_geometry(proj),
        ReconcileKind::Retained(p) => panic!("expected fresh adoption, got retained {p:?}"),
    };
    assert_eq!(readapted.len(), 2);
    assert!(layout.contains_domain(&home.key()));
    assert!(layout.contains_domain(&away.key()));
}
