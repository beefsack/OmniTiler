use std::collections::BTreeSet;

use crate::directional::{Node, NodeId};
use crate::session::DomainKey;

use super::state::{DomainLayout, WorldLayout};

/// Collect every node id (groups and leaves) of a tree.
pub(super) fn collect_node_ids(node: &Node, out: &mut BTreeSet<NodeId>) {
    out.insert(node.id().clone());
    if let Node::Group { children, .. } = node {
        for child in children {
            collect_node_ids(child, out);
        }
    }
}

/// Collect the leaf ids of a tree in child order.
pub(super) fn collect_leaves(node: &Node) -> Vec<NodeId> {
    let mut out = Vec::new();
    collect_leaves_into(node, &mut out);
    out
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

/// Leaf-only membership without collecting. Recurses through every group
/// child regardless of ids, so a malformed group whose id repeats a leaf id
/// still matches only real leaves. Isolated focus queries rely on this.
pub(super) fn tree_has_leaf(node: &Node, leaf: &NodeId) -> bool {
    match node {
        Node::Leaf { id } => id == leaf,
        Node::Group { children, .. } => children.iter().any(|child| tree_has_leaf(child, leaf)),
    }
}

/// Unique node ids and well-formed group shares within one domain.
fn validate_node_shares(node: &Node, seen: &mut BTreeSet<NodeId>) -> bool {
    if node.id().0.is_empty() || !seen.insert(node.id().clone()) {
        return false;
    }
    match node {
        Node::Leaf { .. } => true,
        Node::Group {
            children, shares, ..
        } => {
            if children.len() < 2 || shares.len() != children.len() {
                return false;
            }
            if shares.contains(&0) {
                return false;
            }
            let mut total: u64 = 0;
            for share in shares {
                match total.checked_add(*share) {
                    Some(next) => total = next,
                    None => return false,
                }
            }
            if total == 0 {
                return false;
            }
            children
                .iter()
                .all(|child| validate_node_shares(child, seen))
        }
    }
}

fn validate_domain_geometry(domain: &DomainLayout) -> bool {
    domain.bounds.w > 0
        && domain.bounds.h > 0
        && domain.bounds.x.checked_add(domain.bounds.w).is_some()
        && domain.bounds.y.checked_add(domain.bounds.h).is_some()
        && domain.gap >= 0
        && domain.outer_gap >= 0
}

/// Validate one retained domain: geometry, node identity/shares, reciprocal
/// leaf-to-window bindings, tiled/exception disjointness and a focus leaf
/// that resolves to a linked leaf or is empty. Node identity is scoped to
/// the domain, so independently retained trees may reuse group ids.
pub(super) fn validate_domain_topology(key: &DomainKey, domain: &DomainLayout) -> bool {
    if key.output.0.is_empty() || key.workspace.0.is_empty() {
        return false;
    }
    // Exception and restore-slot rows derive their home from the key: there
    // is no stored output/workspace to recheck, but their window ids must be
    // non-empty.
    if domain
        .exceptions
        .keys()
        .chain(domain.minimized_slots.keys())
        .any(|window| window.0.is_empty())
    {
        return false;
    }
    if !validate_domain_geometry(domain) {
        return false;
    }
    let mut node_ids = BTreeSet::new();
    if let Some(tree) = &domain.tree
        && !validate_node_shares(tree, &mut node_ids)
    {
        return false;
    }
    let valid_leaves: BTreeSet<NodeId> = domain
        .tree
        .as_ref()
        .map(|tree| collect_leaves(tree).into_iter().collect())
        .unwrap_or_default();

    let mut bound_leaves = BTreeSet::new();
    for (window, leaf) in &domain.leaves {
        if window.0.is_empty() || leaf.0.is_empty() {
            return false;
        }
        if !valid_leaves.contains(leaf) || !bound_leaves.insert(leaf.clone()) {
            return false;
        }
    }
    if valid_leaves.iter().any(|leaf| !bound_leaves.contains(leaf)) {
        return false;
    }
    if domain
        .exceptions
        .keys()
        .any(|window| domain.leaves.contains_key(window))
    {
        return false;
    }
    match &domain.focused_leaf {
        None => true,
        Some(leaf) => valid_leaves.contains(leaf) && bound_leaves.contains(leaf),
    }
}

impl WorldLayout {
    /// Node ids across every retained domain (group and leaf). Node identity
    /// is domain-scoped, so ids may repeat across domains.
    #[must_use]
    pub(super) fn all_node_ids(&self) -> BTreeSet<NodeId> {
        let mut ids = BTreeSet::new();
        for domain in self.domains.values() {
            if let Some(tree) = &domain.tree {
                collect_node_ids(tree, &mut ids);
            }
        }
        ids
    }

    /// Reject any retained domain with malformed topology. An empty layout
    /// has no malformed data and validates.
    #[must_use]
    pub(crate) fn validate_topology(&self) -> bool {
        self.domains
            .iter()
            .all(|(key, domain)| validate_domain_topology(key, domain))
    }
}
