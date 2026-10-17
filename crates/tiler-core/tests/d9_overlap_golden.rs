#![forbid(unsafe_code)]
//! D9 overlap-mode golden geometry/navigation cases (selected 2026-10-17).
//!
//! Production acceptance against [`tiler_core::geometry::project_overlap`]
//! and [`tiler_core::size_hints::project_overlap_with_hints`]:
//! - effective extent `max(assigned, gap*(n-1)+n)` per group, existing
//!   reserve-one/share rounding from the assigned origin, cross extent
//!   unchanged, parent slot unchanged, recurse, then B6 minimum expansion.
//! - Strict projection remains strict (every exhausted golden asserts it).

use tiler_core::directional::{Axis, Direction, FocusPlan, Node, NodeId, plan_focus};
use tiler_core::geometry::{ProjectedLeaf, Rect, project, project_overlap};
use tiler_core::size_hints::{WindowSizeHints, project_overlap_with_hints, project_with_hints};

fn leaf(id: &str) -> Node {
    Node::Leaf {
        id: NodeId::from(id),
    }
}

fn hgroup(id: &str, children: Vec<Node>, shares: Vec<u64>) -> Node {
    Node::Group {
        id: NodeId::from(id),
        axis: Axis::Horizontal,
        children,
        shares,
    }
}

fn vgroup(id: &str, children: Vec<Node>, shares: Vec<u64>) -> Node {
    Node::Group {
        id: NodeId::from(id),
        axis: Axis::Vertical,
        children,
        shares,
    }
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn lookup(out: &[ProjectedLeaf], id: &str) -> Rect {
    out.iter()
        .find(|projected| projected.leaf == NodeId::from(id))
        .unwrap_or_else(|| panic!("missing golden leaf {id}"))
        .rect
}

fn intersects(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

// O1: flat exhausted group, tiny 1px tiles. Bounds w=4 gap=2 needs 7.
// Strict: avail 0 < 3 refuses. D9: effective 7, avail 3, all 1px.
#[test]
fn flat_exhausted_tiles_overflow_domain_as_single_pixels() {
    let tree = hgroup("root", vec![leaf("A"), leaf("B"), leaf("C")], vec![1, 1, 1]);
    let bounds = rect(0, 0, 4, 10);
    assert!(project(&tree, bounds, 2).is_err());

    let first = project_overlap(&tree, bounds, 2).expect("overlap projects");
    let second = project_overlap(&tree, bounds, 2).expect("overlap deterministic");
    assert_eq!(first, second);
    assert!(first.capacity_overflow);
    assert_eq!(lookup(&first.leaves, "A"), rect(0, 0, 1, 10));
    assert_eq!(lookup(&first.leaves, "B"), rect(3, 0, 1, 10));
    assert_eq!(lookup(&first.leaves, "C"), rect(6, 0, 1, 10));
    // Every golden tile stays positive; C leaves the domain.
    for projected in &first.leaves {
        assert!(projected.rect.w > 0 && projected.rect.h > 0);
    }
    let c = lookup(&first.leaves, "C");
    assert!(c.x + c.w > bounds.x + bounds.w);

    // Navigation stays tree-relative and deterministic over the overlap.
    assert_eq!(
        plan_focus(&tree, &NodeId::from("A"), Direction::Right),
        Some(FocusPlan::Focused {
            leaf: NodeId::from("B"),
            route: vec![NodeId::from("B")],
        })
    );
    assert_eq!(
        plan_focus(&tree, &NodeId::from("B"), Direction::Right),
        Some(FocusPlan::Focused {
            leaf: NodeId::from("C"),
            route: vec![NodeId::from("C")],
        })
    );
    assert_eq!(
        plan_focus(&tree, &NodeId::from("C"), Direction::Right),
        Some(FocusPlan::Edge)
    );
    assert_eq!(
        plan_focus(&tree, &NodeId::from("C"), Direction::Left),
        Some(FocusPlan::Focused {
            leaf: NodeId::from("B"),
            route: vec![NodeId::from("B")],
        })
    );
    assert_eq!(
        plan_focus(&tree, &NodeId::from("A"), Direction::Left),
        Some(FocusPlan::Edge)
    );
}

// O2: nested overflowing branch crosses the inter-group gap and sibling A.
// Root H[G,A] shares [1,2] on 20px gap 2 gives G 6px; G holds 4 children
// (floor 10). Strict refuses G. D9 lays G children 1px from x=0, so E at
// 9..10 overlaps sibling A (8..20) while D at 6..7 sits in the gap.
#[test]
fn nested_branch_crosses_gap_and_sibling_with_deterministic_escape() {
    let tree = hgroup(
        "root",
        vec![
            hgroup(
                "G",
                vec![leaf("B"), leaf("C"), leaf("D"), leaf("E")],
                vec![1, 1, 1, 1],
            ),
            leaf("A"),
        ],
        vec![1, 2],
    );
    let bounds = rect(0, 0, 20, 10);
    assert!(project(&tree, bounds, 2).is_err());

    let out = project_overlap(&tree, bounds, 2).expect("overlap projects");
    assert!(out.capacity_overflow);
    assert_eq!(lookup(&out.leaves, "B"), rect(0, 0, 1, 10));
    assert_eq!(lookup(&out.leaves, "C"), rect(3, 0, 1, 10));
    assert_eq!(lookup(&out.leaves, "D"), rect(6, 0, 1, 10));
    assert_eq!(lookup(&out.leaves, "E"), rect(9, 0, 1, 10));
    // Parent slot unchanged: A keeps its strict assigned slot.
    let g_slot = rect(0, 0, 6, 10);
    let a_rect = rect(8, 0, 12, 10);
    assert_eq!(lookup(&out.leaves, "A"), a_rect);
    assert!(intersects(&lookup(&out.leaves, "E"), &a_rect));
    assert!(!intersects(&lookup(&out.leaves, "D"), &a_rect));
    assert!(!intersects(&lookup(&out.leaves, "D"), &g_slot));

    // E moving right escapes to A (tree order), A moving left descends to E,
    // even though E geometrically overlaps A.
    assert_eq!(
        plan_focus(&tree, &NodeId::from("E"), Direction::Right),
        Some(FocusPlan::Focused {
            leaf: NodeId::from("A"),
            route: vec![NodeId::from("A")],
        })
    );
    assert_eq!(
        plan_focus(&tree, &NodeId::from("A"), Direction::Left),
        Some(FocusPlan::Focused {
            leaf: NodeId::from("E"),
            route: vec![NodeId::from("G"), NodeId::from("E")],
        })
    );
    assert_eq!(
        plan_focus(&tree, &NodeId::from("E"), Direction::Left),
        Some(FocusPlan::Focused {
            leaf: NodeId::from("D"),
            route: vec![NodeId::from("D")],
        })
    );
    assert_eq!(
        plan_focus(&tree, &NodeId::from("B"), Direction::Left),
        Some(FocusPlan::Edge)
    );
}

// O3: square bounds take the vertical root (square-tie rule); non-zero
// origin; equal-share remainder lands on the last child. Root V[A,G] on 6px
// gap 2 gives 2px each; G holds 3 children (floor 7). D overflows the group
// slot and the domain by exactly 1px.
#[test]
fn square_bounds_overflow_from_nonzero_origin_with_vertical_tie() {
    let tree = vgroup(
        "root",
        vec![
            leaf("A"),
            hgroup("G", vec![leaf("B"), leaf("C"), leaf("D")], vec![1, 1, 1]),
        ],
        vec![1, 1],
    );
    let bounds = rect(10, 20, 6, 6);
    assert!(project(&tree, bounds, 2).is_err());

    let out = project_overlap(&tree, bounds, 2).expect("overlap projects");
    assert!(out.capacity_overflow);
    assert_eq!(lookup(&out.leaves, "A"), rect(10, 20, 6, 2));
    assert_eq!(lookup(&out.leaves, "B"), rect(10, 24, 1, 2));
    assert_eq!(lookup(&out.leaves, "C"), rect(13, 24, 1, 2));
    assert_eq!(lookup(&out.leaves, "D"), rect(16, 24, 1, 2));
    let d = lookup(&out.leaves, "D");
    assert_eq!(d.x + d.w, bounds.x + bounds.w + 1);

    // Perpendicular descent reaches into the overflowing branch; D moving
    // right reports the edge (no geometric nearest-neighbor search).
    assert_eq!(
        plan_focus(&tree, &NodeId::from("A"), Direction::Down),
        Some(FocusPlan::Focused {
            leaf: NodeId::from("B"),
            route: vec![NodeId::from("G"), NodeId::from("B")],
        })
    );
    assert_eq!(
        plan_focus(&tree, &NodeId::from("C"), Direction::Right),
        Some(FocusPlan::Focused {
            leaf: NodeId::from("D"),
            route: vec![NodeId::from("D")],
        })
    );
    assert_eq!(
        plan_focus(&tree, &NodeId::from("D"), Direction::Right),
        Some(FocusPlan::Edge)
    );
    assert_eq!(
        plan_focus(&tree, &NodeId::from("A"), Direction::Left),
        Some(FocusPlan::Edge)
    );
}

// O4: checked failure. Bounds are valid, the strict projector refuses the
// exhausted group, and the overlap layout cannot represent the third offset
// (x would exceed i32::MAX), so it holds truthfully instead of wrapping.
#[test]
fn unrepresentable_overflow_holds_instead_of_wrapping() {
    let tree = hgroup("root", vec![leaf("A"), leaf("B"), leaf("C")], vec![1, 1, 1]);
    let bounds = rect(i32::MAX - 5, 0, 5, 10);
    assert!(project(&tree, bounds, 2).is_err());
    let err = project_overlap(&tree, bounds, 2).expect_err("must hold");
    assert_eq!(err.message(), "segment offset overflows");
    let hinted_err = project_overlap_with_hints(&tree, bounds, 2, &|_| WindowSizeHints::none())
        .expect_err("hinted overlap must hold");
    assert_eq!(hinted_err.message(), "segment offset overflows");
}

// O4b: endpoint failure. Every child origin fits in i32 but the last leaf
// extent endpoint (x + w) cannot be represented, so both overlap
// projectors hold truthfully instead of emitting a wrapping rectangle.
#[test]
fn unrepresentable_leaf_endpoint_holds_instead_of_wrapping() {
    let tree = hgroup("root", vec![leaf("A"), leaf("B"), leaf("C")], vec![1, 1, 1]);
    // Assigned w=2 needs 3 (gap 0, three 1px tiles): effective 3, sizes
    // [1,1,1], origins MAX-2/MAX-1/MAX all fit, but C ends at MAX+1.
    let bounds = rect(i32::MAX - 2, 0, 2, 10);
    assert!(project(&tree, bounds, 0).is_err());
    let err = project_overlap(&tree, bounds, 0).expect_err("must hold");
    assert_eq!(err.message(), "segment offset overflows");
    let hinted_err = project_overlap_with_hints(&tree, bounds, 0, &|_| WindowSizeHints::none())
        .expect_err("hinted overlap must hold");
    assert_eq!(hinted_err.message(), "segment offset overflows");
}

// O4c: vertical endpoint failure (y + h), plain and hinted projectors.
#[test]
fn unrepresentable_vertical_leaf_endpoint_holds_instead_of_wrapping() {
    let tree = vgroup("root", vec![leaf("A"), leaf("B"), leaf("C")], vec![1, 1, 1]);
    let bounds = rect(0, i32::MAX - 2, 10, 2);
    assert!(project(&tree, bounds, 0).is_err());
    let err = project_overlap(&tree, bounds, 0).expect_err("must hold");
    assert_eq!(err.message(), "segment offset overflows");
    let hinted_err = project_overlap_with_hints(&tree, bounds, 0, &|_| WindowSizeHints::none())
        .expect_err("hinted overlap must hold");
    assert_eq!(hinted_err.message(), "segment offset overflows");
}

// O5: B6 applies AFTER overlap and never rescues geometric exhaustion.
// O1 tree with C min_w=2: overlap bases are 1px (infeasible for C), so the
// sizes stay proportional and C is flagged; strict and hinted production
// projectors both still refuse (D1: B6 alone never solves exhaustion).
#[test]
fn minimums_apply_after_overlap_without_rescuing_exhaustion() {
    let tree = hgroup("root", vec![leaf("A"), leaf("B"), leaf("C")], vec![1, 1, 1]);
    let bounds = rect(0, 0, 4, 10);
    let resolve = |id: &NodeId| {
        if id.0 == "C" {
            WindowSizeHints {
                min_w: Some(2),
                ..WindowSizeHints::none()
            }
        } else {
            WindowSizeHints::none()
        }
    };
    let strict_err = project(&tree, bounds, 2).expect_err("strict refuses");
    let hinted_err =
        project_with_hints(&tree, bounds, 2, &resolve).expect_err("hints never rescue");
    assert_eq!(hinted_err.message(), strict_err.message());

    // Hinted overlap keeps the 1px bases and flags C overconstrained while
    // reporting capacity overflow separately from the minimum flag.
    let hinted =
        project_overlap_with_hints(&tree, bounds, 2, &resolve).expect("hinted overlap projects");
    assert!(hinted.capacity_overflow);
    assert_eq!(hinted.overconstrained, vec![NodeId::from("C")]);
    assert_eq!(lookup(&hinted.leaves, "A"), rect(0, 0, 1, 10));
    assert_eq!(lookup(&hinted.leaves, "B"), rect(3, 0, 1, 10));
    assert_eq!(lookup(&hinted.leaves, "C"), rect(6, 0, 1, 10));
    // Golden B6-after verdict: C (w=1) violates its min_w=2; A and B carry
    // no minimums and are satisfied.
    assert!(lookup(&hinted.leaves, "C").w < 2);
}

// Control: where strict projection succeeds, overlap agrees byte-for-byte
// and reports no capacity overflow.
#[test]
fn feasible_control_overlap_matches_strict_projection() {
    let tree = hgroup("root", vec![leaf("A"), leaf("B")], vec![1, 1]);
    let bounds = rect(0, 0, 10, 10);
    let strict = project(&tree, bounds, 2).expect("feasible");
    let overlap = project_overlap(&tree, bounds, 2).expect("overlap projects");
    assert!(!overlap.capacity_overflow);
    assert_eq!(overlap.leaves, strict);
    assert_eq!(lookup(&overlap.leaves, "A"), rect(0, 0, 4, 10));
    assert_eq!(lookup(&overlap.leaves, "B"), rect(6, 0, 4, 10));
    let hinted = project_overlap_with_hints(&tree, bounds, 2, &|_| WindowSizeHints::none())
        .expect("hinted overlap projects");
    assert!(!hinted.capacity_overflow);
    assert!(hinted.overconstrained.is_empty());
    assert_eq!(hinted.leaves, strict);
}
