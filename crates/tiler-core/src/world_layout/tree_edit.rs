//! Tree-mutation helpers for the sole world layout.
//!
//! Ported from the session tree-edit path so the world engine can grow and
//! contract a retained tree without private session state. Group allocation
//! burns serials even when the caller discards the candidate.

use std::collections::BTreeSet;

use super::group_alloc::GroupAllocator;
use crate::directional::{Axis, Node, NodeId, WindowId};
use crate::geometry::Rect;
use crate::policy::LayoutPolicy;

/// Mint one fresh group id from the generation-shared allocator, reserving it
/// in `existing`. Serials burn on trial and rollback and removed ids are never
/// reissued. Fails closed (`None`) on exhaustion.
pub(super) fn generate_group_id(
    alloc: &GroupAllocator,
    existing: &mut BTreeSet<NodeId>,
) -> Option<NodeId> {
    let minted = alloc.mint(existing)?;
    existing.insert(minted.clone());
    Some(minted)
}

/// Fresh leaf id for `window`, suffixing on collision with `existing`.
pub(super) fn generate_leaf_id(window: &WindowId, existing: &mut BTreeSet<NodeId>) -> NodeId {
    let base = format!("leaf-{}", window.0);
    if !existing.contains(&NodeId(base.clone())) {
        return NodeId(base);
    }
    let mut n = 1u32;
    loop {
        let candidate = NodeId(format!("{base}-{n}"));
        if !existing.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Whether any node in `node`'s subtree carries `leaf` as its id.
fn subtree_contains(node: &Node, leaf: &NodeId) -> bool {
    if node.id() == leaf {
        return true;
    }
    match node {
        Node::Leaf { .. } => false,
        Node::Group { children, .. } => children.iter().any(|c| subtree_contains(c, leaf)),
    }
}

/// Wrap the focused leaf in an ordered binary group with the new leaf.
fn nest_focused_with_new(
    policy: &dyn LayoutPolicy,
    node: Node,
    focused: &NodeId,
    new_leaf: Node,
    group_id: NodeId,
    axis: Axis,
) -> Option<Node> {
    match node {
        Node::Leaf { id } if &id == focused => Some(Node::Group {
            id: group_id,
            axis,
            children: vec![Node::Leaf { id }, new_leaf],
            shares: policy.new_group_shares().to_vec(),
        }),
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis: parent_axis,
            children,
            shares,
        } => {
            for (index, child) in children.iter().enumerate() {
                if subtree_contains(child, focused) {
                    let updated = nest_focused_with_new(
                        policy,
                        child.clone(),
                        focused,
                        new_leaf.clone(),
                        group_id.clone(),
                        axis,
                    )?;
                    let mut new_children = children.clone();
                    new_children[index] = updated;
                    return Some(Node::Group {
                        id,
                        axis: parent_axis,
                        children: new_children,
                        shares: shares.clone(),
                    });
                }
            }
            None
        }
    }
}

/// Admission target rect for a caller-validated eligible focus.
pub(super) fn admission_placement_for(
    tree: Option<&Node>,
    bounds: Rect,
    gap: i32,
    eligible: Option<&NodeId>,
) -> Rect {
    if let (Some(tree), Some(leaf)) = (tree, eligible)
        && let Ok(projected) = crate::geometry::project(tree, bounds, gap)
        && let Some(target) = projected.iter().find(|entry| &entry.leaf == leaf)
    {
        return target.rect;
    }
    bounds
}

/// Build the post-insertion tree for one admission leg. An absent tree yields
/// a root leaf; an absent eligible focus wraps the whole root old/new; an
/// eligible focus wraps just that leaf in an ordered binary group.
#[allow(clippy::too_many_arguments)]
pub(super) fn insert_tiled(
    policy: &dyn LayoutPolicy,
    alloc: &GroupAllocator,
    tree: Option<Node>,
    focused: Option<&NodeId>,
    new_leaf_id: NodeId,
    orientation: Axis,
    existing_ids: &mut BTreeSet<NodeId>,
) -> Option<Node> {
    let new_leaf = Node::Leaf { id: new_leaf_id };
    let Some(tree) = tree else {
        return Some(new_leaf);
    };
    let Some(focused) = focused else {
        let group_id = generate_group_id(alloc, existing_ids)?;
        return Some(Node::Group {
            id: group_id,
            axis: orientation,
            children: vec![tree, new_leaf],
            shares: policy.new_group_shares().to_vec(),
        });
    };
    let group_id = generate_group_id(alloc, existing_ids)?;
    nest_focused_with_new(policy, tree, focused, new_leaf, group_id, orientation)
}

/// Contract a retained tree after removing one leaf.
pub(super) fn remove_leaf_from_tree(
    policy: &dyn LayoutPolicy,
    tree: Option<Node>,
    leaf: &NodeId,
) -> Option<Node> {
    let node = tree?;
    remove_node(policy, node, leaf)
}

fn remove_node(policy: &dyn LayoutPolicy, node: Node, leaf: &NodeId) -> Option<Node> {
    match node {
        Node::Leaf { id } => {
            if &id == leaf {
                None
            } else {
                Some(Node::Leaf { id })
            }
        }
        Node::Group {
            id,
            axis,
            children,
            shares,
        } => {
            let original_len = children.len();
            let original_shares = shares.clone();
            let mut new_children = Vec::with_capacity(children.len());
            let mut removed_index: Option<usize> = None;
            for (index, child) in children.into_iter().enumerate() {
                if removed_index.is_none()
                    && child.id() == leaf
                    && matches!(child, Node::Leaf { .. })
                {
                    removed_index = Some(index);
                    continue;
                }
                match remove_node(policy, child, leaf) {
                    Some(updated) => {
                        // A changed subtree that kept its id keeps its slot
                        // share; an emptied subtree drops below.
                        new_children.push(updated);
                    }
                    None => {
                        // Subtree emptied: drop this slot entirely.
                        if removed_index.is_none() {
                            removed_index = Some(index);
                        } else {
                            // More than one slot vanished: fail closed.
                            return None;
                        }
                    }
                }
            }
            let Some(removed) = removed_index else {
                if new_children.len() != original_len {
                    return None;
                }
                return Some(Node::Group {
                    id,
                    axis,
                    children: new_children,
                    shares: original_shares,
                });
            };
            let _ = removed;
            match new_children.len() {
                0 => None,
                1 => Some(new_children.into_iter().next().expect("one child")),
                _ => {
                    let new_shares =
                        policy.proportional_removal_shares(&original_shares, removed)?;
                    if new_shares.len() != new_children.len() {
                        return None;
                    }
                    Some(Node::Group {
                        id,
                        axis,
                        children: new_children,
                        shares: new_shares,
                    })
                }
            }
        }
    }
}

pub(super) fn find_group(tree: &Node, id: &NodeId) -> Option<(Vec<Node>, Axis, NodeId)> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id: gid,
            axis,
            children,
            ..
        } => {
            if gid == id {
                return Some((children.clone(), *axis, gid.clone()));
            }
            for child in children {
                if let Some(found) = find_group(child, id) {
                    return Some(found);
                }
            }
            None
        }
    }
}

pub(super) fn find_group_shares(tree: &Node, id: &NodeId) -> Option<Vec<u64>> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id: gid,
            children,
            shares,
            ..
        } => {
            if gid == id {
                return Some(shares.clone());
            }
            for child in children {
                if let Some(found) = find_group_shares(child, id) {
                    return Some(found);
                }
            }
            None
        }
    }
}

pub(super) fn find_group_full(
    tree: &Node,
    id: &NodeId,
) -> Option<(Vec<Node>, Vec<u64>, NodeId, Axis)> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id: gid,
            axis,
            children,
            shares,
        } => {
            if gid == id {
                return Some((children.clone(), shares.clone(), gid.clone(), *axis));
            }
            for child in children {
                if let Some(found) = find_group_full(child, id) {
                    return Some(found);
                }
            }
            None
        }
    }
}

pub(super) fn find_subtree<'a>(tree: &'a Node, id: &NodeId) -> Option<&'a Node> {
    if tree.id() == id {
        return Some(tree);
    }
    match tree {
        Node::Leaf { .. } => None,
        Node::Group { children, .. } => children.iter().find_map(|c| find_subtree(c, id)),
    }
}

pub(super) fn direct_parent_of_leaf(tree: &Node, leaf: &NodeId) -> Option<NodeId> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group { id, children, .. } => {
            for child in children {
                if child.id() == leaf && matches!(child, Node::Leaf { .. }) {
                    return Some(id.clone());
                }
            }
            for child in children {
                if let Some(found) = direct_parent_of_leaf(child, leaf) {
                    return Some(found);
                }
            }
            None
        }
    }
}

pub(super) fn parent_of_group(tree: &Node, group: &NodeId) -> Option<NodeId> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group { id, children, .. } => {
            for child in children {
                if child.id() == group {
                    return Some(id.clone());
                }
            }
            for child in children {
                if let Some(found) = parent_of_group(child, group) {
                    return Some(found);
                }
            }
            None
        }
    }
}

pub(super) fn replace_child_at(
    tree: &Node,
    group_id: &NodeId,
    index: usize,
    replacement: Node,
    replacement_share: u64,
) -> Option<Node> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            children,
            shares,
        } => {
            if id == group_id {
                if index >= children.len() {
                    return None;
                }
                let mut new_children = children.clone();
                let mut new_shares = shares.clone();
                new_children[index] = replacement;
                new_shares[index] = replacement_share;
                return Some(Node::Group {
                    id: id.clone(),
                    axis: *axis,
                    children: new_children,
                    shares: new_shares,
                });
            }
            for (pos, child) in children.iter().enumerate() {
                if child.id() == group_id || subtree_contains(child, group_id) {
                    let updated =
                        replace_child_at(child, group_id, index, replacement, replacement_share)?;
                    let mut new_children = children.clone();
                    new_children[pos] = updated;
                    return Some(Node::Group {
                        id: id.clone(),
                        axis: *axis,
                        children: new_children,
                        shares: shares.clone(),
                    });
                }
            }
            None
        }
    }
}

pub(super) fn insert_leaf_into_group(
    policy: &dyn LayoutPolicy,
    tree: Node,
    group_id: &NodeId,
    index: usize,
    leaf: Node,
) -> Option<Node> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            mut children,
            shares,
        } => {
            if &id == group_id {
                if index > children.len() {
                    return None;
                }
                let new_shares = policy.proportional_insertion_shares(&shares, index)?;
                children.insert(index, leaf);
                return Some(Node::Group {
                    id,
                    axis,
                    children,
                    shares: new_shares,
                });
            }
            for (pos, child) in children.iter().enumerate() {
                if child.id() == group_id || subtree_contains(child, group_id) {
                    let updated =
                        insert_leaf_into_group(policy, child.clone(), group_id, index, leaf)?;
                    children[pos] = updated;
                    return Some(Node::Group {
                        id,
                        axis,
                        children,
                        shares,
                    });
                }
            }
            None
        }
    }
}
