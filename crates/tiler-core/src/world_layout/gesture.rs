//! Typed synchronous local and cross-output gesture preview/drop operations
//! over [`WorldLayout`].
//!
//! Previews never install preview topology, while preserving real convergence
//! effects and generation-shared group allocator burns on discarded trials.
//! Drops install affected domains atomically after full topology and gated
//! projection validation.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::MAX_REVISION;
use crate::cursor_targeting::FreshPlacement;
use crate::directional::{Node, NodeId, WindowId};
use crate::policy::PriorGroupEdge;
use crate::seed::{EngineWindow, observed_window_from_engine};
use crate::session::{DomainKey, DragHoverPrior, ObservedWindow, OutputDomain, RefusalKind};
use crate::size_hints::{FixedSizePredicate, WindowSizeHints};

use super::adoption::domain_is_empty;
use super::focus::{settle_domain_focus_tail, sync_domain_focus_from_window};
use super::gesture_resolve::{DragResolveInput, resolve_drag_shared};
use super::local_move::geometry_covers_domain;
use super::observation::{domain_observed_known_match, valid_observed_shapes};
use super::outcome::{DragDropOutcome, DragPreviewOutcome, LayoutFailure, PairOperationFailure};
use super::pair::{PairAssemblyFailure, PairCandidate};
use super::pair_operation::{
    PairOperationContext, hints_from_observed, pair_geometry_covers_domains, pair_known_windows,
    pair_observed_known_match, project_pair_geometry, settle_pair_focus_and_prune,
    sync_pair_source_focus_from_window,
};
use super::projection::project_borrowed_geometry;
use super::state::{DomainLayout, LayoutConfig, WorldLayout};
use super::topology::{
    collect_leaves, validate_domain_topology, validate_domain_topology_with_focus,
};
use super::tree_edit::remove_leaf_from_tree;

impl WorldLayout {
    /// Resolve one read-only local drag preview without installing preview
    /// topology. Group allocator serials burn on discarded trials.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_local_drag_preview(
        &self,
        domain: &OutputDomain,
        outer_gap: i32,
        mover: &WindowId,
        observed_focus: &WindowId,
        observed: &[EngineWindow],
        x: i32,
        y: i32,
        hover_prior: Option<&DragHoverPrior>,
    ) -> Result<DragPreviewOutcome, LayoutFailure> {
        let key = domain.key();
        let observed_windows: Vec<ObservedWindow> =
            observed.iter().map(observed_window_from_engine).collect();
        let (mut working, _, is_fresh) = self.prepare_local_drag_working(
            domain,
            outer_gap,
            &observed_windows,
            observed_focus,
            true,
        )?;

        if domain_is_empty(&working) {
            return Err(LayoutFailure::Semantics(RefusalKind::UnknownDomain));
        }
        if is_fresh {
            let _ = sync_domain_focus_from_window(&mut working, mover);
        } else {
            let _ = sync_domain_focus_from_window(&mut working, observed_focus);
        }

        let hints = hints_from_observed(&observed_windows);
        let fixed_admission = if is_fresh {
            false
        } else {
            self.config.fixed_size_admission
        };
        let (source_leaf, base) = begin_local_drag_validated(
            &key,
            &working,
            mover,
            &observed_windows,
            &hints,
            fixed_admission,
        )?;

        let prior = valid_carried_prior(hover_prior, &key, &source_leaf, mover, base, &key);
        let current = working
            .tree
            .as_ref()
            .ok_or(LayoutFailure::Semantics(RefusalKind::MalformedTopology))?;

        let resolved = resolve_drag_shared(
            &DragResolveInput {
                policy: self.policy.as_ref(),
                alloc: &self.group_alloc,
                tree: current,
                domain,
                leaves: &working.leaves,
                exceptions: &working.exceptions,
                work_area: working.bounds,
                source_leaf: &source_leaf,
                source_window: mover,
                prior,
                revision: base,
            },
            x,
            y,
        )
        .map_err(LayoutFailure::Semantics)?;

        if &resolved.placement.tree == current {
            return Err(LayoutFailure::Semantics(RefusalKind::Unchanged));
        }

        let desired_geometry = project_borrowed_geometry(
            &key,
            &resolved.placement.tree,
            &working.leaves,
            working.bounds,
            working.gap,
            &hints,
        )
        .ok_or(LayoutFailure::Semantics(RefusalKind::MalformedTopology))?;

        let proposed_rect = desired_geometry
            .iter()
            .find(|g| g.leaf == source_leaf)
            .map(|g| g.rect)
            .ok_or(LayoutFailure::Semantics(RefusalKind::MalformedTopology))?;

        Ok(DragPreviewOutcome {
            base_revision: base,
            domain: key.clone(),
            source_domain: key.clone(),
            target_window: resolved.target_window,
            proposed_rect,
            hover: DragHoverPrior {
                domain: key.clone(),
                source_leaf,
                source_window: mover.clone(),
                revision: base,
                prior: resolved.next_prior,
                prior_domain: key,
            },
        })
    }

    /// Commit one local drag drop synchronously. Callers own any pre-operation
    /// convergence on an already-retained domain.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_local_drag_drop(
        &mut self,
        domain: &OutputDomain,
        outer_gap: i32,
        mover: &WindowId,
        observed_focus: &WindowId,
        observed: &[EngineWindow],
        x: i32,
        y: i32,
        hover_prior: Option<&DragHoverPrior>,
    ) -> Result<DragDropOutcome, LayoutFailure> {
        let key = domain.key();
        let observed_windows: Vec<ObservedWindow> =
            observed.iter().map(observed_window_from_engine).collect();
        let (mut working, counter, is_fresh) = self.prepare_local_drag_working(
            domain,
            outer_gap,
            &observed_windows,
            observed_focus,
            false,
        )?;

        if domain_is_empty(&working) {
            return Err(LayoutFailure::Semantics(RefusalKind::UnknownDomain));
        }
        if is_fresh {
            let _ = sync_domain_focus_from_window(&mut working, mover);
        } else {
            let _ = sync_domain_focus_from_window(&mut working, observed_focus);
        }

        let hints = hints_from_observed(&observed_windows);
        let fixed_admission = if is_fresh {
            false
        } else {
            self.config.fixed_size_admission
        };
        let (source_leaf, base) = begin_local_drag_validated(
            &key,
            &working,
            mover,
            &observed_windows,
            &hints,
            fixed_admission,
        )?;

        let prior = valid_carried_prior(hover_prior, &key, &source_leaf, mover, base, &key);
        let current = working
            .tree
            .as_ref()
            .ok_or(LayoutFailure::Semantics(RefusalKind::MalformedTopology))?;

        let resolved = resolve_drag_shared(
            &DragResolveInput {
                policy: self.policy.as_ref(),
                alloc: &self.group_alloc,
                tree: current,
                domain,
                leaves: &working.leaves,
                exceptions: &working.exceptions,
                work_area: working.bounds,
                source_leaf: &source_leaf,
                source_window: mover,
                prior,
                revision: base,
            },
            x,
            y,
        )
        .map_err(LayoutFailure::Semantics)?;

        if &resolved.placement.tree == current {
            return Err(LayoutFailure::Semantics(RefusalKind::Unchanged));
        }

        working.tree = Some(resolved.placement.tree);
        if !validate_domain_topology_with_focus(&key, &working, Some(&source_leaf)) {
            return Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology));
        }

        let desired_geometry = project_borrowed_geometry(
            &key,
            working.tree.as_ref().expect("tree set above"),
            &working.leaves,
            working.bounds,
            working.gap,
            &hints,
        )
        .ok_or(LayoutFailure::Semantics(RefusalKind::MalformedTopology))?;

        if !geometry_covers_domain(&desired_geometry, &working.leaves, &key) {
            return Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology));
        }

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

        let stack = working.focus_stack.clone();
        let last = working.last_active.clone();
        working.focused_leaf = Some(source_leaf.clone());
        settle_domain_focus_tail(&mut working, &stack, Some(source_leaf), last.as_ref());
        Self::prune_domain_focus_history(&mut working);
        Self::prune_domain_fixed_size_state(&mut working);
        working.accepted_revision = revision;

        self.domains.insert(key, working);
        if is_fresh {
            self.admission_seq_next = self.admission_seq_next.max(counter);
        }

        Ok(DragDropOutcome {
            base_revision: base,
            revision,
        })
    }

    /// Resolve one cross-output drag preview against an assembled two-domain
    /// pair. Combined convergence persists on both domains; no preview
    /// topology is installed.
    pub(crate) fn run_pair_drag_preview(
        &mut self,
        ctx: &PairOperationContext<'_>,
        mover: &WindowId,
        x: i32,
        y: i32,
        hover_prior: Option<&DragHoverPrior>,
    ) -> Result<DragPreviewOutcome, PairOperationFailure> {
        let (mut pair, observed) = self.prepare_converged_occupied_pair(ctx)?;
        sync_pair_source_focus_from_window(&mut pair, mover);

        let hints = hints_from_observed(&observed);
        let (source_leaf, base) = begin_pair_drag_validated(
            &pair,
            mover,
            &observed,
            &hints,
            self.config.fixed_size_admission,
        )
        .map_err(PairOperationFailure::Layout)?;

        let source_key = pair.source_key();
        let target_key = pair.target_key();
        let target_tree = cross_target_tree(&pair, &source_leaf)
            .ok_or(LayoutFailure::Semantics(RefusalKind::MalformedTopology))?;

        let prior = valid_carried_prior(
            hover_prior,
            &source_key,
            &source_leaf,
            mover,
            base,
            &target_key,
        );

        let resolved = resolve_drag_shared(
            &DragResolveInput {
                policy: self.policy.as_ref(),
                alloc: &self.group_alloc,
                tree: target_tree,
                domain: ctx.target_domain,
                leaves: &pair.target.leaves,
                exceptions: &pair.target.exceptions,
                work_area: pair.target.bounds,
                source_leaf: &source_leaf,
                source_window: mover,
                prior,
                revision: base,
            },
            x,
            y,
        )
        .map_err(|kind| PairOperationFailure::Layout(LayoutFailure::Semantics(kind)))?;

        let mut preview_leaves = pair.target.leaves.clone();
        preview_leaves.insert(mover.clone(), source_leaf.clone());
        let desired_geometry = project_borrowed_geometry(
            &target_key,
            &resolved.placement.tree,
            &preview_leaves,
            pair.target.bounds,
            pair.target.gap,
            &hints,
        )
        .ok_or(LayoutFailure::Semantics(RefusalKind::MalformedTopology))?;

        let proposed_rect = desired_geometry
            .iter()
            .find(|g| g.leaf == source_leaf)
            .map(|g| g.rect)
            .ok_or(LayoutFailure::Semantics(RefusalKind::MalformedTopology))?;

        Ok(DragPreviewOutcome {
            base_revision: base,
            domain: target_key.clone(),
            source_domain: source_key.clone(),
            target_window: resolved.target_window,
            proposed_rect,
            hover: DragHoverPrior {
                domain: source_key,
                source_leaf,
                source_window: mover.clone(),
                revision: base,
                prior: resolved.next_prior,
                prior_domain: target_key,
            },
        })
    }

    /// Commit one cross-output drag drop atomically across the converged pair.
    pub(crate) fn run_pair_drag_drop(
        &mut self,
        ctx: &PairOperationContext<'_>,
        mover: &WindowId,
        x: i32,
        y: i32,
        hover_prior: Option<&DragHoverPrior>,
    ) -> Result<DragDropOutcome, PairOperationFailure> {
        let (mut pair, observed) = self.prepare_converged_occupied_pair(ctx)?;
        sync_pair_source_focus_from_window(&mut pair, mover);

        let hints = hints_from_observed(&observed);
        let (source_leaf, base) = begin_pair_drag_validated(
            &pair,
            mover,
            &observed,
            &hints,
            self.config.fixed_size_admission,
        )
        .map_err(PairOperationFailure::Layout)?;

        let source_key = pair.source_key();
        let target_key = pair.target_key();
        let target_tree = cross_target_tree(&pair, &source_leaf)
            .ok_or(LayoutFailure::Semantics(RefusalKind::MalformedTopology))?;

        let prior = valid_carried_prior(
            hover_prior,
            &source_key,
            &source_leaf,
            mover,
            base,
            &target_key,
        );

        let resolved = resolve_drag_shared(
            &DragResolveInput {
                policy: self.policy.as_ref(),
                alloc: &self.group_alloc,
                tree: target_tree,
                domain: ctx.target_domain,
                leaves: &pair.target.leaves,
                exceptions: &pair.target.exceptions,
                work_area: pair.target.bounds,
                source_leaf: &source_leaf,
                source_window: mover,
                prior,
                revision: base,
            },
            x,
            y,
        )
        .map_err(|kind| PairOperationFailure::Layout(LayoutFailure::Semantics(kind)))?;

        let new_source =
            remove_leaf_from_tree(self.policy.as_ref(), pair.source.tree.take(), &source_leaf);
        pair.source.tree = new_source;
        pair.target.tree = Some(resolved.placement.tree);
        pair.source.leaves.remove(mover);
        pair.target
            .leaves
            .insert(mover.clone(), source_leaf.clone());
        pair.focused_domain = Some(target_key);
        pair.focused_leaf = Some(source_leaf);

        if !pair.validate_pair_topology() {
            return Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology).into());
        }

        let desired_geometry = project_pair_geometry(&pair, &hints)
            .ok_or(LayoutFailure::Semantics(RefusalKind::MalformedTopology))?;
        if !pair_geometry_covers_domains(&desired_geometry, &pair) {
            return Err(LayoutFailure::Semantics(RefusalKind::MalformedTopology).into());
        }

        if base >= MAX_REVISION {
            return Err(LayoutFailure::RevisionLimit {
                base,
                limit: MAX_REVISION,
            }
            .into());
        }
        let revision = base.checked_add(1).ok_or(LayoutFailure::RevisionLimit {
            base,
            limit: MAX_REVISION,
        })?;

        settle_pair_focus_and_prune(&mut pair, false);
        pair.pair_revision = revision;

        self.install_pair(&pair, ctx.source_outer_gap, ctx.target_outer_gap)
            .map_err(|_| PairOperationFailure::Assembly(PairAssemblyFailure::CommitRejected))?;

        Ok(DragDropOutcome {
            base_revision: base,
            revision,
        })
    }

    fn prepare_local_drag_working(
        &self,
        domain: &OutputDomain,
        outer_gap: i32,
        observed: &[ObservedWindow],
        observed_focus: &WindowId,
        converge_retained: bool,
    ) -> Result<(DomainLayout, u64, bool), LayoutFailure> {
        let key = domain.key();
        let focus = (!observed_focus.0.is_empty()).then_some(observed_focus);
        if let Some(original) = self.domain(&key) {
            if original.bounds != domain.bounds || original.gap != domain.gap {
                return Err(LayoutFailure::Semantics(RefusalKind::UnknownDomain));
            }
            if original.outer_gap != outer_gap {
                return Err(LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch));
            }
            if converge_retained {
                let (converged, counter, _) =
                    self.converge_candidate(&key, original, observed, focus, &BTreeSet::new())?;
                return Ok((converged, counter, false));
            }
            return Ok((original.clone(), self.admission_seq_next, false));
        }

        if outer_gap < 0 || !domain.validate() {
            return Err(LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch));
        }
        let legacy_config = LayoutConfig {
            fixed_size_admission: false,
            fixed_size_predicate: FixedSizePredicate::BothAxes,
            fresh_placement: FreshPlacement::default(),
            fresh_cursor: None,
        };
        let fresh = DomainLayout::new(domain.bounds, domain.gap, outer_gap);
        let (converged, counter, _) = self.converge_candidate_with_config(
            &key,
            &fresh,
            observed,
            focus,
            &BTreeSet::new(),
            legacy_config,
        )?;
        Ok((converged, counter, true))
    }
}

fn valid_carried_prior<'a>(
    carried: Option<&'a DragHoverPrior>,
    source_key: &DomainKey,
    source_leaf: &NodeId,
    source_window: &WindowId,
    revision: u64,
    resolution_key: &DomainKey,
) -> Option<&'a PriorGroupEdge> {
    let carried = carried?;
    if &carried.domain != source_key
        || &carried.source_leaf != source_leaf
        || &carried.source_window != source_window
        || carried.revision != revision
        || &carried.prior_domain != resolution_key
    {
        return None;
    }
    carried.prior.as_ref()
}

fn begin_local_drag_validated(
    key: &DomainKey,
    domain: &DomainLayout,
    mover: &WindowId,
    observed: &[ObservedWindow],
    hints: &BTreeMap<WindowId, WindowSizeHints>,
    fixed_admission: bool,
) -> Result<(NodeId, u64), LayoutFailure> {
    let refuse = LayoutFailure::Semantics;
    if !validate_domain_topology(key, domain) {
        return Err(refuse(RefusalKind::MalformedTopology));
    }
    if mover.0.is_empty() || !valid_observed_shapes(observed) {
        return Err(refuse(RefusalKind::MalformedInput));
    }
    for entry in observed {
        if entry.output != key.output || entry.workspace != key.workspace {
            return Err(refuse(RefusalKind::CrossDomainMismatch));
        }
    }
    let known: BTreeSet<&WindowId> = domain
        .leaves
        .keys()
        .chain(domain.exceptions.keys())
        .collect();
    let observed_ids: BTreeSet<&WindowId> = observed.iter().map(|w| &w.window).collect();
    if observed_ids != known || !domain_observed_known_match(key, domain, observed, fixed_admission)
    {
        return Err(refuse(RefusalKind::PartialObservation));
    }
    if domain.exceptions.contains_key(mover) {
        return Err(refuse(RefusalKind::NotTiled));
    }
    let Some(source_leaf) = domain.leaves.get(mover).cloned() else {
        return Err(refuse(RefusalKind::UnknownWindow));
    };
    if domain.focused_leaf.as_ref() != Some(&source_leaf) {
        return Err(refuse(RefusalKind::FocusMismatch));
    }
    if let Some(entry) = observed.iter().find(|w| &w.window == mover)
        && entry.flags().any()
    {
        return Err(refuse(RefusalKind::NotTiled));
    }
    let tree = domain
        .tree
        .as_ref()
        .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
    let geometry =
        project_borrowed_geometry(key, tree, &domain.leaves, domain.bounds, domain.gap, hints)
            .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
    if !geometry.iter().any(|g| g.leaf == source_leaf) {
        return Err(refuse(RefusalKind::MalformedTopology));
    }
    Ok((source_leaf, domain.accepted_revision))
}

fn begin_pair_drag_validated(
    pair: &PairCandidate,
    mover: &WindowId,
    observed: &[ObservedWindow],
    hints: &BTreeMap<WindowId, WindowSizeHints>,
    fixed_admission: bool,
) -> Result<(NodeId, u64), LayoutFailure> {
    let refuse = LayoutFailure::Semantics;
    if !pair.validate_pair_topology() {
        return Err(refuse(RefusalKind::MalformedTopology));
    }
    if mover.0.is_empty() || !valid_observed_shapes(observed) {
        return Err(refuse(RefusalKind::MalformedInput));
    }
    let source_key = pair.source_key();
    let target_key = pair.target_key();
    for entry in observed {
        let key = DomainKey {
            output: entry.output.clone(),
            workspace: entry.workspace.clone(),
        };
        if key != source_key && key != target_key {
            return Err(refuse(RefusalKind::CrossDomainMismatch));
        }
    }
    let known = pair_known_windows(pair);
    let observed_ids: BTreeSet<&WindowId> = observed.iter().map(|w| &w.window).collect();
    if observed_ids != known || !pair_observed_known_match(pair, observed, fixed_admission) {
        return Err(refuse(RefusalKind::PartialObservation));
    }
    if pair.source.exceptions.contains_key(mover) || pair.target.exceptions.contains_key(mover) {
        return Err(refuse(RefusalKind::NotTiled));
    }
    let Some(source_leaf) = pair.source.leaves.get(mover).cloned() else {
        return Err(refuse(RefusalKind::UnknownWindow));
    };
    if pair.focused_domain.as_ref() != Some(&source_key)
        || pair.focused_leaf.as_ref() != Some(&source_leaf)
    {
        return Err(refuse(RefusalKind::FocusMismatch));
    }
    if let Some(entry) = observed.iter().find(|w| &w.window == mover)
        && entry.flags().any()
    {
        return Err(refuse(RefusalKind::NotTiled));
    }
    let source_tree = pair
        .source
        .tree
        .as_ref()
        .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
    let geometry = project_borrowed_geometry(
        &source_key,
        source_tree,
        &pair.source.leaves,
        pair.source.bounds,
        pair.source.gap,
        hints,
    )
    .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
    if !geometry.iter().any(|g| g.leaf == source_leaf) {
        return Err(refuse(RefusalKind::MalformedTopology));
    }
    Ok((source_leaf, pair.pair_revision))
}

fn cross_target_tree<'a>(pair: &'a PairCandidate, source_leaf: &NodeId) -> Option<&'a Node> {
    let target_tree = pair.target.tree.as_ref()?;
    if collect_leaves(target_tree).contains(source_leaf) {
        return None;
    }
    Some(target_tree)
}
