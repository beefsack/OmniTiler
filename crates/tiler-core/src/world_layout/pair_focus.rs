//! Typed directional pair focus (local-first then `Unchanged`-only cross
//! fallback, plus float-origin cross-output focus) over `WorldLayout`.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::MAX_REVISION;
use crate::directional::{
    Direction, FocusPlan, NodeId, WindowId, plan_focus, plan_focus_with_history,
};
use crate::geometry::Rect;
use crate::session::{DesiredGeometry, ObservedWindow, RefusalKind};

use super::focus::domain_focus_resolves;
use super::outcome::{LayoutFailure, PairFocusOutcome, PairOperationFailure};
use super::pair::{PairAssemblyFailure, PairCandidate};
use super::pair_operation::{
    PairOperationContext, hints_from_observed, opposite_direction, pair_geometry_covers_domains,
    pair_known_windows, pair_observed_known_match, project_pair_geometry,
    settle_pair_focus_and_prune,
};
use super::state::{DomainLayout, WorldLayout};
use super::topology::collect_leaves;

struct PlannedPairFocus {
    from_window: WindowId,
    from_leaf: Option<NodeId>,
    to_window: WindowId,
    to_leaf: NodeId,
    crossed: bool,
    route: Vec<NodeId>,
    geometry: Vec<DesiredGeometry>,
}

impl WorldLayout {
    /// Converge both domains of a canonical pair, then plan and commit one
    /// directional focus transition (local-first with `Unchanged`-only cross
    /// fallback, or float-origin cross-output focus).
    ///
    /// Convergence is installed nonconsumingly before any command refusal.
    pub(crate) fn run_directional_pair_focus(
        &mut self,
        ctx: &PairOperationContext<'_>,
        subject: &WindowId,
        direction: Direction,
        float_subject: bool,
    ) -> Result<PairFocusOutcome, PairOperationFailure> {
        let (mut pair, observed) = self.prepare_converged_pair(ctx, true)?;

        if subject.0.is_empty() {
            return Err(LayoutFailure::Semantics(RefusalKind::MalformedInput).into());
        }

        let source_key = pair.source_key();
        let target_key = pair.target_key();
        let maximized_cross = observed
            .iter()
            .any(|entry| entry.window == *subject && entry.maximized && !entry.fullscreen);

        let planned = if float_subject {
            if !(maximized_cross || matches!(direction, Direction::Left | Direction::Right)) {
                return Err(LayoutFailure::Semantics(RefusalKind::Unchanged).into());
            }
            if subject != ctx.observed_focus {
                return Err(LayoutFailure::Semantics(RefusalKind::FocusMismatch).into());
            }
            if !observed.iter().any(|entry| {
                entry.window == *subject
                    && entry.output == source_key.output
                    && entry.workspace == source_key.workspace
                    && entry.floating
            }) {
                return Err(LayoutFailure::Semantics(RefusalKind::FocusMismatch).into());
            }
            pair_cross_output_focus(
                &pair,
                subject,
                direction,
                &observed,
                self.config.fixed_size_admission,
                true,
            )?
        } else {
            match pair_local_focus(
                &pair,
                subject,
                direction,
                &observed,
                self.config.fixed_size_admission,
            ) {
                Ok(plan) => plan,
                Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
                    RefusalKind::Unchanged,
                ))) if maximized_cross
                    || matches!(direction, Direction::Left | Direction::Right) =>
                {
                    pair_cross_output_focus(
                        &pair,
                        subject,
                        direction,
                        &observed,
                        self.config.fixed_size_admission,
                        false,
                    )?
                }
                Err(err) => return Err(err),
            }
        };

        let base = pair.pair_revision;
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

        let dest_key = if planned.crossed {
            target_key.clone()
        } else {
            source_key.clone()
        };
        pair.pair_revision = revision;
        pair.focused_domain = Some(dest_key.clone());
        pair.focused_leaf = Some(planned.to_leaf.clone());
        settle_pair_focus_and_prune(&mut pair, true);

        self.install_pair(&pair, ctx.source_outer_gap, None)
            .map_err(|_| PairOperationFailure::Assembly(PairAssemblyFailure::CommitRejected))?;

        Ok(PairFocusOutcome {
            base_revision: base,
            revision,
            direction,
            from_window: planned.from_window,
            from_leaf: planned.from_leaf,
            to_window: planned.to_window,
            to_leaf: planned.to_leaf,
            crossed: planned.crossed,
            cross_source_output: planned.crossed.then(|| source_key.output.clone()),
            cross_source_workspace: planned.crossed.then(|| source_key.workspace.clone()),
            target_output: dest_key.output,
            target_workspace: dest_key.workspace,
            route: planned.route,
            geometry: planned.geometry,
        })
    }
}

fn validate_pair_focus_observation(
    pair: &PairCandidate,
    subject: &WindowId,
    observed: &[ObservedWindow],
    fixed_admission: bool,
) -> Result<(), PairOperationFailure> {
    let refuse = |kind| PairOperationFailure::Layout(LayoutFailure::Semantics(kind));
    if !pair.validate_pair_topology() {
        return Err(refuse(RefusalKind::MalformedTopology));
    }
    if subject.0.is_empty() || !super::observation::valid_observed_shapes(observed) {
        return Err(refuse(RefusalKind::MalformedInput));
    }
    let source_key = pair.source_key();
    let target_key = pair.target_key();
    for entry in observed {
        let matches_source =
            entry.output == source_key.output && entry.workspace == source_key.workspace;
        let matches_target =
            entry.output == target_key.output && entry.workspace == target_key.workspace;
        if !matches_source && !matches_target {
            return Err(refuse(RefusalKind::CrossDomainMismatch));
        }
    }
    let known = pair_known_windows(pair);
    let observed_ids: BTreeSet<&WindowId> = observed.iter().map(|w| &w.window).collect();
    if observed_ids != known {
        return Err(refuse(RefusalKind::PartialObservation));
    }
    if !pair_observed_known_match(pair, observed, fixed_admission) {
        return Err(refuse(RefusalKind::PartialObservation));
    }
    if !known.contains(subject) {
        return Err(refuse(RefusalKind::UnknownWindow));
    }
    Ok(())
}

fn pair_local_focus(
    pair: &PairCandidate,
    subject: &WindowId,
    direction: Direction,
    observed: &[ObservedWindow],
    fixed_admission: bool,
) -> Result<PlannedPairFocus, PairOperationFailure> {
    let refuse = |kind| PairOperationFailure::Layout(LayoutFailure::Semantics(kind));
    validate_pair_focus_observation(pair, subject, observed, fixed_admission)?;

    if pair.source.exceptions.contains_key(subject) || pair.target.exceptions.contains_key(subject)
    {
        return Err(refuse(RefusalKind::NotTiled));
    }

    let source_key = pair.source_key();
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

    if let Some(entry) = observed.iter().find(|w| w.window == focused_window)
        && (entry.maximized || entry.fullscreen)
    {
        return Err(refuse(RefusalKind::Unchanged));
    }

    let Some(tree) = pair.source.tree.as_ref() else {
        return Err(refuse(RefusalKind::FocusMismatch));
    };
    let history_newest_first: Vec<NodeId> = pair
        .source
        .focus_history
        .iter()
        .rev()
        .filter(|leaf| domain_focus_resolves(&pair.source, leaf))
        .cloned()
        .collect();
    let Some(focus_plan) =
        plan_focus_with_history(tree, focused_leaf, direction, &history_newest_first)
    else {
        return Err(refuse(RefusalKind::MalformedTopology));
    };
    let (target_leaf, route) = match focus_plan {
        FocusPlan::Focused { leaf, route } => (leaf, route),
        FocusPlan::Edge => return Err(refuse(RefusalKind::Unchanged)),
    };

    let Some(target_window) = pair
        .source
        .leaves
        .iter()
        .find_map(|(w, l)| (l == &target_leaf).then(|| w.clone()))
    else {
        return Err(refuse(RefusalKind::MalformedTopology));
    };
    if pair.source.exceptions.contains_key(&target_window) {
        return Err(refuse(RefusalKind::MalformedTopology));
    }
    if target_leaf == *focused_leaf {
        return Err(refuse(RefusalKind::Unchanged));
    }

    let hints = hints_from_observed(observed);
    let geometry = project_pair_geometry(pair, &hints)
        .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
    if !pair_geometry_covers_domains(&geometry, pair) {
        return Err(refuse(RefusalKind::MalformedTopology));
    }

    Ok(PlannedPairFocus {
        from_window: focused_window,
        from_leaf: Some(focused_leaf.clone()),
        to_window: target_window,
        to_leaf: target_leaf,
        crossed: false,
        route,
        geometry,
    })
}

fn pair_cross_output_focus(
    pair: &PairCandidate,
    subject: &WindowId,
    direction: Direction,
    observed: &[ObservedWindow],
    fixed_admission: bool,
    float_subject: bool,
) -> Result<PlannedPairFocus, PairOperationFailure> {
    let refuse = |kind| PairOperationFailure::Layout(LayoutFailure::Semantics(kind));
    let maximized_cross = observed
        .iter()
        .any(|entry| entry.window == *subject && entry.maximized && !entry.fullscreen);
    if !(maximized_cross || matches!(direction, Direction::Left | Direction::Right)) {
        return Err(refuse(RefusalKind::Unchanged));
    }

    validate_pair_focus_observation(pair, subject, observed, fixed_admission)?;

    let source_key = pair.source_key();
    let target_key = pair.target_key();

    let focused_leaf: Option<NodeId> = if float_subject {
        if !pair.source.exceptions.contains_key(subject) {
            return Err(refuse(RefusalKind::FocusMismatch));
        }
        let Some(entry) = observed.iter().find(|entry| &entry.window == subject) else {
            return Err(refuse(RefusalKind::PartialObservation));
        };
        if !(entry.floating || entry.sticky)
            || entry.output != source_key.output
            || entry.workspace != source_key.workspace
        {
            return Err(refuse(RefusalKind::NotTiled));
        }
        None
    } else {
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
        let overlay_subject = observed
            .iter()
            .any(|entry| entry.window == *subject && (entry.maximized || entry.fullscreen));
        if !overlay_subject {
            let Some(tree) = pair.source.tree.as_ref() else {
                return Err(refuse(RefusalKind::FocusMismatch));
            };
            let Some(focus_plan) = plan_focus(tree, focused_leaf, direction) else {
                return Err(refuse(RefusalKind::MalformedTopology));
            };
            if !matches!(focus_plan, FocusPlan::Edge) {
                return Err(refuse(RefusalKind::Unchanged));
            }
        }
        Some(focused_leaf.clone())
    };

    if pair.source_domain.adjacent.get(&direction) != Some(&target_key.output)
        || pair
            .target_domain
            .adjacent
            .get(&opposite_direction(direction))
            != Some(&source_key.output)
    {
        return Err(refuse(RefusalKind::Unchanged));
    }

    let Some(target_tree) = pair.target.tree.as_ref() else {
        return Err(refuse(RefusalKind::Unchanged));
    };

    let float_target = if float_subject {
        let Some(target_leaf) = remembered_leaf(&pair.target) else {
            return Err(refuse(RefusalKind::Unchanged));
        };
        let Some(target_window) = pair
            .target
            .leaves
            .iter()
            .find_map(|(w, l)| (l == &target_leaf).then(|| w.clone()))
        else {
            return Err(refuse(RefusalKind::NotTiled));
        };
        if pair.target.exceptions.contains_key(&target_window) {
            return Err(refuse(RefusalKind::NotTiled));
        }
        Some(target_leaf)
    } else {
        None
    };

    let hints = hints_from_observed(observed);
    let geometry = project_pair_geometry(pair, &hints)
        .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
    if !pair_geometry_covers_domains(&geometry, pair) {
        return Err(refuse(RefusalKind::MalformedTopology));
    }

    let target_leaf = if float_subject {
        float_target.expect("remembered leaf checked above")
    } else {
        let Some(ref focused_id) = focused_leaf else {
            return Err(refuse(RefusalKind::FocusMismatch));
        };
        let Some(source_rect) = geometry.iter().find_map(|entry| {
            (entry.output == source_key.output
                && entry.workspace == source_key.workspace
                && entry.leaf == *focused_id)
                .then_some(entry.rect)
        }) else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        let source_perp = match direction {
            Direction::Left | Direction::Right => (
                source_rect.y,
                source_rect
                    .y
                    .checked_add(source_rect.h)
                    .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?,
            ),
            Direction::Up | Direction::Down => (
                source_rect.x,
                source_rect
                    .x
                    .checked_add(source_rect.w)
                    .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?,
            ),
        };
        let by_leaf: BTreeMap<&NodeId, Rect> = geometry
            .iter()
            .filter(|entry| {
                entry.output == target_key.output && entry.workspace == target_key.workspace
            })
            .map(|entry| (&entry.leaf, entry.rect))
            .collect();
        let mut rects_dfs: Vec<(NodeId, Rect)> = Vec::new();
        for leaf in collect_leaves(target_tree) {
            let linked = pair.target.leaves.values().any(|bound| bound == &leaf);
            if !linked {
                continue;
            }
            if let Some(rect) = by_leaf.get(&leaf) {
                rects_dfs.push((leaf, *rect));
            }
        }
        let history = cross_output_history_newest_first(&pair.target);
        let Some(target_leaf) = crate::session::rank_cross_output_edge_candidates(
            &rects_dfs,
            direction,
            &history,
            source_perp,
        ) else {
            return Err(refuse(RefusalKind::Unchanged));
        };
        target_leaf
    };

    let Some(target_window) = pair
        .target
        .leaves
        .iter()
        .find_map(|(w, l)| (l == &target_leaf).then(|| w.clone()))
    else {
        return Err(refuse(RefusalKind::NotTiled));
    };
    if pair.target.exceptions.contains_key(&target_window) {
        return Err(refuse(RefusalKind::NotTiled));
    }

    Ok(PlannedPairFocus {
        from_window: subject.clone(),
        from_leaf: focused_leaf,
        to_window: target_window,
        to_leaf: target_leaf.clone(),
        crossed: true,
        route: vec![target_leaf],
        geometry,
    })
}

fn remembered_leaf(domain: &DomainLayout) -> Option<NodeId> {
    domain
        .last_active
        .as_ref()
        .filter(|leaf| domain_focus_resolves(domain, leaf))
        .cloned()
}

fn cross_output_history_newest_first(domain: &DomainLayout) -> Vec<NodeId> {
    let mut out: Vec<NodeId> = Vec::new();
    if let Some(leaf) = &domain.last_active {
        out.push(leaf.clone());
    }
    for leaf in domain.focus_stack.iter().rev() {
        if !out.contains(leaf) {
            out.push(leaf.clone());
        }
    }
    out
}
