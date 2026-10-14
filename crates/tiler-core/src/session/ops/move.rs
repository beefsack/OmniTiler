//! Session directional move operations: snapshot, proposal, and verification.
//!
//! Responsibility: own the move `impl Session` family. World state,
//! shared projection/validation helpers, transaction mechanics, and tests
//! stay in the parent session module.

use super::super::*;

impl super::super::Session {
    /// Directional snapshot for movement: source plus every domain sharing
    /// the source workspace (adjacent or not), plus directly adjacent target
    /// domains on any workspace (the adjacent output's currently selected
    /// logical workspace). The frozen planner validates against unique
    /// outputs; ambiguous duplicate output ids fail closed via adjacency
    /// validation. Window links cover tiled windows in those domains only.
    pub(in crate::session) fn move_snapshot(&self, source: &DomainKey) -> Option<Snapshot> {
        use crate::directional::Output;
        let source_domain = self.domains.iter().find(|d| &d.key() == source)?;
        let mut outputs = Vec::new();
        for domain in &self.domains {
            if domain.workspace != source_domain.workspace {
                continue;
            }
            let tree = self.trees.get(&domain.key()).cloned().flatten();
            outputs.push(Output {
                id: domain.id.clone(),
                workspace: domain.workspace.clone(),
                tree,
                adjacent: domain.adjacent.clone(),
            });
        }
        // Adjacent targets on any workspace: exactly one domain per adjacent
        // output id joins the snapshot when not already included.
        for target_output in source_domain.adjacent.values() {
            if outputs.iter().any(|o| &o.id == target_output) {
                continue;
            }
            let mut matches = self.domains.iter().filter(|d| &d.id == target_output);
            let Some(domain) = matches.next() else {
                continue;
            };
            if matches.next().is_some() {
                continue;
            }
            let tree = self.trees.get(&domain.key()).cloned().flatten();
            outputs.push(Output {
                id: domain.id.clone(),
                workspace: domain.workspace.clone(),
                tree,
                adjacent: domain.adjacent.clone(),
            });
        }
        outputs.iter().find(|o| o.id == source.output)?;
        let mut windows = Vec::new();
        for link in self.windows.values() {
            // Only links whose (output, workspace) resolves to an included
            // snapshot output participate; others would be stale.
            if outputs
                .iter()
                .any(|o| o.id == link.output && o.workspace == link.workspace)
            {
                windows.push(link.clone());
            }
        }
        Some(Snapshot { outputs, windows })
    }

    /// Propose directional movement for the selected exact opaque
    /// `(domain, window)` pair.
    ///
    /// R-MOV-03 default entry: plans under [`SameAxisMove::GroupWithNeighbor`](crate::directional::SameAxisMove::GroupWithNeighbor).
    /// Adapters carrying an explicit same-axis setting use
    /// [`Session::propose_move_with_same_axis`]; this wrapper preserves the
    /// historical wrap behavior for legacy callers byte-for-byte.
    ///
    /// The supplied opaque [`WindowId`] must equal the authoritative logical
    /// focused tiled window in exactly `domain`; mismatch, unknown windows,
    /// or exception windows refuse without pending. The held
    /// [`LayoutPolicy`] movement planner is invoked on a
    /// directional snapshot (source workspace plus adjacent targets on any
    /// workspace), then exactly its [`MovePlan`]
    /// is applied mechanically to the desired topology via
    /// [`apply_move_operation`], which fail-closes against every frozen
    /// planner semantic field. R1-R3 affect only the source domain; R4
    /// affects source plus the adjacent target (the adjacent output's
    /// currently selected logical workspace). The mover
    /// remains focused (for R4 the focus domain changes).
    ///
    /// Transactional like lifecycle: refusals and planner noops leave state
    /// unchanged with no pending; only an accepted plan stages pending desired
    /// state through the shared single reconciler slot.
    pub fn propose_move(
        &mut self,
        domain: &DomainKey,
        window: &WindowId,
        direction: Direction,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &Capabilities,
    ) -> Result<SessionMovePlan, ProposeError> {
        self.propose_move_with_same_axis(
            domain,
            window,
            direction,
            session_observation,
            correlation_id,
            capabilities,
            crate::directional::SameAxisMove::GroupWithNeighbor,
        )
    }

    /// Propose directional movement under an explicit R-MOV-03 same-axis mode.
    ///
    /// Identical to [`Session::propose_move`] except the carried
    /// [`SameAxisMove`](crate::directional::SameAxisMove) selects the R2c
    /// leaf-neighbor behavior for this subsequent move only; retained trees
    /// are never rebuilt. `GroupWithNeighbor` is exactly [`Session::propose_move`].
    #[allow(clippy::too_many_arguments)]
    pub fn propose_move_with_same_axis(
        &mut self,
        domain: &DomainKey,
        window: &WindowId,
        direction: Direction,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &Capabilities,
        same_axis_move: crate::directional::SameAxisMove,
    ) -> Result<SessionMovePlan, ProposeError> {
        if let Some(reason) = self.reconciler.divergence() {
            return Err(ProposeError::Diverged(reason));
        }
        if self.has_pending() {
            return Err(ProposeError::PendingExists);
        }
        if self.drag.is_some() {
            return Err(ProposeError::PendingExists);
        }
        let Some(_source_domain) = self.domains.iter().find(|d| &d.key() == domain).cloned() else {
            return Err(ProposeError::Refused(RefusalKind::UnknownDomain));
        };
        if !self.validate_current_topology() {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        if !valid_observed_shapes(&session_observation.windows) {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        for entry in &session_observation.windows {
            if self.domain_for(&entry.output, &entry.workspace).is_none() {
                return Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch));
            }
        }
        // Completeness: observed must equal known tiled-plus-exception set.
        let known: BTreeSet<&WindowId> =
            self.windows.keys().chain(self.exceptions.keys()).collect();
        let observed_ids: BTreeSet<&WindowId> = session_observation
            .windows
            .iter()
            .map(|w| &w.window)
            .collect();
        if observed_ids != known {
            return Err(ProposeError::Refused(RefusalKind::PartialObservation));
        }
        if !self.observed_known_match(&session_observation.windows, None) {
            return Err(ProposeError::Refused(RefusalKind::PartialObservation));
        }
        if window.0.is_empty() {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        // Exact opaque scope: supplied window must equal the authoritative
        // logical focused tiled window in exactly `domain`.
        if !self.windows.contains_key(window) && !self.exceptions.contains_key(window) {
            return Err(ProposeError::Refused(RefusalKind::UnknownWindow));
        }
        if self.exceptions.contains_key(window) {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        }
        let (Some(focused_domain), Some(focused_leaf)) =
            (self.focused_domain.clone(), self.focused_leaf.clone())
        else {
            return Err(ProposeError::Refused(RefusalKind::FocusMismatch));
        };
        if &focused_domain != domain {
            return Err(ProposeError::Refused(RefusalKind::FocusMismatch));
        }
        let Some(focused_window) = self.focused_window_for(&focused_leaf, domain) else {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        };
        if self.exceptions.contains_key(&focused_window) {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        }
        if window != &focused_window {
            return Err(ProposeError::Refused(RefusalKind::FocusMismatch));
        }
        let Some(snapshot) = self.move_snapshot(domain) else {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        };
        let intent = crate::directional::MoveIntent {
            source_output: domain.output.clone(),
            focused_leaf: focused_leaf.clone(),
            focused_window: focused_window.clone(),
            direction,
            same_axis_move,
        };
        let outcome = self.policy().plan_move(&snapshot, &intent, capabilities);
        let plan = match outcome {
            crate::directional::MoveOutcome::Planned(plan) => plan,
            crate::directional::MoveOutcome::Noop { .. } => {
                return Err(ProposeError::Refused(RefusalKind::PlannerNoop));
            }
            crate::directional::MoveOutcome::Rejected { reason } => {
                if reason.kind == crate::directional::RejectionKind::UnsupportedTopology {
                    return Err(ProposeError::Refused(RefusalKind::UnsupportedCapability));
                }
                return Err(ProposeError::Refused(RefusalKind::PlannerRejected));
            }
        };
        let base_revision = session_observation.observation.revision;
        let Some((desired_trees, desired_windows, desired_focus_domain, desired_focus_leaf)) =
            apply_move_operation(
                self.policy(),
                &self.trees,
                &self.windows,
                &self.domains,
                domain,
                &focused_leaf,
                direction,
                &plan,
                base_revision,
            )
        else {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        };
        // Strict no cross-domain except source/adjacent R4 target.
        if !move_touches_only_allowed(
            &self.trees,
            &desired_trees,
            domain,
            &plan.operation,
            &self.domains,
        ) {
            return Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch));
        }
        if !validate_topology(
            &self.domains,
            &desired_trees,
            &desired_windows,
            &self.exceptions,
            &Some(desired_focus_domain.clone()),
            &Some(desired_focus_leaf.clone()),
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let affected: Vec<DomainKey> = match &plan.operation {
            MoveOperation::CrossOutput {
                target_output,
                target_workspace,
                ..
            } => {
                let target_key = DomainKey {
                    output: target_output.clone(),
                    workspace: target_workspace.clone(),
                };
                vec![domain.clone(), target_key]
            }
            _ => vec![domain.clone()],
        };
        let desired_geometry = project_affected_geometry(
            &self.domains,
            &desired_trees,
            &desired_windows,
            &affected,
            &hints_from_observed(&session_observation.windows),
        )
        .map_err(|_| ProposeError::Refused(RefusalKind::MalformedTopology))?;
        // Completeness: every tiled window in affected domains must have a
        // positive rectangle.
        if !geometry_covers_affected(&desired_geometry, &desired_windows, &affected) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let dispatch = match self.reconciler.propose(
            &plan,
            &session_observation.observation,
            correlation_id,
            capabilities,
        ) {
            Ok(dispatch) => dispatch,
            Err(crate::reconcile::ProposeError::PendingExists) => {
                return Err(ProposeError::PendingExists);
            }
            Err(crate::reconcile::ProposeError::Diverged(reason)) => {
                self.pending_desired = None;
                self.drag = None;
                return Err(ProposeError::Diverged(reason));
            }
        };
        self.pending_desired = Some(PendingDesired {
            trees: desired_trees.clone(),
            windows: desired_windows.clone(),
            focused_domain: Some(desired_focus_domain.clone()),
            focused_leaf: Some(desired_focus_leaf.clone()),
            last_active: self.updated_last_active(
                &Some(desired_focus_domain.clone()),
                &Some(desired_focus_leaf.clone()),
                &desired_trees,
                &desired_windows,
            ),
            exceptions: self.exceptions.clone(),
            retained_float_geometry: self.retained_float_geometry.clone(),
            automatic_fixed: self.automatic_fixed.clone(),
            fixed_tile_override: self.fixed_tile_override.clone(),
        });
        Ok(SessionMovePlan {
            dispatch,
            desired_snapshot: self.snapshot_for(&desired_trees, &desired_windows),
            desired_focus_domain,
            desired_focus_leaf,
            desired_geometry,
        })
    }

    /// Commit a pending movement plan after acknowledgement. On commit the
    /// pending desired topology/focus apply atomically and the accepted
    /// revision advances by exactly one. Terminal divergence clears the pending
    /// desired state. Lifecycle plans must use [`Session::verify_lifecycle`].
    pub fn verify_move(&mut self, post: &PostObservation) -> Result<Commit, VerifyError> {
        match self.reconciler.verify(post) {
            Ok(commit) => {
                if let Some(desired) = self.pending_desired.take() {
                    self.trees = desired.trees;
                    self.windows = desired.windows;
                    self.focused_domain = desired.focused_domain;
                    self.focused_leaf = desired.focused_leaf;
                    self.focus_stack = self.updated_focus_stack(
                        &self.focused_domain,
                        &self.focused_leaf,
                        &self.trees,
                        &self.windows,
                    );
                    self.last_active = desired.last_active;
                    self.exceptions = desired.exceptions;
                    self.retained_float_geometry = desired.retained_float_geometry;
                    self.automatic_fixed = desired.automatic_fixed;
                    self.fixed_tile_override = desired.fixed_tile_override;
                    self.prune_fixed_size_state();
                    self.accepted_fingerprint = commit.fingerprint;
                }
                Ok(commit)
            }
            Err(VerifyError::Diverged(reason)) => {
                self.pending_desired = None;
                self.drag = None;
                Err(VerifyError::Diverged(reason))
            }
            Err(other) => Err(other),
        }
    }
}

pub(in crate::session) fn step_for_direction(direction: Direction) -> i32 {
    match direction {
        Direction::Right | Direction::Down => 1,
        Direction::Left | Direction::Up => -1,
    }
}

pub(in crate::session) fn generate_move_group_id(
    focused_leaf: &NodeId,
    base_revision: u64,
    rule: &str,
    existing: &BTreeSet<NodeId>,
) -> NodeId {
    let base = format!("grp-{}-r{}-{}", focused_leaf.0, base_revision, rule);
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

pub(in crate::session) fn find_group_id(tree: &Node, id: &NodeId) -> bool {
    if tree.id() == id {
        return matches!(tree, Node::Group { .. });
    }
    match tree {
        Node::Leaf { .. } => false,
        Node::Group { children, .. } => children.iter().any(|c| find_group_id(c, id)),
    }
}

pub(in crate::session) fn replace_node_by_id(
    tree: Node,
    target: &NodeId,
    replacement: Node,
) -> Option<Node> {
    if tree.id() == target {
        return Some(replacement);
    }
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            children,
            shares,
        } => {
            let mut new_children = children;
            for child in new_children.iter_mut() {
                if child.id() == target || subtree_contains_id(child, target) {
                    let updated = replace_node_by_id(child.clone(), target, replacement.clone())?;
                    *child = updated;
                    return Some(Node::Group {
                        id,
                        axis,
                        children: new_children,
                        shares,
                    });
                }
            }
            // Direct child fast path already covered; no match.
            None
        }
    }
}

pub(in crate::session) fn group_children_len(tree: &Node, id: &NodeId) -> Option<usize> {
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

pub(in crate::session) type AppliedMove = (
    BTreeMap<DomainKey, Option<Node>>,
    BTreeMap<WindowId, WindowLink>,
    DomainKey,
    NodeId,
);

/// Mechanically apply exactly the frozen planner [`MovePlan`] to the desired
/// topology. Fail-closed against every frozen planner semantic field: the
/// plan `rule` must equal its operation rule and the canonical rule for the
/// operation variant, capability/preconditions must be canonical, and the
/// intent must bind source output, focused leaf/window, and direction.
/// Per-variant structural bindings (direction/axis/edge/parent/container,
/// insertion kind/index/target, split midpoint/axis/focused-side, R2a len==2,
/// R2c directional neighbor, R3 edge/parent/insertion-or-R1, R4 root edge)
/// are validated against the pre-mutation source topology before any
/// mechanical edit. Returns desired trees/windows plus desired focus on
/// success. New wrapper groups take deterministic unique ids; untouched
/// subtree identity/order/shares are preserved; entrant shares are `1` and
/// new splits are `[1, 1]`; R2c wrappers carry the summed pair share;
/// R2a swaps preserve the window-share binding (no resize). No replacement
/// planning occurs here.
#[allow(clippy::too_many_arguments)]
pub(in crate::session) fn apply_move_operation(
    policy: &dyn LayoutPolicy,
    trees: &BTreeMap<DomainKey, Option<Node>>,
    windows: &BTreeMap<WindowId, WindowLink>,
    domains: &[OutputDomain],
    source: &DomainKey,
    focused_leaf: &NodeId,
    direction: Direction,
    plan: &MovePlan,
    base_revision: u64,
) -> Option<AppliedMove> {
    use crate::directional::{EscapeContinuation, FocusedSide, Insertion, Rule};
    let operation = &plan.operation;
    // Frozen binding: plan must exactly match its own semantic operation.
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
        .contains(&crate::directional::Precondition::AdapterMustVerifyPostconditions)
    {
        return None;
    }
    // Canonical rule per operation variant. SwapNeighbor admits two rules:
    // R2a (binary swap) and R2c (N-ary swap under SwapWithNeighbor); every other
    // variant admits exactly one. The per-arm checks below keep each rule's
    // structural bindings disjoint, so neither rule weakens the other.
    let canonical_ok = match operation {
        MoveOperation::WrapPerpendicular { .. } => plan.rule == Rule::R1,
        MoveOperation::SwapNeighbor { .. } => plan.rule == Rule::R2a || plan.rule == Rule::R2c,
        MoveOperation::InsertIntoGroup { .. } => plan.rule == Rule::R2b,
        MoveOperation::SplitGroupChild { .. } => plan.rule == Rule::R2b,
        MoveOperation::WrapNeighbor { .. } => plan.rule == Rule::R2c,
        MoveOperation::EscapeParent { .. } => plan.rule == Rule::R3,
        MoveOperation::CrossOutput { .. } => plan.rule == Rule::R4,
    };
    if !canonical_ok || operation.rule() != plan.rule {
        return None;
    }
    // Intentional bindings.
    if plan.intent.source_output != source.output {
        return None;
    }
    if plan.intent.focused_leaf != *focused_leaf {
        return None;
    }
    if plan.intent.direction != direction {
        return None;
    }
    let mover_window = windows
        .values()
        .find(|l| {
            &l.leaf == focused_leaf && l.output == source.output && l.workspace == source.workspace
        })?
        .window
        .clone();
    if plan.intent.focused_window != mover_window {
        return None;
    }
    let mut desired_trees = trees.clone();
    let mut desired_windows = windows.clone();
    let mut node_ids = BTreeSet::new();
    for tree in desired_trees.values().flatten() {
        collect_node_ids(tree, &mut node_ids);
    }
    let mover_leaf_node = Node::Leaf {
        id: focused_leaf.clone(),
    };
    let direction_axis = Axis::for_direction(direction);
    let step = step_for_direction(direction);
    match operation {
        MoveOperation::WrapPerpendicular {
            container, axis, ..
        } => {
            // Intentional axis: operation axis must name the D axis.
            if *axis != direction_axis {
                return None;
            }
            let tree = desired_trees.get(source).cloned().flatten()?;
            if !find_group_id(&tree, container) {
                return None;
            }
            // Extract focused leaf from its container.
            let (container_children, container_axis, container_id) =
                match find_group(&tree, container) {
                    Some((children, axis, id)) => (children, axis, id),
                    None => return None,
                };
            // Container must be the direct parent and perpendicular to D.
            if container_axis == direction_axis {
                return None;
            }
            if direct_parent_of_leaf(&tree, focused_leaf)? != *container {
                return None;
            }
            let pos = container_children
                .iter()
                .position(|c| c.id() == focused_leaf)?;
            // Focused must be a direct leaf child.
            if !matches!(container_children[pos], Node::Leaf { .. }) {
                return None;
            }
            let mut remaining_children = container_children.clone();
            let mut remaining_shares = find_group_shares(&tree, container)?;
            remaining_children.remove(pos);
            remaining_shares.remove(pos);
            let remainder: Node = if remaining_children.len() == 1 {
                remaining_children.into_iter().next()?
            } else if remaining_children.is_empty() {
                return None;
            } else {
                Node::Group {
                    id: container_id.clone(),
                    axis: container_axis,
                    children: remaining_children,
                    shares: remaining_shares,
                }
            };
            // W at the D end: negative first, positive last.
            let new_id = generate_move_group_id(focused_leaf, base_revision, "r1", &node_ids);
            node_ids.insert(new_id.clone());
            let new_group = if step_for_direction(direction) == -1 {
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
            let new_tree = replace_node_by_id(tree, container, new_group)?;
            desired_trees.insert(source.clone(), Some(new_tree));
            Some((
                desired_trees,
                desired_windows,
                source.clone(),
                focused_leaf.clone(),
            ))
        }
        MoveOperation::SwapNeighbor {
            container,
            neighbor,
            ..
        } => {
            use crate::directional::SameAxisMove;
            let tree = desired_trees.get(source).cloned().flatten()?;
            // R2a: exactly 2 direct children, parallel container, directional
            // leaf neighbor. Uneven shares travel with their windows (no
            // resize): `swap_direct_children` swaps children and shares
            // together so each window retains its absolute share. Unchanged
            // under both same-axis modes.
            //
            // R2c swap (item 3.2): the same adjacent-leaf swap in an N-ary
            // (3+) parallel container, admitted only under SwapWithNeighbor with
            // adjacent direct leaf siblings. The forged combinations refuse:
            // R2c swap under GroupWithNeighbor, and R2a-shape plans mislabeled R2c.
            if plan.rule == Rule::R2c
                && operation.rule() == Rule::R2c
                && plan.intent.same_axis_move == SameAxisMove::SwapWithNeighbor
            {
                let (children, shares, caxis) = match find_group(&tree, container) {
                    Some((children, axis, _)) => {
                        let s = find_group_shares(&tree, container)?;
                        (children, s, axis)
                    }
                    None => return None,
                };
                if children.len() < 3 {
                    return None;
                }
                if caxis != direction_axis {
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
                let updated = swap_direct_children(tree, container, focused_leaf, neighbor)?;
                desired_trees.insert(source.clone(), Some(updated));
                return Some((
                    desired_trees,
                    desired_windows,
                    source.clone(),
                    focused_leaf.clone(),
                ));
            }
            if plan.rule != Rule::R2a || operation.rule() != Rule::R2a {
                return None;
            }
            let (children, shares, caxis) = match find_group(&tree, container) {
                Some((children, axis, _)) => {
                    let s = find_group_shares(&tree, container)?;
                    (children, s, axis)
                }
                None => return None,
            };
            if children.len() != 2 {
                return None;
            }
            if caxis != direction_axis {
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
            let updated = swap_direct_children(tree, container, focused_leaf, neighbor)?;
            desired_trees.insert(source.clone(), Some(updated));
            Some((
                desired_trees,
                desired_windows,
                source.clone(),
                focused_leaf.clone(),
            ))
        }
        MoveOperation::WrapNeighbor {
            container,
            neighbor,
            focused_before_neighbor,
            axis,
            ..
        } => {
            // R2c: 3+ parallel children, directional neighbor, operation axis
            // names the container orientation (== D axis).
            if *axis != direction_axis {
                return None;
            }
            let tree = desired_trees.get(source).cloned().flatten()?;
            let (children, shares, cid, caxis) = find_group_full(&tree, container)?;
            if children.len() < 3 {
                return None;
            }
            if caxis != direction_axis || caxis != *axis {
                return None;
            }
            if direct_parent_of_leaf(&tree, focused_leaf)? != *container {
                return None;
            }
            let iw = children.iter().position(|c| c.id() == focused_leaf)?;
            let ineighbor = children.iter().position(|c| c.id() == neighbor)?;
            // Must be adjacent with the advertised order and direction.
            if (iw as i32 - ineighbor as i32).abs() != 1 {
                return None;
            }
            if ineighbor as i32 != iw as i32 + step {
                return None;
            }
            if *focused_before_neighbor != (iw < ineighbor) {
                return None;
            }
            // Strict mode binding (item 3.2): under SwapWithNeighbor the planner emits
            // SwapNeighbor for adjacent direct leaf siblings, so a WrapNeighbor
            // naming a direct leaf sibling under SwapWithNeighbor is forged and
            // refuses. Group neighbors keep the wrap rule under both modes.
            if plan.intent.same_axis_move == crate::directional::SameAxisMove::SwapWithNeighbor
                && matches!(children.get(ineighbor), Some(Node::Leaf { .. }))
            {
                return None;
            }
            if &caxis != axis {
                // Operation axis names the container orientation; enforce.
                return None;
            }
            let first = iw.min(ineighbor);
            let pair_share: u64 = shares[iw].checked_add(shares[ineighbor])?;
            let w_node = children[iw].clone();
            let s_node = children[ineighbor].clone();
            if w_node.id() != focused_leaf {
                return None;
            }
            let mut new_children = children.clone();
            let mut new_shares = shares.clone();
            // Remove higher index first.
            let hi = iw.max(ineighbor);
            let lo = iw.min(ineighbor);
            new_children.remove(hi);
            new_shares.remove(hi);
            new_children.remove(lo);
            new_shares.remove(lo);
            let new_id = generate_move_group_id(focused_leaf, base_revision, "r2c", &node_ids);
            node_ids.insert(new_id.clone());
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
            let updated = replace_group_children(tree, &cid, new_children, new_shares)?;
            desired_trees.insert(source.clone(), Some(updated));
            Some((
                desired_trees,
                desired_windows,
                source.clone(),
                focused_leaf.clone(),
            ))
        }
        MoveOperation::InsertIntoGroup {
            container,
            target_group,
            insertion_index,
            insertion,
            ..
        } => {
            let tree = desired_trees.get(source).cloned().flatten()?;
            // R2b insert: len==2 parallel container, directional group
            // neighbor == target, insertion kind/index/target constraints.
            let (container_children, container_axis) = match find_group(&tree, container) {
                Some((children, axis, _)) => (children, axis),
                None => return None,
            };
            if container_children.len() != 2 {
                return None;
            }
            if container_axis != direction_axis {
                return None;
            }
            if direct_parent_of_leaf(&tree, focused_leaf)? != *container {
                return None;
            }
            let iw = container_children
                .iter()
                .position(|c| c.id() == focused_leaf)?;
            let ineighbor = (iw as i32 + step) as usize;
            let (t_children, t_axis) = match find_group(&tree, target_group) {
                Some((children, axis, _)) => (children, axis),
                None => return None,
            };
            // Neighbor in D must be exactly the target group.
            let neighbor_node = container_children.get(ineighbor)?;
            if neighbor_node.id() != target_group {
                return None;
            }
            if !matches!(neighbor_node, Node::Group { .. }) {
                return None;
            }
            if t_children.len() < 2 {
                return None;
            }
            // Insertion kind/index must match frozen planner semantics:
            // parallel target => NearEdge at the W-adjacent edge (0 or len);
            // perpendicular even target => Midpoint at n/2.
            if t_axis == container_axis {
                if *insertion != Insertion::NearEdge {
                    return None;
                }
                let expected = if iw < ineighbor { 0 } else { t_children.len() };
                if *insertion_index != expected {
                    return None;
                }
            } else {
                if t_children.len() % 2 != 0 {
                    return None;
                }
                if *insertion != Insertion::Midpoint {
                    return None;
                }
                if *insertion_index != t_children.len() / 2 {
                    return None;
                }
            }
            // Extract mover from its container first. `None` (emptied source)
            // cannot happen for a planned move (SingleRootLeaf is a noop).
            let working = remove_leaf_from_tree(policy, Some(tree), focused_leaf)?;
            // Target must still exist after extraction with unchanged length.
            let (post_children, _) = match find_group(&working, target_group) {
                Some((children, axis, _)) => (children, axis),
                None => return None,
            };
            if post_children.len() != t_children.len() {
                return None;
            }
            if *insertion_index > post_children.len() {
                return None;
            }
            let updated = insert_leaf_into_group(
                policy,
                working,
                target_group,
                *insertion_index,
                mover_leaf_node,
            )?;
            desired_trees.insert(source.clone(), Some(updated));
            Some((
                desired_trees,
                desired_windows,
                source.clone(),
                focused_leaf.clone(),
            ))
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
            // R2b split: len==2 parallel container, perpendicular odd target,
            // midpoint victim, D-axis split, focused side matches step.
            if *axis != direction_axis {
                return None;
            }
            let tree = desired_trees.get(source).cloned().flatten()?;
            let (container_children, container_axis) = match find_group(&tree, container) {
                Some((children, axis, _)) => (children, axis),
                None => return None,
            };
            if container_children.len() != 2 {
                return None;
            }
            if container_axis != direction_axis {
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
            let (t_children, t_axis) = match find_group(&tree, target_group) {
                Some((children, axis, _)) => (children, axis),
                None => return None,
            };
            if t_axis == container_axis {
                return None;
            }
            if t_children.len() % 2 == 0 || t_children.len() < 3 {
                return None;
            }
            if *target_child_index != t_children.len() / 2 {
                return None;
            }
            if t_children[*target_child_index].id() != target_child {
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
            if *target_child_index >= children.len() {
                return None;
            }
            if children[*target_child_index].id() != target_child {
                return None;
            }
            let victim = children[*target_child_index].clone();
            let victim_share = find_group_shares(&working, target_group)?[*target_child_index];
            let new_id = generate_move_group_id(focused_leaf, base_revision, "r2b", &node_ids);
            node_ids.insert(new_id.clone());
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
            let updated = replace_child_at(
                &working,
                target_group,
                *target_child_index,
                split,
                victim_share,
            )?;
            desired_trees.insert(source.clone(), Some(updated));
            Some((
                desired_trees,
                desired_windows,
                source.clone(),
                focused_leaf.clone(),
            ))
        }
        MoveOperation::EscapeParent {
            container,
            parent,
            container_child_index,
            parent_insertion_index,
            continuation,
            ..
        } => {
            let tree = desired_trees.get(source).cloned().flatten()?;
            // R3: parallel container at its D edge, actual parent binding,
            // same-axis insertion immediately on the D side or perpendicular
            // R1 wrap of the parent.
            let (container_children, container_axis) = match find_group(&tree, container) {
                Some((children, axis, _)) => (children, axis),
                None => return None,
            };
            if container_axis != direction_axis {
                return None;
            }
            if direct_parent_of_leaf(&tree, focused_leaf)? != *container {
                return None;
            }
            let iw = container_children
                .iter()
                .position(|c| c.id() == focused_leaf)?;
            // Must sit at the container edge in D (no neighbor).
            if iw as i32 + step >= 0 && (iw as i32 + step) < container_children.len() as i32 {
                return None;
            }
            let (parent_children, parent_axis) = match find_group(&tree, parent) {
                Some((children, axis, _)) => (children, axis),
                None => return None,
            };
            let actual_container_index =
                parent_children.iter().position(|c| c.id() == container)?;
            if actual_container_index != *container_child_index {
                return None;
            }
            // Container must be a direct child of the parent.
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
                if *continuation != EscapeContinuation::R1 {
                    return None;
                }
                if parent_insertion_index.is_some() {
                    return None;
                }
                if parent_axis == direction_axis {
                    return None;
                }
            }
            let mut working = remove_leaf_from_tree(policy, Some(tree), focused_leaf)?;
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
                    working =
                        insert_leaf_into_group(policy, working, parent, index, mover_leaf_node)?;
                }
                EscapeContinuation::R1 => {
                    // Perpendicular receiving parent: wrap the parent with the
                    // mover at the D end (new D-axis split).
                    let parent_subtree = find_subtree(&working, parent)?.clone();
                    let axis = Axis::for_direction(direction);
                    if axis != direction_axis {
                        return None;
                    }
                    let new_id =
                        generate_move_group_id(focused_leaf, base_revision, "r3r1", &node_ids);
                    node_ids.insert(new_id.clone());
                    let wrapped = if step_for_direction(direction) == -1 {
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
                    working = replace_node_by_id(working, parent, wrapped)?;
                }
            }
            desired_trees.insert(source.clone(), Some(working));
            Some((
                desired_trees,
                desired_windows,
                source.clone(),
                focused_leaf.clone(),
            ))
        }
        MoveOperation::CrossOutput {
            target_output,
            target_workspace,
            source_root_child_index,
            target,
            ..
        } => {
            use crate::directional::CrossOutputTarget;
            let target_key = DomainKey {
                output: target_output.clone(),
                workspace: target_workspace.clone(),
            };
            // Target must be a known domain (the adjacent output's currently
            // selected logical workspace, which may differ from source).
            domains.iter().find(|d| d.key() == target_key)?;
            if target_key == *source {
                return None;
            }
            // Adjacency must name this exact target output in the requested
            // direction, and the target must name the source back.
            let source_domain = domains.iter().find(|d| &d.key() == source)?;
            if source_domain.adjacent.get(&direction) != Some(target_output) {
                return None;
            }
            let target_domain = domains.iter().find(|d| d.key() == target_key)?;
            if target_domain.adjacent.get(&opposite_direction(direction)) != Some(&source.output) {
                return None;
            }
            // Only the four cardinal crossings apply (the planner emits only
            // `Direction` values; fail closed on anything else if it arrives).
            let source_tree = desired_trees.get(source).cloned().flatten()?;
            // R4: the mover sits exhausted at the source root edge in D: a
            // root-group edge child, or the sole root leaf itself (index 0,
            // REQ-MOV-08/OUT-01 item 5.1). Target occupancy must match.
            let sole_leaf_source =
                matches!(source_tree, Node::Leaf { .. }) && source_tree.id() == focused_leaf;
            if sole_leaf_source {
                if *source_root_child_index != 0 {
                    return None;
                }
            } else {
                if !matches!(source_tree, Node::Group { .. }) {
                    return None;
                }
                let (children, _shares, _sid, _saxis) =
                    find_group_full(&source_tree, source_tree.id())?;
                if *source_root_child_index >= children.len() {
                    return None;
                }
                if children[*source_root_child_index].id() != focused_leaf {
                    return None;
                }
                // Source root edge: first child for negative D, last for positive.
                let expected_edge = if step == -1 {
                    0
                } else {
                    children.len().checked_sub(1)?
                };
                if *source_root_child_index != expected_edge {
                    return None;
                }
                if !matches!(children[*source_root_child_index], Node::Leaf { .. }) {
                    return None;
                }
            }
            // Extract mover from source (a sole leaf empties its domain).
            let new_source = remove_leaf_from_tree(policy, Some(source_tree), focused_leaf);
            desired_trees.insert(source.clone(), new_source);
            // COSMIC R4: wrap the whole target in a new top-level D-axis
            // split, mover nearest the source (first on Right/Down, last
            // on Left/Up).
            let target_tree = desired_trees.get(&target_key).cloned().flatten();
            match target {
                CrossOutputTarget::Empty => {
                    if target_tree.is_some() {
                        return None;
                    }
                    desired_trees.insert(target_key.clone(), Some(mover_leaf_node));
                }
                CrossOutputTarget::Occupied => {
                    let existing = target_tree?;
                    let axis = Axis::for_direction(direction);
                    let mover_first = step_for_direction(direction) == 1;
                    let new_id =
                        generate_move_group_id(focused_leaf, base_revision, "r4", &node_ids);
                    node_ids.insert(new_id.clone());
                    let combined = if mover_first {
                        Node::Group {
                            id: new_id,
                            axis,
                            children: vec![mover_leaf_node, existing],
                            shares: policy.new_group_shares().to_vec(),
                        }
                    } else {
                        Node::Group {
                            id: new_id,
                            axis,
                            children: vec![existing, mover_leaf_node],
                            shares: policy.new_group_shares().to_vec(),
                        }
                    };
                    desired_trees.insert(target_key.clone(), Some(combined));
                }
            }
            // Mover link follows to the target domain; leaf identity kept.
            let mover_window = windows
                .values()
                .find(|l| {
                    &l.leaf == focused_leaf
                        && l.output == source.output
                        && l.workspace == source.workspace
                })?
                .window
                .clone();
            desired_windows.insert(
                mover_window.clone(),
                WindowLink {
                    window: mover_window,
                    leaf: focused_leaf.clone(),
                    output: target_key.output.clone(),
                    workspace: target_key.workspace.clone(),
                },
            );
            Some((
                desired_trees,
                desired_windows,
                target_key,
                focused_leaf.clone(),
            ))
        }
    }
}

/// Strict domain isolation: only the source domain may change, except R4
/// which may change exactly source plus its adjacent target (the adjacent
/// output's currently selected logical workspace, possibly cross-workspace).
pub(in crate::session) fn move_touches_only_allowed(
    before: &BTreeMap<DomainKey, Option<Node>>,
    after: &BTreeMap<DomainKey, Option<Node>>,
    source: &DomainKey,
    operation: &MoveOperation,
    domains: &[OutputDomain],
) -> bool {
    let mut changed = Vec::new();
    for (key, before_tree) in before {
        let after_tree = after.get(key);
        if after_tree != Some(before_tree) {
            changed.push(key.clone());
        }
    }
    // Any unknown keys fail closed.
    if after.keys().any(|k| !before.contains_key(k)) {
        return false;
    }
    match operation {
        MoveOperation::CrossOutput {
            target_output,
            target_workspace,
            ..
        } => {
            let target = DomainKey {
                output: target_output.clone(),
                workspace: target_workspace.clone(),
            };
            if domains.iter().find(|d| d.key() == target).is_none() {
                return false;
            }
            if target == *source {
                return false;
            }
            changed.len() == 2 && changed.contains(source) && changed.contains(&target)
        }
        _ => changed.len() == 1 && changed.contains(source),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::directional::{
        Axis, Capabilities, Direction, MoveIntent, MoveOperation, MoveOutcome, Node, NodeId,
        OutputId, Rule, SameAxisMove, Snapshot, WindowId, WindowLink, WorkspaceId,
    };
    use crate::geometry::Rect;
    use crate::policy::default_policy;
    use crate::session::{DomainKey, OutputDomain};

    use super::apply_move_operation;

    /// Session-side world for one N-ary group test: trees, windows, domains,
    /// plus the matching planner snapshot.
    type NaryWorld = (
        BTreeMap<DomainKey, Option<Node>>,
        BTreeMap<WindowId, WindowLink>,
        Vec<OutputDomain>,
        Snapshot,
    );

    fn domain_key() -> DomainKey {
        DomainKey {
            output: OutputId::from("source"),
            workspace: WorkspaceId::from("workspace-1"),
        }
    }

    fn output_domain() -> OutputDomain {
        OutputDomain {
            id: OutputId::from("source"),
            workspace: WorkspaceId::from("workspace-1"),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 1200,
                h: 800,
            },
            gap: 0,
            adjacent: BTreeMap::new(),
        }
    }

    fn leaf(id: &str) -> Node {
        Node::Leaf {
            id: NodeId::from(id),
        }
    }

    /// Session-side world for one N-ary group: trees/windows/domains plus the
    /// matching planner snapshot. Window ids derive from leaf ids (`w-<leaf>`).
    fn nary_world(children: Vec<Node>, shares: Vec<u64>) -> NaryWorld {
        let key = domain_key();
        let tree = Node::Group {
            id: NodeId::from("root"),
            axis: Axis::Horizontal,
            children,
            shares,
        };
        let mut leaves = Vec::new();
        let mut stack = vec![&tree];
        while let Some(node) = stack.pop() {
            match node {
                Node::Leaf { id } => leaves.push(id.clone()),
                Node::Group { children, .. } => stack.extend(children),
            }
        }
        let mut windows = BTreeMap::new();
        let mut links = Vec::new();
        for leaf_id in &leaves {
            let link = WindowLink {
                window: WindowId(format!("w-{}", leaf_id.0)),
                leaf: leaf_id.clone(),
                output: key.output.clone(),
                workspace: key.workspace.clone(),
            };
            windows.insert(link.window.clone(), link.clone());
            links.push(link);
        }
        let snapshot = Snapshot {
            outputs: vec![crate::directional::Output {
                id: key.output.clone(),
                workspace: key.workspace.clone(),
                tree: Some(tree.clone()),
                adjacent: BTreeMap::new(),
            }],
            windows: links,
        };
        let mut trees = BTreeMap::new();
        trees.insert(key, Some(tree));
        (trees, windows, vec![output_domain()], snapshot)
    }

    fn intent(leaf: &str, direction: Direction, mode: SameAxisMove) -> MoveIntent {
        MoveIntent {
            source_output: OutputId::from("source"),
            focused_leaf: NodeId::from(leaf),
            focused_window: WindowId(format!("w-{leaf}")),
            direction,
            same_axis_move: mode,
        }
    }

    fn plan_for(snapshot: &Snapshot, intent: &MoveIntent) -> crate::directional::MovePlan {
        match crate::directional::plan_move_with_capabilities(
            snapshot,
            intent,
            &Capabilities::full(),
        ) {
            MoveOutcome::Planned(plan) => plan,
            other => panic!("expected planned, got {other:?}"),
        }
    }

    fn child_ids(tree: &Node) -> Vec<String> {
        match tree {
            Node::Leaf { id } => vec![id.0.clone()],
            Node::Group { children, .. } => children.iter().map(|c| c.id().0.clone()).collect(),
        }
    }

    fn root_shares(tree: &Node) -> Vec<u64> {
        match tree {
            Node::Group { shares, .. } => shares.clone(),
            Node::Leaf { .. } => panic!("expected root group"),
        }
    }

    #[test]
    fn flat_swap_apply_swaps_adjacent_leaves_with_traveling_shares() {
        // R-MOV-09 selected shape, both directions: H[A,B*,C,D] shares
        // [1,2,3,4], move B right gives H[A,C,B,D] shares [1,3,2,4]; move C
        // left from a fresh world gives the same order/shares. Focus stays on
        // the mover, no wrapper appears, shares travel with their windows.
        for (focused, direction) in [("B", Direction::Right), ("C", Direction::Left)] {
            let (trees, windows, domains, snapshot) = nary_world(
                vec![leaf("A"), leaf("B"), leaf("C"), leaf("D")],
                vec![1, 2, 3, 4],
            );
            let key = domain_key();
            let plan = plan_for(
                &snapshot,
                &intent(focused, direction, SameAxisMove::SwapWithNeighbor),
            );
            assert_eq!(plan.rule, Rule::R2c, "{focused:?} {direction:?}");
            assert!(
                matches!(
                    plan.operation,
                    MoveOperation::SwapNeighbor {
                        rule: Rule::R2c,
                        ..
                    }
                ),
                "{focused:?} {direction:?}"
            );
            let policy = default_policy();
            let (desired_trees, _, focus_domain, focus_leaf) = apply_move_operation(
                &*policy,
                &trees,
                &windows,
                &domains,
                &key,
                &NodeId::from(focused),
                direction,
                &plan,
                7,
            )
            .unwrap_or_else(|| panic!("flat swap applies {focused:?} {direction:?}"));
            assert_eq!(focus_domain, key);
            assert_eq!(focus_leaf, NodeId::from(focused));
            let updated = desired_trees.get(&key).cloned().flatten().expect("tree");
            assert_eq!(child_ids(&updated), vec!["A", "C", "B", "D"]);
            assert_eq!(root_shares(&updated), vec![1, 3, 2, 4]);
        }
    }

    #[test]
    fn wrap_apply_keeps_nesting_under_default_mode() {
        // Same world under the GroupWithNeighbor default still nests: the wrapper
        // carries the summed pair share and the flat order is untouched.
        let (trees, windows, domains, snapshot) = nary_world(
            vec![leaf("A"), leaf("B"), leaf("C"), leaf("D")],
            vec![1, 1, 1, 1],
        );
        let key = domain_key();
        let plan = plan_for(
            &snapshot,
            &intent("B", Direction::Right, SameAxisMove::GroupWithNeighbor),
        );
        assert!(matches!(plan.operation, MoveOperation::WrapNeighbor { .. }));
        let policy = default_policy();
        let (desired_trees, _, _, _) = apply_move_operation(
            &*policy,
            &trees,
            &windows,
            &domains,
            &key,
            &NodeId::from("B"),
            Direction::Right,
            &plan,
            7,
        )
        .expect("wrap applies");
        let updated = desired_trees.get(&key).cloned().flatten().expect("tree");
        assert_eq!(child_ids(&updated).len(), 3);
        assert_eq!(root_shares(&updated), vec![1, 2, 1]);
    }

    #[test]
    fn forged_mode_mismatches_refuse_without_weakening_either_rule() {
        let (trees, windows, domains, snapshot) = nary_world(
            vec![leaf("A"), leaf("B"), leaf("C"), leaf("D")],
            vec![1, 1, 1, 1],
        );
        let key = domain_key();
        let policy = default_policy();
        // A genuine flat-swap plan relabeled to the wrap mode refuses: the
        // R2c swap is admitted only under SwapWithNeighbor.
        let swap = plan_for(
            &snapshot,
            &intent("B", Direction::Right, SameAxisMove::SwapWithNeighbor),
        );
        let mut forged_wrap_mode = swap.clone();
        forged_wrap_mode.intent.same_axis_move = SameAxisMove::GroupWithNeighbor;
        assert!(
            apply_move_operation(
                &*policy,
                &trees,
                &windows,
                &domains,
                &key,
                &NodeId::from("B"),
                Direction::Right,
                &forged_wrap_mode,
                7,
            )
            .is_none(),
            "R2c swap under GroupWithNeighbor must refuse"
        );
        // A genuine wrap plan relabeled to flat-swap refuses when the neighbor
        // is a direct leaf (the planner would have emitted a swap there).
        let wrap = plan_for(
            &snapshot,
            &intent("B", Direction::Right, SameAxisMove::GroupWithNeighbor),
        );
        let mut forged_flat_mode = wrap.clone();
        forged_flat_mode.intent.same_axis_move = SameAxisMove::SwapWithNeighbor;
        assert!(
            apply_move_operation(
                &*policy,
                &trees,
                &windows,
                &domains,
                &key,
                &NodeId::from("B"),
                Direction::Right,
                &forged_flat_mode,
                7,
            )
            .is_none(),
            "leaf-neighbor wrap under SwapWithNeighbor must refuse"
        );
        // A non-adjacent R2c swap forgery (B names D) refuses.
        let mut forged_neighbor = swap.clone();
        forged_neighbor.operation = MoveOperation::SwapNeighbor {
            rule: Rule::R2c,
            container: NodeId::from("root"),
            neighbor: NodeId::from("D"),
        };
        assert!(
            apply_move_operation(
                &*policy,
                &trees,
                &windows,
                &domains,
                &key,
                &NodeId::from("B"),
                Direction::Right,
                &forged_neighbor,
                7,
            )
            .is_none(),
            "non-adjacent R2c swap must refuse"
        );
        // The unmodified plans still apply, so the strict checks weaken
        // neither the wrap rule nor the flat-swap rule.
        assert!(
            apply_move_operation(
                &*policy,
                &trees,
                &windows,
                &domains,
                &key,
                &NodeId::from("B"),
                Direction::Right,
                &swap,
                7,
            )
            .is_some()
        );
        assert!(
            apply_move_operation(
                &*policy,
                &trees,
                &windows,
                &domains,
                &key,
                &NodeId::from("B"),
                Direction::Right,
                &wrap,
                7,
            )
            .is_some()
        );
    }

    #[test]
    fn rmov10_group_neighbor_wrap_output_shares_for_record() {
        // R-MOV-10 reference shape: H[A,B*,V[C,D],E] with sibling shares
        // [1,2,3,4] and V shares [1,1], move B right under the wrap default.
        // The existing leaf/group rule wraps B+V: root H[A,W,E] shares
        // [1,5,4] with W = H[B,V] shares [1,1]; V keeps [1,1]; focus stays B.
        let (trees, windows, domains, snapshot) = nary_world(
            vec![
                leaf("A"),
                leaf("B"),
                Node::Group {
                    id: NodeId::from("V"),
                    axis: Axis::Vertical,
                    children: vec![leaf("C"), leaf("D")],
                    shares: vec![1, 1],
                },
                leaf("E"),
            ],
            vec![1, 2, 3, 4],
        );
        let key = domain_key();
        let plan = plan_for(
            &snapshot,
            &intent("B", Direction::Right, SameAxisMove::GroupWithNeighbor),
        );
        assert!(matches!(plan.operation, MoveOperation::WrapNeighbor { .. }));
        let policy = default_policy();
        let (desired_trees, _, focus_domain, focus_leaf) = apply_move_operation(
            &*policy,
            &trees,
            &windows,
            &domains,
            &key,
            &NodeId::from("B"),
            Direction::Right,
            &plan,
            7,
        )
        .expect("group-neighbor wrap applies");
        assert_eq!(focus_domain, key);
        assert_eq!(focus_leaf, NodeId::from("B"));
        let updated = desired_trees.get(&key).cloned().flatten().expect("tree");
        match &updated {
            Node::Group {
                children, shares, ..
            } => {
                assert_eq!(shares, &vec![1, 5, 4]);
                assert_eq!(children.len(), 3);
                assert_eq!(children[0].id().0, "A");
                assert_eq!(children[2].id().0, "E");
                match &children[1] {
                    Node::Group {
                        children, shares, ..
                    } => {
                        assert_eq!(shares, &vec![1, 1]);
                        assert_eq!(child_ids(&children[0]), vec!["B"]);
                        match &children[1] {
                            Node::Group {
                                id,
                                children,
                                shares,
                                ..
                            } => {
                                assert_eq!(id.0, "V");
                                assert_eq!(shares, &vec![1, 1]);
                                let v_children: Vec<String> =
                                    children.iter().map(|c| c.id().0.clone()).collect();
                                assert_eq!(v_children, vec!["C", "D"]);
                            }
                            other => panic!("expected V group, got {other:?}"),
                        }
                    }
                    other => panic!("expected wrapper group, got {other:?}"),
                }
            }
            other => panic!("expected root group, got {other:?}"),
        }
    }

    #[test]
    fn flat_swap_forgeries_against_binary_and_group_neighbors_refuse() {
        // R2c-labeled swap in a binary container refuses (flat-swap is N-ary
        // only; the binary shape stays R2a).
        let (trees, windows, domains, snapshot) =
            nary_world(vec![leaf("A"), leaf("B")], vec![1, 1]);
        let key = domain_key();
        let policy = default_policy();
        let r2a = plan_for(
            &snapshot,
            &intent("A", Direction::Right, SameAxisMove::SwapWithNeighbor),
        );
        assert_eq!(r2a.rule, Rule::R2a);
        let mut forged = r2a.clone();
        forged.rule = Rule::R2c;
        forged.operation = MoveOperation::SwapNeighbor {
            rule: Rule::R2c,
            container: NodeId::from("root"),
            neighbor: NodeId::from("B"),
        };
        assert!(
            apply_move_operation(
                &*policy,
                &trees,
                &windows,
                &domains,
                &key,
                &NodeId::from("A"),
                Direction::Right,
                &forged,
                7,
            )
            .is_none(),
            "binary R2c swap must refuse"
        );
        // R-MOV-10: a wrap plan against a group neighbor stays valid under
        // SwapWithNeighbor (restricted scope, no broadening to groups).
        let (trees, windows, domains, snapshot) = nary_world(
            vec![
                leaf("A"),
                leaf("B"),
                Node::Group {
                    id: NodeId::from("V"),
                    axis: Axis::Vertical,
                    children: vec![leaf("C"), leaf("D")],
                    shares: vec![1, 1],
                },
                leaf("E"),
            ],
            vec![1, 1, 1, 1],
        );
        let wrap = plan_for(
            &snapshot,
            &intent("B", Direction::Right, SameAxisMove::GroupWithNeighbor),
        );
        assert!(matches!(wrap.operation, MoveOperation::WrapNeighbor { .. }));
        let mut flat_group_wrap = wrap.clone();
        flat_group_wrap.intent.same_axis_move = SameAxisMove::SwapWithNeighbor;
        assert!(
            apply_move_operation(
                &*policy,
                &trees,
                &windows,
                &domains,
                &key,
                &NodeId::from("B"),
                Direction::Right,
                &flat_group_wrap,
                7,
            )
            .is_some(),
            "group-neighbor wrap stays valid under SwapWithNeighbor"
        );
    }
}
