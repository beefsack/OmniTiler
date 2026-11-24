//! Per-output `ScaledResize` behavior through the typed resize and drag
//! entry points: keyboard schedules, pointer minima, group-edge zones, and
//! the source-vs-target metric selection rule.
//!
//! The metrics themselves (`from_scale` rounding, overflow refusal) are
//! unit-tested in [`crate::policy`]; these tests prove the layout paths
//! consume them instead of the logical constants.

use std::collections::BTreeMap;

use crate::contract::{DragSide, ResizeMode};
use crate::directional::{Axis, Direction, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::policy::{PriorGroupEdge, ScaledResize};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, OutputDomain, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::local_operation::LocalOperationContext;
use super::outcome::{LayoutFailure, LocalOperationFailure};
use super::pair_operation::PairOperationContext;
use super::state::{DomainLayout, WorldLayout};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn win(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn nid(id: &str) -> NodeId {
    NodeId(id.to_owned())
}

fn ws_key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn domain_desc(key: &DomainKey, bounds: Rect, gap: i32) -> OutputDomain {
    OutputDomain {
        id: key.output.clone(),
        workspace: key.workspace.clone(),
        bounds,
        gap,
        adjacent: BTreeMap::new(),
    }
}

fn eng(id: &str, key: &DomainKey, tile: Rect) -> EngineWindow {
    EngineWindow {
        window: win(id),
        output: key.output.clone(),
        workspace: key.workspace.clone(),
        rect: tile,
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

/// Two-tile horizontal domain shared [500, 500] on `bounds`.
fn two_tile(bounds: Rect) -> DomainLayout {
    let mut d = DomainLayout::new(bounds, 0, 0);
    d.tree = Some(Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![Node::Leaf { id: nid("a") }, Node::Leaf { id: nid("b") }],
        shares: vec![500, 500],
    });
    d.leaves.insert(win("w1"), nid("a"));
    d.leaves.insert(win("w2"), nid("b"));
    d.focused_leaf = Some(nid("a"));
    d.focus_stack = vec![nid("b"), nid("a")];
    d.focus_history = vec![nid("b"), nid("a")];
    d.last_active = Some(nid("a"));
    d.admission_order.insert(win("w1"), 1);
    d.admission_order.insert(win("w2"), 2);
    d.accepted_revision = 5;
    d
}

/// Three-tile vertical domain with the domain gap used by the pointer
/// projection (`gap` 8 leaves a horizontal strip between stacked tiles).
/// Distinct node/window ids from the source helper keep pair assembly free
/// of duplicate-state collisions.
fn three_tile_vertical(bounds: Rect, gap: i32, group: &str) -> DomainLayout {
    let mut d = DomainLayout::new(bounds, gap, 0);
    d.tree = Some(Node::Group {
        id: nid(group),
        axis: Axis::Vertical,
        children: vec![
            Node::Leaf { id: nid("t-a") },
            Node::Leaf { id: nid("t-b") },
            Node::Leaf { id: nid("t-c") },
        ],
        shares: vec![1, 1, 1],
    });
    d.leaves.insert(win("w3"), nid("t-a"));
    d.leaves.insert(win("w4"), nid("t-b"));
    d.leaves.insert(win("w5"), nid("t-c"));
    d.focused_leaf = Some(nid("t-a"));
    d.focus_stack = vec![nid("t-c"), nid("t-b"), nid("t-a")];
    d.focus_history = vec![nid("t-c"), nid("t-b"), nid("t-a")];
    d.last_active = Some(nid("t-a"));
    d.admission_order.insert(win("w3"), 1);
    d.admission_order.insert(win("w4"), 2);
    d.admission_order.insert(win("w5"), 3);
    d.accepted_revision = 9;
    d
}

fn two_tile_observed(key: &DomainKey, half_width: i32) -> Vec<EngineWindow> {
    vec![
        eng("w1", key, rect(0, 0, half_width, 600)),
        eng("w2", key, rect(half_width, 0, half_width, 600)),
    ]
}

fn three_tile_observed(key: &DomainKey, x_off: i32) -> Vec<EngineWindow> {
    // Heights match the projection exactly: 194 + gap 8 + 194 + gap 8 + 196.
    vec![
        eng("w3", key, rect(x_off, 0, 800, 194)),
        eng("w4", key, rect(x_off, 202, 800, 194)),
        eng("w5", key, rect(x_off, 404, 800, 196)),
    ]
}

fn resize_geometry(outcome: &super::outcome::ResizeOutcome, window: &WindowId) -> Rect {
    outcome
        .geometry
        .iter()
        .find(|g| &g.window == window)
        .unwrap_or_else(|| panic!("geometry for {window:?}"))
        .rect
}

/// Keyboard resize through the production `LocalOperationContext` path:
/// the step is the scaled schedule entry, not the logical 12px.
#[test]
fn keyboard_resize_uses_scaled_schedule() {
    for (scale, expected_left, expected_right) in [
        (1.0, 988, 1012),
        // press 0 logical step 12 scaled individually: 15 at 1.25.
        (1.25, 985, 1015),
        // 24 at 2.0.
        (2.0, 976, 1024),
    ] {
        let key = ws_key("out-1", "ws-1");
        let bounds = rect(0, 0, 2000, 600);
        let desc = domain_desc(&key, bounds, 0);
        let resize = ScaledResize::from_scale(scale).expect("scale");
        let observed = two_tile_observed(&key, 1000);
        let mut layout = WorldLayout::new();
        layout.domains.insert(key.clone(), two_tile(bounds));

        let ctx = LocalOperationContext {
            domain: &desc,
            outer_gap: 0,
            resize,
            observed_focus: &win("w2"),
            observed: &observed,
            preconverged: true,
        };
        let outcome = layout
            .run_keyboard_resize(&ctx, &win("w2"), Direction::Left, ResizeMode::Outwards, 0)
            .unwrap_or_else(|e| panic!("scale {scale} resize: {e:?}"));

        assert_eq!(
            resize_geometry(&outcome, &win("w1")),
            rect(0, 0, expected_left, 600),
            "scale {scale} shrink side"
        );
        assert_eq!(
            resize_geometry(&outcome, &win("w2")),
            rect(expected_left, 0, expected_right, 600),
            "scale {scale} grow side"
        );
    }
}

/// Pointer resize through the production path: an 800px pair admits the
/// 360px logical child minimum but not the 720px minimum at 2.0, so the
/// same boundary is a plan at 1.0 and `Unchanged` at 1.25/2.0.
#[test]
fn pointer_resize_uses_scaled_child_minima() {
    let key = ws_key("out-1", "ws-1");
    let bounds = rect(0, 0, 800, 600);
    let desc = domain_desc(&key, bounds, 0);
    let observed = two_tile_observed(&key, 400);

    let mut layout = WorldLayout::new();
    layout.domains.insert(key.clone(), two_tile(bounds));
    let unscaled = ScaledResize::unscaled();
    let ctx = LocalOperationContext {
        domain: &desc,
        outer_gap: 0,
        resize: unscaled,
        observed_focus: &win("w2"),
        observed: &observed,
        preconverged: true,
    };
    let outcome = layout
        .run_pointer_resize(&ctx, &win("w2"), Direction::Left, 300)
        .expect("logical 360 minimum admits the clamp");
    assert_eq!(resize_geometry(&outcome, &win("w1")), rect(0, 0, 360, 600));
    assert_eq!(
        resize_geometry(&outcome, &win("w2")),
        rect(360, 0, 440, 600)
    );

    for scale in [1.25, 2.0] {
        let key = ws_key("out-1", "ws-1");
        let desc = domain_desc(&key, bounds, 0);
        let observed = two_tile_observed(&key, 400);
        let mut layout = WorldLayout::new();
        layout.domains.insert(key.clone(), two_tile(bounds));
        let ctx = LocalOperationContext {
            domain: &desc,
            outer_gap: 0,
            resize: ScaledResize::from_scale(scale).expect("scale"),
            observed_focus: &win("w2"),
            observed: &observed,
            preconverged: true,
        };
        let err = layout
            .run_pointer_resize(&ctx, &win("w2"), Direction::Left, 300)
            .expect_err("pair below twice the scaled minimum refuses");
        assert_eq!(
            err,
            LocalOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::Unchanged)),
            "scale {scale}"
        );
    }
}

/// Local drag preview classifies the root group's edge zone with the
/// domain's scaled depths. The pointer sits in the horizontal gap between
/// stacked tiles, 40px from the group's left edge: outside the logical 32px
/// zone, inside the 64px zone at 2.0.
#[test]
fn local_drag_group_edge_zone_uses_scaled_depth() {
    for (resize, expect_edge) in [
        (ScaledResize::unscaled(), false),
        (ScaledResize::from_scale(2.0).expect("scale"), true),
    ] {
        let key = ws_key("out-1", "ws-1");
        let bounds = rect(0, 0, 800, 600);
        let desc = domain_desc(&key, bounds, 8);
        let observed = three_tile_observed(&key, 0);
        let mut layout = WorldLayout::new();
        layout
            .domains
            .insert(key.clone(), three_tile_vertical(bounds, 8, "g-root"));

        let preview = layout
            .run_local_drag_preview(
                &desc,
                0,
                resize,
                &win("w5"),
                &win("w5"),
                &observed,
                40,
                198,
                None,
            )
            .unwrap_or_else(|e| panic!("preview {resize:?}: {e:?}"));

        if expect_edge {
            assert_eq!(
                preview.hover.prior,
                Some(PriorGroupEdge {
                    group: nid("g-root"),
                    edge: DragSide::Left,
                })
            );
        } else {
            assert_eq!(preview.hover.prior, None);
        }
    }
}

/// Cross-output pair drag classifies the target domain's group edge with
/// the target output's metrics; no source metrics enter the resolver, so a
/// 40px target-edge hit only happens when `target_resize` carries 2.0.
#[test]
fn pair_drag_group_edge_zone_uses_target_metrics() {
    let source_key = ws_key("out-a", "ws-1");
    let target_key = ws_key("out-b", "ws-1");
    let source_bounds = rect(0, 0, 800, 600);
    let target_bounds = rect(800, 0, 800, 600);
    let source_desc = domain_desc(&source_key, source_bounds, 0);
    let target_desc = domain_desc(&target_key, target_bounds, 8);
    let source_observed = two_tile_observed(&source_key, 400);
    let target_observed = three_tile_observed(&target_key, 800);
    let mut observed = source_observed.clone();
    observed.extend(target_observed);

    for (target_resize, expect_edge) in [
        (ScaledResize::unscaled(), false),
        (ScaledResize::from_scale(2.0).expect("scale"), true),
    ] {
        let mut layout = WorldLayout::new();
        layout
            .domains
            .insert(source_key.clone(), two_tile(source_bounds));
        layout.domains.insert(
            target_key.clone(),
            three_tile_vertical(target_bounds, 8, "g-target"),
        );
        let ctx = PairOperationContext {
            source_domain: &source_desc,
            target_domain: &target_desc,
            source_outer_gap: 0,
            target_outer_gap: Some(0),
            observed_focus: &win("w1"),
            observed: &observed,
            closed: &[],
        };

        let preview = layout
            .run_pair_drag_preview(&ctx, target_resize, &win("w1"), 840, 198, None)
            .unwrap_or_else(|e| panic!("pair preview {target_resize:?}: {e:?}"));

        assert_eq!(preview.domain, target_key);
        assert_eq!(preview.target_window, win("w3"));
        if expect_edge {
            // Pair assembly recanonicalizes target group ids, so assert the
            // classified edge rather than the minted id.
            assert_eq!(
                preview.hover.prior.as_ref().map(|prior| prior.edge),
                Some(DragSide::Left)
            );
        } else {
            assert_eq!(preview.hover.prior, None);
        }
    }
}
