//! Typed directional pair move (R1-R4 plus R4 capacity-float fallback) over
//! `WorldLayout`.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{
    Axis, Capabilities, CrossOutputTarget, Direction, MoveIntent, MoveOperation, MoveOutcome,
    MovePlan, Node, NodeId, Output, Precondition, RejectionKind, Rule, SameAxisMove, Snapshot,
    WindowId, WindowLink,
};
use crate::policy::LayoutPolicy;
use crate::session::{DesiredGeometry, DomainKey, RefusalKind};

use super::group_alloc::GroupAllocator;
use crate::size_hints::WindowSizeHints;

use super::local_move::{apply_local_move_operation, geometry_covers_domain, step_for_direction};
use super::observation::valid_observed_shapes;
use super::outcome::{LayoutFailure, PairMoveCross, PairMoveOutcome, PairOperationFailure};
use super::pair::{PairAssemblyFailure, PairCandidate};
use super::pair_operation::{
    PairOperationContext, hints_from_observed, opposite_direction, pair_geometry_covers_domains,
    pair_known_windows, pair_observed_known_match, project_pair_geometry,
};
use super::pair_transfer::{commit_pair_revision_and_settle, stage_capacity_float_on_pair};
use super::projection::{project_borrowed_geometry, sort_geometry};
use super::state::WorldLayout;
use super::topology::collect_node_ids;
use super::tree_edit::{generate_group_id, remove_leaf_from_tree};

impl WorldLayout {
    /// Converge both domains of a canonical pair, then plan and commit one
    /// directional move (R1-R3 local, R4 cross-output, or R4 capacity-float
    /// fallback on projected geometric exhaustion).
    ///
    /// Convergence is installed nonconsumingly before any command refusal.
    pub(crate) fn run_directional_pair_move(
        &mut self,
        ctx: &PairOperationContext<'_>,
        subject: &WindowId,
        direction: Direction,
        cross_output_transfer: bool,
        same_axis_move: SameAxisMove,
    ) -> Result<PairMoveOutcome, PairOperationFailure> {
        let (mut pair, observed) = self.prepare_converged_pair(ctx, true)?;

        let refuse = |kind| PairOperationFailure::Layout(LayoutFailure::Semantics(kind));
        if !pair.validate_pair_topology() {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        if !valid_observed_shapes(&observed) {
            return Err(refuse(RefusalKind::MalformedInput));
        }
        let source_key = pair.source_key();
        let target_key = pair.target_key();
        for entry in &observed {
            let matches_source =
                entry.output == source_key.output && entry.workspace == source_key.workspace;
            let matches_target =
                entry.output == target_key.output && entry.workspace == target_key.workspace;
            if !matches_source && !matches_target {
                return Err(refuse(RefusalKind::CrossDomainMismatch));
            }
        }
        let known = pair_known_windows(&pair);
        let observed_ids: BTreeSet<&WindowId> = observed.iter().map(|w| &w.window).collect();
        if observed_ids != known {
            return Err(refuse(RefusalKind::PartialObservation));
        }
        if !pair_observed_known_match(&pair, &observed, self.config.fixed_size_admission) {
            return Err(refuse(RefusalKind::PartialObservation));
        }
        if subject.0.is_empty() {
            return Err(refuse(RefusalKind::MalformedInput));
        }
        if !known.contains(subject) {
            return Err(refuse(RefusalKind::UnknownWindow));
        }
        if pair.source.exceptions.contains_key(subject)
            || pair.target.exceptions.contains_key(subject)
        {
            return Err(refuse(RefusalKind::NotTiled));
        }

        let (Some(focused_domain), Some(focused_leaf)) = (&pair.focused_domain, &pair.focused_leaf)
        else {
            return Err(refuse(RefusalKind::FocusMismatch));
        };
        if focused_domain != &source_key {
            return Err(refuse(RefusalKind::FocusMismatch));
        }
        let focused_leaf = focused_leaf.clone();
        let Some(focused_window) = pair
            .source
            .leaves
            .iter()
            .find_map(|(w, l)| (l == &focused_leaf).then(|| w.clone()))
        else {
            return Err(refuse(RefusalKind::NotTiled));
        };
        if pair.source.exceptions.contains_key(&focused_window) {
            return Err(refuse(RefusalKind::NotTiled));
        }
        if subject != &focused_window {
            return Err(refuse(RefusalKind::FocusMismatch));
        }

        let snapshot = pair_move_snapshot(&pair);
        let intent = MoveIntent {
            source_output: source_key.output.clone(),
            focused_leaf: focused_leaf.clone(),
            focused_window: focused_window.clone(),
            direction,
            same_axis_move,
        };
        let mut capabilities = Capabilities::full();
        capabilities.cross_output_transfer = cross_output_transfer;
        let plan = match self.policy.plan_move(&snapshot, &intent, &capabilities) {
            MoveOutcome::Planned(plan) => plan,
            MoveOutcome::Noop { .. } => return Err(refuse(RefusalKind::PlannerNoop)),
            MoveOutcome::Rejected { reason } => {
                if reason.kind == RejectionKind::UnsupportedTopology {
                    return Err(refuse(RefusalKind::UnsupportedCapability));
                }
                return Err(refuse(RefusalKind::PlannerRejected));
            }
        };

        let hints = hints_from_observed(&observed);
        let is_cross = matches!(plan.operation, MoveOperation::CrossOutput { .. });

        let desired_geometry: Vec<DesiredGeometry> = if is_cross {
            let orig_source_tree = pair.source.tree.clone();
            let orig_target_tree = pair.target.tree.clone();
            let Some(_) = apply_pair_cross_move(
                self.policy.as_ref(),
                &self.group_alloc,
                &mut pair,
                &focused_leaf,
                &focused_window,
                direction,
                &plan,
            ) else {
                return Err(refuse(RefusalKind::MalformedTopology));
            };
            if !pair.validate_pair_topology() {
                return Err(refuse(RefusalKind::MalformedTopology));
            }
            match project_pair_geometry(&pair, &hints) {
                Some(geometry) => {
                    if !pair_geometry_covers_domains(&geometry, &pair) {
                        return Err(refuse(RefusalKind::MalformedTopology));
                    }
                    geometry
                }
                None => {
                    return self.run_r4_capacity_float(
                        ctx,
                        &mut pair,
                        &focused_window,
                        &focused_leaf,
                        orig_source_tree,
                        orig_target_tree,
                        &hints,
                    );
                }
            }
        } else {
            let Some(source_tree) = pair.source.tree.take() else {
                return Err(refuse(RefusalKind::MalformedTopology));
            };
            let Some(updated_tree) = apply_local_move_operation(
                self.policy.as_ref(),
                &self.group_alloc,
                source_tree,
                &pair.source.leaves,
                &source_key,
                &focused_leaf,
                direction,
                &plan,
            ) else {
                return Err(refuse(RefusalKind::MalformedTopology));
            };
            pair.source.tree = Some(updated_tree);
            if !pair.validate_pair_topology() {
                return Err(refuse(RefusalKind::MalformedTopology));
            }
            let mut geometry = project_borrowed_geometry(
                &source_key,
                pair.source.tree.as_ref().expect("tree set above"),
                &pair.source.leaves,
                pair.source.bounds,
                pair.source.gap,
                &hints,
            )
            .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
            sort_geometry(&mut geometry);
            if !geometry_covers_domain(&geometry, &pair.source.leaves, &source_key) {
                return Err(refuse(RefusalKind::MalformedTopology));
            }
            geometry
        };

        let (base, revision) = commit_pair_revision_and_settle(&mut pair)?;

        let target_outer_override = if is_cross { ctx.target_outer_gap } else { None };
        self.install_pair(&pair, ctx.source_outer_gap, target_outer_override)
            .map_err(|_| PairOperationFailure::Assembly(PairAssemblyFailure::CommitRejected))?;

        let cross = is_cross.then(|| PairMoveCross {
            intent_window: focused_window.clone(),
            source_output: source_key.output.clone(),
            source_workspace: source_key.workspace.clone(),
            target_output: target_key.output.clone(),
            target_workspace: target_key.workspace.clone(),
        });

        Ok(PairMoveOutcome::Moved {
            base_revision: base,
            revision,
            rule: plan.rule,
            capability: plan.required_capability,
            direction,
            window: focused_window,
            focus_leaf: focused_leaf,
            geometry: desired_geometry,
            cross,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn run_r4_capacity_float(
        &mut self,
        ctx: &PairOperationContext<'_>,
        pair: &mut PairCandidate,
        mover_window: &WindowId,
        mover_leaf: &NodeId,
        orig_source_tree: Option<Node>,
        orig_target_tree: Option<Node>,
        hints: &BTreeMap<WindowId, WindowSizeHints>,
    ) -> Result<PairMoveOutcome, PairOperationFailure> {
        let source_key = pair.source_key();
        let target_key = pair.target_key();
        let mut orig_source_leaves = pair.source.leaves.clone();
        orig_source_leaves.insert(mover_window.clone(), mover_leaf.clone());

        let (base, revision, geometry) = stage_capacity_float_on_pair(
            pair,
            mover_window,
            mover_leaf,
            orig_source_tree.as_ref(),
            &orig_source_leaves,
            orig_target_tree,
            None,
            true,
            true,
            hints,
        )?;

        self.install_pair(pair, ctx.source_outer_gap, ctx.target_outer_gap)
            .map_err(|_| PairOperationFailure::Assembly(PairAssemblyFailure::CommitRejected))?;

        Ok(PairMoveOutcome::CapacityFloat {
            base_revision: base,
            revision,
            window: mover_window.clone(),
            leaf: mover_leaf.clone(),
            source_output: source_key.output,
            source_workspace: source_key.workspace,
            target_output: target_key.output,
            target_workspace: target_key.workspace,
            follow: true,
            geometry,
        })
    }
}

fn pair_move_snapshot(pair: &PairCandidate) -> Snapshot {
    let source_key = pair.source_key();
    let target_key = pair.target_key();
    let outputs = vec![
        Output {
            id: source_key.output.clone(),
            workspace: source_key.workspace.clone(),
            tree: pair.source.tree.clone(),
            adjacent: pair.source_domain.adjacent.clone(),
        },
        Output {
            id: target_key.output.clone(),
            workspace: target_key.workspace.clone(),
            tree: pair.target.tree.clone(),
            adjacent: pair.target_domain.adjacent.clone(),
        },
    ];
    let mut windows = Vec::with_capacity(pair.source.leaves.len() + pair.target.leaves.len());
    for (window, leaf) in &pair.source.leaves {
        windows.push(WindowLink {
            window: window.clone(),
            leaf: leaf.clone(),
            output: source_key.output.clone(),
            workspace: source_key.workspace.clone(),
        });
    }
    for (window, leaf) in &pair.target.leaves {
        windows.push(WindowLink {
            window: window.clone(),
            leaf: leaf.clone(),
            output: target_key.output.clone(),
            workspace: target_key.workspace.clone(),
        });
    }
    Snapshot { outputs, windows }
}

fn apply_pair_cross_move(
    policy: &dyn LayoutPolicy,
    alloc: &GroupAllocator,
    pair: &mut PairCandidate,
    focused_leaf: &NodeId,
    focused_window: &WindowId,
    direction: Direction,
    plan: &MovePlan,
) -> Option<Option<Node>> {
    let operation = &plan.operation;
    if plan.rule != Rule::R4
        || operation.rule() != Rule::R4
        || plan.required_capability != operation.required_capability()
        || plan.preconditions != operation.preconditions()
        || plan.preconditions.len() > crate::contract::MAX_PRECONDITIONS
        || !plan
            .preconditions
            .contains(&Precondition::AdapterMustVerifyPostconditions)
    {
        return None;
    }
    let MoveOperation::CrossOutput {
        target_output,
        target_workspace,
        source_root_child_index,
        target,
        ..
    } = operation
    else {
        return None;
    };
    let source_key = pair.source_key();
    let target_key = pair.target_key();
    if plan.intent.source_output != source_key.output
        || plan.intent.focused_leaf != *focused_leaf
        || plan.intent.focused_window != *focused_window
        || plan.intent.direction != direction
    {
        return None;
    }
    let planned_target = DomainKey {
        output: target_output.clone(),
        workspace: target_workspace.clone(),
    };
    if planned_target != target_key || planned_target == source_key {
        return None;
    }
    if pair.source_domain.adjacent.get(&direction) != Some(target_output)
        || pair
            .target_domain
            .adjacent
            .get(&opposite_direction(direction))
            != Some(&source_key.output)
    {
        return None;
    }

    let step = step_for_direction(direction);
    let source_tree = pair.source.tree.take()?;
    let sole_leaf_source =
        matches!(source_tree, Node::Leaf { .. }) && source_tree.id() == focused_leaf;
    if sole_leaf_source {
        if *source_root_child_index != 0 {
            return None;
        }
    } else {
        let Node::Group { ref children, .. } = source_tree else {
            return None;
        };
        if *source_root_child_index >= children.len()
            || children[*source_root_child_index].id() != focused_leaf
        {
            return None;
        }
        let expected_edge = if step == -1 {
            0
        } else {
            children.len().checked_sub(1)?
        };
        if *source_root_child_index != expected_edge
            || !matches!(children[*source_root_child_index], Node::Leaf { .. })
        {
            return None;
        }
    }

    let mut node_ids = BTreeSet::new();
    collect_node_ids(&source_tree, &mut node_ids);
    if let Some(target_tree) = &pair.target.tree {
        collect_node_ids(target_tree, &mut node_ids);
    }

    let new_source = remove_leaf_from_tree(policy, Some(source_tree), focused_leaf);
    pair.source.tree = new_source.clone();

    let mover_leaf_node = Node::Leaf {
        id: focused_leaf.clone(),
    };
    let target_tree = pair.target.tree.take();
    match target {
        CrossOutputTarget::Empty => {
            if target_tree.is_some() {
                return None;
            }
            pair.target.tree = Some(mover_leaf_node);
        }
        CrossOutputTarget::Occupied => {
            let existing = target_tree?;
            let axis = Axis::for_direction(direction);
            let mover_first = step == 1;
            let new_id = generate_group_id(alloc, &mut node_ids)?;
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
            pair.target.tree = Some(combined);
        }
    }

    pair.source.leaves.remove(focused_window)?;
    pair.target
        .leaves
        .insert(focused_window.clone(), focused_leaf.clone());
    pair.focused_domain = Some(target_key);
    pair.focused_leaf = Some(focused_leaf.clone());

    Some(new_source)
}
