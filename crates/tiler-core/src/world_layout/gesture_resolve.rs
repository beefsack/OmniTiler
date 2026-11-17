//! Pure gesture pointer-target resolution and drag tree-placement algorithms.
//!
//! Ported from the retired session drag resolver without retained session,
//! pending, or reconciler state. Group allocation burns serials on the shared
//! generation allocator even when a preview or trial placement is discarded.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::DragSide;
use crate::directional::{Axis, Node, NodeId, WindowId};
use crate::geometry::Rect;
use crate::policy::{LayoutPolicy, PriorGroupEdge};
use crate::session::{GroupAllocator, OutputDomain, RefusalKind};

use super::state::DomainException;
use super::topology::{collect_leaves, collect_node_ids};
use super::tree_edit::{
    direct_parent_of_leaf, find_group, find_group_full, find_group_shares, find_subtree,
    generate_group_id, insert_leaf_into_group, parent_of_group, remove_leaf_from_tree,
    replace_child_at,
};

pub(super) struct GroupLayout {
    pub(super) id: NodeId,
    pub(super) axis: Axis,
    pub(super) rect: Rect,
    pub(super) child_starts: Vec<i64>,
}

pub(super) struct DragPlacement {
    pub(super) tree: Node,
}

pub(super) struct SharedDragResolved {
    pub(super) target_window: WindowId,
    pub(super) placement: DragPlacement,
    pub(super) next_prior: Option<PriorGroupEdge>,
}

pub(super) struct DragResolveInput<'a> {
    pub(super) policy: &'a dyn LayoutPolicy,
    pub(super) alloc: &'a GroupAllocator,
    pub(super) tree: &'a Node,
    pub(super) domain: &'a OutputDomain,
    pub(super) leaves: &'a BTreeMap<WindowId, NodeId>,
    pub(super) exceptions: &'a BTreeMap<WindowId, DomainException>,
    pub(super) work_area: Rect,
    pub(super) source_leaf: &'a NodeId,
    pub(super) source_window: &'a WindowId,
    pub(super) prior: Option<&'a PriorGroupEdge>,
    pub(super) revision: u64,
}

pub(super) fn resolve_drag_shared(
    input: &DragResolveInput<'_>,
    x: i32,
    y: i32,
) -> Result<SharedDragResolved, RefusalKind> {
    let area = input.work_area;
    if x < area.x
        || y < area.y
        || x.checked_sub(area.x).is_none_or(|dx| dx >= area.w)
        || y.checked_sub(area.y).is_none_or(|dy| dy >= area.h)
    {
        return Err(RefusalKind::CrossDomainMismatch);
    }
    let projected = crate::geometry::project(input.tree, input.domain.bounds, input.domain.gap)
        .map_err(|_| RefusalKind::MalformedTopology)?;
    let mut leaf_rects: BTreeMap<NodeId, Rect> = BTreeMap::new();
    for leaf in &projected {
        leaf_rects.insert(leaf.leaf.clone(), leaf.rect);
    }
    let layouts = drag_group_layouts(input.tree, &leaf_rects);
    let layout_by_id: BTreeMap<&NodeId, &GroupLayout> =
        layouts.iter().map(|l| (&l.id, l)).collect();

    let mut current_id = input.tree.id().clone();
    loop {
        let Some(node) = find_subtree(input.tree, &current_id) else {
            return Err(RefusalKind::MalformedTopology);
        };
        match node {
            Node::Leaf { .. } => break,
            Node::Group { children, .. } => {
                let mut next: Option<NodeId> = None;
                for child in children {
                    let contains = match child {
                        Node::Leaf { id } => {
                            leaf_rects.get(id).is_some_and(|r| contains_point(r, x, y))
                        }
                        Node::Group { id, .. } => layout_by_id
                            .get(id)
                            .is_some_and(|l| contains_point(&l.rect, x, y)),
                    };
                    if contains {
                        next = Some(child.id().clone());
                        break;
                    }
                }
                match next {
                    Some(id) => current_id = id,
                    None => break,
                }
            }
        }
    }

    let Some(target_node) = find_subtree(input.tree, &current_id) else {
        return Err(RefusalKind::MalformedTopology);
    };
    match target_node {
        Node::Group { .. } => {
            let layout = layout_by_id
                .get(&current_id)
                .ok_or(RefusalKind::MalformedTopology)?;
            if let Some(edge) =
                input
                    .policy
                    .classify_group_point(&layout.rect, &layout.id, x, y, input.prior)
            {
                let rep =
                    group_rep_window(input.tree, &layout.id, input.leaves, input.source_window)
                        .ok_or(RefusalKind::MalformedTopology)?;
                if input.exceptions.contains_key(&rep) {
                    return Err(RefusalKind::MalformedTopology);
                }
                if edge.axis().is_none() || edge.before().is_none() {
                    return Err(RefusalKind::UnsupportedCapability);
                }
                let placement = apply_group_edge_placement(
                    input.policy,
                    input.alloc,
                    input.tree,
                    input.source_leaf,
                    &layout.id,
                    edge,
                )
                .ok_or(RefusalKind::MalformedTopology)?;
                return Ok(SharedDragResolved {
                    target_window: rep,
                    placement,
                    next_prior: Some(PriorGroupEdge {
                        group: layout.id.clone(),
                        edge,
                    }),
                });
            }
            let offset = match layout.axis {
                Axis::Horizontal => i64::from(x),
                Axis::Vertical => i64::from(y),
            };
            let predecessor = input
                .policy
                .insertion_index_for_offset(&layout.child_starts, offset);
            let rep = group_rep_window(input.tree, &layout.id, input.leaves, input.source_window)
                .ok_or(RefusalKind::MalformedTopology)?;
            if input.exceptions.contains_key(&rep) {
                return Err(RefusalKind::MalformedTopology);
            }
            let placement = apply_group_interior_placement(
                input.policy,
                input.tree,
                input.source_leaf,
                &layout.id,
                predecessor,
                input.revision,
            )
            .ok_or(RefusalKind::MalformedTopology)?;
            let side = match layout.axis {
                Axis::Horizontal => DragSide::Left,
                Axis::Vertical => DragSide::Top,
            };
            if side.axis().is_none() || side.before().is_none() {
                return Err(RefusalKind::UnsupportedCapability);
            }
            Ok(SharedDragResolved {
                target_window: rep,
                placement,
                next_prior: None,
            })
        }
        Node::Leaf { id } => {
            if id == input.source_leaf {
                return Err(RefusalKind::Unchanged);
            }
            let rect = leaf_rects
                .get(id)
                .copied()
                .ok_or(RefusalKind::MalformedTopology)?;
            let Some(side) = input.policy.classify_window_point(&rect, x, y) else {
                return Err(RefusalKind::Unchanged);
            };
            if side.is_stack() {
                return Err(RefusalKind::UnsupportedCapability);
            }
            let target_window = input
                .leaves
                .iter()
                .find_map(|(window, leaf)| (leaf == id).then(|| window.clone()))
                .ok_or(RefusalKind::MalformedTopology)?;
            if input.exceptions.contains_key(&target_window) {
                return Err(RefusalKind::MalformedTopology);
            }
            if side.axis().is_none() || side.before().is_none() {
                return Err(RefusalKind::UnsupportedCapability);
            }
            let placement = apply_drag_placement(
                input.policy,
                input.alloc,
                input.tree,
                input.source_leaf,
                id,
                side,
            )
            .ok_or(RefusalKind::MalformedTopology)?;
            Ok(SharedDragResolved {
                target_window,
                placement,
                next_prior: None,
            })
        }
    }
}

fn contains_point(rect: &Rect, x: i32, y: i32) -> bool {
    x >= rect.x
        && y >= rect.y
        && x.checked_sub(rect.x).is_some_and(|dx| dx < rect.w)
        && y.checked_sub(rect.y).is_some_and(|dy| dy < rect.h)
}

fn apply_drag_placement(
    policy: &dyn LayoutPolicy,
    alloc: &GroupAllocator,
    tree: &Node,
    source_leaf: &NodeId,
    target_leaf: &NodeId,
    side: DragSide,
) -> Option<DragPlacement> {
    if source_leaf == target_leaf {
        return None;
    }
    let (Some(axis), Some(before)) = (side.axis(), side.before()) else {
        return None;
    };
    let mover = Node::Leaf {
        id: source_leaf.clone(),
    };
    let working = if collect_leaves(tree).contains(source_leaf) {
        let working = remove_leaf_from_tree(policy, Some(tree.clone()), source_leaf)?;
        if collect_leaves(&working).contains(source_leaf) {
            return None;
        }
        working
    } else {
        tree.clone()
    };
    if !collect_leaves(&working).contains(target_leaf) {
        return None;
    }
    let mut node_ids = BTreeSet::new();
    collect_node_ids(&working, &mut node_ids);
    let parent = direct_parent_of_leaf(&working, target_leaf);
    if let Some(parent_id) = parent {
        let (children, parent_axis, _) = find_group(&working, &parent_id)?;
        if parent_axis == axis {
            let index = children.iter().position(|c| c.id() == target_leaf)?;
            let insertion_index = if before { index } else { index + 1 };
            if insertion_index > children.len() {
                return None;
            }
            let updated =
                insert_leaf_into_group(policy, working, &parent_id, insertion_index, mover)?;
            return Some(DragPlacement { tree: updated });
        }
    }
    let new_id = generate_group_id(alloc, &mut node_ids)?;
    let new_shares = policy.new_group_shares().to_vec();
    let wrapper = if before {
        Node::Group {
            id: new_id,
            axis,
            children: vec![
                mover,
                Node::Leaf {
                    id: target_leaf.clone(),
                },
            ],
            shares: new_shares,
        }
    } else {
        Node::Group {
            id: new_id,
            axis,
            children: vec![
                Node::Leaf {
                    id: target_leaf.clone(),
                },
                mover,
            ],
            shares: new_shares,
        }
    };
    if working.id() == target_leaf {
        return Some(DragPlacement { tree: wrapper });
    }
    let parent_id = direct_parent_of_leaf(&working, target_leaf)?;
    let (children, _, _) = find_group(&working, &parent_id)?;
    let shares = find_group_shares(&working, &parent_id)?;
    let index = children.iter().position(|c| c.id() == target_leaf)?;
    let target_share = *shares.get(index)?;
    let updated = replace_child_at(&working, &parent_id, index, wrapper, target_share)?;
    Some(DragPlacement { tree: updated })
}

fn apply_group_edge_placement(
    policy: &dyn LayoutPolicy,
    alloc: &GroupAllocator,
    tree: &Node,
    source_leaf: &NodeId,
    group_id: &NodeId,
    side: DragSide,
) -> Option<DragPlacement> {
    let (Some(axis), Some(before)) = (side.axis(), side.before()) else {
        return None;
    };
    if source_leaf == group_id {
        return None;
    }
    let mover = Node::Leaf {
        id: source_leaf.clone(),
    };
    let working = if collect_leaves(tree).contains(source_leaf) {
        let working = remove_leaf_from_tree(policy, Some(tree.clone()), source_leaf)?;
        if collect_leaves(&working).contains(source_leaf) {
            return None;
        }
        working
    } else {
        tree.clone()
    };
    let (children, _shares, gid, group_axis) = find_group_full(&working, group_id)?;
    debug_assert_eq!(&gid, group_id);
    if children.is_empty() {
        return None;
    }
    let mut node_ids = BTreeSet::new();
    collect_node_ids(&working, &mut node_ids);
    if group_axis == axis {
        let insertion_index = if before { 0 } else { children.len() };
        let updated = insert_leaf_into_group(policy, working, group_id, insertion_index, mover)?;
        return Some(DragPlacement { tree: updated });
    }
    let new_id = generate_group_id(alloc, &mut node_ids)?;
    let new_shares = policy.new_group_shares().to_vec();
    let group_node = find_subtree(&working, group_id)?.clone();
    let wrapper = if before {
        Node::Group {
            id: new_id,
            axis,
            children: vec![mover, group_node],
            shares: new_shares,
        }
    } else {
        Node::Group {
            id: new_id,
            axis,
            children: vec![group_node, mover],
            shares: new_shares,
        }
    };
    if working.id() == group_id {
        return Some(DragPlacement { tree: wrapper });
    }
    let parent_id = parent_of_group(&working, group_id)?;
    let (siblings, _, _) = find_group(&working, &parent_id)?;
    let parent_shares = find_group_shares(&working, &parent_id)?;
    let index = siblings.iter().position(|c| c.id() == group_id)?;
    let target_share = *parent_shares.get(index)?;
    let updated = replace_child_at(&working, &parent_id, index, wrapper, target_share)?;
    Some(DragPlacement { tree: updated })
}

fn apply_group_interior_placement(
    policy: &dyn LayoutPolicy,
    tree: &Node,
    source_leaf: &NodeId,
    group_id: &NodeId,
    predecessor: usize,
    base_revision: u64,
) -> Option<DragPlacement> {
    if source_leaf == group_id {
        return None;
    }
    let mover = Node::Leaf {
        id: source_leaf.clone(),
    };
    let working = if collect_leaves(tree).contains(source_leaf) {
        let working = remove_leaf_from_tree(policy, Some(tree.clone()), source_leaf)?;
        if collect_leaves(&working).contains(source_leaf) {
            return None;
        }
        working
    } else {
        tree.clone()
    };
    let (children, _, _, _) = find_group_full(&working, group_id)?;
    let len = children.len();
    let insertion_index = len.min(predecessor.saturating_add(1));
    if insertion_index > len {
        return None;
    }
    let _ = base_revision;
    let updated = insert_leaf_into_group(policy, working, group_id, insertion_index, mover)?;
    Some(DragPlacement { tree: updated })
}

fn drag_group_layouts(tree: &Node, leaf_rects: &BTreeMap<NodeId, Rect>) -> Vec<GroupLayout> {
    let mut out = Vec::new();
    collect_group_layouts(tree, leaf_rects, &mut out);
    out
}

fn collect_group_layouts(
    node: &Node,
    leaf_rects: &BTreeMap<NodeId, Rect>,
    out: &mut Vec<GroupLayout>,
) {
    if let Node::Group {
        id, axis, children, ..
    } = node
    {
        let mut leaves = Vec::new();
        collect_leaves_into(node, &mut leaves);
        let mut min_x: Option<i64> = None;
        let mut min_y: Option<i64> = None;
        let mut max_x: Option<i64> = None;
        let mut max_y: Option<i64> = None;
        for leaf in &leaves {
            let Some(rect) = leaf_rects.get(leaf) else {
                continue;
            };
            let x0 = i64::from(rect.x);
            let y0 = i64::from(rect.y);
            let x1 = x0 + i64::from(rect.w);
            let y1 = y0 + i64::from(rect.h);
            min_x = Some(min_x.map_or(x0, |m| m.min(x0)));
            min_y = Some(min_y.map_or(y0, |m| m.min(y0)));
            max_x = Some(max_x.map_or(x1, |m| m.max(x1)));
            max_y = Some(max_y.map_or(y1, |m| m.max(y1)));
        }
        if let (Some(x0), Some(y0), Some(x1), Some(y1)) = (min_x, min_y, max_x, max_y) {
            let rect = Rect {
                x: i32::try_from(x0).unwrap_or(i32::MIN),
                y: i32::try_from(y0).unwrap_or(i32::MIN),
                w: i32::try_from(x1 - x0).unwrap_or(0),
                h: i32::try_from(y1 - y0).unwrap_or(0),
            };
            let mut child_starts = Vec::with_capacity(children.len());
            for child in children {
                let mut c_leaves = Vec::new();
                collect_leaves_into(child, &mut c_leaves);
                let mut start: Option<i64> = None;
                for leaf in &c_leaves {
                    let Some(r) = leaf_rects.get(leaf) else {
                        continue;
                    };
                    let s = match axis {
                        Axis::Horizontal => i64::from(r.x),
                        Axis::Vertical => i64::from(r.y),
                    };
                    start = Some(start.map_or(s, |m| m.min(s)));
                }
                child_starts.push(start.unwrap_or(0));
            }
            out.push(GroupLayout {
                id: id.clone(),
                axis: *axis,
                rect,
                child_starts,
            });
        }
        for child in children {
            collect_group_layouts(child, leaf_rects, out);
        }
    }
}

fn collect_leaves_into(node: &Node, out: &mut Vec<NodeId>) {
    match node {
        Node::Leaf { id } => out.push(id.clone()),
        Node::Group { children, .. } => {
            for child in children {
                collect_leaves_into(child, out);
            }
        }
    }
}

fn group_rep_window(
    tree: &Node,
    group: &NodeId,
    leaves: &BTreeMap<WindowId, NodeId>,
    source_window: &WindowId,
) -> Option<WindowId> {
    let node = find_subtree(tree, group)?;
    for leaf in collect_leaves(node) {
        let Some(window) = leaves.iter().find_map(|(w, l)| (l == &leaf).then_some(w)) else {
            continue;
        };
        if window != source_window {
            return Some(window.clone());
        }
    }
    None
}
