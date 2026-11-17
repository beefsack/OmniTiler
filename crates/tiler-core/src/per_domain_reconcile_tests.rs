//! Relocated per-domain layout regression tests (reconcile/admit/snap-back,
//! margins, pointer-resize, directional focus/move, update-gaps topology).

use crate::directional::{Direction, OutputId, SameAxisMove, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, OutputDomain};
use crate::size_hints::WindowSizeHints;
use crate::world_layout::{
    LocalOperationContext, RetainedProjectionOutcome, RetainedReconcileFailure, WorldLayout,
};

/// Retired adapter parity values (inner gap 8, outer gap 8) for the
/// relocated observation fixtures below.
const INNER_GAP: i32 = 8;
const OUTER_GAP: i32 = 8;

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn test_domain(bounds: Rect, gap: i32) -> OutputDomain {
    OutputDomain {
        id: OutputId("display-0".to_owned()),
        workspace: WorkspaceId("workspace-0".to_owned()),
        bounds,
        gap,
        adjacent: std::collections::BTreeMap::new(),
    }
}

fn engine_windows(domain: &OutputDomain, windows: &[(WindowId, Rect)]) -> Vec<EngineWindow> {
    windows
        .iter()
        .map(|(w, r)| EngineWindow {
            window: w.clone(),
            output: domain.id.clone(),
            workspace: domain.workspace.clone(),
            rect: *r,
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        })
        .collect()
}

enum ReconcileReply {
    Fresh(RetainedProjectionOutcome),
    Retained(RetainedProjectionOutcome),
}

impl ReconcileReply {
    fn projection(self) -> RetainedProjectionOutcome {
        match self {
            Self::Fresh(p) | Self::Retained(p) => p,
        }
    }
}

fn reconcile(
    layout: &mut WorldLayout,
    domain: &OutputDomain,
    outer_gap: i32,
    windows: &[(WindowId, Rect)],
    focused: Option<&WindowId>,
) -> ReconcileReply {
    let key = domain.key();
    let observed = engine_windows(domain, windows);
    let focus = focused.cloned().unwrap_or_else(|| WindowId(String::new()));
    layout
        .converge_retained_observation(&key, &observed, &focus, &[])
        .expect("observation convergence succeeds");
    match layout
        .reconcile_retained(
            &key,
            domain.bounds,
            domain.gap,
            outer_gap,
            &focus,
            &observed,
            &[],
        )
        .expect("retained reconcile succeeds")
    {
        Some(proj) => ReconcileReply::Retained(proj),
        None => ReconcileReply::Fresh(
            layout
                .reconcile_fresh(domain, outer_gap, &focus, &observed)
                .expect("fresh reconcile succeeds")
                .projection,
        ),
    }
}

#[test]
fn engine_admits_then_removes_windows() {
    let mut layout = WorldLayout::new();
    let domain = test_domain(rect(0, 0, 1600, 900), INNER_GAP);

    // Two windows admitted through fresh reconcile.
    let first = reconcile(
        &mut layout,
        &domain,
        OUTER_GAP,
        &[
            (WindowId("w1".to_owned()), rect(0, 0, 800, 884)),
            (WindowId("w2".to_owned()), rect(800, 0, 800, 884)),
        ],
        None,
    )
    .projection();
    assert!(!first.geometry.is_empty());

    // Retained reconcile projects geometry.
    match reconcile(
        &mut layout,
        &domain,
        OUTER_GAP,
        &[
            (WindowId("w1".to_owned()), rect(0, 0, 800, 884)),
            (WindowId("w2".to_owned()), rect(800, 0, 800, 884)),
        ],
        None,
    ) {
        ReconcileReply::Retained(plan) => assert_eq!(plan.geometry.len(), 2),
        ReconcileReply::Fresh(other) => panic!("expected retained projection, got fresh {other:?}"),
    }

    // Close one window: convergence removes it, projection covers the survivor.
    match reconcile(
        &mut layout,
        &domain,
        OUTER_GAP,
        &[(WindowId("w1".to_owned()), rect(0, 0, 800, 884))],
        None,
    ) {
        ReconcileReply::Retained(plan) => {
            assert_eq!(plan.geometry.len(), 1);
            assert!(plan.geometry.iter().all(|g| g.window.0 == "w1"));
        }
        ReconcileReply::Fresh(other) => panic!("expected retained projection, got fresh {other:?}"),
    }
}

#[test]
fn engine_manual_geometry_never_adopted() {
    let mut layout = WorldLayout::new();
    let domain = test_domain(rect(0, 0, 1600, 900), INNER_GAP);
    reconcile(
        &mut layout,
        &domain,
        OUTER_GAP,
        &[
            (WindowId("w1".to_owned()), rect(0, 0, 800, 884)),
            (WindowId("w2".to_owned()), rect(800, 0, 800, 884)),
        ],
        None,
    );
    // User drags w1 elsewhere without a drag-drop op: retained reconcile
    // reprojects from topology (snap-back), proving the gesture route is
    // required to honor manual moves.
    match reconcile(
        &mut layout,
        &domain,
        OUTER_GAP,
        &[
            (WindowId("w1".to_owned()), rect(400, 200, 800, 600)),
            (WindowId("w2".to_owned()), rect(800, 0, 800, 884)),
        ],
        None,
    ) {
        ReconcileReply::Retained(plan) => {
            let w1 = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "w1")
                .expect("w1 projected");
            assert_ne!(w1.rect, rect(400, 200, 800, 600));
        }
        ReconcileReply::Fresh(other) => panic!("expected retained projection, got fresh {other:?}"),
    }
}

#[test]
fn engine_projects_visible_margins_and_inner_gap() {
    // Full rcWork in, inset domain out: 8px visible margin on every side plus
    // the 8px inner gap between tiles. Desired rects are visible-frame rects
    // the adapter converts to outer rects per app; the invisible border may
    // land outside rcWork and that is correct.
    let work = rect(0, 0, 1600, 900);
    let bounds = crate::geometry::inset_bounds(work, OUTER_GAP).expect("inset");
    let domain = test_domain(bounds, INNER_GAP);
    let mut layout = WorldLayout::new();
    let geometry = reconcile(
        &mut layout,
        &domain,
        OUTER_GAP,
        &[
            (WindowId("w1".to_owned()), rect(0, 0, 800, 884)),
            (WindowId("w2".to_owned()), rect(800, 0, 800, 884)),
        ],
        None,
    )
    .projection()
    .geometry;
    assert_eq!(geometry.len(), 2);
    let mut by_x = geometry;
    by_x.sort_by_key(|g| g.rect.x);
    let (left, right) = (&by_x[0], &by_x[1]);
    assert_eq!((left.rect.x, left.rect.y), (8, 8));
    assert_eq!((left.rect.w, left.rect.h), (788, 884));
    assert_eq!((right.rect.x, right.rect.w), (8 + 788 + INNER_GAP, 788));
    assert!(!left.overconstrained && !left.client_clamped);
}

#[test]
fn engine_pointer_resize_returns_resize_plan() {
    let mut layout = WorldLayout::new();
    let bounds = crate::geometry::inset_bounds(rect(0, 0, 1600, 900), OUTER_GAP).expect("inset");
    let domain = test_domain(bounds, INNER_GAP);
    let windows = [
        (WindowId("w1".to_owned()), rect(8, 8, 788, 884)),
        (WindowId("w2".to_owned()), rect(804, 8, 788, 884)),
    ];
    reconcile(&mut layout, &domain, OUTER_GAP, &windows, None);
    let w1 = WindowId("w1".to_owned());
    let observed = engine_windows(&domain, &windows);
    let ctx = LocalOperationContext {
        domain: &domain,
        outer_gap: OUTER_GAP,
        observed_focus: &w1,
        observed: &observed,
        preconverged: false,
    };
    // w1 holds the right leaf (see margin test): its left edge borders
    // w2, so "left" moves the shared boundary. A "right" resize on the
    // rightmost leaf has no neighbor and correctly refuses `Unchanged`.
    let plan = layout
        .run_pointer_resize(&ctx, &w1, Direction::Left, 700)
        .expect("pointer resize succeeds");
    assert!(!plan.geometry.is_empty());
}

fn seeded_layout(
    windows: &[(WindowId, Rect)],
    focused: &WindowId,
) -> (WorldLayout, OutputDomain, Vec<EngineWindow>) {
    let mut layout = WorldLayout::new();
    let domain = test_domain(rect(0, 0, 1600, 900), INNER_GAP);
    reconcile(&mut layout, &domain, OUTER_GAP, windows, Some(focused));
    let observed = engine_windows(&domain, windows);
    (layout, domain, observed)
}

#[test]
fn engine_focus_moves_through_nested_topology() {
    let w1 = WindowId("w1".to_owned());
    let w2 = WindowId("w2".to_owned());
    let w3 = WindowId("w3".to_owned());
    let observed_rects = vec![
        (w1.clone(), rect(8, 8, 500, 884)),
        (w2.clone(), rect(516, 8, 500, 884)),
        (w3.clone(), rect(1024, 8, 568, 884)),
    ];
    let (mut layout, domain, observed) = seeded_layout(&observed_rects, &w1);
    // Focus right from w1 lands on its neighbor with full geometry.
    let ctx = LocalOperationContext {
        domain: &domain,
        outer_gap: OUTER_GAP,
        observed_focus: &w1,
        observed: &observed,
        preconverged: false,
    };
    let plan = layout
        .run_local_focus(&ctx, &w1, Direction::Right)
        .expect("focus must plan directionally");
    assert_eq!(plan.to_window, w2);
    assert_eq!(plan.geometry.len(), 3);

    // Move uses the move route with full desired geometry.
    let mv_ctx = LocalOperationContext {
        domain: &domain,
        outer_gap: OUTER_GAP,
        observed_focus: &w2,
        observed: &observed,
        preconverged: false,
    };
    let plan = layout
        .run_local_move(
            &mv_ctx,
            &w2,
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
        )
        .expect("move must plan directionally");
    assert_eq!(plan.geometry.len(), 3);
}

#[test]
fn retained_update_gaps_preserves_topology() {
    fn bounds_rect() -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        }
    }
    let mut layout = WorldLayout::new();
    let key = DomainKey {
        output: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    let domain = OutputDomain {
        id: key.output.clone(),
        workspace: key.workspace.clone(),
        bounds: bounds_rect(),
        gap: 8,
        adjacent: std::collections::BTreeMap::new(),
    };
    let windows = vec![
        (WindowId("w1".to_owned()), bounds_rect()),
        (WindowId("w2".to_owned()), bounds_rect()),
    ];
    let w1 = WindowId("w1".to_owned());
    reconcile(&mut layout, &domain, 8, &windows, Some(&w1));
    let tree_before = layout.tree(&key).cloned().expect("tree");
    let bindings_before: Vec<_> = layout
        .bindings(&key)
        .map(|(w, l)| (w.clone(), l.clone()))
        .collect();

    // Plain reconcile with drifted gaps refuses instead of adopting.
    let observed = engine_windows(&domain, &windows);
    let err = layout
        .reconcile_retained(&key, bounds_rect(), 4, 12, &w1, &observed, &[])
        .expect_err("gap drift must refuse");
    assert_eq!(err, RetainedReconcileFailure::InnerGapMismatch);

    // The retained update-gaps path adopts without destroying topology.
    assert_eq!(layout.domain_geometry(&key), Some((bounds_rect(), 8)));
    assert_eq!(layout.domain_outer_gap(&key), Some(8));
    layout
        .update_domain_gaps(&key, bounds_rect(), 4, 12, &w1, &observed)
        .expect("update-gaps adopts");
    assert_eq!(
        layout.tree(&key).cloned().expect("tree"),
        tree_before,
        "topology survives the gap update"
    );
    let bindings_after: Vec<_> = layout
        .bindings(&key)
        .map(|(w, l)| (w.clone(), l.clone()))
        .collect();
    assert_eq!(bindings_before, bindings_after);
    assert_eq!(layout.domain_geometry(&key), Some((bounds_rect(), 4)));
    assert_eq!(layout.domain_outer_gap(&key), Some(12));
}

#[test]
fn engine_edge_focus_is_a_no_op_rejection() {
    let w1 = WindowId("w1".to_owned());
    let w2 = WindowId("w2".to_owned());
    let observed_rects = vec![
        (w1.clone(), rect(8, 8, 788, 884)),
        (w2.clone(), rect(804, 8, 788, 884)),
    ];
    let (mut layout, domain, observed) = seeded_layout(&observed_rects, &w1);
    // Focus left off the left edge: no neighbor, no actuation.
    let ctx = LocalOperationContext {
        domain: &domain,
        outer_gap: OUTER_GAP,
        observed_focus: &w1,
        observed: &observed,
        preconverged: false,
    };
    let err = layout
        .run_local_focus(&ctx, &w1, Direction::Left)
        .expect_err("edge focus must refuse without geometry");
    assert!(matches!(
        err,
        crate::world_layout::LocalOperationFailure::Layout(_)
    ));
}
