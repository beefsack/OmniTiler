//! Deterministic portable geometry projector over the directional domain topology.
//!
//! Depends only on [`crate::directional`] model types and `std`: no
//! native/platform state, no actuation, no float math, no dependencies.
//! [`project`] subdivides `bounds` along each [`Node::Group`] axis
//! proportionally to its positive integer `shares`, preserving child order.
//! Sibling segments are separated by exactly `gap` device units when the gap
//! budget fits the parent extent; otherwise projection fails closed.
//! [`inset_bounds`] owns the outer domain inset: it shrinks work-area bounds
//! by `outer_gap` on every side before projection, failing closed when the
//! outer gap is negative, overflows, or exhausts the bounds.

use std::collections::HashSet;
use std::fmt;

use crate::directional::{Axis, Node, NodeId};

/// Integer pixel rectangle. `w` and `h` are extents, so the covered range is
/// `[x, x + w)` by `[y, y + h)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// One projected leaf in deterministic child-order visitation order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedLeaf {
    pub leaf: NodeId,
    pub rect: Rect,
}

/// Fail-closed projection rejection with a fixed diagnostic message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionError {
    message: String,
}

impl ProjectionError {
    /// Diagnostic message (fixed redacted vocabulary; never echoes ids).
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProjectionError {}

fn error(message: impl Into<String>) -> ProjectionError {
    ProjectionError {
        message: message.into(),
    }
}

fn valid_rect(rect: &Rect) -> bool {
    rect.w > 0
        && rect.h > 0
        && rect.x.checked_add(rect.w).is_some()
        && rect.y.checked_add(rect.h).is_some()
}

fn validate_tree(node: &Node, seen: &mut HashSet<NodeId>) -> Result<(), ProjectionError> {
    if node.id().0.is_empty() {
        return Err(error("node identities must be non-empty"));
    }
    if !seen.insert(node.id().clone()) {
        return Err(error("node identities must be unique"));
    }
    match node {
        Node::Leaf { .. } => Ok(()),
        Node::Group {
            children, shares, ..
        } => {
            if children.len() < 2 {
                return Err(error("groups must have at least two children"));
            }
            if shares.len() != children.len() {
                return Err(error("group shares must align exactly with children"));
            }
            if shares.contains(&0) {
                return Err(error("group shares must be positive"));
            }
            let mut total: u64 = 0;
            for share in shares {
                total = total
                    .checked_add(*share)
                    .ok_or_else(|| error("group shares total overflows"))?;
            }
            if total == 0 {
                return Err(error("group shares must be positive"));
            }
            for child in children {
                validate_tree(child, seen)?;
            }
            Ok(())
        }
    }
}

/// Shrink work-area `bounds` by `outer_gap` on every side.
///
/// Fails closed when `outer_gap` is negative, when the doubled inset
/// overflows, or when the inset exhausts either extent; the returned rect
/// always has positive `w`/`h`.
pub fn inset_bounds(bounds: Rect, outer_gap: i32) -> Result<Rect, ProjectionError> {
    if outer_gap < 0 {
        return Err(error("outer gap must be non-negative"));
    }
    let doubled: i64 = i64::from(outer_gap)
        .checked_mul(2)
        .ok_or_else(|| error("outer gap exhausts bounds"))?;
    if doubled >= i64::from(bounds.w) || doubled >= i64::from(bounds.h) {
        return Err(error("outer gap exhausts bounds"));
    }
    let inset = Rect {
        x: bounds
            .x
            .checked_add(outer_gap)
            .ok_or_else(|| error("outer gap exhausts bounds"))?,
        y: bounds
            .y
            .checked_add(outer_gap)
            .ok_or_else(|| error("outer gap exhausts bounds"))?,
        w: i32::try_from(i64::from(bounds.w) - doubled)
            .map_err(|_| error("outer gap exhausts bounds"))?,
        h: i32::try_from(i64::from(bounds.h) - doubled)
            .map_err(|_| error("outer gap exhausts bounds"))?,
    };
    if !valid_rect(&inset) {
        return Err(error("outer gap exhausts bounds"));
    }
    Ok(inset)
}

/// Project `tree` leaves into `bounds` separated by `gap`.
///
/// - `bounds` must have positive `w`/`h`; `gap` must be non-negative.
/// - Every group needs exactly one positive share per child with a
///   non-overflowing `u64` total; ids must be non-empty and unique.
/// - Integer allocation only: one unit is reserved for each sibling, then each
///   non-last sibling takes `1 + floor((avail - n) * share / total)` along the
///   group axis where `avail = extent - gap * (n - 1)`; the last sibling takes
///   the remainder. Gaps are exactly `gap` between sibling segments.
/// - Result is in child-order leaf visitation order; every leaf rect is
///   contained in `bounds` and no two leaf rects overlap.
pub fn project(tree: &Node, bounds: Rect, gap: i32) -> Result<Vec<ProjectedLeaf>, ProjectionError> {
    if !valid_rect(&bounds) {
        return Err(error("bounds must have positive extents"));
    }
    if gap < 0 {
        return Err(error("gap must be non-negative"));
    }
    validate_tree(tree, &mut HashSet::new())?;
    let mut out = Vec::new();
    layout_into(tree, bounds, gap, &mut out)?;
    Ok(out)
}

/// Both callers validate positive shares, nonzero total and `avail >= shares.len()`.
pub(crate) fn proportional_base_sizes(
    shares: &[u64],
    total: u64,
    distributable: i64,
    avail: i64,
) -> Vec<i64> {
    let mut sizes: Vec<i64> = Vec::with_capacity(shares.len());
    let mut used: i64 = 0;
    for (index, share) in shares.iter().enumerate() {
        if index + 1 == shares.len() {
            sizes.push(avail - used);
        } else {
            let proportional =
                (i128::from(distributable) * i128::from(*share) / i128::from(total)) as i64;
            let size = proportional + 1;
            sizes.push(size);
            used += size;
        }
    }
    sizes
}

fn layout_into(
    node: &Node,
    rect: Rect,
    gap: i32,
    out: &mut Vec<ProjectedLeaf>,
) -> Result<(), ProjectionError> {
    match node {
        Node::Leaf { id } => {
            out.push(ProjectedLeaf {
                leaf: id.clone(),
                rect,
            });
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
                    .ok_or_else(|| error("group shares total overflows"))?;
            }
            if total == 0 {
                return Err(error("group shares must be positive"));
            }
            let extent: i64 = match axis {
                Axis::Horizontal => i64::from(rect.w),
                Axis::Vertical => i64::from(rect.h),
            };
            let gaps_total: i64 = i64::from(gap)
                .checked_mul(n as i64 - 1)
                .ok_or_else(|| error("gap budget overflows"))?;
            if gaps_total > extent {
                return Err(error("gap budget exceeds parent extent"));
            }
            let avail: i64 = extent - gaps_total;
            if avail < n as i64 {
                return Err(error(
                    "parent extent cannot represent positive child segments",
                ));
            }
            // Reserve one unit per child before proportional allocation so a
            // valid skewed share vector never produces a zero-area leaf.
            let distributable = avail - n as i64;
            let sizes = proportional_base_sizes(shares, total, distributable, avail);
            let mut cursor: i64 = match axis {
                Axis::Horizontal => i64::from(rect.x),
                Axis::Vertical => i64::from(rect.y),
            };
            for (index, (child, size)) in children.iter().zip(sizes.iter()).enumerate() {
                let size_i32: i32 = (*size)
                    .try_into()
                    .map_err(|_| error("segment size overflows"))?;
                if *size < 0 {
                    return Err(error("segment size is negative"));
                }
                let child_rect = match axis {
                    Axis::Horizontal => Rect {
                        x: i32::try_from(cursor).map_err(|_| error("segment offset overflows"))?,
                        y: rect.y,
                        w: size_i32,
                        h: rect.h,
                    },
                    Axis::Vertical => Rect {
                        x: rect.x,
                        y: i32::try_from(cursor).map_err(|_| error("segment offset overflows"))?,
                        w: rect.w,
                        h: size_i32,
                    },
                };
                layout_into(child, child_rect, gap, out)?;
                if index + 1 < children.len() {
                    cursor = cursor
                        .checked_add(*size)
                        .and_then(|next| next.checked_add(i64::from(gap)))
                        .ok_or_else(|| error("segment offset overflows"))?;
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directional::Axis;

    fn leaf(id: &str) -> Node {
        Node::Leaf {
            id: NodeId::from(id),
        }
    }

    fn group(id: &str, axis: Axis, children: Vec<Node>, shares: Vec<u64>) -> Node {
        Node::Group {
            id: NodeId::from(id),
            axis,
            children,
            shares,
        }
    }

    fn equal_group(id: &str, axis: Axis, children: Vec<Node>) -> Node {
        let shares = vec![1u64; children.len()];
        group(id, axis, children, shares)
    }

    fn rects_intersect(a: &Rect, b: &Rect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    fn contained(inner: &Rect, outer: &Rect) -> bool {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.x + inner.w <= outer.x + outer.w
            && inner.y + inner.h <= outer.y + outer.h
    }

    #[test]
    fn unequal_shares_allocate_proportionally_in_order() {
        let tree = group(
            "root",
            Axis::Horizontal,
            vec![leaf("A"), leaf("B"), leaf("C")],
            vec![3, 1, 1],
        );
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 20,
        };
        let out = project(&tree, bounds, 0).expect("valid projection");
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].leaf, NodeId::from("A"));
        assert_eq!(out[1].leaf, NodeId::from("B"));
        assert_eq!(out[2].leaf, NodeId::from("C"));
        // Reserve one unit each, then floor(97*3/5)=58,
        // floor(97*1/5)=19, and give the final child the remainder.
        assert_eq!(
            out[0].rect,
            Rect {
                x: 0,
                y: 0,
                w: 59,
                h: 20
            }
        );
        assert_eq!(
            out[1].rect,
            Rect {
                x: 59,
                y: 0,
                w: 20,
                h: 20
            }
        );
        assert_eq!(
            out[2].rect,
            Rect {
                x: 79,
                y: 0,
                w: 21,
                h: 20
            }
        );
    }

    #[test]
    fn equal_ternary_projection_has_portable_final_remainder() {
        // Portable share contract, not a COSMIC pixel-parity claim.
        let tree = group(
            "root",
            Axis::Horizontal,
            vec![leaf("A"), leaf("B"), leaf("C")],
            vec![1, 1, 1],
        );
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 8,
            h: 8,
        };
        let out = project(&tree, bounds, 0).expect("valid projection");
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].leaf, NodeId::from("A"));
        assert_eq!(out[1].leaf, NodeId::from("B"));
        assert_eq!(out[2].leaf, NodeId::from("C"));
        assert_eq!(
            out[0].rect,
            Rect {
                x: 0,
                y: 0,
                w: 2,
                h: 8
            }
        );
        assert_eq!(
            out[1].rect,
            Rect {
                x: 2,
                y: 0,
                w: 2,
                h: 8
            }
        );
        assert_eq!(
            out[2].rect,
            Rect {
                x: 4,
                y: 0,
                w: 4,
                h: 8
            }
        );
        assert_eq!(out[0].rect.w + out[1].rect.w + out[2].rect.w, 8);
        assert_eq!(out[1].rect.x, out[0].rect.x + out[0].rect.w);
        assert_eq!(out[2].rect.x, out[1].rect.x + out[1].rect.w);
    }

    #[test]
    fn rejects_misaligned_and_zero_shares() {
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 90,
            h: 30,
        };
        // Too few shares.
        let short = group(
            "root",
            Axis::Horizontal,
            vec![leaf("A"), leaf("B")],
            vec![1],
        );
        assert!(project(&short, bounds, 0).is_err());
        // Too many shares.
        let long = group(
            "root",
            Axis::Horizontal,
            vec![leaf("A"), leaf("B")],
            vec![1, 1, 1],
        );
        assert!(project(&long, bounds, 0).is_err());
        // Zero share.
        let zero = group(
            "root",
            Axis::Horizontal,
            vec![leaf("A"), leaf("B")],
            vec![1, 0],
        );
        assert!(project(&zero, bounds, 0).is_err());
        // Overflowing total.
        let overflow = group(
            "root",
            Axis::Horizontal,
            vec![leaf("A"), leaf("B")],
            vec![u64::MAX, 1],
        );
        assert!(project(&overflow, bounds, 0).is_err());
    }

    #[test]
    fn rejects_malformed_rects_gap_and_topology() {
        let leaf_tree = leaf("A");
        assert!(
            project(
                &leaf_tree,
                Rect {
                    x: 0,
                    y: 0,
                    w: 0,
                    h: 10
                },
                0
            )
            .is_err()
        );
        assert!(
            project(
                &leaf_tree,
                Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10
                },
                -1
            )
            .is_err()
        );
        // Single-child group.
        let single = group("root", Axis::Horizontal, vec![leaf("A")], vec![1]);
        assert!(
            project(
                &single,
                Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10
                },
                0
            )
            .is_err()
        );
        // Duplicate ids.
        let duplicate = equal_group("root", Axis::Horizontal, vec![leaf("A"), leaf("A")]);
        assert!(
            project(
                &duplicate,
                Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10
                },
                0
            )
            .is_err()
        );
        // Gap budget exceeding the parent extent fails closed.
        let tight = equal_group("root", Axis::Horizontal, vec![leaf("A"), leaf("B")]);
        assert!(
            project(
                &tight,
                Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10
                },
                11
            )
            .is_err()
        );
        // Highly skewed valid weights still give every child a positive extent.
        let skewed = group(
            "skewed",
            Axis::Horizontal,
            vec![leaf("A"), leaf("B"), leaf("C")],
            vec![100, 1, 1],
        );
        let projected = project(
            &skewed,
            Rect {
                x: 0,
                y: 0,
                w: 3,
                h: 10,
            },
            0,
        )
        .expect("positive segments are representable");
        assert!(projected.iter().all(|leaf| leaf.rect.w > 0));
        // Every projected leaf needs a positive extent.
        assert!(
            project(
                &tight,
                Rect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 10
                },
                0
            )
            .is_err()
        );
        // Outer inset fails closed without touching the segment budget.
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 60,
        };
        assert!(inset_bounds(bounds, -1).is_err());
        assert!(inset_bounds(bounds, 50).is_err());
        assert!(
            inset_bounds(
                Rect {
                    x: 0,
                    y: 0,
                    w: 16,
                    h: 16
                },
                8
            )
            .is_err()
        );
    }

    #[test]
    fn projection_is_deterministic_gap_exact_and_contained() {
        let tree = group(
            "root",
            Axis::Horizontal,
            vec![
                leaf("A"),
                group(
                    "inner",
                    Axis::Vertical,
                    vec![leaf("B"), leaf("C")],
                    vec![1, 3],
                ),
            ],
            vec![1, 1],
        );
        let bounds = Rect {
            x: 10,
            y: 20,
            w: 94,
            h: 60,
        };
        let gap = 8;
        let first = project(&tree, bounds, gap).expect("valid projection");
        let second = project(&tree, bounds, gap).expect("valid projection");
        assert_eq!(first, second);
        assert_eq!(first.len(), 3);
        // Root avail 94-8=86, halves of 43 each.
        assert_eq!(
            first[0].rect,
            Rect {
                x: 10,
                y: 20,
                w: 43,
                h: 60
            }
        );
        // Inner avail 60-8=52: reserve two, then floor(50*1/4)+1=13.
        assert_eq!(
            first[1].rect,
            Rect {
                x: 61,
                y: 20,
                w: 43,
                h: 13
            }
        );
        assert_eq!(
            first[2].rect,
            Rect {
                x: 61,
                y: 41,
                w: 43,
                h: 39
            }
        );
        // Internal gaps are exactly `gap`.
        assert_eq!(first[1].rect.x, first[0].rect.x + first[0].rect.w + gap);
        assert_eq!(first[2].rect.y, first[1].rect.y + first[1].rect.h + gap);
        for leaf in &first {
            assert!(contained(&leaf.rect, &bounds), "leaf escapes bounds");
        }
        for i in 0..first.len() {
            for j in (i + 1)..first.len() {
                assert!(
                    !rects_intersect(&first[i].rect, &first[j].rect),
                    "leaf rects overlap"
                );
            }
        }
        // Outer inset shrinks every side before projection; leaves stay
        // inside both the inset and the carried bounds, and an overflowing
        // segment budget still fails closed after a valid inset.
        let inset = inset_bounds(bounds, 8).expect("valid outer inset");
        assert_eq!(
            inset,
            Rect {
                x: 18,
                y: 28,
                w: 78,
                h: 44
            }
        );
        let outer = project(&tree, inset, gap).expect("valid projection");
        assert_eq!(outer[1].rect.x, outer[0].rect.x + outer[0].rect.w + gap);
        for leaf in &outer {
            assert!(contained(&leaf.rect, &inset), "leaf escapes inset");
            assert!(contained(&leaf.rect, &bounds), "leaf escapes bounds");
        }
        assert!(project(&tree, inset, 79).is_err());
    }

    #[test]
    fn cosmic_default_nested_h_a_v_b_c_matches_effective_outer_8() {
        // COSMIC default theme raw (outer,inner)=(0,8) renders effective
        // work-area edge margin outer+inner=8. Keep portable outer_gap=8, not raw 0.
        let tree = equal_group(
            "root",
            Axis::Horizontal,
            vec![
                leaf("A"),
                equal_group("inner", Axis::Vertical, vec![leaf("B"), leaf("C")]),
            ],
        );
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        let inset = inset_bounds(bounds, 8).expect("valid outer inset");
        let out = project(&tree, inset, 8).expect("valid projection");
        assert_eq!(out.len(), 3);
        assert_eq!(
            out[0].rect,
            Rect {
                x: 8,
                y: 8,
                w: 38,
                h: 84
            }
        );
        assert_eq!(
            out[1].rect,
            Rect {
                x: 54,
                y: 8,
                w: 38,
                h: 38
            }
        );
        assert_eq!(
            out[2].rect,
            Rect {
                x: 54,
                y: 54,
                w: 38,
                h: 38
            }
        );
    }

    #[test]
    fn bounded_share_axis_nesting_matrix_stays_deterministic() {
        let share_sets: &[Vec<u64>] = &[
            vec![1, 1],
            vec![2, 1],
            vec![1, 2],
            vec![3, 1, 1],
            vec![1, 1, 1, 1],
        ];
        let axes = [Axis::Horizontal, Axis::Vertical];
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        };
        for shares in share_sets {
            for root_axis in axes {
                for inner_axis in axes {
                    let n = shares.len();
                    let children: Vec<Node> = (0..n)
                        .map(|i| Node::Leaf {
                            id: NodeId(format!("L{i}")),
                        })
                        .collect();
                    let inner = group("inner", inner_axis, children, shares.clone());
                    let tree = equal_group("root", root_axis, vec![leaf("edge"), inner]);
                    for gap in [0, 2] {
                        let first = project(&tree, bounds, gap).expect("matrix case valid");
                        let second = project(&tree, bounds, gap).expect("matrix case valid");
                        assert_eq!(first, second, "nondeterministic projection");
                        assert_eq!(first.len(), n + 1);
                        for leaf in &first {
                            assert!(contained(&leaf.rect, &bounds));
                        }
                        for i in 0..first.len() {
                            for j in (i + 1)..first.len() {
                                assert!(!rects_intersect(&first[i].rect, &first[j].rect));
                            }
                        }
                    }
                }
            }
        }
    }
}
