//! Direct, typed synchronous single-domain directional move (R1-R3) over
//! `WorldLayout`.
//!
//! Callers own convergence, fresh seeding, pair/cross routing and raw command
//! decoding. This module runs one R1-R3 move on a local domain candidate and
//! installs the result atomically on success.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::MAX_REVISION;
use crate::directional::{
    Axis, Capabilities, Direction, EscapeContinuation, FocusedSide, Insertion, MoveIntent,
    MoveOperation, MoveOutcome, MovePlan, Node, NodeId, Output, Precondition, RejectionKind, Rule,
    SameAxisMove, Snapshot, WindowId, WindowLink,
};
use crate::policy::LayoutPolicy;
use crate::session::{DesiredGeometry, DomainKey, GroupAllocator, ObservedWindow, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::focus::{settle_domain_focus_tail, sync_domain_focus_from_window};
use super::observation::{domain_observed_known_match, valid_observed_shapes};
use super::outcome::{LayoutFailure, LocalMoveOutcome};
use super::projection::{project_borrowed_geometry, sort_geometry};
use super::state::{DomainLayout, WorldLayout};
use super::topology::{collect_node_ids, validate_domain_topology};
use super::tree_edit::{generate_group_id, remove_leaf_from_tree};

impl WorldLayout {
    /// Plan and apply one local (R1-R3) directional move for `subject` in
    /// converged domain `key`.
    ///
    /// `observed_focus` is synced on a local candidate before focus checks so
    /// a refusal leaves all domain state untouched (only generation-shared
    /// group allocator serials burned during trial tree edits survive).
    pub(crate) fn local_move(
        &mut self,
        key: &DomainKey,
        subject: &WindowId,
        direction: Direction,
        same_axis_move: SameAxisMove,
        observed_focus: &WindowId,
        observed: &[ObservedWindow],
    ) -> Result<LocalMoveOutcome, LayoutFailure> {
        let refuse = LayoutFailure::Semantics;

        let Some(original) = self.domain(key) else {
            return Err(refuse(RefusalKind::UnknownDomain));
        };
        if !validate_domain_topology(key, original) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        if !valid_observed_shapes(observed) {
            return Err(refuse(RefusalKind::MalformedInput));
        }
        for entry in observed {
            if entry.output != key.output || entry.workspace != key.workspace {
                return Err(refuse(RefusalKind::CrossDomainMismatch));
            }
        }

        let mut candidate = original.clone();

        let known: BTreeSet<&WindowId> = candidate
            .leaves
            .keys()
            .chain(candidate.exceptions.keys())
            .collect();
        let observed_ids: BTreeSet<&WindowId> = observed.iter().map(|w| &w.window).collect();
        if observed_ids != known {
            return Err(refuse(RefusalKind::PartialObservation));
        }
        if !domain_observed_known_match(key, &candidate, observed, self.config.fixed_size_admission)
        {
            return Err(refuse(RefusalKind::PartialObservation));
        }

        if subject.0.is_empty() {
            return Err(refuse(RefusalKind::MalformedInput));
        }
        if !candidate.leaves.contains_key(subject) && !candidate.exceptions.contains_key(subject) {
            return Err(refuse(RefusalKind::UnknownWindow));
        }
        if candidate.exceptions.contains_key(subject) {
            return Err(refuse(RefusalKind::NotTiled));
        }

        let focused_leaf = sync_domain_focus_from_window(&mut candidate, observed_focus)
            .or_else(|| candidate.focused_leaf.clone());
        let Some(focused_leaf) = focused_leaf else {
            return Err(refuse(RefusalKind::FocusMismatch));
        };
        let focused_window = candidate
            .leaves
            .iter()
            .find_map(|(window, leaf)| (leaf == &focused_leaf).then(|| window.clone()));
        let Some(focused_window) = focused_window else {
            return Err(refuse(RefusalKind::NotTiled));
        };
        if candidate.exceptions.contains_key(&focused_window) {
            return Err(refuse(RefusalKind::NotTiled));
        }
        if subject != &focused_window {
            return Err(refuse(RefusalKind::FocusMismatch));
        }

        let snapshot = single_domain_move_snapshot(key, &candidate);
        let intent = MoveIntent {
            source_output: key.output.clone(),
            focused_leaf: focused_leaf.clone(),
            focused_window: focused_window.clone(),
            direction,
            same_axis_move,
        };
        let plan = match self
            .policy
            .plan_move(&snapshot, &intent, &Capabilities::full())
        {
            MoveOutcome::Planned(plan) => plan,
            MoveOutcome::Noop { .. } => {
                return Err(refuse(RefusalKind::PlannerNoop));
            }
            MoveOutcome::Rejected { reason } => {
                if reason.kind == RejectionKind::UnsupportedTopology {
                    return Err(refuse(RefusalKind::UnsupportedCapability));
                }
                return Err(refuse(RefusalKind::PlannerRejected));
            }
        };

        let Some(tree) = candidate.tree.take() else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        let Some(updated_tree) = apply_local_move_operation(
            self.policy.as_ref(),
            &self.group_alloc,
            tree,
            &candidate.leaves,
            key,
            &focused_leaf,
            direction,
            &plan,
        ) else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        candidate.tree = Some(updated_tree);
        if !validate_domain_topology(key, &candidate) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        let hints: BTreeMap<WindowId, WindowSizeHints> = observed
            .iter()
            .map(|entry| (entry.window.clone(), entry.hints))
            .collect();
        let mut desired_geometry = project_borrowed_geometry(
            key,
            candidate.tree.as_ref().expect("tree set above"),
            &candidate.leaves,
            candidate.bounds,
            candidate.gap,
            &hints,
        )
        .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
        sort_geometry(&mut desired_geometry);
        if !geometry_covers_domain(&desired_geometry, &candidate.leaves, key) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        let base = original.accepted_revision;
        if base >= MAX_REVISION {
            return Err(LayoutFailure::RevisionLimit {
                base,
                limit: MAX_REVISION,
            });
        }
        let revision = base.checked_add(1).ok_or(LayoutFailure::RevisionLimit {
            base,
            limit: MAX_REVISION,
        })?;

        let stack_base = candidate.focus_stack.clone();
        let current_last_active = candidate.last_active.clone();
        settle_domain_focus_tail(
            &mut candidate,
            &stack_base,
            Some(focused_leaf.clone()),
            current_last_active.as_ref(),
        );
        WorldLayout::prune_domain_fixed_size_state(&mut candidate);
        WorldLayout::prune_domain_capacity_state(&mut candidate);
        candidate.accepted_revision = revision;

        *self.domains.get_mut(key).expect("validated domain") = candidate;

        Ok(LocalMoveOutcome {
            base_revision: base,
            revision,
            rule: plan.rule,
            capability: plan.required_capability,
            direction,
            window: focused_window,
            focus_leaf: focused_leaf,
            geometry: desired_geometry,
        })
    }
}

fn single_domain_move_snapshot(key: &DomainKey, domain: &DomainLayout) -> Snapshot {
    let output = Output {
        id: key.output.clone(),
        workspace: key.workspace.clone(),
        tree: domain.tree.clone(),
        adjacent: BTreeMap::new(),
    };
    let windows = domain
        .leaves
        .iter()
        .map(|(window, leaf)| WindowLink {
            window: window.clone(),
            leaf: leaf.clone(),
            output: key.output.clone(),
            workspace: key.workspace.clone(),
        })
        .collect();
    Snapshot {
        outputs: vec![output],
        windows,
    }
}

pub(super) fn geometry_covers_domain(
    geometry: &[DesiredGeometry],
    leaves: &BTreeMap<WindowId, NodeId>,
    key: &DomainKey,
) -> bool {
    let expected: BTreeSet<&NodeId> = leaves.values().collect();
    let got: BTreeSet<&NodeId> = geometry
        .iter()
        .filter(|g| g.output == key.output && g.workspace == key.workspace)
        .map(|g| &g.leaf)
        .collect();
    if expected != got {
        return false;
    }
    for entry in geometry
        .iter()
        .filter(|g| g.output == key.output && g.workspace == key.workspace)
    {
        if entry.window.0.is_empty() || entry.leaf.0.is_empty() {
            return false;
        }
        if entry.rect.w <= 0 || entry.rect.h <= 0 {
            return false;
        }
        if leaves.get(&entry.window) != Some(&entry.leaf) {
            return false;
        }
    }
    for entry in geometry {
        if entry.output != key.output || entry.workspace != key.workspace {
            return false;
        }
    }
    true
}

fn step_for_direction(direction: Direction) -> i32 {
    match direction {
        Direction::Right | Direction::Down => 1,
        Direction::Left | Direction::Up => -1,
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_local_move_operation(
    policy: &dyn LayoutPolicy,
    alloc: &GroupAllocator,
    tree: Node,
    leaves: &BTreeMap<WindowId, NodeId>,
    source: &DomainKey,
    focused_leaf: &NodeId,
    direction: Direction,
    plan: &MovePlan,
) -> Option<Node> {
    let operation = &plan.operation;
    if plan.rule != operation.rule() {
        return None;
    }
    if plan.required_capability != operation.required_capability() {
        return None;
    }
    if plan.preconditions != operation.preconditions() {
        return None;
    }
    if plan.preconditions.len() > crate::contract::MAX_PRECONDITIONS {
        return None;
    }
    if !plan
        .preconditions
        .contains(&Precondition::AdapterMustVerifyPostconditions)
    {
        return None;
    }
    let canonical_ok = match operation {
        MoveOperation::WrapPerpendicular { .. } => plan.rule == Rule::R1,
        MoveOperation::SwapNeighbor { .. } => plan.rule == Rule::R2a || plan.rule == Rule::R2c,
        MoveOperation::InsertIntoGroup { .. } => plan.rule == Rule::R2b,
        MoveOperation::SplitGroupChild { .. } => plan.rule == Rule::R2b,
        MoveOperation::WrapNeighbor { .. } => plan.rule == Rule::R2c,
        MoveOperation::EscapeParent { .. } => plan.rule == Rule::R3,
        MoveOperation::CrossOutput { .. } => false,
    };
    if !canonical_ok || operation.rule() != plan.rule {
        return None;
    }
    if plan.intent.source_output != source.output {
        return None;
    }
    if plan.intent.focused_leaf != *focused_leaf {
        return None;
    }
    if plan.intent.direction != direction {
        return None;
    }
    let mover_window = leaves
        .iter()
        .find_map(|(window, leaf)| (leaf == focused_leaf).then(|| window.clone()))?;
    if plan.intent.focused_window != mover_window {
        return None;
    }

    let mut node_ids = BTreeSet::new();
    collect_node_ids(&tree, &mut node_ids);
    let mover_leaf_node = Node::Leaf {
        id: focused_leaf.clone(),
    };
    let direction_axis = Axis::for_direction(direction);
    let step = step_for_direction(direction);

    match operation {
        MoveOperation::WrapPerpendicular {
            container, axis, ..
        } => {
            if *axis != direction_axis {
                return None;
            }
            if !find_group_id(&tree, container) {
                return None;
            }
            let (container_children, container_axis, container_id) = find_group(&tree, container)?;
            if container_axis == direction_axis {
                return None;
            }
            if direct_parent_of_leaf(&tree, focused_leaf)? != *container {
                return None;
            }
            let pos = container_children
                .iter()
                .position(|c| c.id() == focused_leaf)?;
            if !matches!(container_children[pos], Node::Leaf { .. }) {
                return None;
            }
            let mut remaining_children = container_children;
            let mut remaining_shares = find_group_shares(&tree, container)?;
            remaining_children.remove(pos);
            remaining_shares.remove(pos);
            let remainder: Node = if remaining_children.len() == 1 {
                remaining_children.into_iter().next()?
            } else if remaining_children.is_empty() {
                return None;
            } else {
                Node::Group {
                    id: container_id,
                    axis: container_axis,
                    children: remaining_children,
                    shares: remaining_shares,
                }
            };
            let new_id = generate_group_id(alloc, &mut node_ids)?;
            let new_group = if step == -1 {
                Node::Group {
                    id: new_id,
                    axis: *axis,
                    children: vec![mover_leaf_node, remainder],
                    shares: policy.new_group_shares().to_vec(),
                }
            } else {
                Node::Group {
                    id: new_id,
                    axis: *axis,
                    children: vec![remainder, mover_leaf_node],
                    shares: policy.new_group_shares().to_vec(),
                }
            };
            replace_node_by_id(tree, container, new_group)
        }
        MoveOperation::SwapNeighbor {
            container,
            neighbor,
            ..
        } => {
            if plan.rule == Rule::R2c
                && operation.rule() == Rule::R2c
                && plan.intent.same_axis_move == SameAxisMove::SwapWithNeighbor
            {
                let (children, caxis, _) = find_group(&tree, container)?;
                let shares = find_group_shares(&tree, container)?;
                if children.len() < 3 || caxis != direction_axis {
                    return None;
                }
                if direct_parent_of_leaf(&tree, focused_leaf)? != *container {
                    return None;
                }
                let iw = children.iter().position(|c| c.id() == focused_leaf)?;
                let ineighbor = children.iter().position(|c| c.id() == neighbor)?;
                if (iw as i32 - ineighbor as i32).abs() != 1 {
                    return None;
                }
                if ineighbor as i32 != iw as i32 + step {
                    return None;
                }
                if !matches!(children[iw], Node::Leaf { .. })
                    || !matches!(children[ineighbor], Node::Leaf { .. })
                {
                    return None;
                }
                if shares.len() != children.len() {
                    return None;
                }
                return swap_direct_children(tree, container, focused_leaf, neighbor);
            }
            if plan.rule != Rule::R2a || operation.rule() != Rule::R2a {
                return None;
            }
            let (children, caxis, _) = find_group(&tree, container)?;
            let shares = find_group_shares(&tree, container)?;
            if children.len() != 2 || caxis != direction_axis {
                return None;
            }
            if direct_parent_of_leaf(&tree, focused_leaf)? != *container {
                return None;
            }
            let iw = children.iter().position(|c| c.id() == focused_leaf)?;
            let ineighbor = children.iter().position(|c| c.id() == neighbor)?;
            if (iw as i32 - ineighbor as i32).abs() != 1 {
                return None;
            }
            if ineighbor as i32 != iw as i32 + step {
                return None;
            }
            if !matches!(children[iw], Node::Leaf { .. })
                || !matches!(children[ineighbor], Node::Leaf { .. })
            {
                return None;
            }
            if shares.len() != 2 {
                return None;
            }
            swap_direct_children(tree, container, focused_leaf, neighbor)
        }
        MoveOperation::WrapNeighbor {
            container,
            neighbor,
            focused_before_neighbor,
            axis,
            ..
        } => {
            if *axis != direction_axis {
                return None;
            }
            let (children, shares, cid, caxis) = find_group_full(&tree, container)?;
            if children.len() < 3 || caxis != direction_axis || caxis != *axis {
                return None;
            }
            if direct_parent_of_leaf(&tree, focused_leaf)? != *container {
                return None;
            }
            let iw = children.iter().position(|c| c.id() == focused_leaf)?;
            let ineighbor = children.iter().position(|c| c.id() == neighbor)?;
            if (iw as i32 - ineighbor as i32).abs() != 1 {
                return None;
            }
            if ineighbor as i32 != iw as i32 + step {
                return None;
            }
            if *focused_before_neighbor != (iw < ineighbor) {
                return None;
            }
            if plan.intent.same_axis_move == SameAxisMove::SwapWithNeighbor
                && matches!(children.get(ineighbor), Some(Node::Leaf { .. }))
            {
                return None;
            }
            let first = iw.min(ineighbor);
            let pair_share: u64 = shares[iw].checked_add(shares[ineighbor])?;
            let w_node = children[iw].clone();
            let s_node = children[ineighbor].clone();
            if w_node.id() != focused_leaf {
                return None;
            }
            let mut new_children = children;
            let mut new_shares = shares;
            let hi = iw.max(ineighbor);
            let lo = iw.min(ineighbor);
            new_children.remove(hi);
            new_shares.remove(hi);
            new_children.remove(lo);
            new_shares.remove(lo);
            let new_id = generate_group_id(alloc, &mut node_ids)?;
            let wrapper = if *focused_before_neighbor {
                Node::Group {
                    id: new_id,
                    axis: *axis,
                    children: vec![w_node, s_node],
                    shares: policy.new_group_shares().to_vec(),
                }
            } else {
                Node::Group {
                    id: new_id,
                    axis: *axis,
                    children: vec![s_node, w_node],
                    shares: policy.new_group_shares().to_vec(),
                }
            };
            new_children.insert(first, wrapper);
            new_shares.insert(first, pair_share);
            replace_group_children(tree, &cid, new_children, new_shares)
        }
        MoveOperation::InsertIntoGroup {
            container,
            target_group,
            insertion_index,
            insertion,
            ..
        } => {
            let (container_children, container_axis, _) = find_group(&tree, container)?;
            if container_children.len() != 2 || container_axis != direction_axis {
                return None;
            }
            if direct_parent_of_leaf(&tree, focused_leaf)? != *container {
                return None;
            }
            let iw = container_children
                .iter()
                .position(|c| c.id() == focused_leaf)?;
            let ineighbor = (iw as i32 + step) as usize;
            let neighbor_node = container_children.get(ineighbor)?;
            if neighbor_node.id() != target_group || !matches!(neighbor_node, Node::Group { .. }) {
                return None;
            }
            let (t_children, t_axis, _) = find_group(&tree, target_group)?;
            if t_children.len() < 2 {
                return None;
            }
            if t_axis == container_axis {
                if *insertion != Insertion::NearEdge {
                    return None;
                }
                let expected = if iw < ineighbor { 0 } else { t_children.len() };
                if *insertion_index != expected {
                    return None;
                }
            } else {
                if t_children.len() % 2 != 0
                    || *insertion != Insertion::Midpoint
                    || *insertion_index != t_children.len() / 2
                {
                    return None;
                }
            }
            let working = remove_leaf_from_tree(policy, Some(tree), focused_leaf)?;
            let (post_children, _, _) = find_group(&working, target_group)?;
            if post_children.len() != t_children.len() || *insertion_index > post_children.len() {
                return None;
            }
            insert_leaf_into_group(
                policy,
                working,
                target_group,
                *insertion_index,
                mover_leaf_node,
            )
        }
        MoveOperation::SplitGroupChild {
            container,
            target_group,
            target_child,
            target_child_index,
            focused_side,
            axis,
            ..
        } => {
            if *axis != direction_axis {
                return None;
            }
            let (container_children, container_axis, _) = find_group(&tree, container)?;
            if container_children.len() != 2 || container_axis != direction_axis {
                return None;
            }
            if direct_parent_of_leaf(&tree, focused_leaf)? != *container {
                return None;
            }
            let iw = container_children
                .iter()
                .position(|c| c.id() == focused_leaf)?;
            let ineighbor = (iw as i32 + step) as usize;
            let neighbor_node = container_children.get(ineighbor)?;
            if neighbor_node.id() != target_group {
                return None;
            }
            let (t_children, t_axis, _) = find_group(&tree, target_group)?;
            if t_axis == container_axis
                || t_children.len() % 2 == 0
                || t_children.len() < 3
                || *target_child_index != t_children.len() / 2
                || t_children[*target_child_index].id() != target_child
            {
                return None;
            }
            let expected_side = if step == -1 {
                FocusedSide::First
            } else {
                FocusedSide::Second
            };
            if *focused_side != expected_side {
                return None;
            }
            let working = remove_leaf_from_tree(policy, Some(tree), focused_leaf)?;
            let (children, _, _, _) = find_group_full(&working, target_group)?;
            if *target_child_index >= children.len()
                || children[*target_child_index].id() != target_child
            {
                return None;
            }
            let victim = children[*target_child_index].clone();
            let victim_share = find_group_shares(&working, target_group)?[*target_child_index];
            let new_id = generate_group_id(alloc, &mut node_ids)?;
            let split = match focused_side {
                FocusedSide::First => Node::Group {
                    id: new_id,
                    axis: *axis,
                    children: vec![mover_leaf_node, victim],
                    shares: policy.new_group_shares().to_vec(),
                },
                FocusedSide::Second => Node::Group {
                    id: new_id,
                    axis: *axis,
                    children: vec![victim, mover_leaf_node],
                    shares: policy.new_group_shares().to_vec(),
                },
            };
            replace_child_at(
                &working,
                target_group,
                *target_child_index,
                split,
                victim_share,
            )
        }
        MoveOperation::EscapeParent {
            container,
            parent,
            container_child_index,
            parent_insertion_index,
            continuation,
            ..
        } => {
            let (container_children, container_axis, _) = find_group(&tree, container)?;
            if container_axis != direction_axis {
                return None;
            }
            if direct_parent_of_leaf(&tree, focused_leaf)? != *container {
                return None;
            }
            let iw = container_children
                .iter()
                .position(|c| c.id() == focused_leaf)?;
            if iw as i32 + step >= 0 && (iw as i32 + step) < container_children.len() as i32 {
                return None;
            }
            let (parent_children, parent_axis, _) = find_group(&tree, parent)?;
            let actual_container_index =
                parent_children.iter().position(|c| c.id() == container)?;
            if actual_container_index != *container_child_index {
                return None;
            }
            if parent_of_group(&tree, container)? != *parent {
                return None;
            }
            let same_axis = parent_axis == container_axis;
            if same_axis {
                if *continuation != EscapeContinuation::None {
                    return None;
                }
                let expected = actual_container_index + usize::from(step == 1);
                if *parent_insertion_index != Some(expected) {
                    return None;
                }
            } else {
                if *continuation != EscapeContinuation::R1
                    || parent_insertion_index.is_some()
                    || parent_axis == direction_axis
                {
                    return None;
                }
            }
            let working = remove_leaf_from_tree(policy, Some(tree), focused_leaf)?;
            match continuation {
                EscapeContinuation::None => {
                    let index = (*parent_insertion_index)?;
                    if !find_group_id(&working, parent) {
                        return None;
                    }
                    let post_len = group_children_len(&working, parent)?;
                    if index > post_len {
                        return None;
                    }
                    insert_leaf_into_group(policy, working, parent, index, mover_leaf_node)
                }
                EscapeContinuation::R1 => {
                    let parent_subtree = find_subtree(&working, parent)?.clone();
                    let axis = Axis::for_direction(direction);
                    if axis != direction_axis {
                        return None;
                    }
                    let new_id = generate_group_id(alloc, &mut node_ids)?;
                    let wrapped = if step == -1 {
                        Node::Group {
                            id: new_id,
                            axis,
                            children: vec![mover_leaf_node, parent_subtree],
                            shares: policy.new_group_shares().to_vec(),
                        }
                    } else {
                        Node::Group {
                            id: new_id,
                            axis,
                            children: vec![parent_subtree, mover_leaf_node],
                            shares: policy.new_group_shares().to_vec(),
                        }
                    };
                    replace_node_by_id(working, parent, wrapped)
                }
            }
        }
        MoveOperation::CrossOutput { .. } => None,
    }
}

fn subtree_contains_id(node: &Node, target: &NodeId) -> bool {
    if node.id() == target {
        return true;
    }
    match node {
        Node::Leaf { .. } => false,
        Node::Group { children, .. } => children.iter().any(|c| subtree_contains_id(c, target)),
    }
}

fn find_group_id(tree: &Node, id: &NodeId) -> bool {
    if tree.id() == id {
        return matches!(tree, Node::Group { .. });
    }
    match tree {
        Node::Leaf { .. } => false,
        Node::Group { children, .. } => children.iter().any(|c| find_group_id(c, id)),
    }
}

fn find_group(tree: &Node, id: &NodeId) -> Option<(Vec<Node>, Axis, NodeId)> {
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

fn find_group_shares(tree: &Node, id: &NodeId) -> Option<Vec<u64>> {
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

fn group_children_len(tree: &Node, id: &NodeId) -> Option<usize> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id: gid, children, ..
        } => {
            if gid == id {
                return Some(children.len());
            }
            for child in children {
                if let Some(len) = group_children_len(child, id) {
                    return Some(len);
                }
            }
            None
        }
    }
}

fn find_subtree<'a>(tree: &'a Node, id: &NodeId) -> Option<&'a Node> {
    if tree.id() == id {
        return Some(tree);
    }
    match tree {
        Node::Leaf { .. } => None,
        Node::Group { children, .. } => children.iter().find_map(|c| find_subtree(c, id)),
    }
}

fn direct_parent_of_leaf(tree: &Node, leaf: &NodeId) -> Option<NodeId> {
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

fn parent_of_group(tree: &Node, group: &NodeId) -> Option<NodeId> {
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

fn replace_node_by_id(tree: Node, target: &NodeId, replacement: Node) -> Option<Node> {
    if tree.id() == target {
        return Some(replacement);
    }
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            mut children,
            shares,
        } => {
            for child in children.iter_mut() {
                if child.id() == target || subtree_contains_id(child, target) {
                    let updated = replace_node_by_id(child.clone(), target, replacement)?;
                    *child = updated;
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

fn replace_group_children(
    tree: Node,
    group_id: &NodeId,
    children: Vec<Node>,
    shares: Vec<u64>,
) -> Option<Node> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            children: mut old_children,
            shares: old_shares,
        } => {
            if &id == group_id {
                if children.len() < 2 || shares.len() != children.len() {
                    return None;
                }
                return Some(Node::Group {
                    id,
                    axis,
                    children,
                    shares,
                });
            }
            for child in old_children.iter_mut() {
                if child.id() == group_id || subtree_contains_id(child, group_id) {
                    let updated =
                        replace_group_children(child.clone(), group_id, children, shares)?;
                    *child = updated;
                    return Some(Node::Group {
                        id,
                        axis,
                        children: old_children,
                        shares: old_shares,
                    });
                }
            }
            None
        }
    }
}

fn replace_child_at(
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
                if child.id() == group_id || subtree_contains_id(child, group_id) {
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

fn insert_leaf_into_group(
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
                if child.id() == group_id || subtree_contains_id(child, group_id) {
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

fn swap_direct_children(tree: Node, container: &NodeId, a: &NodeId, b: &NodeId) -> Option<Node> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            mut children,
            mut shares,
        } => {
            if &id == container {
                let ia = children.iter().position(|c| c.id() == a)?;
                let ib = children.iter().position(|c| c.id() == b)?;
                if !matches!(children[ia], Node::Leaf { .. })
                    || !matches!(children[ib], Node::Leaf { .. })
                {
                    return None;
                }
                children.swap(ia, ib);
                shares.swap(ia, ib);
                return Some(Node::Group {
                    id,
                    axis,
                    children,
                    shares,
                });
            }
            for (pos, child) in children.iter().enumerate() {
                if child.id() == container || subtree_contains_id(child, container) {
                    let updated = swap_direct_children(child.clone(), container, a, b)?;
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
