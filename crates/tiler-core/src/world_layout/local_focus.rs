//! Direct, typed synchronous single-domain directional focus over
//! `WorldLayout`.
//!
//! Callers own convergence, fresh seeding, cross-output focus routing and raw
//! command decoding. This module runs one local directional focus transition on
//! a local domain candidate and installs the result atomically on success.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::MAX_REVISION;
use crate::directional::{Direction, FocusPlan, NodeId, WindowId, plan_focus_with_history};
use crate::session::{DomainKey, ObservedWindow, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::focus::{
    domain_focus_resolves, settle_domain_accepted_focus_tail, sync_domain_focus_from_window,
};
use super::local_move::geometry_covers_domain;
use super::observation::{domain_observed_known_match, valid_observed_shapes};
use super::outcome::{LayoutFailure, LocalFocusOutcome};
use super::projection::{project_borrowed_geometry, sort_geometry};
use super::state::{DomainLayout, WorldLayout};
use super::topology::validate_domain_topology;

impl WorldLayout {
    /// Plan and apply one local directional focus change for `subject` in
    /// converged domain `key`.
    ///
    /// `observed_focus` is synced on a local candidate before focus checks so
    /// a refusal leaves all domain state untouched. Maximized or fullscreen
    /// focused subjects refuse as `RefusalKind::Unchanged` in all directions.
    /// On success, the tree is unchanged, `focus_stack`, `focus_history` and
    /// `last_active` advance for the accepted target, fixed-size provenance is
    /// pruned (capacity provenance is untouched), and the per-domain accepted
    /// revision advances by one.
    #[cfg(test)]
    pub(crate) fn local_focus(
        &mut self,
        key: &DomainKey,
        subject: &WindowId,
        direction: Direction,
        observed_focus: &WindowId,
        observed: &[ObservedWindow],
    ) -> Result<LocalFocusOutcome, LayoutFailure> {
        let Some(original) = self.domain(key) else {
            return Err(LayoutFailure::Semantics(RefusalKind::UnknownDomain));
        };
        let (candidate, outcome) = self.local_focus_on_candidate(
            key,
            original,
            subject,
            direction,
            observed_focus,
            observed,
            self.config.fixed_size_admission,
        )?;
        *self.domains.get_mut(key).expect("validated domain") = candidate;
        Ok(outcome)
    }

    /// Build a staged local directional focus candidate without installing it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn local_focus_on_candidate(
        &self,
        key: &DomainKey,
        original: &DomainLayout,
        subject: &WindowId,
        direction: Direction,
        observed_focus: &WindowId,
        observed: &[ObservedWindow],
        fixed_admission: bool,
    ) -> Result<(DomainLayout, LocalFocusOutcome), LayoutFailure> {
        let refuse = LayoutFailure::Semantics;

        if !validate_domain_topology(key, original) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        if subject.0.is_empty() {
            return Err(refuse(RefusalKind::MalformedInput));
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
        if !domain_observed_known_match(key, &candidate, observed, fixed_admission) {
            return Err(refuse(RefusalKind::PartialObservation));
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
        if subject != &focused_window {
            return Err(refuse(RefusalKind::FocusMismatch));
        }

        if let Some(entry) = observed.iter().find(|w| w.window == focused_window)
            && (entry.maximized || entry.fullscreen)
        {
            return Err(refuse(RefusalKind::Unchanged));
        }

        let Some(tree) = candidate.tree.as_ref() else {
            return Err(refuse(RefusalKind::FocusMismatch));
        };
        let history_newest_first: Vec<NodeId> = candidate
            .focus_history
            .iter()
            .rev()
            .filter(|leaf| domain_focus_resolves(&candidate, leaf))
            .cloned()
            .collect();
        let Some(focus_plan) =
            plan_focus_with_history(tree, &focused_leaf, direction, &history_newest_first)
        else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        let (target_leaf, route) = match focus_plan {
            FocusPlan::Focused { leaf, route } => (leaf, route),
            FocusPlan::Edge => {
                return Err(refuse(RefusalKind::Unchanged));
            }
        };

        let Some(target_window) = candidate
            .leaves
            .iter()
            .find_map(|(window, leaf)| (leaf == &target_leaf).then(|| window.clone()))
        else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        if candidate.exceptions.contains_key(&target_window) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        if target_leaf == focused_leaf {
            return Err(refuse(RefusalKind::Unchanged));
        }

        let hints: BTreeMap<WindowId, WindowSizeHints> = observed
            .iter()
            .map(|entry| (entry.window.clone(), entry.hints))
            .collect();
        let mut desired_geometry = project_borrowed_geometry(
            key,
            tree,
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
        settle_domain_accepted_focus_tail(
            &mut candidate,
            &stack_base,
            Some(target_leaf.clone()),
            current_last_active.as_ref(),
        );
        WorldLayout::prune_domain_fixed_size_state(&mut candidate);
        candidate.accepted_revision = revision;

        Ok((
            candidate,
            LocalFocusOutcome {
                base_revision: base,
                revision,
                direction,
                from_window: focused_window,
                from_leaf: focused_leaf,
                to_window: target_window,
                to_leaf: target_leaf,
                route,
                geometry: desired_geometry,
            },
        ))
    }
}
