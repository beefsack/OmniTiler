//! Best-effort admission policy (D2/D3 of `best-effort-admission.md`).
//!
//! Two global settings with validated wire tokens. Anything else falls back
//! to the default at the adapter boundary. This module owns the shared
//! values, their parse/default handling, and the pure D2 anchor-fallback
//! search over one domain tree: ordinary-anchor-first splittability,
//! largest/closest/no-search selection, and the candidate split shape.
//! Float/overlap actuation and session lifecycle gating live elsewhere.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{Axis, Node, NodeId, WindowId};
use crate::geometry::{Rect, project};
use crate::size_hints::{WindowSizeHints, project_with_hints};

/// D2 anchor fallback when the ordinary anchor cannot split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AnchorFallback {
    /// Search the domain for the largest splittable window (default).
    #[default]
    LargestSplittable,
    /// Search sibling subtrees nearest-first in topology order.
    ClosestRelative,
    /// Skip search; policy exhaustion feeds the no-space setting directly.
    NoSearch,
}

impl AnchorFallback {
    /// Wire token for this value.
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::LargestSplittable => "largest-splittable-window",
            Self::ClosestRelative => "closest-splittable-relative",
            Self::NoSearch => "no-search",
        }
    }

    /// Validated parse of a wire token. `None` for anything else, including
    /// empty strings: callers fall back to the default fail-closed.
    #[must_use]
    pub fn parse_wire(value: &str) -> Option<Self> {
        match value {
            "largest-splittable-window" => Some(Self::LargestSplittable),
            "closest-splittable-relative" => Some(Self::ClosestRelative),
            "no-search" => Some(Self::NoSearch),
            _ => None,
        }
    }

    /// Normalized mode for an optional raw string: missing (`None`) or any
    /// invalid token falls back to the default.
    #[must_use]
    pub fn normalize_wire(value: Option<&str>) -> Self {
        value.and_then(Self::parse_wire).unwrap_or_default()
    }
}

/// D3 no-space result when no split is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NoSpaceResult {
    /// Float the window; tiling stays intact (default).
    #[default]
    Float,
    /// Keep tiled membership with positive overflowing geometry (D9).
    AllowOverlap,
}

impl NoSpaceResult {
    /// Wire token for this value.
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::Float => "float",
            Self::AllowOverlap => "allow-overlap",
        }
    }

    /// Validated parse of a wire token. `None` for anything else, including
    /// empty strings: callers fall back to the default fail-closed.
    #[must_use]
    pub fn parse_wire(value: &str) -> Option<Self> {
        match value {
            "float" => Some(Self::Float),
            "allow-overlap" => Some(Self::AllowOverlap),
            _ => None,
        }
    }

    /// Normalized mode for an optional raw string: missing (`None`) or any
    /// invalid token falls back to the default.
    #[must_use]
    pub fn normalize_wire(value: Option<&str>) -> Self {
        value.and_then(Self::parse_wire).unwrap_or_default()
    }
}

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

/// Closest-relative ring order: ascend from `anchor` one ancestor at a time;
/// at each ancestor visit sibling-subtree leaves (excluding the just-visited
/// child subtree) depth-first in child order, ranking sibling subtrees by
/// absolute child-index distance then ascending child index. "Closest" is
/// topology only, never screen distance or MRU. Ancestor groups themselves
/// are never targets.
#[must_use]
pub fn closest_relative_order(tree: &Node, anchor: &NodeId) -> Vec<NodeId> {
    let mut path: Vec<(Vec<Node>, usize)> = Vec::new();
    if !find_path(tree, anchor, &mut path) {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    // `path` runs root..parent-of-anchor; walk from the anchor's parent up.
    for depth in (0..path.len()).rev() {
        let (siblings, visited) = &path[depth];
        let mut order: Vec<usize> = (0..siblings.len()).filter(|i| *i != *visited).collect();
        order.sort_by(|a, b| {
            let da = (*a as i64 - *visited as i64).abs();
            let db = (*b as i64 - *visited as i64).abs();
            da.cmp(&db).then(a.cmp(b))
        });
        for index in order {
            let mut leaves = Vec::new();
            collect(&siblings[index], &mut leaves);
            for leaf in leaves {
                if leaf != *anchor && seen.insert(leaf.clone()) {
                    out.push(leaf);
                }
            }
        }
    }
    out
}

/// Ancestor chain for `anchor`: each entry holds the parent's children plus
/// the index of the child on the anchor path. Empty when the anchor is the
/// root leaf itself or absent.
fn find_path(node: &Node, anchor: &NodeId, path: &mut Vec<(Vec<Node>, usize)>) -> bool {
    match node {
        Node::Leaf { id } => id == anchor,
        Node::Group { children, .. } => {
            for (index, child) in children.iter().enumerate() {
                if child.id() == anchor || subtree_has(child, anchor) {
                    path.push((children.clone(), index));
                    if child.id() == anchor {
                        return true;
                    }
                    return find_path(child, anchor, path);
                }
            }
            false
        }
    }
}

fn subtree_has(node: &Node, anchor: &NodeId) -> bool {
    if node.id() == anchor {
        return true;
    }
    match node {
        Node::Leaf { .. } => false,
        Node::Group { children, .. } => children.iter().any(|c| subtree_has(c, anchor)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_wire_roundtrips_with_largest_default() {
        assert_eq!(AnchorFallback::default(), AnchorFallback::LargestSplittable);
        assert_eq!(
            AnchorFallback::LargestSplittable.as_wire_str(),
            "largest-splittable-window"
        );
        assert_eq!(
            AnchorFallback::ClosestRelative.as_wire_str(),
            "closest-splittable-relative"
        );
        assert_eq!(AnchorFallback::NoSearch.as_wire_str(), "no-search");
        assert_eq!(
            AnchorFallback::parse_wire("largest-splittable-window"),
            Some(AnchorFallback::LargestSplittable)
        );
        assert_eq!(
            AnchorFallback::parse_wire("closest-splittable-relative"),
            Some(AnchorFallback::ClosestRelative)
        );
        assert_eq!(
            AnchorFallback::parse_wire("no-search"),
            Some(AnchorFallback::NoSearch)
        );
        for invalid in ["", "largest", "no_search", "largest-splittable-window "] {
            assert_eq!(AnchorFallback::parse_wire(invalid), None, "{invalid:?}");
        }
        assert_eq!(
            AnchorFallback::normalize_wire(None),
            AnchorFallback::LargestSplittable
        );
        assert_eq!(
            AnchorFallback::normalize_wire(Some("bogus")),
            AnchorFallback::LargestSplittable
        );
    }

    #[test]
    fn no_space_wire_roundtrips_with_float_default() {
        assert_eq!(NoSpaceResult::default(), NoSpaceResult::Float);
        assert_eq!(NoSpaceResult::Float.as_wire_str(), "float");
        assert_eq!(NoSpaceResult::AllowOverlap.as_wire_str(), "allow-overlap");
        assert_eq!(
            NoSpaceResult::parse_wire("float"),
            Some(NoSpaceResult::Float)
        );
        assert_eq!(
            NoSpaceResult::parse_wire("allow-overlap"),
            Some(NoSpaceResult::AllowOverlap)
        );
        for invalid in ["", "overlap", "allow_overlap", "float "] {
            assert_eq!(NoSpaceResult::parse_wire(invalid), None, "{invalid:?}");
        }
        assert_eq!(NoSpaceResult::normalize_wire(None), NoSpaceResult::Float);
        assert_eq!(
            NoSpaceResult::normalize_wire(Some("bogus")),
            NoSpaceResult::Float
        );
    }

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

    #[test]
    fn closest_order_walks_sibling_rings_nearest_first() {
        // H[A, V[B, C]] anchored at C: sibling ring [B], then the next
        // ancestor ring [A]. Ancestor groups are never targets.
        let tree = group(
            "root",
            Axis::Horizontal,
            vec![
                leaf("a"),
                group(
                    "inner",
                    Axis::Vertical,
                    vec![leaf("b"), leaf("c")],
                    vec![1, 1],
                ),
            ],
            vec![1, 1],
        );
        assert_eq!(
            closest_relative_order(&tree, &NodeId::from("c")),
            vec![NodeId::from("b"), NodeId::from("a")]
        );
        assert_eq!(
            closest_relative_order(&tree, &NodeId::from("b")),
            vec![NodeId::from("c"), NodeId::from("a")]
        );
        // Unknown anchors visit nothing.
        assert!(closest_relative_order(&tree, &NodeId::from("x")).is_empty());
    }
}
