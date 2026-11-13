//! Best-effort admission policy (D2/D3 of `best-effort-admission.md`).
//!
//! Fixed float-on-exhaustion and fixed largest-splittable fallback:
//! ordinary-anchor-first splittability, largest-splittable selection, and
//! the candidate split shape. Float actuation and session lifecycle gating
//! live elsewhere.
//!
//! Host compensation in the best-effort admission search: output geometry
//! is finite and clients enforce their own minimum sizes, so insertion can
//! be geometrically impossible without any policy choice. The D2 fallback
//! search (and the D3 float result) places the window anyway instead of
//! refusing it.

use std::collections::BTreeMap;

use crate::directional::{Axis, Node, NodeId, WindowId};
use crate::geometry::{Rect, project};
use crate::size_hints::{WindowSizeHints, project_with_hints};

/// Whether inserting the equal old/new wrapper on `anchor` keeps the whole
/// tree strictly projectable (geometric D1 scope: plain [`project`] plus the
/// normal hinted [`project_with_hints`]; `overconstrained` minimum flags
/// alone never disqualify, preserving B6).
///
/// `candidate` is the post-insertion tree. `hints` resolves per-window hints
/// for the domain (unknown windows resolve to none, advisory only).
#[must_use]
pub fn tree_admits_candidate(
    candidate: &Node,
    bounds: Rect,
    gap: i32,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
    leaf_to_window: &BTreeMap<NodeId, WindowId>,
) -> bool {
    if project(candidate, bounds, gap).is_err() {
        return false;
    }
    let resolve = |leaf: &NodeId| {
        leaf_to_window
            .get(leaf)
            .and_then(|window| hints.get(window))
            .copied()
            .unwrap_or_else(WindowSizeHints::none)
    };
    project_with_hints(candidate, bounds, gap, &resolve).is_ok()
}

/// Build the post-insertion tree for splitting `anchor`: wrap the anchor leaf
/// in an ordered old/new binary group on its long edge (horizontal when its
/// plain allocation is wider than tall, vertical otherwise including square
/// ties), with equal shares. Never the short edge, no ancestor wrap, no
/// rebalance. Returns `None` when the anchor is absent or ids collide.
#[must_use]
pub fn split_candidate_at(
    tree: &Node,
    anchor: &NodeId,
    anchor_rect: Rect,
    new_leaf: NodeId,
    group_id: NodeId,
) -> Option<Node> {
    let axis = if anchor_rect.w > anchor_rect.h {
        Axis::Horizontal
    } else {
        Axis::Vertical
    };
    wrap_leaf(tree, anchor, new_leaf, group_id, axis)
}

fn wrap_leaf(
    node: &Node,
    anchor: &NodeId,
    new_leaf: NodeId,
    group_id: NodeId,
    axis: Axis,
) -> Option<Node> {
    match node {
        Node::Leaf { id } if id == anchor => Some(Node::Group {
            id: group_id,
            axis,
            children: vec![Node::Leaf { id: id.clone() }, Node::Leaf { id: new_leaf }],
            shares: vec![1, 1],
        }),
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis: parent_axis,
            children,
            shares,
        } => {
            for (index, child) in children.iter().enumerate() {
                if let Some(updated) =
                    wrap_leaf(child, anchor, new_leaf.clone(), group_id.clone(), axis)
                {
                    let mut new_children = children.clone();
                    new_children[index] = updated;
                    return Some(Node::Group {
                        id: id.clone(),
                        axis: *parent_axis,
                        children: new_children,
                        shares: shares.clone(),
                    });
                }
            }
            None
        }
    }
}

/// Child-order leaf visitation with per-leaf DFS indices.
pub(crate) fn dfs_leaves(tree: &Node) -> Vec<NodeId> {
    let mut out = Vec::new();
    collect(tree, &mut out);
    out
}

fn collect(node: &Node, out: &mut Vec<NodeId>) {
    match node {
        Node::Leaf { id } => out.push(id.clone()),
        Node::Group { children, .. } => {
            for child in children {
                collect(child, out);
            }
        }
    }
}

/// Rank fallback candidates largest-first: descending plain-projected
/// `width*height` (wide integer), ties by ascending DFS child-order leaf
/// index, then ascending stable window-lifetime identity.
#[must_use]
pub fn rank_largest_candidates(
    areas: &BTreeMap<NodeId, i64>,
    dfs_order: &[NodeId],
    leaf_to_window: &BTreeMap<NodeId, WindowId>,
) -> Vec<NodeId> {
    let index_of: BTreeMap<&NodeId, usize> = dfs_order
        .iter()
        .enumerate()
        .map(|(i, id)| (id, i))
        .collect();
    let mut ids: Vec<NodeId> = areas.keys().cloned().collect();
    ids.sort_by(|a, b| {
        let area_cmp = areas.get(b).unwrap_or(&0).cmp(areas.get(a).unwrap_or(&0));
        if area_cmp != std::cmp::Ordering::Equal {
            return area_cmp;
        }
        let ia = index_of.get(a).copied().unwrap_or(usize::MAX);
        let ib = index_of.get(b).copied().unwrap_or(usize::MAX);
        if ia != ib {
            return ia.cmp(&ib);
        }
        let wa = leaf_to_window.get(a).map(|w| w.0.as_str()).unwrap_or("");
        let wb = leaf_to_window.get(b).map(|w| w.0.as_str()).unwrap_or("");
        wa.cmp(wb)
    });
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn split_candidate_uses_long_edge_with_square_tie_vertical() {
        fn wrapper_axis(tree: &Node) -> Option<Axis> {
            match tree {
                Node::Leaf { .. } => None,
                Node::Group { children, .. } => {
                    for child in children {
                        if let Node::Group { id, axis, .. } = child
                            && id == &NodeId::from("g")
                        {
                            return Some(*axis);
                        }
                        if let Some(found) = wrapper_axis(child) {
                            return Some(found);
                        }
                    }
                    None
                }
            }
        }
        let tree = group(
            "root",
            Axis::Horizontal,
            vec![leaf("a"), leaf("b")],
            vec![1, 1],
        );
        // Wide allocation splits horizontally (width).
        let wide = split_candidate_at(
            &tree,
            &NodeId::from("a"),
            Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 4,
            },
            NodeId::from("n"),
            NodeId::from("g"),
        )
        .expect("wraps");
        assert_eq!(wrapper_axis(&wide), Some(Axis::Horizontal));
        // Tall allocation splits vertically.
        let tall = split_candidate_at(
            &tree,
            &NodeId::from("a"),
            Rect {
                x: 0,
                y: 0,
                w: 4,
                h: 10,
            },
            NodeId::from("n"),
            NodeId::from("g"),
        )
        .expect("wraps");
        assert_eq!(wrapper_axis(&tall), Some(Axis::Vertical));
        // Square tie splits vertically; the short edge is never tried.
        let square = split_candidate_at(
            &tree,
            &NodeId::from("a"),
            Rect {
                x: 0,
                y: 0,
                w: 8,
                h: 8,
            },
            NodeId::from("n"),
            NodeId::from("g"),
        )
        .expect("wraps");
        assert_eq!(wrapper_axis(&square), Some(Axis::Vertical));
        // Absent anchors never wrap.
        assert!(
            split_candidate_at(
                &tree,
                &NodeId::from("missing"),
                Rect {
                    x: 0,
                    y: 0,
                    w: 8,
                    h: 8
                },
                NodeId::from("n"),
                NodeId::from("g"),
            )
            .is_none()
        );
    }

    #[test]
    fn largest_ranking_breaks_equal_areas_by_dfs_then_identity() {
        // Equal areas on opposite sides of the anchor: DFS child order
        // wins before lifetime identity.
        let order = vec![
            NodeId::from("left"),
            NodeId::from("mid"),
            NodeId::from("right"),
        ];
        let areas: BTreeMap<NodeId, i64> = order.iter().map(|id| (id.clone(), 100)).collect();
        let links: BTreeMap<NodeId, WindowId> = ["left", "mid", "right"]
            .into_iter()
            .map(|name| (NodeId::from(name), WindowId(name.to_owned())))
            .collect();
        assert_eq!(
            rank_largest_candidates(&areas, &order, &links),
            vec![
                NodeId::from("left"),
                NodeId::from("mid"),
                NodeId::from("right")
            ]
        );
        // Strictly larger area wins regardless of position.
        let mut skewed = areas.clone();
        skewed.insert(NodeId::from("right"), 400);
        assert_eq!(
            rank_largest_candidates(&skewed, &order, &links)[0],
            NodeId::from("right")
        );
    }
}
