//! Direct tests for global-unique two-workspace swap over [`WorldLayout`].

use std::collections::BTreeSet;

use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::{DomainKey, ExceptionFlags};

use super::outcome::{WorkspaceSwapFailure, WorkspaceSwapOutcome};
use super::state::{DomainException, DomainLayout, DomainRestoreSlot, WorldLayout};
use super::workspace_swap::WorkspaceSwapSide;

fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn node(id: &str) -> NodeId {
    NodeId(id.to_owned())
}

fn window(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn leaf(id: &str) -> Node {
    Node::Leaf { id: node(id) }
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn side(
    output_src: &str,
    output_tgt: &str,
    ws: &str,
    bounds: Rect,
    gap: i32,
    outer: i32,
) -> WorkspaceSwapSide {
    WorkspaceSwapSide {
        source: key(output_src, ws),
        target: key(output_tgt, ws),
        bounds,
        gap,
        outer_gap: outer,
    }
}

fn make_tiled_domain(
    bounds: Rect,
    gap: i32,
    outer_gap: i32,
    prefix: &str,
    rev: u64,
) -> DomainLayout {
    let mut domain = DomainLayout::new(bounds, gap, outer_gap);
    let l1 = node(&format!("{prefix}-l1"));
    let l2 = node(&format!("{prefix}-l2"));
    let w1 = window(&format!("{prefix}-w1"));
    let w2 = window(&format!("{prefix}-w2"));
    domain.tree = Some(Node::Group {
        id: node(&format!("{prefix}-grp")),
        axis: Axis::Horizontal,
        children: vec![Node::Leaf { id: l1.clone() }, Node::Leaf { id: l2.clone() }],
        shares: vec![2, 1],
    });
    domain.leaves.insert(w1.clone(), l1.clone());
    domain.leaves.insert(w2.clone(), l2.clone());
    domain.focused_leaf = Some(l1.clone());
    domain.focus_stack = vec![l2.clone(), l1.clone()];
    domain.focus_history = vec![l2, l1.clone()];
    domain.last_active = Some(l1);
    domain.accepted_revision = rev;
    domain.admission_order.insert(w1, 1);
    domain.admission_order.insert(w2, 2);
    domain
}

#[test]
fn two_way_relocation_preserves_topology_focus_slots_and_revisions() {
    let mut layout = WorldLayout::new();
    let bounds_a = rect(0, 0, 800, 600);
    let bounds_b = rect(800, 0, 1200, 900);

    let mut dom_a = make_tiled_domain(bounds_a, 4, 8, "a", 5);
    dom_a.minimized_slots.insert(
        window("a-min"),
        DomainRestoreSlot {
            leaf: node("a-min-leaf"),
            tree: leaf("a-min-leaf"),
        },
    );
    let dom_b = make_tiled_domain(bounds_b, 6, 10, "b", 9);

    layout.domains.insert(key("out-a", "ws-1"), dom_a);
    layout.domains.insert(key("out-b", "ws-2"), dom_b);

    let leg_a = side("out-a", "out-b", "ws-1", bounds_b, 6, 10);
    let leg_b = side("out-b", "out-a", "ws-2", bounds_a, 4, 8);

    let outcome = layout
        .swap_workspaces(&leg_a, &leg_b, &BTreeSet::new())
        .expect("two-way swap should succeed");

    assert_eq!(
        outcome,
        WorkspaceSwapOutcome {
            moved_a: true,
            moved_b: true,
            stay_a: false,
            stay_b: false,
        }
    );
    assert!(!layout.contains_domain(&key("out-a", "ws-1")));
    assert!(!layout.contains_domain(&key("out-b", "ws-2")));

    let moved_a = layout.domain(&key("out-b", "ws-1")).expect("moved a");
    assert_eq!(moved_a.bounds, bounds_b);
    assert_eq!(moved_a.gap, 6);
    assert_eq!(moved_a.outer_gap, 10);
    assert_eq!(moved_a.accepted_revision, 5);
    assert_eq!(moved_a.focused_leaf, Some(node("a-l1")));
    assert!(moved_a.minimized_slots.contains_key(&window("a-min")));

    let moved_b = layout.domain(&key("out-a", "ws-2")).expect("moved b");
    assert_eq!(moved_b.bounds, bounds_a);
    assert_eq!(moved_b.gap, 4);
    assert_eq!(moved_b.outer_gap, 8);
    assert_eq!(moved_b.accepted_revision, 9);
    assert_eq!(moved_b.focused_leaf, Some(node("b-l1")));
    assert!(layout.validate_topology());
}

#[test]
fn sticky_exceptions_stay_on_physical_outputs_with_provenance_marks() {
    let mut layout = WorldLayout::new();
    let bounds_a = rect(0, 0, 800, 600);
    let bounds_b = rect(800, 0, 1000, 700);

    let mut dom_a = make_tiled_domain(bounds_a, 4, 8, "a", 3);
    let sticky_a = window("sticky-a");
    let float_rect_a = rect(40, 50, 300, 200);
    dom_a.exceptions.insert(
        sticky_a.clone(),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                ..ExceptionFlags::none()
            },
            floating_geometry: Some(float_rect_a),
        },
    );
    dom_a
        .retained_float_geometry
        .insert(sticky_a.clone(), float_rect_a);
    dom_a.automatic_fixed.insert(sticky_a.clone());
    dom_a.capacity_floats.insert(sticky_a.clone());
    dom_a.admission_order.insert(sticky_a.clone(), 11);

    let mut dom_b = make_tiled_domain(bounds_b, 8, 12, "b", 7);
    let sticky_b = window("sticky-b");
    let float_rect_b = rect(850, 60, 250, 180);
    dom_b.exceptions.insert(
        sticky_b.clone(),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                ..ExceptionFlags::none()
            },
            floating_geometry: Some(float_rect_b),
        },
    );
    dom_b
        .retained_float_geometry
        .insert(sticky_b.clone(), float_rect_b);
    dom_b.fixed_tile_override.insert(sticky_b.clone());
    dom_b.admission_order.insert(sticky_b.clone(), 22);

    layout.domains.insert(key("out-a", "ws-1"), dom_a);
    layout.domains.insert(key("out-b", "ws-2"), dom_b);

    let sticky = BTreeSet::from([sticky_a.clone(), sticky_b.clone()]);
    let leg_a = side("out-a", "out-b", "ws-1", bounds_b, 8, 12);
    let leg_b = side("out-b", "out-a", "ws-2", bounds_a, 4, 8);

    let outcome = layout
        .swap_workspaces(&leg_a, &leg_b, &sticky)
        .expect("sticky swap should succeed");

    assert_eq!(
        outcome,
        WorkspaceSwapOutcome {
            moved_a: true,
            moved_b: true,
            stay_a: false,
            stay_b: false,
        }
    );

    let moved_a = layout.domain(&key("out-b", "ws-1")).expect("ws-1 on out-b");
    assert!(!moved_a.exceptions.contains_key(&sticky_a));
    assert!(moved_a.exceptions.contains_key(&sticky_b));
    assert_eq!(
        moved_a.retained_float_geometry.get(&sticky_b),
        Some(&float_rect_b)
    );
    assert!(moved_a.fixed_tile_override.contains(&sticky_b));
    assert_eq!(moved_a.admission_order.get(&sticky_b), Some(&22));

    let moved_b = layout.domain(&key("out-a", "ws-2")).expect("ws-2 on out-a");
    assert!(!moved_b.exceptions.contains_key(&sticky_b));
    assert!(moved_b.exceptions.contains_key(&sticky_a));
    assert_eq!(
        moved_b.retained_float_geometry.get(&sticky_a),
        Some(&float_rect_a)
    );
    assert!(moved_b.automatic_fixed.contains(&sticky_a));
    assert!(moved_b.capacity_floats.contains(&sticky_a));
    assert_eq!(moved_b.admission_order.get(&sticky_a), Some(&11));
    assert!(layout.validate_topology());
}

#[test]
fn rootless_sticky_only_side_still_relocates_as_empty_host() {
    let mut layout = WorldLayout::new();
    let bounds_a = rect(0, 0, 800, 600);
    let bounds_b = rect(800, 0, 1000, 700);

    let mut dom_a = DomainLayout::new(bounds_a, 4, 8);
    let sticky_a = window("sticky-a");
    dom_a.exceptions.insert(
        sticky_a.clone(),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                ..ExceptionFlags::none()
            },
            floating_geometry: Some(rect(10, 10, 200, 150)),
        },
    );
    dom_a.accepted_revision = 2;

    let mut dom_b = make_tiled_domain(bounds_b, 8, 12, "b", 4);
    let sticky_b = window("sticky-b");
    dom_b.exceptions.insert(
        sticky_b.clone(),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                ..ExceptionFlags::none()
            },
            floating_geometry: Some(rect(820, 20, 200, 150)),
        },
    );

    layout.domains.insert(key("out-a", "ws-1"), dom_a);
    layout.domains.insert(key("out-b", "ws-2"), dom_b);

    let sticky = BTreeSet::from([sticky_a.clone(), sticky_b.clone()]);
    let leg_a = side("out-a", "out-b", "ws-1", bounds_b, 8, 12);
    let leg_b = side("out-b", "out-a", "ws-2", bounds_a, 4, 8);

    let outcome = layout
        .swap_workspaces(&leg_a, &leg_b, &sticky)
        .expect("rootless sticky-only side should relocate");

    assert_eq!(
        outcome,
        WorkspaceSwapOutcome {
            moved_a: true,
            moved_b: true,
            stay_a: false,
            stay_b: false,
        }
    );
    assert!(!layout.contains_domain(&key("out-a", "ws-1")));
    let moved_a = layout
        .domain(&key("out-b", "ws-1"))
        .expect("moved rootless a");
    assert!(moved_a.tree.is_none());
    assert!(moved_a.leaves.is_empty());
    assert!(moved_a.exceptions.contains_key(&sticky_b));
    assert!(!moved_a.exceptions.contains_key(&sticky_a));

    let moved_b = layout.domain(&key("out-a", "ws-2")).expect("moved b");
    assert!(moved_b.exceptions.contains_key(&sticky_a));
    assert!(layout.validate_topology());
}

#[test]
fn absent_opposite_side_derives_focus_cleared_rootless_sticky_stay() {
    let mut layout = WorldLayout::new();
    let bounds_a = rect(0, 0, 800, 600);
    let bounds_b = rect(800, 0, 1000, 700);

    let mut dom_a = make_tiled_domain(bounds_a, 4, 8, "a", 6);
    let sticky_a = window("sticky-a");
    let float_rect = rect(30, 30, 220, 160);
    dom_a.exceptions.insert(
        sticky_a.clone(),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                ..ExceptionFlags::none()
            },
            floating_geometry: Some(float_rect),
        },
    );
    dom_a
        .retained_float_geometry
        .insert(sticky_a.clone(), float_rect);
    dom_a.automatic_fixed.insert(sticky_a.clone());
    dom_a.minimized_slots.insert(
        window("a-min"),
        DomainRestoreSlot {
            leaf: node("a-min-leaf"),
            tree: leaf("a-min-leaf"),
        },
    );

    layout.domains.insert(key("out-a", "ws-1"), dom_a);

    let sticky = BTreeSet::from([sticky_a.clone()]);
    let leg_a = side("out-a", "out-b", "ws-1", bounds_b, 8, 12);
    let leg_b = side("out-b", "out-a", "ws-2", bounds_a, 4, 8);

    let outcome = layout
        .swap_workspaces(&leg_a, &leg_b, &sticky)
        .expect("stay should derive when opposite side is absent");

    assert_eq!(
        outcome,
        WorkspaceSwapOutcome {
            moved_a: true,
            moved_b: false,
            stay_a: false,
            stay_b: true,
        }
    );
    let moved_a = layout.domain(&key("out-b", "ws-1")).expect("moved a");
    assert!(!moved_a.exceptions.contains_key(&sticky_a));
    assert!(moved_a.minimized_slots.contains_key(&window("a-min")));

    let stay_b = layout.domain(&key("out-a", "ws-2")).expect("stay b");
    assert!(stay_b.tree.is_none());
    assert!(stay_b.leaves.is_empty());
    assert!(stay_b.minimized_slots.is_empty());
    assert_eq!(stay_b.focused_leaf, None);
    assert!(stay_b.focus_stack.is_empty());
    assert!(stay_b.focus_history.is_empty());
    assert_eq!(stay_b.last_active, None);
    assert_eq!(stay_b.accepted_revision, 6);
    assert!(stay_b.exceptions.contains_key(&sticky_a));
    assert_eq!(
        stay_b.retained_float_geometry.get(&sticky_a),
        Some(&float_rect)
    );
    assert!(stay_b.automatic_fixed.contains(&sticky_a));
    assert!(layout.validate_topology());
}

#[test]
fn stale_empty_source_is_left_untouched_when_not_moved() {
    let mut layout = WorldLayout::new();
    let bounds_a = rect(0, 0, 800, 600);
    let bounds_b = rect(800, 0, 1000, 700);

    let stale_a = DomainLayout::new(bounds_a, 4, 8);
    let stale_b = DomainLayout::new(bounds_b, 6, 10);
    layout.domains.insert(key("out-a", "ws-1"), stale_a);
    layout.domains.insert(key("out-b", "ws-2"), stale_b);

    let leg_a = side("out-a", "out-b", "ws-1", bounds_b, 6, 10);
    let leg_b = side("out-b", "out-a", "ws-2", bounds_a, 4, 8);

    let outcome = layout
        .swap_workspaces(&leg_a, &leg_b, &BTreeSet::new())
        .expect("stale empty sides succeed with no move");

    assert_eq!(
        outcome,
        WorkspaceSwapOutcome {
            moved_a: false,
            moved_b: false,
            stay_a: false,
            stay_b: false,
        }
    );
    assert!(layout.contains_domain(&key("out-a", "ws-1")));
    assert!(layout.contains_domain(&key("out-b", "ws-2")));
    assert!(!layout.contains_domain(&key("out-b", "ws-1")));
    assert!(!layout.contains_domain(&key("out-a", "ws-2")));
}

#[test]
fn sticky_tiled_or_minimized_refuses_atomically() {
    let mut layout = WorldLayout::new();
    let bounds_a = rect(0, 0, 800, 600);
    let bounds_b = rect(800, 0, 1000, 700);

    let dom_a = make_tiled_domain(bounds_a, 4, 8, "a", 3);
    let mut dom_b = make_tiled_domain(bounds_b, 6, 10, "b", 5);
    dom_b.minimized_slots.insert(
        window("b-min"),
        DomainRestoreSlot {
            leaf: node("b-min-leaf"),
            tree: leaf("b-min-leaf"),
        },
    );
    layout.domains.insert(key("out-a", "ws-1"), dom_a);
    layout.domains.insert(key("out-b", "ws-2"), dom_b);

    let leg_a = side("out-a", "out-b", "ws-1", bounds_b, 6, 10);
    let leg_b = side("out-b", "out-a", "ws-2", bounds_a, 4, 8);

    let sticky_tiled = BTreeSet::from([window("a-w1")]);
    assert_eq!(
        layout.swap_workspaces(&leg_a, &leg_b, &sticky_tiled),
        Err(WorkspaceSwapFailure::StickyTiledOrMinimized)
    );
    assert!(layout.contains_domain(&key("out-a", "ws-1")));
    assert!(layout.contains_domain(&key("out-b", "ws-2")));
    assert!(!layout.contains_domain(&key("out-b", "ws-1")));

    let sticky_min = BTreeSet::from([window("b-min")]);
    assert_eq!(
        layout.swap_workspaces(&leg_a, &leg_b, &sticky_min),
        Err(WorkspaceSwapFailure::StickyTiledOrMinimized)
    );
    assert!(layout.contains_domain(&key("out-a", "ws-1")));
    assert!(layout.contains_domain(&key("out-b", "ws-2")));
}

#[test]
fn target_collision_and_invalid_geometry_refuse_atomically() {
    let mut layout = WorldLayout::new();
    let bounds_a = rect(0, 0, 800, 600);
    let bounds_b = rect(800, 0, 1000, 700);

    layout.domains.insert(
        key("out-a", "ws-1"),
        make_tiled_domain(bounds_a, 4, 8, "a", 1),
    );
    layout.domains.insert(
        key("out-b", "ws-1"),
        make_tiled_domain(bounds_b, 6, 10, "c", 2),
    );

    let leg_a = side("out-a", "out-b", "ws-1", bounds_b, 6, 10);
    let leg_b = side("out-b", "out-a", "ws-2", bounds_a, 4, 8);
    assert_eq!(
        layout.swap_workspaces(&leg_a, &leg_b, &BTreeSet::new()),
        Err(WorkspaceSwapFailure::TargetOccupied)
    );
    assert!(layout.contains_domain(&key("out-a", "ws-1")));
    assert!(layout.contains_domain(&key("out-b", "ws-1")));
    assert!(!layout.contains_domain(&key("out-a", "ws-2")));

    layout.domains.remove(&key("out-b", "ws-1"));
    let bad_geo = side("out-a", "out-b", "ws-1", rect(800, 0, 0, 700), 6, 10);
    assert_eq!(
        layout.swap_workspaces(&bad_geo, &leg_b, &BTreeSet::new()),
        Err(WorkspaceSwapFailure::InvalidGeometry)
    );
    assert!(layout.contains_domain(&key("out-a", "ws-1")));
    assert!(!layout.contains_domain(&key("out-b", "ws-1")));
}

#[test]
fn malformed_source_with_sticky_refuses_atomically() {
    let mut layout = WorldLayout::new();
    let bounds_a = rect(0, 0, 800, 600);
    let bounds_b = rect(800, 0, 1000, 700);

    let mut dom_a = make_tiled_domain(bounds_a, 4, 8, "a", 6);
    let sticky_a = window("sticky-a");
    let float_rect = rect(30, 30, 220, 160);
    dom_a.exceptions.insert(
        sticky_a.clone(),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                ..ExceptionFlags::none()
            },
            floating_geometry: Some(float_rect),
        },
    );
    // Corrupt topology: leaf binding points to an absent node id so side-A
    // preparation fails atomically even with an empty opposite side.
    dom_a
        .leaves
        .insert(window("ghost-win"), node("missing-node"));
    layout.domains.insert(key("out-a", "ws-1"), dom_a);

    let sticky = BTreeSet::from([sticky_a]);
    let leg_a = side("out-a", "out-b", "ws-1", bounds_b, 8, 12);
    let leg_b = side("out-b", "out-a", "ws-2", bounds_a, 4, 8);

    assert_eq!(
        layout.swap_workspaces(&leg_a, &leg_b, &sticky),
        Err(WorkspaceSwapFailure::MalformedTopology)
    );
    assert!(layout.contains_domain(&key("out-a", "ws-1")));
    assert!(!layout.contains_domain(&key("out-b", "ws-1")));
    assert!(!layout.contains_domain(&key("out-a", "ws-2")));
    assert!(!layout.contains_domain(&key("out-b", "ws-2")));
    let kept = layout.domain(&key("out-a", "ws-1")).expect("source kept");
    assert_eq!(kept.accepted_revision, 6);
    assert!(kept.exceptions.contains_key(&window("sticky-a")));
}
