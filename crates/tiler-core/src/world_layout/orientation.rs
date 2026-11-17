//! Direct, typed parent-axis orientation toggle over `WorldLayout`.
//!
//! The caller owns convergence/seeding and raw command decoding. This
//! operation takes a validated, converged domain and its complete observation
//! and applies an accepted flip synchronously to that domain only.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::MAX_REVISION;
use crate::directional::{Axis, Node, NodeId, WindowId};
use crate::session::{DomainKey, ObservedWindow, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::focus::{settle_domain_focus_tail, sync_domain_focus_from_window};
use super::outcome::{OrientationFailure, OrientationOutcome};
use super::projection::{project_borrowed_geometry, sort_geometry};
use super::state::{DomainLayout, WorldLayout};
use super::topology::validate_domain_topology;

impl WorldLayout {
    /// Flip the focused tiled window's immediate parent group split axis for
    /// `subject` in the converged domain `key`.
    ///
    /// `observed_focus` is the effective focus window for this domain; focus
    /// sync and the commit settle tail run on a local clone only, so a refusal
    /// leaves retained layout untouched. `observed` must cover the domain's
    /// complete known tiled-plus-exception set. On success the candidate is
    /// installed, the per-domain accepted revision advances by exactly one,
    /// and a typed geometry/focus/group outcome is returned. No native
    /// acceptance is claimed.
    pub(crate) fn toggle_orientation(
        &mut self,
        key: &DomainKey,
        subject: &WindowId,
        observed_focus: &WindowId,
        observed: &[ObservedWindow],
    ) -> Result<OrientationOutcome, OrientationFailure> {
        let refuse = OrientationFailure::Semantics;

        let Some(original) = self.domain(key) else {
            return Err(refuse(RefusalKind::UnknownDomain));
        };
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
        if !domain_observed_known_match(key, &candidate, observed, self.config.fixed_size_admission)
        {
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
        if candidate.exceptions.contains_key(&focused_window) {
            return Err(refuse(RefusalKind::NotTiled));
        }
        if subject != &focused_window {
            return Err(refuse(RefusalKind::FocusMismatch));
        }
        if let Some(entry) = observed.iter().find(|w| w.window == focused_window)
            && entry.flags().any()
        {
            return Err(refuse(RefusalKind::NotTiled));
        }

        let Some(mut tree) = candidate.tree.take() else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        let Some(parent) = direct_parent_of_leaf(&tree, &focused_leaf) else {
            return Err(refuse(RefusalKind::Unchanged));
        };
        if !flip_group_axis(&mut tree, &parent) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        candidate.tree = Some(tree);
        if !validate_domain_topology(key, &candidate) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        // Affected-domain geometry gate only: no admission/repair fit proof.
        // Complete binding/topology is already ensured by candidate validation.
        let hints: BTreeMap<WindowId, WindowSizeHints> = observed
            .iter()
            .map(|entry| (entry.window.clone(), entry.hints))
            .collect();
        let mut geometry = project_borrowed_geometry(
            key,
            candidate.tree.as_ref().expect("tree set above"),
            &candidate.leaves,
            candidate.bounds,
            candidate.gap,
            &hints,
        )
        .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
        sort_geometry(&mut geometry);

        let base = original.accepted_revision;
        if base >= MAX_REVISION {
            return Err(OrientationFailure::RevisionLimit {
                base,
                limit: MAX_REVISION,
            });
        }
        let revision = base
            .checked_add(1)
            .ok_or(OrientationFailure::RevisionLimit {
                base,
                limit: MAX_REVISION,
            })?;

        // Commit focus bookkeeping always normalizes the MRU/history/last-active
        // tail from the selected focus, even when the focus sync above missed.
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

        Ok(OrientationOutcome {
            base_revision: base,
            revision,
            window: subject.clone(),
            leaf: focused_leaf,
            group: parent,
            output: key.output.clone(),
            workspace: key.workspace.clone(),
            geometry,
        })
    }
}

fn valid_observed_shapes(observed: &[ObservedWindow]) -> bool {
    let mut seen = BTreeSet::new();
    for entry in observed {
        if entry.window.0.is_empty() || entry.output.0.is_empty() || entry.workspace.0.is_empty() {
            return false;
        }
        if !seen.insert(entry.window.clone()) {
            return false;
        }
    }
    true
}

/// Domain-scoped observed/known binding with no admission: exact homing and
/// either strict exception flags or, with fixed-size admission enabled, only
/// the portable floating flag.
fn domain_observed_known_match(
    key: &DomainKey,
    domain: &DomainLayout,
    observed: &[ObservedWindow],
    fixed_admission: bool,
) -> bool {
    let by_id: BTreeMap<&WindowId, &ObservedWindow> =
        observed.iter().map(|w| (&w.window, w)).collect();
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
    true
}

/// Direct parent group id of a leaf, if the leaf is a direct child.
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

/// Flip one group's split axis in place, preserving child order and shares.
/// Returns false when the group id resolves to no group (never a leaf).
fn flip_group_axis(tree: &mut Node, group: &NodeId) -> bool {
    match tree {
        Node::Leaf { .. } => false,
        Node::Group {
            id, axis, children, ..
        } => {
            if id == group {
                *axis = match axis {
                    Axis::Horizontal => Axis::Vertical,
                    Axis::Vertical => Axis::Horizontal,
                };
                return true;
            }
            for child in children {
                if flip_group_axis(child, group) {
                    return true;
                }
            }
            false
        }
    }
}
