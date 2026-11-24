//! Typed explicit two-domain window send (workspace and output sends,
//! including D2 fallback, D3 capacity-float, and D28/D16 deferred exception
//! sends) over `WorldLayout`.

use std::collections::{BTreeMap, BTreeSet};

use crate::admission::tree_admits_candidate;
use crate::directional::{NodeId, WindowId};
use crate::geometry::project;
use crate::seed::observed_window_from_engine;
use crate::session::{DomainKey, ObservedWindow, RefusalKind};

use super::admission::select_admission_target;
use super::convergence_phases::candidate_focus_stack_fallback;
use super::focus::domain_focus_resolves;
use super::observation::valid_observed_shapes;
use super::outcome::{LayoutFailure, PairOperationFailure, PairSendKind, PairSendOutcome};
use super::pair::{PairAssemblyFailure, PairCandidate};
use super::pair_operation::{
    PairOperationContext, hints_from_observed, pair_geometry_covers_domains, pair_known_windows,
    pair_observed_known_match, project_pair_geometry, sync_pair_source_focus_from_window,
};
use super::pair_transfer::{
    commit_pair_revision_and_settle, follow_or_stay_pair_focus, stage_capacity_float_on_pair,
};
use super::projection::project_borrowed_geometry;
use super::state::{DomainLayout, WorldLayout};
use super::topology::{collect_leaves, collect_node_ids};
use super::tree_edit::{insert_tiled, remove_leaf_from_tree};

impl WorldLayout {
    /// Converge both domains of a canonical pair (seeding an absent source once
    /// from its source-side observation), install the converged observation
    /// nonconsumingly, and then plan and commit one explicit window send
    /// (ordinary tiled, D2 fallback, D3 capacity-float, or D28/D16 deferred
    /// floating-exception send).
    pub(crate) fn run_pair_send(
        &mut self,
        ctx: &PairOperationContext<'_>,
        subject: &WindowId,
        follow: bool,
        allow_cross_output: bool,
    ) -> Result<PairSendOutcome, PairOperationFailure> {
        let refuse = |kind| PairOperationFailure::Layout(LayoutFailure::Semantics(kind));
        if subject.0.is_empty() {
            return Err(refuse(RefusalKind::MalformedInput));
        }
        if ctx.observed_focus.0.is_empty() || subject != ctx.observed_focus {
            return Err(refuse(RefusalKind::FocusMismatch));
        }

        let source_key = ctx.source_domain.key();
        if !self.contains_domain(&source_key) {
            let source_observed: Vec<ObservedWindow> = ctx
                .observed
                .iter()
                .filter(|w| w.output == source_key.output && w.workspace == source_key.workspace)
                .map(observed_window_from_engine)
                .collect();
            let seed_focus = (!ctx.observed_focus.0.is_empty()).then_some(ctx.observed_focus);
            if let Err(err) = self.converge_fresh_domain_legacy(
                ctx.source_domain,
                ctx.source_outer_gap,
                &source_observed,
                seed_focus,
            ) {
                return Err(match err {
                    LayoutFailure::RevisionLimit { base, limit } => {
                        PairOperationFailure::Layout(LayoutFailure::RevisionLimit { base, limit })
                    }
                    LayoutFailure::Semantics(_) => PairOperationFailure::SeedFailed,
                });
            }
        }

        let target_key = ctx.target_domain.key();
        // A visible target observation is not the only admission anchor: a
        // target whose residents are all hidden keeps their restore slots,
        // and the send must split that retained layout instead of seeding an
        // empty target that later strands the hidden resident behind the
        // arrival.
        let target_observed_nonempty = ctx
            .observed
            .iter()
            .any(|w| w.output == target_key.output && w.workspace == target_key.workspace);
        let include_retained_target =
            target_observed_nonempty || self.has_minimized_resident(&target_key);
        let mut pair = self.assemble_send_pair_with_target(
            ctx.source_domain,
            ctx.target_domain,
            include_retained_target,
        )?;
        let observed: Vec<ObservedWindow> = ctx
            .observed
            .iter()
            .map(observed_window_from_engine)
            .collect();
        let hidden = BTreeSet::new();
        let focus = (!ctx.observed_focus.0.is_empty()).then_some(ctx.observed_focus);
        self.converge_pair(&mut pair, &observed, focus, &hidden)?;
        self.install_pair(&pair, ctx.source_outer_gap, None)
            .map_err(|_| PairOperationFailure::Assembly(PairAssemblyFailure::CommitRejected))?;
        sync_pair_source_focus_from_window(&mut pair, ctx.observed_focus);

        let source_focus_synced = pair
            .source
            .leaves
            .get(ctx.observed_focus)
            .is_some_and(|leaf| domain_focus_resolves(&pair.source, leaf));
        if !source_focus_synced
            && !deferred_send_eligible(&pair.source, subject, &source_key, &observed)
        {
            return Err(refuse(RefusalKind::FocusMismatch));
        }

        if !pair.validate_pair_topology() {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        if !valid_observed_shapes(&observed) {
            return Err(refuse(RefusalKind::MalformedInput));
        }
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

        if pair.source.exceptions.contains_key(subject) {
            return self.run_deferred_send(
                ctx,
                &mut pair,
                subject,
                follow,
                allow_cross_output,
                &observed,
            );
        }

        self.run_tiled_send(
            ctx,
            &mut pair,
            subject,
            follow,
            allow_cross_output,
            &observed,
        )
    }

    fn run_deferred_send(
        &mut self,
        ctx: &PairOperationContext<'_>,
        pair: &mut PairCandidate,
        subject: &WindowId,
        follow: bool,
        allow_cross_output: bool,
        observed: &[ObservedWindow],
    ) -> Result<PairSendOutcome, PairOperationFailure> {
        let refuse = |kind| PairOperationFailure::Layout(LayoutFailure::Semantics(kind));
        let source_key = pair.source_key();
        let target_key = pair.target_key();

        if !deferred_send_eligible(&pair.source, subject, &source_key, observed) {
            return Err(refuse(RefusalKind::NotTiled));
        }
        if !pair_observed_rest_match(pair, observed, subject, self.config.fixed_size_admission) {
            return Err(refuse(RefusalKind::PartialObservation));
        }
        check_send_scope(&source_key, &target_key, allow_cross_output)?;

        let record = pair
            .source
            .exceptions
            .remove(subject)
            .ok_or_else(|| refuse(RefusalKind::NotTiled))?;
        pair.target.exceptions.insert(subject.clone(), record);

        follow_or_stay_pair_focus(pair, follow, None);

        if !pair.validate_pair_topology() {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        let hints = hints_from_observed(observed);
        let geometry = project_pair_geometry(pair, &hints)
            .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
        if !pair_geometry_covers_domains(&geometry, pair)
            || geometry.iter().any(|g| &g.window == subject)
        {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        let (base_revision, revision) = commit_pair_revision_and_settle(pair)?;
        let stay_focus_window = stay_focus_window(pair, follow);
        let focus_leaf = pair.focused_leaf.clone();

        self.install_pair(pair, ctx.source_outer_gap, None)
            .map_err(|_| PairOperationFailure::Assembly(PairAssemblyFailure::CommitRejected))?;

        Ok(PairSendOutcome {
            base_revision,
            revision,
            kind: PairSendKind::Deferred,
            window: subject.clone(),
            source_output: source_key.output,
            source_workspace: source_key.workspace,
            target_output: target_key.output,
            target_workspace: target_key.workspace,
            follow,
            focus_leaf,
            stay_focus_window,
            geometry,
        })
    }

    #[allow(clippy::too_many_lines)]
    fn run_tiled_send(
        &mut self,
        ctx: &PairOperationContext<'_>,
        pair: &mut PairCandidate,
        subject: &WindowId,
        follow: bool,
        allow_cross_output: bool,
        observed: &[ObservedWindow],
    ) -> Result<PairSendOutcome, PairOperationFailure> {
        let refuse = |kind| PairOperationFailure::Layout(LayoutFailure::Semantics(kind));
        if !pair_observed_known_match(pair, observed, self.config.fixed_size_admission) {
            return Err(refuse(RefusalKind::PartialObservation));
        }
        let source_key = pair.source_key();
        let target_key = pair.target_key();

        let Some(mover_leaf) = pair.source.leaves.get(subject).cloned() else {
            return Err(refuse(if pair.target.leaves.contains_key(subject) {
                RefusalKind::FocusMismatch
            } else if pair.target.exceptions.contains_key(subject) {
                RefusalKind::NotTiled
            } else {
                RefusalKind::UnknownWindow
            }));
        };
        let (Some(focused_domain), Some(focused_leaf)) = (&pair.focused_domain, &pair.focused_leaf)
        else {
            return Err(refuse(RefusalKind::FocusMismatch));
        };
        if focused_domain != &source_key {
            return Err(refuse(RefusalKind::FocusMismatch));
        }
        let Some(focused_window) = pair
            .source
            .leaves
            .iter()
            .find_map(|(w, l)| (l == focused_leaf).then(|| w.clone()))
        else {
            return Err(refuse(RefusalKind::NotTiled));
        };
        if subject != &focused_window {
            return Err(refuse(RefusalKind::FocusMismatch));
        }
        check_send_scope(&source_key, &target_key, allow_cross_output)?;

        let orig_source_tree = pair.source.tree.clone();
        let orig_source_leaves = pair.source.leaves.clone();
        let orig_target_tree = pair.target.tree.clone();
        if !orig_source_tree
            .as_ref()
            .is_some_and(|tree| collect_leaves(tree).contains(&mover_leaf))
        {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        let new_source =
            remove_leaf_from_tree(self.policy.as_ref(), orig_source_tree.clone(), &mover_leaf);
        if let Some(ref tree) = new_source
            && collect_leaves(tree).contains(&mover_leaf)
        {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        let hints = hints_from_observed(observed);
        let stored_anchor = pair
            .target
            .last_active
            .as_ref()
            .filter(|leaf| domain_focus_resolves(&pair.target, leaf))
            .cloned();
        let mru_anchor = candidate_focus_stack_fallback(&pair.target, &pair.target.focus_stack);
        let remembered = stored_anchor.or(mru_anchor);

        let axis = remembered
            .as_ref()
            .and_then(|leaf| {
                let target_tree = orig_target_tree.as_ref()?;
                project_borrowed_geometry(
                    &target_key,
                    target_tree,
                    &pair.target.leaves,
                    pair.target.bounds,
                    pair.target.gap,
                    &hints,
                )?
                .into_iter()
                .find(|g| &g.leaf == leaf)
                .map(|g| self.policy.admission_axis_for_rect(&g.rect))
            })
            .unwrap_or_else(|| self.policy.admission_axis_for_rect(&pair.target.bounds));

        let mut node_ids = BTreeSet::new();
        if let Some(tree) = &orig_source_tree {
            collect_node_ids(tree, &mut node_ids);
        }
        if let Some(tree) = &orig_target_tree {
            collect_node_ids(tree, &mut node_ids);
        }

        let ordinary_target = insert_tiled(
            self.policy.as_ref(),
            &self.group_alloc,
            orig_target_tree.clone(),
            remembered.as_ref(),
            mover_leaf.clone(),
            axis,
            &mut node_ids,
        )
        .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
        if !collect_leaves(&ordinary_target).contains(&mover_leaf) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        pair.source.tree = new_source;
        pair.source.leaves.remove(subject);
        pair.target
            .leaves
            .insert(subject.clone(), mover_leaf.clone());

        let mut target_links: BTreeMap<NodeId, WindowId> = pair
            .target
            .leaves
            .iter()
            .map(|(w, l)| (l.clone(), w.clone()))
            .collect();
        target_links.insert(mover_leaf.clone(), subject.clone());

        let ordinary_admits = {
            let tree_ok = tree_admits_candidate(
                &ordinary_target,
                pair.target.bounds,
                pair.target.gap,
                &hints,
                &target_links,
            );
            if tree_ok {
                pair.target.tree = Some(ordinary_target.clone());
                project_pair_geometry(pair, &hints).is_some()
            } else {
                false
            }
        };

        let new_target = if ordinary_admits {
            ordinary_target
        } else {
            let fallback_anchors: BTreeMap<NodeId, crate::geometry::Rect> =
                match orig_target_tree.as_ref() {
                    Some(tree) => project(tree, pair.target.bounds, pair.target.gap)
                        .map(|leaves| {
                            leaves
                                .into_iter()
                                .map(|leaf| (leaf.leaf, leaf.rect))
                                .collect()
                        })
                        .unwrap_or_default(),
                    None => BTreeMap::new(),
                };
            let mut group_ids = std::iter::from_fn(|| {
                let minted = self.group_alloc.mint(&node_ids)?;
                node_ids.insert(minted.clone());
                Some(minted)
            });
            match select_admission_target(
                orig_target_tree.as_ref(),
                pair.target.bounds,
                pair.target.gap,
                remembered.as_ref(),
                Some(&ordinary_target),
                fallback_anchors,
                &hints,
                &target_links,
                &mover_leaf,
                &mut group_ids,
            ) {
                Ok(selected) => selected.tree,
                Err(()) => {
                    let mover_live_frame = if allow_cross_output {
                        None
                    } else {
                        ctx.observed
                            .iter()
                            .find(|entry| &entry.window == subject)
                            .map(|entry| entry.rect)
                    };
                    let (base_revision, revision, geometry) = stage_capacity_float_on_pair(
                        pair,
                        subject,
                        &mover_leaf,
                        orig_source_tree.as_ref(),
                        &orig_source_leaves,
                        orig_target_tree,
                        mover_live_frame,
                        allow_cross_output,
                        follow,
                        &hints,
                    )?;
                    let stay_focus_window = stay_focus_window(pair, follow);
                    let focus_leaf = pair.focused_leaf.clone();

                    self.install_pair(pair, ctx.source_outer_gap, None)
                        .map_err(|_| {
                            PairOperationFailure::Assembly(PairAssemblyFailure::CommitRejected)
                        })?;

                    return Ok(PairSendOutcome {
                        base_revision,
                        revision,
                        kind: PairSendKind::CapacityFloat,
                        window: subject.clone(),
                        source_output: source_key.output,
                        source_workspace: source_key.workspace,
                        target_output: target_key.output,
                        target_workspace: target_key.workspace,
                        follow,
                        focus_leaf,
                        stay_focus_window,
                        geometry,
                    });
                }
            }
        };

        pair.target.tree = Some(new_target);
        follow_or_stay_pair_focus(pair, follow, Some(&mover_leaf));

        if !pair.validate_pair_topology() {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        let geometry = project_pair_geometry(pair, &hints)
            .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
        if !pair_geometry_covers_domains(&geometry, pair) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        let (base_revision, revision) = commit_pair_revision_and_settle(pair)?;
        let stay_focus_window = stay_focus_window(pair, follow);
        let focus_leaf = pair.focused_leaf.clone();

        self.install_pair(pair, ctx.source_outer_gap, None)
            .map_err(|_| PairOperationFailure::Assembly(PairAssemblyFailure::CommitRejected))?;

        Ok(PairSendOutcome {
            base_revision,
            revision,
            kind: PairSendKind::Tiled,
            window: subject.clone(),
            source_output: source_key.output,
            source_workspace: source_key.workspace,
            target_output: target_key.output,
            target_workspace: target_key.workspace,
            follow,
            focus_leaf,
            stay_focus_window,
            geometry,
        })
    }
}

fn check_send_scope(
    source_key: &DomainKey,
    target_key: &DomainKey,
    allow_cross_output: bool,
) -> Result<(), PairOperationFailure> {
    let refuse = |kind| PairOperationFailure::Layout(LayoutFailure::Semantics(kind));
    if allow_cross_output {
        if target_key.output == source_key.output {
            return Err(refuse(RefusalKind::CrossDomainMismatch));
        }
    } else {
        if target_key == source_key {
            return Err(refuse(RefusalKind::Unchanged));
        }
        if target_key.output != source_key.output {
            return Err(refuse(RefusalKind::CrossDomainMismatch));
        }
    }
    Ok(())
}

fn stay_focus_window(pair: &PairCandidate, follow: bool) -> Option<WindowId> {
    if follow {
        return None;
    }
    let source_key = pair.source_key();
    if pair.focused_domain.as_ref() != Some(&source_key) {
        return None;
    }
    let leaf = pair.focused_leaf.as_ref()?;
    pair.source
        .leaves
        .iter()
        .find_map(|(w, l)| (l == leaf).then(|| w.clone()))
}

fn deferred_send_eligible(
    source: &DomainLayout,
    window: &WindowId,
    source_key: &DomainKey,
    observed: &[ObservedWindow],
) -> bool {
    let Some(record) = source.exceptions.get(window) else {
        return false;
    };
    if !(record.flags.floating
        && !record.flags.fullscreen
        && !record.flags.maximized
        && !record.flags.sticky)
    {
        return false;
    }
    let Some(entry) = observed.iter().find(|w| &w.window == window) else {
        return false;
    };
    if entry.output != source_key.output || entry.workspace != source_key.workspace {
        return false;
    }
    if !entry.floating && entry.fullscreen && !entry.maximized && !entry.sticky {
        return !entry.fixed_suppress;
    }
    if entry.floating && !entry.fullscreen && !entry.maximized && !entry.sticky {
        return !entry.fixed_suppress;
    }
    if entry.floating && !entry.sticky && (entry.fullscreen ^ entry.maximized) {
        return !entry.fixed_suppress;
    }
    false
}

fn pair_observed_rest_match(
    pair: &PairCandidate,
    observed: &[ObservedWindow],
    except: &WindowId,
    fixed_admission: bool,
) -> bool {
    let by_id: BTreeMap<&WindowId, &ObservedWindow> =
        observed.iter().map(|w| (&w.window, w)).collect();
    let source_key = pair.source_key();
    let target_key = pair.target_key();
    for (key, domain) in [(&source_key, &pair.source), (&target_key, &pair.target)] {
        for id in domain.leaves.keys() {
            let Some(entry) = by_id.get(id) else {
                return false;
            };
            if entry.output != key.output || entry.workspace != key.workspace {
                return false;
            }
            if fixed_admission {
                if entry.floating {
                    return false;
                }
            } else if entry.flags().any() {
                return false;
            }
        }
        for (id, exception) in &domain.exceptions {
            if id == except {
                continue;
            }
            let Some(entry) = by_id.get(id) else {
                return false;
            };
            if entry.output != key.output || entry.workspace != key.workspace {
                return false;
            }
            if fixed_admission {
                if entry.floating != exception.flags.floating {
                    return false;
                }
            } else if entry.flags() != exception.flags {
                return false;
            }
        }
    }
    observed.len()
        == pair.source.leaves.len()
            + pair.target.leaves.len()
            + pair.source.exceptions.len()
            + pair.target.exceptions.len()
}
