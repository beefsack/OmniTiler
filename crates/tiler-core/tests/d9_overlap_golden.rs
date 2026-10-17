#![forbid(unsafe_code)]
//! D9 overlap-mode golden geometry/navigation cases (selected 2026-10-17).
//!
//! Tests-only reference oracle: NO production implementation here. The strict
//! projector (`tiler_core::geometry::project`) stays fail-closed; these cases
//! pin the selected D9 positive-overflow semantics so a future production
//! projector can be accepted against checked-in numbers:
//! - effective extent `max(assigned, gap*(n-1)+n)` per group, existing
//!   reserve-one/share rounding from the assigned origin, cross extent
//!   unchanged, parent slot unchanged, recurse, then B6 minimum expansion.
//! - Strict projection remains strict (every exhausted golden asserts it).
//!
//! Future production call sites: `geometry.rs` (`project`/`layout_into`, new
//! overlap entry point), `size_hints.rs` (`project_with_hints`/`layout_hinted`
//! for B6-after-overlap ordering), and the admission/hold consumers
//! (`session/ops/lifecycle.rs`, `world_publish.rs`, platform adapters).

use std::collections::HashSet;

use tiler_core::directional::{Axis, Direction, FocusPlan, Node, NodeId, plan_focus};
use tiler_core::geometry::{Rect, project};
use tiler_core::size_hints::{WindowSizeHints, enforce_minimums, project_with_hints};

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

fn validate_topology(node: &Node, seen: &mut HashSet<NodeId>) -> Result<(), String> {
    if node.id().0.is_empty() {
        return Err("node identities must be non-empty".to_string());
    }
    if !seen.insert(node.id().clone()) {
        return Err("node identities must be unique".to_string());
    }
    match node {
        Node::Leaf { .. } => Ok(()),
        Node::Group {
            children, shares, ..
        } => {
            if children.len() < 2 {
                return Err("groups must have at least two children".to_string());
            }
            if shares.len() != children.len() {
                return Err("group shares must align exactly with children".to_string());
            }
            if shares.contains(&0) {
                return Err("group shares must be positive".to_string());
            }
            let mut total: u64 = 0;
            for share in shares {
                total = total
                    .checked_add(*share)
                    .ok_or_else(|| "group shares total overflows".to_string())?;
            }
            if total == 0 {
                return Err("group shares must be positive".to_string());
            }
            for child in children {
                validate_topology(child, seen)?;
            }
            Ok(())
        }
    }
}

/// Tests-only D9 reference oracle (never shipped): strict validation, then
/// per-group `effective = max(assigned, gap*(n-1)+n)` with the existing
/// reserve-one/share rounding laid out from the ASSIGNED origin, recursed.
fn overlap_reference(tree: &Node, bounds: Rect, gap: i32) -> Result<Vec<(NodeId, Rect)>, String> {
    if bounds.w <= 0
        || bounds.h <= 0
        || bounds.x.checked_add(bounds.w).is_none()
        || bounds.y.checked_add(bounds.h).is_none()
    {
        return Err("bounds must have positive extents".to_string());
    }
    if gap < 0 {
        return Err("gap must be non-negative".to_string());
    }
    validate_topology(tree, &mut HashSet::new())?;
    let mut out = Vec::new();
    layout_overlap(tree, bounds, gap, &mut out)?;
    Ok(out)
}

fn layout_overlap(
    node: &Node,
    rect: Rect,
    gap: i32,
    out: &mut Vec<(NodeId, Rect)>,
) -> Result<(), String> {
    match node {
        Node::Leaf { id } => {
            out.push((id.clone(), rect));
            Ok(())
        }
        Node::Group {
            axis,
            children,
            shares,
            ..
        } => {
            let n = children.len();
            let mut total: u64 = 0;
            for share in shares {
                total = total
                    .checked_add(*share)
                    .ok_or_else(|| "group shares total overflows".to_string())?;
            }
            let assigned: i64 = match axis {
                Axis::Horizontal => i64::from(rect.w),
                Axis::Vertical => i64::from(rect.h),
            };
            let gaps_total: i64 = i64::from(gap)
                .checked_mul(n as i64 - 1)
                .ok_or_else(|| "gap budget overflows".to_string())?;
            // D9 floor: gap budget plus one pixel per child.
            let floor = gaps_total
                .checked_add(n as i64)
                .ok_or_else(|| "gap budget overflows".to_string())?;
            let effective = assigned.max(floor);
            let avail = effective - gaps_total;
            let distributable = avail - n as i64;
            // Same reserve-one rounding as the strict projector.
            let mut sizes: Vec<i64> = Vec::with_capacity(n);
            let mut used: i64 = 0;
            for (index, share) in shares.iter().enumerate() {
                if index + 1 == n {
                    sizes.push(avail - used);
                } else {
                    let proportional =
                        (i128::from(distributable) * i128::from(*share) / i128::from(total)) as i64;
                    sizes.push(proportional + 1);
                    used += proportional + 1;
                }
            }
            let mut cursor: i64 = match axis {
                Axis::Horizontal => i64::from(rect.x),
                Axis::Vertical => i64::from(rect.y),
            };
            for (index, (child, size)) in children.iter().zip(sizes.iter()).enumerate() {
                let size_i32: i32 = (*size)
                    .try_into()
                    .map_err(|_| "segment size overflows".to_string())?;
                let child_rect = match axis {
                    Axis::Horizontal => Rect {
                        x: i32::try_from(cursor)
                            .map_err(|_| "segment offset overflows".to_string())?,
                        y: rect.y,
                        w: size_i32,
                        h: rect.h,
                    },
                    Axis::Vertical => Rect {
                        x: rect.x,
                        y: i32::try_from(cursor)
                            .map_err(|_| "segment offset overflows".to_string())?,
                        w: rect.w,
                        h: size_i32,
                    },
                };
                layout_overlap(child, child_rect, gap, out)?;
                if index + 1 < n {
                    cursor = cursor
                        .checked_add(*size)
                        .and_then(|next| next.checked_add(i64::from(gap)))
                        .ok_or_else(|| "segment offset overflows".to_string())?;
                }
            }
            Ok(())
        }
    }
}

fn lookup(out: &[(NodeId, Rect)], id: &str) -> Rect {
    out.iter()
        .find(|(leaf, _)| leaf == &NodeId::from(id))
        .unwrap_or_else(|| panic!("missing golden leaf {id}"))
        .1
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

    let first = overlap_reference(&tree, bounds, 2).expect("oracle projects");
    let second = overlap_reference(&tree, bounds, 2).expect("oracle deterministic");
    assert_eq!(first, second);
    assert_eq!(lookup(&first, "A"), rect(0, 0, 1, 10));
    assert_eq!(lookup(&first, "B"), rect(3, 0, 1, 10));
    assert_eq!(lookup(&first, "C"), rect(6, 0, 1, 10));
    // Every golden tile stays positive; C leaves the domain.
    for (_, r) in &first {
        assert!(r.w > 0 && r.h > 0);
    }
    assert!(lookup(&first, "C").x + lookup(&first, "C").w > bounds.x + bounds.w);

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

    let out = overlap_reference(&tree, bounds, 2).expect("oracle projects");
    assert_eq!(lookup(&out, "B"), rect(0, 0, 1, 10));
    assert_eq!(lookup(&out, "C"), rect(3, 0, 1, 10));
    assert_eq!(lookup(&out, "D"), rect(6, 0, 1, 10));
    assert_eq!(lookup(&out, "E"), rect(9, 0, 1, 10));
    // Parent slot unchanged: A keeps its strict assigned slot.
    let g_slot = rect(0, 0, 6, 10);
    let a_rect = rect(8, 0, 12, 10);
    assert_eq!(lookup(&out, "A"), a_rect);
    assert!(intersects(&lookup(&out, "E"), &a_rect));
    assert!(!intersects(&lookup(&out, "D"), &a_rect));
    assert!(!intersects(&lookup(&out, "D"), &g_slot));

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

    let out = overlap_reference(&tree, bounds, 2).expect("oracle projects");
    assert_eq!(lookup(&out, "A"), rect(10, 20, 6, 2));
    assert_eq!(lookup(&out, "B"), rect(10, 24, 1, 2));
    assert_eq!(lookup(&out, "C"), rect(13, 24, 1, 2));
    assert_eq!(lookup(&out, "D"), rect(16, 24, 1, 2));
    let d = lookup(&out, "D");
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
    let err = overlap_reference(&tree, bounds, 2).expect_err("must hold");
    assert_eq!(err, "segment offset overflows");
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

    // Overlap base sizes with the B6 enforce step applied after: floors
    // [1,1,2] exceed the 3px effective total, so production enforce refuses
    // without mutation and C stays flagged overconstrained.
    let mut sizes = vec![1i64, 1, 1];
    assert!(!enforce_minimums(&mut sizes, &[1, 1, 2]));
    assert_eq!(sizes, vec![1i64, 1, 1]);
    let out = overlap_reference(&tree, bounds, 2).expect("oracle projects");
    assert_eq!(lookup(&out, "A"), rect(0, 0, 1, 10));
    assert_eq!(lookup(&out, "B"), rect(3, 0, 1, 10));
    assert_eq!(lookup(&out, "C"), rect(6, 0, 1, 10));
    // Golden B6-after verdict: C (w=1) violates its min_w=2; A and B carry
    // no minimums and are satisfied.
    assert!(lookup(&out, "C").w < 2);
}

// Control: where strict projection succeeds, the oracle agrees byte-for-byte,
// proving the reference formula matches production outside exhaustion.
#[test]
fn feasible_control_oracle_matches_strict_projection() {
    let tree = hgroup("root", vec![leaf("A"), leaf("B")], vec![1, 1]);
    let bounds = rect(0, 0, 10, 10);
    let strict = project(&tree, bounds, 2).expect("feasible");
    let oracle = overlap_reference(&tree, bounds, 2).expect("oracle projects");
    assert_eq!(oracle.len(), strict.len());
    for (leaf, r) in &oracle {
        let found = strict
            .iter()
            .find(|projected| &projected.leaf == leaf)
            .expect("strict has the leaf");
        assert_eq!(*r, found.rect);
    }
    assert_eq!(lookup(&oracle, "A"), rect(0, 0, 4, 10));
    assert_eq!(lookup(&oracle, "B"), rect(6, 0, 4, 10));
}
