//! Direct, typed synchronous keyboard, pointer edge, and pointer corner
//! resize operations over `WorldLayout`.
//!
//! Callers own convergence, seeding, and raw command parsing. Each operation
//! runs against one converged retained domain and its complete observation,
//! keeping pre-operation focus sync and intermediate states on a local domain
//! clone so any refusal leaves retained layout state untouched.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::{MAX_REVISION, ResizeMode, ResizeOperation};
use crate::directional::{Axis, Direction, Node, NodeId, WindowId, apply_resize_shares};
use crate::policy::LayoutPolicy;
use crate::session::{DesiredGeometry, DomainKey, ObservedWindow, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::focus::sync_domain_focus_from_window;
use super::observation::{
    domain_observed_known_match, domain_pointer_observation_matches, valid_observed_shapes,
};
use super::outcome::{LayoutFailure, PointerCornerResizeOutcome, ResizeOutcome};
use super::projection::{project_borrowed_geometry, sort_geometry};
use super::state::{DomainLayout, WorldLayout};
use super::topology::{collect_leaves, validate_domain_topology};

impl WorldLayout {
    /// COSMIC keyboard pixel resize for `subject` in the converged domain `key`.
    ///
    /// `observed_focus` is synced on a local candidate before focus checks so
    /// a refusal never leaks focus or history changes. On success, the
    /// candidate is installed with the exact `verify_resize` commit tail and
    /// a single per-domain revision increment.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn keyboard_resize(
        &mut self,
        key: &DomainKey,
        subject: &WindowId,
        direction: Direction,
        mode: ResizeMode,
        press_index: u32,
        observed_focus: &WindowId,
        observed: &[ObservedWindow],
    ) -> Result<ResizeOutcome, LayoutFailure> {
        let Some(original) = self.domain(key) else {
            return Err(LayoutFailure::Semantics(RefusalKind::UnknownDomain));
        };
        let (candidate, outcome) = self.keyboard_resize_on_candidate(
            key,
            original,
            subject,
            direction,
            mode,
            press_index,
            observed_focus,
            observed,
            self.config.fixed_size_admission,
        )?;
        *self.domains.get_mut(key).expect("validated domain") = candidate;
        Ok(outcome)
    }

    /// Build a staged COSMIC keyboard pixel resize candidate without installing
    /// it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn keyboard_resize_on_candidate(
        &self,
        key: &DomainKey,
        original: &DomainLayout,
        subject: &WindowId,
        direction: Direction,
        mode: ResizeMode,
        press_index: u32,
        observed_focus: &WindowId,
        observed: &[ObservedWindow],
        fixed_admission: bool,
    ) -> Result<(DomainLayout, ResizeOutcome), LayoutFailure> {
        let refuse = LayoutFailure::Semantics;

        if !validate_domain_topology(key, original) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        if subject.0.is_empty() || !valid_observed_shapes(observed) {
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

        let focus_leaf = sync_domain_focus_from_window(&mut candidate, observed_focus)
            .or_else(|| candidate.focused_leaf.clone());
        let Some(focus_leaf) = focus_leaf else {
            return Err(refuse(RefusalKind::FocusMismatch));
        };
        let focused_window = candidate
            .leaves
            .iter()
            .find_map(|(window, leaf)| (leaf == &focus_leaf).then(|| window.clone()));
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

        let Some(tree) = candidate.tree.clone() else {
            return Err(refuse(RefusalKind::FocusMismatch));
        };
        let hints = hints_from_observed(observed);
        let accepted_geometry = project_resize_geometry(key, &tree, &candidate, &hints)
            .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;

        let step_px = self.policy.keyboard_step_px(press_index);
        let Some(derived) = derive_keyboard_pixel_shares(
            self.policy.as_ref(),
            &tree,
            &focus_leaf,
            direction,
            mode,
            step_px,
            &accepted_geometry,
        ) else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        let (target, new_shares) = match derived {
            PixelDerived::Unchanged => return Err(refuse(RefusalKind::Unchanged)),
            PixelDerived::BelowMinimum => return Err(refuse(RefusalKind::PairBelowMinimum)),
            PixelDerived::Planned { target, new_shares } => (target, new_shares),
        };
        let Some(updated_tree) = apply_resize_shares(&tree, &target.group_id, &new_shares) else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        if updated_tree == tree {
            return Err(refuse(RefusalKind::Unchanged));
        }
        candidate.tree = Some(updated_tree);
        if !validate_domain_topology(key, &candidate) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        let desired_geometry = project_resize_geometry(
            key,
            candidate.tree.as_ref().expect("tree set above"),
            &candidate,
            &hints,
        )
        .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
        if !keyboard_minimum_holds(
            self.policy.as_ref(),
            &desired_geometry,
            &target,
            key,
            Axis::for_direction(direction),
            mode,
        ) {
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

        let operation = build_resize_operation(
            key,
            &focus_leaf,
            &focused_window,
            direction,
            mode,
            &target,
            &new_shares,
        );

        settle_resize_commit_tail(&mut candidate, &focus_leaf, revision);

        Ok((
            candidate,
            ResizeOutcome {
                base_revision: base,
                revision,
                operation,
                geometry: desired_geometry,
                focus_leaf,
            },
        ))
    }

    /// COSMIC pointer edge resize for `subject` in the converged domain `key`.
    ///
    /// `subject` need not equal the active focused window, and overlay flags on
    /// other tiled members are tolerated while `subject` itself must remain
    /// unflagged and exceptions must match exact homing and flags. Active focus
    /// is preserved across the resize.
    #[cfg(test)]
    pub(crate) fn pointer_resize(
        &mut self,
        key: &DomainKey,
        subject: &WindowId,
        direction: Direction,
        proposed_boundary: i32,
        observed_focus: &WindowId,
        observed: &[ObservedWindow],
    ) -> Result<ResizeOutcome, LayoutFailure> {
        let Some(original) = self.domain(key) else {
            return Err(LayoutFailure::Semantics(RefusalKind::UnknownDomain));
        };
        let (candidate, outcome) = self.pointer_resize_on_candidate(
            key,
            original,
            subject,
            direction,
            proposed_boundary,
            observed_focus,
            observed,
        )?;
        *self.domains.get_mut(key).expect("validated domain") = candidate;
        Ok(outcome)
    }

    /// Build a staged COSMIC pointer edge resize candidate without installing
    /// it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn pointer_resize_on_candidate(
        &self,
        key: &DomainKey,
        original: &DomainLayout,
        subject: &WindowId,
        direction: Direction,
        proposed_boundary: i32,
        observed_focus: &WindowId,
        observed: &[ObservedWindow],
    ) -> Result<(DomainLayout, ResizeOutcome), LayoutFailure> {
        let refuse = LayoutFailure::Semantics;

        if !validate_domain_topology(key, original) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        if subject.0.is_empty() || !valid_observed_shapes(observed) {
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
        if !domain_pointer_observation_matches(key, &candidate, observed, subject) {
            return Err(refuse(RefusalKind::PartialObservation));
        }

        if !candidate.leaves.contains_key(subject) && !candidate.exceptions.contains_key(subject) {
            return Err(refuse(RefusalKind::UnknownWindow));
        }
        if candidate.exceptions.contains_key(subject) {
            return Err(refuse(RefusalKind::NotTiled));
        }

        let focus_leaf = sync_domain_focus_from_window(&mut candidate, observed_focus)
            .or_else(|| candidate.focused_leaf.clone());
        let Some(focus_leaf) = focus_leaf else {
            return Err(refuse(RefusalKind::FocusMismatch));
        };
        let focused_window = candidate
            .leaves
            .iter()
            .find_map(|(window, leaf)| (leaf == &focus_leaf).then(|| window.clone()));
        let Some(focused_window) = focused_window else {
            return Err(refuse(RefusalKind::NotTiled));
        };
        if candidate.exceptions.contains_key(&focused_window) {
            return Err(refuse(RefusalKind::NotTiled));
        }
        if focused_window == *subject
            && let Some(entry) = observed.iter().find(|w| w.window == focused_window)
            && entry.flags().any()
        {
            return Err(refuse(RefusalKind::NotTiled));
        }

        let Some(target_leaf) = candidate.leaves.get(subject).cloned() else {
            return Err(refuse(RefusalKind::UnknownWindow));
        };
        if let Some(entry) = observed.iter().find(|w| w.window == *subject)
            && entry.flags().any()
        {
            return Err(refuse(RefusalKind::NotTiled));
        }

        let wanted = Axis::for_direction(direction);
        let inside_work_area = match wanted {
            Axis::Horizontal => {
                proposed_boundary >= candidate.bounds.x
                    && proposed_boundary <= candidate.bounds.x + candidate.bounds.w
            }
            Axis::Vertical => {
                proposed_boundary >= candidate.bounds.y
                    && proposed_boundary <= candidate.bounds.y + candidate.bounds.h
            }
        };
        if !inside_work_area {
            return Err(refuse(RefusalKind::MalformedInput));
        }

        let Some(tree) = candidate.tree.clone() else {
            return Err(refuse(RefusalKind::FocusMismatch));
        };
        if !collect_leaves(&tree).contains(&target_leaf) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        let hints = hints_from_observed(observed);
        let accepted_geometry = project_resize_geometry(key, &tree, &candidate, &hints)
            .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;

        let Some(pointer) = derive_pointer_shares(
            self.policy.as_ref(),
            &tree,
            &target_leaf,
            direction,
            proposed_boundary,
            candidate.gap,
            &accepted_geometry,
        ) else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        let (target, new_shares, effective_boundary, mode) = match pointer {
            PointerDerived::Unchanged => return Err(refuse(RefusalKind::Unchanged)),
            PointerDerived::Planned {
                target,
                new_shares,
                effective_boundary,
                mode,
            } => (target, new_shares, effective_boundary, mode),
        };
        let Some(updated_tree) = apply_resize_shares(&tree, &target.group_id, &new_shares) else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        if updated_tree == tree {
            return Err(refuse(RefusalKind::Unchanged));
        }
        candidate.tree = Some(updated_tree);
        if !validate_domain_topology(key, &candidate) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        let desired_tree = candidate.tree.as_ref().expect("tree set above");
        let desired_geometry = project_resize_geometry(key, desired_tree, &candidate, &hints)
            .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
        if !pointer_minimum_holds(
            self.policy.as_ref(),
            &desired_geometry,
            &target,
            key,
            wanted,
        ) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        let Some(projected_boundary) =
            pointer_projected_boundary(&desired_geometry, desired_tree, &target, wanted)
        else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        if projected_boundary != effective_boundary {
            return Err(refuse(RefusalKind::Unchanged));
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

        let operation = build_resize_operation(
            key,
            &target_leaf,
            subject,
            direction,
            mode,
            &target,
            &new_shares,
        );

        settle_resize_commit_tail(&mut candidate, &focus_leaf, revision);

        Ok((
            candidate,
            ResizeOutcome {
                base_revision: base,
                revision,
                operation,
                geometry: desired_geometry,
                focus_leaf,
            },
        ))
    }

    /// Atomic dual-axis (corner) COSMIC pointer pixel resize for `subject` in
    /// the converged domain `key`.
    ///
    /// The two perpendicular directions/boundaries may be supplied in either
    /// order; they are normalized once to horizontal-first, vertical-second.
    /// Same-axis directions refuse with `MalformedInput`. Both axes run on one
    /// local candidate with intermediate reprojection before the vertical axis
    /// derives, and any refusal leaves both axes and all domain focus/history/
    /// revision state untouched.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pointer_resize_corner(
        &mut self,
        key: &DomainKey,
        subject: &WindowId,
        direction_1: Direction,
        boundary_1: i32,
        direction_2: Direction,
        boundary_2: i32,
        observed_focus: &WindowId,
        observed: &[ObservedWindow],
    ) -> Result<PointerCornerResizeOutcome, LayoutFailure> {
        if Axis::for_direction(direction_1) == Axis::for_direction(direction_2) {
            return Err(LayoutFailure::Semantics(RefusalKind::MalformedInput));
        }
        let Some(original) = self.domain(key) else {
            return Err(LayoutFailure::Semantics(RefusalKind::UnknownDomain));
        };
        let (candidate, outcome) = self.pointer_resize_corner_on_candidate(
            key,
            original,
            subject,
            direction_1,
            boundary_1,
            direction_2,
            boundary_2,
            observed_focus,
            observed,
        )?;
        *self.domains.get_mut(key).expect("validated domain") = candidate;
        Ok(outcome)
    }

    /// Build a staged atomic dual-axis (corner) COSMIC pointer pixel resize
    /// candidate without installing it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn pointer_resize_corner_on_candidate(
        &self,
        key: &DomainKey,
        original: &DomainLayout,
        subject: &WindowId,
        direction_1: Direction,
        boundary_1: i32,
        direction_2: Direction,
        boundary_2: i32,
        observed_focus: &WindowId,
        observed: &[ObservedWindow],
    ) -> Result<(DomainLayout, PointerCornerResizeOutcome), LayoutFailure> {
        let refuse = LayoutFailure::Semantics;

        if Axis::for_direction(direction_1) == Axis::for_direction(direction_2) {
            return Err(refuse(RefusalKind::MalformedInput));
        }
        let (direction_h, boundary_h, direction_v, boundary_v) =
            if Axis::for_direction(direction_1) == Axis::Horizontal {
                (direction_1, boundary_1, direction_2, boundary_2)
            } else {
                (direction_2, boundary_2, direction_1, boundary_1)
            };

        if !validate_domain_topology(key, original) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        if subject.0.is_empty() || !valid_observed_shapes(observed) {
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
        if !domain_pointer_observation_matches(key, &candidate, observed, subject) {
            return Err(refuse(RefusalKind::PartialObservation));
        }

        if !candidate.leaves.contains_key(subject) && !candidate.exceptions.contains_key(subject) {
            return Err(refuse(RefusalKind::UnknownWindow));
        }
        if candidate.exceptions.contains_key(subject) {
            return Err(refuse(RefusalKind::NotTiled));
        }

        let focus_leaf = sync_domain_focus_from_window(&mut candidate, observed_focus)
            .or_else(|| candidate.focused_leaf.clone());
        let Some(focus_leaf) = focus_leaf else {
            return Err(refuse(RefusalKind::FocusMismatch));
        };
        let focused_window = candidate
            .leaves
            .iter()
            .find_map(|(window, leaf)| (leaf == &focus_leaf).then(|| window.clone()));
        let Some(focused_window) = focused_window else {
            return Err(refuse(RefusalKind::NotTiled));
        };
        if candidate.exceptions.contains_key(&focused_window) {
            return Err(refuse(RefusalKind::NotTiled));
        }
        if focused_window == *subject
            && let Some(entry) = observed.iter().find(|w| w.window == focused_window)
            && entry.flags().any()
        {
            return Err(refuse(RefusalKind::NotTiled));
        }

        let Some(target_leaf) = candidate.leaves.get(subject).cloned() else {
            return Err(refuse(RefusalKind::UnknownWindow));
        };
        if let Some(entry) = observed.iter().find(|w| w.window == *subject)
            && entry.flags().any()
        {
            return Err(refuse(RefusalKind::NotTiled));
        }

        let inside_h = boundary_h >= candidate.bounds.x
            && boundary_h <= candidate.bounds.x + candidate.bounds.w;
        let inside_v = boundary_v >= candidate.bounds.y
            && boundary_v <= candidate.bounds.y + candidate.bounds.h;
        if !inside_h || !inside_v {
            return Err(refuse(RefusalKind::MalformedInput));
        }

        let Some(tree) = candidate.tree.clone() else {
            return Err(refuse(RefusalKind::FocusMismatch));
        };
        if !collect_leaves(&tree).contains(&target_leaf) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        let hints = hints_from_observed(observed);
        let accepted_geometry = project_resize_geometry(key, &tree, &candidate, &hints)
            .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;

        let Some(pointer_h) = derive_pointer_shares(
            self.policy.as_ref(),
            &tree,
            &target_leaf,
            direction_h,
            boundary_h,
            candidate.gap,
            &accepted_geometry,
        ) else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        let (target_h, shares_h, effective_h, mode_h) = match pointer_h {
            PointerDerived::Unchanged => return Err(refuse(RefusalKind::Unchanged)),
            PointerDerived::Planned {
                target,
                new_shares,
                effective_boundary,
                mode,
            } => (target, new_shares, effective_boundary, mode),
        };
        let Some(intermediate_tree) = apply_resize_shares(&tree, &target_h.group_id, &shares_h)
        else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };

        let intermediate_geometry =
            project_resize_geometry(key, &intermediate_tree, &candidate, &hints)
                .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;

        let Some(pointer_v) = derive_pointer_shares(
            self.policy.as_ref(),
            &intermediate_tree,
            &target_leaf,
            direction_v,
            boundary_v,
            candidate.gap,
            &intermediate_geometry,
        ) else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        let (target_v, shares_v, effective_v, mode_v) = match pointer_v {
            PointerDerived::Unchanged => return Err(refuse(RefusalKind::Unchanged)),
            PointerDerived::Planned {
                target,
                new_shares,
                effective_boundary,
                mode,
            } => (target, new_shares, effective_boundary, mode),
        };
        let Some(updated_tree) =
            apply_resize_shares(&intermediate_tree, &target_v.group_id, &shares_v)
        else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        if updated_tree == tree {
            return Err(refuse(RefusalKind::Unchanged));
        }
        candidate.tree = Some(updated_tree);
        if !validate_domain_topology(key, &candidate) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }

        let desired_tree = candidate.tree.as_ref().expect("tree set above");
        let desired_geometry = project_resize_geometry(key, desired_tree, &candidate, &hints)
            .ok_or_else(|| refuse(RefusalKind::MalformedTopology))?;
        if !pointer_minimum_holds(
            self.policy.as_ref(),
            &desired_geometry,
            &target_h,
            key,
            Axis::Horizontal,
        ) || !pointer_minimum_holds(
            self.policy.as_ref(),
            &desired_geometry,
            &target_v,
            key,
            Axis::Vertical,
        ) {
            return Err(refuse(RefusalKind::MalformedTopology));
        }
        let (Some(projected_h), Some(projected_v)) = (
            pointer_projected_boundary(
                &desired_geometry,
                desired_tree,
                &target_h,
                Axis::Horizontal,
            ),
            pointer_projected_boundary(&desired_geometry, desired_tree, &target_v, Axis::Vertical),
        ) else {
            return Err(refuse(RefusalKind::MalformedTopology));
        };
        if projected_h != effective_h || projected_v != effective_v {
            return Err(refuse(RefusalKind::Unchanged));
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

        let operation_h = build_resize_operation(
            key,
            &target_leaf,
            subject,
            direction_h,
            mode_h,
            &target_h,
            &shares_h,
        );
        let operation_v = build_resize_operation(
            key,
            &target_leaf,
            subject,
            direction_v,
            mode_v,
            &target_v,
            &shares_v,
        );

        settle_resize_commit_tail(&mut candidate, &focus_leaf, revision);

        Ok((
            candidate,
            PointerCornerResizeOutcome {
                base_revision: base,
                revision,
                direction_h,
                boundary_h,
                operation_h,
                direction_v,
                boundary_v,
                operation_v,
                geometry: desired_geometry,
                focus_leaf,
            },
        ))
    }
}

fn hints_from_observed(observed: &[ObservedWindow]) -> BTreeMap<WindowId, WindowSizeHints> {
    observed
        .iter()
        .map(|entry| (entry.window.clone(), entry.hints))
        .collect()
}

fn project_resize_geometry(
    key: &DomainKey,
    tree: &Node,
    domain: &DomainLayout,
    hints: &BTreeMap<WindowId, WindowSizeHints>,
) -> Option<Vec<DesiredGeometry>> {
    let mut geometry =
        project_borrowed_geometry(key, tree, &domain.leaves, domain.bounds, domain.gap, hints)?;
    sort_geometry(&mut geometry);
    if geometry.len() != domain.leaves.len() {
        return None;
    }
    Some(geometry)
}

fn build_resize_operation(
    key: &DomainKey,
    target_leaf: &NodeId,
    subject: &WindowId,
    direction: Direction,
    mode: ResizeMode,
    target: &PointerTarget,
    new_shares: &[u64],
) -> ResizeOperation {
    ResizeOperation {
        domain_output: key.output.clone(),
        domain_workspace: key.workspace.clone(),
        focused_leaf: target_leaf.clone(),
        focused_window: subject.clone(),
        direction,
        mode,
        target_group: target.group_id.clone(),
        focused_child: target.focused_child.clone(),
        neighbor_child: target.neighbor_child.clone(),
        focused_index: target.focused_index,
        neighbor_index: target.neighbor_index,
        old_shares: target.old_shares.clone(),
        new_shares: new_shares.to_vec(),
    }
}

/// Exact `verify_resize` commit tail:
/// - sets `focused_leaf`,
/// - prunes stale leaves from `focus_stack` and unconditionally demotes/appends `focused_leaf`,
/// - prunes `focus_history` without appending,
/// - preserves `last_active`, `capacity_floats`, and `admission_order`,
/// - prunes fixed-size provenance only (`prune_domain_fixed_size_state`),
/// - advances `accepted_revision`.
fn settle_resize_commit_tail(domain: &mut DomainLayout, focused_leaf: &NodeId, revision: u64) {
    let valid_leaves: BTreeSet<NodeId> = domain
        .tree
        .as_ref()
        .map(|tree| collect_leaves(tree).into_iter().collect())
        .unwrap_or_default();
    let linked_leaves: BTreeSet<NodeId> = domain.leaves.values().cloned().collect();
    let resolves = |leaf: &NodeId| valid_leaves.contains(leaf) && linked_leaves.contains(leaf);

    domain.focus_stack.retain(resolves);
    domain.focus_stack.retain(|leaf| leaf != focused_leaf);
    domain.focus_stack.push(focused_leaf.clone());

    domain.focus_history.retain(resolves);
    domain.focused_leaf = Some(focused_leaf.clone());
    WorldLayout::prune_domain_fixed_size_state(domain);
    domain.accepted_revision = revision;
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PointerTarget {
    group_id: NodeId,
    focused_index: usize,
    neighbor_index: usize,
    focused_child: NodeId,
    neighbor_child: NodeId,
    old_shares: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PointerDerived {
    Unchanged,
    Planned {
        target: PointerTarget,
        new_shares: Vec<u64>,
        effective_boundary: i32,
        mode: ResizeMode,
    },
}

struct PointerLevel {
    group_id: NodeId,
    axis: Axis,
    children: Vec<NodeId>,
    shares: Vec<u64>,
    child_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PixelDerived {
    Unchanged,
    BelowMinimum,
    Planned {
        target: PointerTarget,
        new_shares: Vec<u64>,
    },
}

fn subtree_contains(node: &Node, target: &NodeId) -> bool {
    match node {
        Node::Leaf { id } => id == target,
        Node::Group { id, children, .. } => {
            id == target || children.iter().any(|c| subtree_contains(c, target))
        }
    }
}

fn find_node_by_id<'a>(node: &'a Node, target: &NodeId) -> Option<&'a Node> {
    if node.id() == target {
        return Some(node);
    }
    match node {
        Node::Leaf { .. } => None,
        Node::Group { children, .. } => children
            .iter()
            .find_map(|child| find_node_by_id(child, target)),
    }
}

fn pointer_path(node: &Node, focused: &NodeId, out: &mut Vec<PointerLevel>) -> bool {
    match node {
        Node::Leaf { .. } => false,
        Node::Group {
            id,
            axis,
            children,
            shares,
        } => {
            for (index, child) in children.iter().enumerate() {
                if child.id() == focused {
                    out.push(PointerLevel {
                        group_id: id.clone(),
                        axis: *axis,
                        children: children.iter().map(|c| c.id().clone()).collect(),
                        shares: shares.clone(),
                        child_index: index,
                    });
                    return true;
                }
                if subtree_contains(child, focused) {
                    let found = pointer_path(child, focused, out);
                    if found {
                        out.push(PointerLevel {
                            group_id: id.clone(),
                            axis: *axis,
                            children: children.iter().map(|c| c.id().clone()).collect(),
                            shares: shares.clone(),
                            child_index: index,
                        });
                        return true;
                    }
                    return false;
                }
            }
            false
        }
    }
}

fn pointer_child_extent(
    tree: &Node,
    child: &NodeId,
    axis: Axis,
    geometry: &[DesiredGeometry],
) -> Option<i64> {
    let mut leaves = Vec::new();
    collect_pointer_leaves(tree, child, &mut leaves);
    if leaves.is_empty() {
        return None;
    }
    let mut positions: Vec<(i32, i32)> = Vec::new();
    for leaf in &leaves {
        let entry = geometry.iter().find(|g| &g.leaf == leaf)?;
        positions.push(match axis {
            Axis::Horizontal => (entry.rect.x, entry.rect.w),
            Axis::Vertical => (entry.rect.y, entry.rect.h),
        });
        if entry.rect.w <= 0 || entry.rect.h <= 0 {
            return None;
        }
    }
    let min = positions.iter().map(|(o, _)| i64::from(*o)).min()?;
    let max = positions
        .iter()
        .map(|(o, e)| i64::from(*o) + i64::from(*e))
        .max()?;
    Some(max - min)
}

fn collect_pointer_leaves(node: &Node, target: &NodeId, out: &mut Vec<NodeId>) {
    if node.id() == target {
        for id in collect_leaves(node) {
            out.push(id);
        }
        return;
    }
    if let Node::Group { children, .. } = node {
        for child in children {
            if subtree_contains(child, target) || child.id() == target {
                collect_pointer_leaves(child, target, out);
            }
        }
    }
}

fn pointer_group_origin(
    tree: &Node,
    group: &NodeId,
    axis: Axis,
    geometry: &[DesiredGeometry],
) -> Option<i64> {
    let mut leaves = Vec::new();
    collect_pointer_group_leaves(tree, group, &mut leaves);
    if leaves.is_empty() {
        return None;
    }
    let mut min: Option<i64> = None;
    for leaf in &leaves {
        let entry = geometry.iter().find(|g| &g.leaf == leaf)?;
        let origin = match axis {
            Axis::Horizontal => i64::from(entry.rect.x),
            Axis::Vertical => i64::from(entry.rect.y),
        };
        min = Some(min.map_or(origin, |m: i64| m.min(origin)));
    }
    min
}

fn collect_pointer_group_leaves(node: &Node, group: &NodeId, out: &mut Vec<NodeId>) {
    if node.id() == group {
        for leaf in collect_leaves(node) {
            out.push(leaf);
        }
        return;
    }
    if let Node::Group { children, .. } = node {
        for child in children {
            if subtree_contains(child, group) || child.id() == group {
                collect_pointer_group_leaves(child, group, out);
            }
        }
    }
}

fn pixel_shares_for_clamped(
    shares: &[u64],
    lo: usize,
    clamped_left: i64,
    distributable: i64,
    old_total: u64,
    old_pair: u64,
) -> Option<Vec<u64>> {
    if distributable <= 0 || old_total == 0 || old_pair == 0 || clamped_left <= 0 {
        return None;
    }
    let distributable_u = u128::from(distributable as u64);
    let old_total_u = u128::from(old_total);
    let required_k = distributable_u
        .div_ceil(old_total_u)
        .min(u128::from(u64::MAX)) as u64;
    let mut candidates = [1u64, required_k];
    candidates.sort_unstable();
    let left_u = u128::from(clamped_left as u64);
    let q = left_u - 1;
    for scale in candidates.into_iter().filter(|k| *k >= 1) {
        if scale == 0 {
            continue;
        }
        let scaled_total = shares
            .iter()
            .try_fold(0u64, |acc, s| acc.checked_add(s.checked_mul(scale)?))?;
        let scaled_pair = old_pair.checked_mul(scale)?;
        if scaled_total == 0 || scaled_pair == 0 {
            continue;
        }
        let mut scaled_ok = true;
        for (index, share) in shares.iter().enumerate() {
            if index == lo || index == lo + 1 {
                continue;
            }
            if share.checked_mul(scale).is_none() {
                scaled_ok = false;
                break;
            }
        }
        if !scaled_ok {
            continue;
        }
        let total_prime = u128::from(scaled_total);
        let pair_prime = u128::from(scaled_pair);
        let lower = (q * total_prime).div_ceil(distributable_u);
        let upper = ((q + 1) * total_prime - 1) / distributable_u;
        if lower > upper {
            continue;
        }
        let lower = lower.max(1);
        let upper = upper.min(pair_prime - 1);
        if lower > upper {
            continue;
        }
        let left_new_u64: u64 = lower.try_into().ok()?;
        let right_new_u64 = scaled_pair.checked_sub(left_new_u64)?;
        if right_new_u64 < 1 {
            continue;
        }
        let scaled_lo = shares[lo].checked_mul(scale)?;
        if left_new_u64 == scaled_lo {
            continue;
        }
        let mut new_shares = Vec::with_capacity(shares.len());
        for (index, share) in shares.iter().enumerate() {
            let value = if index == lo {
                left_new_u64
            } else if index == lo + 1 {
                right_new_u64
            } else {
                share.checked_mul(scale)?
            };
            if value == 0 {
                return None;
            }
            new_shares.push(value);
        }
        return Some(new_shares);
    }
    None
}

fn derive_keyboard_pixel_shares(
    policy: &dyn LayoutPolicy,
    tree: &Node,
    focused_leaf: &NodeId,
    edge: Direction,
    mode: ResizeMode,
    step_px: i32,
    geometry: &[DesiredGeometry],
) -> Option<PixelDerived> {
    if focused_leaf.0.is_empty() || step_px <= 0 {
        return None;
    }
    let wanted = Axis::for_direction(edge);
    let step: i32 = match edge {
        Direction::Right | Direction::Down => 1,
        Direction::Left | Direction::Up => -1,
    };
    let mut levels = Vec::new();
    if !pointer_path(tree, focused_leaf, &mut levels) {
        return Some(PixelDerived::Unchanged);
    }
    let mut feasible_exhausted = false;
    let mut threshold_exhausted = false;
    for level in &levels {
        if level.axis != wanted {
            continue;
        }
        let neighbor = level.child_index as i32 + step;
        if neighbor < 0 || (neighbor as usize) >= level.children.len() {
            continue;
        }
        let neighbor_index = neighbor as usize;
        let focused_index = level.child_index;
        if level.shares.len() != level.children.len()
            || focused_index >= level.shares.len()
            || neighbor_index >= level.shares.len()
            || level.shares.contains(&0)
        {
            return None;
        }
        let lo = focused_index.min(neighbor_index);
        if focused_index.abs_diff(neighbor_index) != 1 {
            return None;
        }
        let mut sizes: Vec<i64> = Vec::with_capacity(level.children.len());
        for child in &level.children {
            sizes.push(pointer_child_extent(tree, child, wanted, geometry)?);
            if sizes.last().is_some_and(|s| *s <= 0) {
                return None;
            }
        }
        let pair_sum = sizes[focused_index].checked_add(sizes[neighbor_index])?;
        if !policy.pair_admits_resize(pair_sum, pair_sum, wanted) {
            threshold_exhausted = true;
            continue;
        }
        let focused_size = sizes[focused_index];
        let neighbor_size = sizes[neighbor_index];
        let (shrink, grow) = if mode == ResizeMode::Inwards {
            (focused_size, neighbor_size)
        } else {
            (neighbor_size, focused_size)
        };
        let Some((new_shrink, new_grow)) =
            policy.clamp_keyboard_shrink_pair(shrink, grow, i64::from(step_px), wanted)
        else {
            if pair_sum < policy.pair_min_for_axis(wanted) {
                threshold_exhausted = true;
            } else {
                feasible_exhausted = true;
            }
            continue;
        };
        if new_shrink == shrink && new_grow == grow {
            feasible_exhausted = true;
            continue;
        }
        let focused_is_left = focused_index == lo;
        let clamped_left = if focused_is_left {
            if mode == ResizeMode::Inwards {
                new_shrink
            } else {
                new_grow
            }
        } else if mode == ResizeMode::Inwards {
            new_grow
        } else {
            new_shrink
        };
        let current_left = sizes[lo];
        if clamped_left == current_left {
            feasible_exhausted = true;
            continue;
        }
        let mut avail_total: i64 = 0;
        for size in &sizes {
            avail_total = avail_total.checked_add(*size)?;
        }
        let distributable = avail_total.checked_sub(sizes.len() as i64)?;
        let mut old_total: u64 = 0;
        for share in &level.shares {
            old_total = old_total.checked_add(*share)?;
        }
        let old_pair = level.shares[focused_index].checked_add(level.shares[neighbor_index])?;
        let Some(new_shares) = pixel_shares_for_clamped(
            &level.shares,
            lo,
            clamped_left,
            distributable,
            old_total,
            old_pair,
        ) else {
            feasible_exhausted = true;
            continue;
        };
        return Some(PixelDerived::Planned {
            target: PointerTarget {
                group_id: level.group_id.clone(),
                focused_index,
                neighbor_index,
                focused_child: level.children[focused_index].clone(),
                neighbor_child: level.children[neighbor_index].clone(),
                old_shares: level.shares.clone(),
            },
            new_shares,
        });
    }
    if threshold_exhausted {
        return Some(PixelDerived::BelowMinimum);
    }
    if feasible_exhausted {
        return Some(PixelDerived::Unchanged);
    }
    Some(PixelDerived::Unchanged)
}

fn derive_pointer_shares(
    policy: &dyn LayoutPolicy,
    tree: &Node,
    focused_leaf: &NodeId,
    direction: Direction,
    proposed_boundary: i32,
    gap: i32,
    geometry: &[DesiredGeometry],
) -> Option<PointerDerived> {
    if focused_leaf.0.is_empty() {
        return None;
    }
    let wanted = Axis::for_direction(direction);
    let step: i32 = match direction {
        Direction::Right | Direction::Down => 1,
        Direction::Left | Direction::Up => -1,
    };
    let mut levels = Vec::new();
    if !pointer_path(tree, focused_leaf, &mut levels) {
        return Some(PointerDerived::Unchanged);
    }
    let mut feasible_exhausted = false;
    for level in &levels {
        if level.axis != wanted {
            continue;
        }
        let neighbor = level.child_index as i32 + step;
        if neighbor < 0 || (neighbor as usize) >= level.children.len() {
            continue;
        }
        let neighbor_index = neighbor as usize;
        let focused_index = level.child_index;
        if level.shares.len() != level.children.len()
            || focused_index >= level.shares.len()
            || neighbor_index >= level.shares.len()
            || level.shares.contains(&0)
        {
            return None;
        }
        let lo = focused_index.min(neighbor_index);
        if focused_index.abs_diff(neighbor_index) != 1 {
            return None;
        }
        let mut sizes: Vec<i64> = Vec::with_capacity(level.children.len());
        for child in &level.children {
            sizes.push(pointer_child_extent(tree, child, wanted, geometry)?);
            if sizes.last() == Some(&0) || sizes.last().is_some_and(|s| *s <= 0) {
                return None;
            }
        }
        let pair_avail = sizes[focused_index].checked_add(sizes[neighbor_index])?;
        if !policy.pair_admits_resize(pair_avail, pair_avail, wanted) {
            feasible_exhausted = true;
            continue;
        }
        let group_origin = pointer_group_origin(tree, &level.group_id, wanted, geometry)?;
        let gap = i64::from(gap);
        let mut prefix: i64 = 0;
        for size in sizes.iter().take(lo) {
            prefix = prefix.checked_add(*size)?.checked_add(gap)?;
        }
        let pair_start = group_origin.checked_add(prefix)?;
        let left_desired = i64::from(proposed_boundary).checked_sub(pair_start)?;
        let clamped_left = match policy.clamp_pair_split(pair_avail, left_desired, wanted) {
            Some((first, _)) => first,
            None => {
                feasible_exhausted = true;
                continue;
            }
        };
        if clamped_left == sizes[lo] {
            feasible_exhausted = true;
            continue;
        }
        let mut avail_total: i64 = 0;
        for size in &sizes {
            avail_total = avail_total.checked_add(*size)?;
        }
        let distributable = avail_total.checked_sub(sizes.len() as i64)?;
        let mut old_total: u64 = 0;
        for share in &level.shares {
            old_total = old_total.checked_add(*share)?;
        }
        let old_pair = level.shares[focused_index].checked_add(level.shares[neighbor_index])?;
        let Some(new_shares) = pixel_shares_for_clamped(
            &level.shares,
            lo,
            clamped_left,
            distributable,
            old_total,
            old_pair,
        ) else {
            feasible_exhausted = true;
            continue;
        };
        let effective_boundary = i32::try_from(pair_start.checked_add(clamped_left)?).ok()?;
        let focused_is_left = focused_index == lo;
        let focused_grows = if focused_is_left {
            clamped_left > sizes[lo]
        } else {
            clamped_left < sizes[lo]
        };
        let mode = if focused_grows {
            ResizeMode::Outwards
        } else {
            ResizeMode::Inwards
        };
        let target = PointerTarget {
            group_id: level.group_id.clone(),
            focused_index,
            neighbor_index,
            focused_child: level.children[focused_index].clone(),
            neighbor_child: level.children[neighbor_index].clone(),
            old_shares: level.shares.clone(),
        };
        return Some(PointerDerived::Planned {
            target,
            new_shares,
            effective_boundary,
            mode,
        });
    }
    if feasible_exhausted {
        return Some(PointerDerived::Unchanged);
    }
    Some(PointerDerived::Unchanged)
}

fn pointer_minimum_holds(
    policy: &dyn LayoutPolicy,
    geometry: &[DesiredGeometry],
    target: &PointerTarget,
    domain: &DomainKey,
    axis: Axis,
) -> bool {
    let mut by_leaf: BTreeMap<&NodeId, &DesiredGeometry> = BTreeMap::new();
    for entry in geometry {
        if entry.output != domain.output || entry.workspace != domain.workspace {
            continue;
        }
        if entry.rect.w <= 0 || entry.rect.h <= 0 {
            return false;
        }
        by_leaf.insert(&entry.leaf, entry);
    }
    let min = policy.child_min_for_axis(axis);
    for child in [&target.focused_child, &target.neighbor_child] {
        if let Some(entry) = by_leaf.get(child) {
            let span = match axis {
                Axis::Horizontal => i64::from(entry.rect.w),
                Axis::Vertical => i64::from(entry.rect.h),
            };
            if span < min {
                return false;
            }
        }
    }
    true
}

fn keyboard_minimum_holds(
    policy: &dyn LayoutPolicy,
    geometry: &[DesiredGeometry],
    target: &PointerTarget,
    domain: &DomainKey,
    axis: Axis,
    mode: ResizeMode,
) -> bool {
    let mut by_leaf: BTreeMap<&NodeId, &DesiredGeometry> = BTreeMap::new();
    for entry in geometry {
        if entry.output != domain.output || entry.workspace != domain.workspace {
            continue;
        }
        if entry.rect.w <= 0 || entry.rect.h <= 0 {
            return false;
        }
        by_leaf.insert(&entry.leaf, entry);
    }
    let min = policy.child_min_for_axis(axis);
    let shrink_child = if mode == ResizeMode::Inwards {
        &target.focused_child
    } else {
        &target.neighbor_child
    };
    if let Some(entry) = by_leaf.get(shrink_child) {
        let span = match axis {
            Axis::Horizontal => i64::from(entry.rect.w),
            Axis::Vertical => i64::from(entry.rect.h),
        };
        if span < min {
            return false;
        }
    }
    true
}

fn pointer_projected_boundary(
    geometry: &[DesiredGeometry],
    tree: &Node,
    target: &PointerTarget,
    axis: Axis,
) -> Option<i32> {
    let left_child = if target.focused_index < target.neighbor_index {
        &target.focused_child
    } else {
        &target.neighbor_child
    };
    let left_node = find_node_by_id(tree, left_child)?;
    let leaves = collect_leaves(left_node);
    if leaves.is_empty() {
        return None;
    }
    let mut by_leaf: BTreeMap<&NodeId, &DesiredGeometry> = BTreeMap::new();
    for entry in geometry {
        by_leaf.insert(&entry.leaf, entry);
    }
    let mut end: Option<i64> = None;
    for leaf in &leaves {
        let entry = by_leaf.get(leaf)?;
        let leaf_end = match axis {
            Axis::Horizontal => i64::from(entry.rect.x) + i64::from(entry.rect.w),
            Axis::Vertical => i64::from(entry.rect.y) + i64::from(entry.rect.h),
        };
        end = Some(end.map_or(leaf_end, |m: i64| m.max(leaf_end)));
    }
    let value = end?;
    i32::try_from(value).ok()
}
