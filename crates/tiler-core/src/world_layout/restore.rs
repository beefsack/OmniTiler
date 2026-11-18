//! Best-effort minimize rejoin for the sole world layout.
//!
//! Ported from the session rejoin path so a stored restore slot can rejoin a
//! current domain tree using only local domain leaf bindings, with no
//! output/workspace or link copies and no session state machine dependency.

use std::collections::{BTreeMap, BTreeSet};

use super::group_alloc::GroupAllocator;
use crate::directional::{Axis, Node, NodeId, WindowId};

use super::state::DomainRestoreSlot;
use super::topology::{collect_leaves, tree_has_leaf};
use super::tree_edit::generate_group_id;

#[cfg(test)]
#[path = "restore_tests.rs"]
mod tests;

/// Whether any node in `node`'s subtree carries `id` as its node id.
/// Used to locate a group by id, so group ids match as well as leaf ids.
fn subtree_contains_id(node: &Node, id: &NodeId) -> bool {
    if node.id() == id {
        return true;
    }
    match node {
        Node::Leaf { .. } => false,
        Node::Group { children, .. } => children.iter().any(|c| subtree_contains_id(c, id)),
    }
}

/// Find a group by id, returning its children, shares, id and axis.
fn find_group_full(tree: &Node, id: &NodeId) -> Option<(Vec<Node>, Vec<u64>, NodeId, Axis)> {
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

/// Immediate parent group of a leaf: group id, axis, child index of the
/// direct slot containing the leaf, and group shares. `None` when the leaf
/// is the root or absent.
fn find_parent_group(tree: &Node, leaf: &NodeId) -> Option<(NodeId, Axis, usize, Vec<u64>)> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            children,
            shares,
        } => {
            for (index, child) in children.iter().enumerate() {
                if tree_has_leaf(child, leaf) {
                    if matches!(child, Node::Leaf { .. }) {
                        return Some((id.clone(), *axis, index, shares.clone()));
                    }
                    return find_parent_group(child, leaf);
                }
            }
            None
        }
    }
}

/// Exact integer greatest common divisor for share normalization.
fn gcd_pair(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

/// Divide every share by the exact common divisor when above one (always
/// exact by GCD construction; ratios and fractions are preserved).
fn normalize_shares(shares: &mut [u64]) {
    let mut divisor = 0u64;
    for share in shares.iter() {
        divisor = gcd_pair(divisor, *share);
        if divisor == 1 {
            break;
        }
    }
    if divisor > 1 {
        for share in shares.iter_mut() {
            *share /= divisor;
        }
    }
}

/// Exact integer greatest common divisor over wide share products (the
/// scaled restore products can exceed `u64` before reduction).
fn gcd_wide(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

/// Full inserted share vector preserving the stored old fraction
/// `old_share / old_total` against current survivor `shares`, entrant at
/// `index`: the entrant takes `old_share * current_total` while each
/// survivor scales by the stored remainder, GCD-reduced. The current
/// weights rescale up to the entrant instead of rounding the entrant down
/// to the current unit, so a skewed old fraction stays exact (for example
/// old 1/10 against current `[1, 1]` rejoins `[2, 9, 9]`, never `[1, 1,
/// 1]`), and survivor ratios are preserved exactly. `None` on
/// empty/zero input, `u64` overflow of any entry, or `u64` overflow of the
/// combined total (group totals must not overflow; callers fall back to
/// fresh admission).
fn comparable_inserted_shares(
    old_share: u64,
    old_total: u128,
    current: &[u64],
    index: usize,
) -> Option<Vec<u64>> {
    if old_share == 0
        || old_total <= old_share as u128
        || current.is_empty()
        || current.contains(&0)
    {
        return None;
    }
    if index > current.len() {
        return None;
    }
    let rest = old_total - old_share as u128;
    let total: u128 = current.iter().map(|share| *share as u128).sum();
    if total == 0 {
        return None;
    }
    let mut scaled: Vec<u128> = Vec::with_capacity(current.len() + 1);
    scaled.push((old_share as u128) * total);
    for share in current {
        scaled.push((*share as u128) * rest);
    }
    let mut divisor: u128 = 0;
    for value in &scaled {
        divisor = gcd_wide(divisor, *value);
        if divisor == 1 {
            break;
        }
    }
    if divisor == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(scaled.len());
    for value in scaled {
        out.push(u64::try_from(value / divisor).ok()?);
    }
    let entrant = out.remove(0);
    out.insert(index, entrant);
    // Group totals must not overflow even when every entry fits: a
    // representable-components/nonrepresentable-total vector falls back
    // instead of producing an invalid tree.
    let _: u64 = out
        .iter()
        .try_fold(0u64, |total, share| total.checked_add(*share))?;
    Some(out)
}

/// Insert a leaf into the named group at `index` carrying caller-computed
/// `shares` (length must equal children plus one, all positive). Structural
/// twin of ordinary proportional admission for rejoins whose entrant share
/// preserves the stored old fraction instead of the proportional average.
fn insert_leaf_with_shares(
    tree: Node,
    group_id: &NodeId,
    index: usize,
    leaf: Node,
    shares: Vec<u64>,
) -> Option<Node> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            mut children,
            shares: current,
        } => {
            if &id == group_id {
                if index > children.len()
                    || shares.len() != children.len() + 1
                    || shares.contains(&0)
                {
                    return None;
                }
                children.insert(index, leaf);
                return Some(Node::Group {
                    id,
                    axis,
                    children,
                    shares,
                });
            }
            for (pos, child) in children.iter().enumerate() {
                if child.id() == group_id || subtree_contains_id(child, group_id) {
                    let updated = insert_leaf_with_shares(
                        child.clone(),
                        group_id,
                        index,
                        leaf.clone(),
                        shares.clone(),
                    )?;
                    children[pos] = updated;
                    return Some(Node::Group {
                        id,
                        axis,
                        children,
                        shares: current,
                    });
                }
            }
            None
        }
    }
}

/// Replace a leaf with a new subtree, preserving the leaf's slot share in
/// its parent (the parent keeps its children count and shares; the new
/// subtree carries its own internal shares).
fn replace_leaf(tree: Node, leaf: &NodeId, node: Node) -> Option<Node> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            mut children,
            shares,
        } => {
            if let Some(pos) = children
                .iter()
                .position(|child| matches!(child, Node::Leaf { id: leaf_id } if leaf_id == leaf))
            {
                children[pos] = node;
                return Some(Node::Group {
                    id,
                    axis,
                    children,
                    shares,
                });
            }
            for (pos, child) in children.iter().enumerate() {
                if tree_has_leaf(child, leaf) {
                    let updated = replace_leaf(child.clone(), leaf, node.clone())?;
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

/// Best-effort minimize rejoin: return the current domain tree with the
/// stored leaf rejoined in its old neighborhood, or `None` for fresh
/// ordinary admission.
///
/// Stored context only (the pre-minimize tree plus the removed leaf; no
/// tree-identity prerequisite): the anchor is the live surviving former
/// leaf nearest the stored leaf in old leaf order, preferring
/// same-old-parent siblings. A survivor is live only while its leaf id is
/// currently bound to a window in this domain, so stale or reused ids
/// never anchor. The rejoin preserves the old side (before/after the
/// anchor). A surviving old group (same id and axis, still holding a live
/// survivor) takes an insert at the clamped old index with an entrant share
/// preserving the stored old fraction against the current total; otherwise
/// the anchor leaf regroups into a new group on the stored axis and side,
/// splitting the stored old share against the anchor's old share
/// (same-parent anchor) or the stored remainder. Resizes, admissions,
/// sibling closes with a survivor, swaps, other minimize/restore roundtrips,
/// and gaps/resolution changes therefore do not alone invalidate: only an
/// absent live-neighbor set (or an unusable leaf id) returns `None`. Callers
/// gate homing/domain liveness and the exact fast path before this.
pub(super) fn best_effort_rejoin(
    alloc: &GroupAllocator,
    slot: &DomainRestoreSlot,
    current: &Option<Node>,
    leaves: &BTreeMap<WindowId, NodeId>,
    existing_ids: &mut BTreeSet<NodeId>,
) -> Option<Node> {
    if existing_ids.contains(&slot.leaf) {
        return None;
    }
    let current = current.clone()?;
    let old_order = collect_leaves(&slot.tree);
    let restored_pos = old_order.iter().position(|leaf| leaf == &slot.leaf)?;
    let (old_group, old_axis, old_index, old_shares) = find_parent_group(&slot.tree, &slot.leaf)?;
    let old_share = *old_shares.get(old_index)?;
    // Wide sum: share inputs are `u64` and their total can overflow it.
    let old_total: u128 = old_shares.iter().map(|share| *share as u128).sum();
    if old_total <= old_share as u128 {
        return None;
    }
    // A live survivor is a former leaf (never the restored leaf itself)
    // currently bound to a window in this domain.
    let live = |leaf: &NodeId| leaf != &slot.leaf && leaves.values().any(|bound| bound == leaf);
    let mut survivors: Vec<(usize, NodeId)> = old_order
        .iter()
        .enumerate()
        .filter(|(_, leaf)| live(leaf))
        .map(|(pos, leaf)| (pos, leaf.clone()))
        .collect();
    if survivors.is_empty() {
        return None;
    }
    // Surviving old group (same id and axis, still holding a live survivor;
    // the id check alone could match a reused generated id): insert at the
    // clamped old index with a comparable entrant share.
    if let Some((children, shares, _, axis)) = find_group_full(&current, &old_group)
        && axis == old_axis
        && children
            .iter()
            .flat_map(collect_leaves)
            .any(|leaf| live(&leaf))
    {
        let index = old_index.min(children.len());
        let rejoined = comparable_inserted_shares(old_share, old_total, &shares, index)?;
        let rejoined = insert_leaf_with_shares(
            current,
            &old_group,
            index,
            Node::Leaf {
                id: slot.leaf.clone(),
            },
            rejoined,
        )?;
        existing_ids.insert(slot.leaf.clone());
        return Some(rejoined);
    }
    // Same-old-parent siblings first (sibling regroup), else the nearest
    // live surviving former leaf in old leaf order (deterministic tie-break).
    let same_parent: Vec<(usize, NodeId)> = survivors
        .iter()
        .filter(|(_, leaf)| {
            find_parent_group(&slot.tree, leaf).is_some_and(|(group, _, _, _)| group == old_group)
        })
        .cloned()
        .collect();
    if !same_parent.is_empty() {
        survivors = same_parent;
    }
    survivors.sort_by_key(|(pos, _)| (pos.abs_diff(restored_pos), *pos));
    let (anchor_pos, anchor) = survivors.into_iter().next().expect("nonempty survivors");
    let before = restored_pos < anchor_pos;
    // Anchor-side share: the anchor's own old share for a same-parent
    // anchor, else the stored remainder as the faithful proxy. The
    // remainder is wide arithmetic; an unrepresentable proxy falls back to
    // fresh admission rather than truncating the fraction.
    let anchor_share = find_parent_group(&slot.tree, &anchor)
        .filter(|(group, _, _, _)| group == &old_group)
        .and_then(|(_, _, index, shares)| shares.get(index).copied())
        .or_else(|| u64::try_from(old_total - old_share as u128).ok())?;
    let mut regroup = if before {
        vec![old_share, anchor_share]
    } else {
        vec![anchor_share, old_share]
    };
    normalize_shares(&mut regroup);
    let group_id = generate_group_id(alloc, existing_ids)?;
    existing_ids.insert(slot.leaf.clone());
    let children = if before {
        vec![
            Node::Leaf {
                id: slot.leaf.clone(),
            },
            Node::Leaf { id: anchor.clone() },
        ]
    } else {
        vec![
            Node::Leaf { id: anchor.clone() },
            Node::Leaf {
                id: slot.leaf.clone(),
            },
        ]
    };
    let regroup = Node::Group {
        id: group_id,
        axis: old_axis,
        children,
        shares: regroup,
    };
    if current == (Node::Leaf { id: anchor.clone() }) {
        return Some(regroup);
    }
    replace_leaf(current, &anchor, regroup)
}
