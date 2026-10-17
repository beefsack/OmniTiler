//! Session parent split-axis toggle (R-LAY-01): flip the focused tiled
//! window's immediate parent group axis, including root.
//!
//! Responsibility: own the toggle `impl Session` family. World state,
//! shared projection/validation helpers, transaction mechanics, and tests
//! stay in the parent session module.
//!
//! Reference: COSMIC `TilingLayout::update_orientation` with no explicit
//! orientation (`tiling/mod.rs` 2089-2129): resolve the focused node's parent
//! group and flip its orientation. COSMIC rescales pixel `sizes` across the
//! flip; this port keeps proportional `shares` untouched (resolution
//! independent), preserving child order, shares, and focus exactly. Geometry
//! reprojects through the existing hints-aware projector, so client minima
//! behave exactly like every other workflow. No orientation hint is retained;
//! subsequent admissions keep the long-edge rule, and singleton collapse on
//! removal is unchanged (a toggle never removes, so it never collapses).

use super::super::*;

impl super::super::Session {
    /// Propose flipping the focused tiled window's immediate parent group
    /// axis for the supplied exact opaque window.
    ///
    /// Binding mirrors [`Session::propose_resize`]: the supplied window must
    /// equal the authoritative focused tiled window; unknown windows,
    /// exception (floating) windows, focus mismatch, and observed overlay
    /// flags (floating/fullscreen/maximized/sticky) on the focused entry
    /// refuse exactly like resize (`UnknownWindow`, `NotTiled`,
    /// `FocusMismatch`). Completeness (observed equals known tiled-plus-
    /// exception set) refuses as `PartialObservation` like removal. The domain
    /// is the authoritative focused domain (this command carries no domain);
    /// observed entries homed on any retained domain pass, others refuse as
    /// `CrossDomainMismatch`.
    ///
    /// A lone root leaf (focused leaf has no parent group) refuses as
    /// [`RefusalKind::Unchanged`] with no plan and no pending. Only an
    /// accepted plan stages pending desired state through the shared single
    /// reconciler slot, committing via acknowledge-then-`verify_lifecycle`.
    pub fn propose_toggle_orientation(
        &mut self,
        window: &WindowId,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &LifecycleCapabilities,
    ) -> Result<SessionPlan, ProposeError> {
        if let Some(reason) = self.reconciler.divergence() {
            return Err(ProposeError::Diverged(reason));
        }
        if self.has_pending() {
            return Err(ProposeError::PendingExists);
        }
        if self.drag.is_some() {
            return Err(ProposeError::PendingExists);
        }
        if !self.validate_current_topology() {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        if window.0.is_empty() {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        if !valid_observed_shapes(&session_observation.windows) {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        for entry in &session_observation.windows {
            if self.domain_for(&entry.output, &entry.workspace).is_none() {
                return Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch));
            }
        }
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
        if !self.windows.contains_key(window) && !self.exceptions.contains_key(window) {
            return Err(ProposeError::Refused(RefusalKind::UnknownWindow));
        }
        if self.exceptions.contains_key(window) {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        }
        let (Some(domain), Some(focused_leaf)) =
            (self.focused_domain.clone(), self.focused_leaf.clone())
        else {
            return Err(ProposeError::Refused(RefusalKind::FocusMismatch));
        };
        let Some(focused_window) = self.focused_window_for(&focused_leaf, &domain) else {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        };
        if self.exceptions.contains_key(&focused_window) {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        }
        if window != &focused_window {
            return Err(ProposeError::Refused(RefusalKind::FocusMismatch));
        }
        // Overlay flags on the focused entry refuse like resize: a
        // floating/fullscreen/maximized/sticky focus is not a tiled subject.
        // Focused-overlay and workspace-floating gates additionally stay in
        // the adapter (KWin `windowIsFullscreen`/`windowIsMaximized` focused-
        // id refusals plus the `isTiledDomain` gate), which never dispatches
        // those subjects here; sibling overlays ride as ordinary tiles.
        if let Some(entry) = session_observation
            .windows
            .iter()
            .find(|w| w.window == focused_window)
            && entry.flags().any()
        {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        }
        let Some(tree) = self.trees.get(&domain).cloned().flatten() else {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        };
        // Lone root leaf (or focused leaf with no parent group): no-op.
        let Some(parent) = direct_parent_of_leaf(&tree, &focused_leaf) else {
            return Err(ProposeError::Refused(RefusalKind::Unchanged));
        };
        let mut desired_tree = tree;
        if !flip_group_axis(&mut desired_tree, &parent) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let mut desired_trees = self.trees.clone();
        desired_trees.insert(domain.clone(), Some(desired_tree.clone()));
        let desired_windows = self.windows.clone();
        // Focus is preserved exactly: same domain, same leaf.
        let desired_focus_domain = Some(domain.clone());
        let desired_focus_leaf = Some(focused_leaf.clone());
        if !validate_topology(
            &self.domains,
            &desired_trees,
            &desired_windows,
            &self.exceptions,
            &desired_focus_domain,
            &desired_focus_leaf,
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        // Complete desired geometry reprojects through the existing
        // hints-aware projector, so client minima behave exactly like every
        // other workflow (satisfiable minimums take slack, unsatisfiable
        // windows keep the proportional fallback flagged overconstrained).
        // Gated so accepted overlap domains keep working.
        let desired_geometry = crate::session::capacity::project_affected_gated(
            &self.domains,
            &desired_trees,
            &desired_windows,
            std::slice::from_ref(&domain),
            &hints_from_observed(&session_observation.windows),
            &|key| {
                self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                    || self.overlap_accepted.contains(key)
            },
        )
        .ok_or(ProposeError::Refused(RefusalKind::MalformedTopology))?;
        let intent = LifecycleIntent::ToggleOrientation {
            window: window.clone(),
            output: domain.output.clone(),
            workspace: domain.workspace.clone(),
        };
        let operation = LifecycleOperation::ToggleOrientation {
            window: window.clone(),
            leaf: focused_leaf.clone(),
            group: parent,
            output: domain.output.clone(),
            workspace: domain.workspace.clone(),
        };
        let plan = LifecyclePlan::for_operation(intent, operation);
        let dispatch = match self.reconciler.propose_lifecycle(
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
        let desired_snapshot = self.snapshot_for(&desired_trees, &desired_windows);
        self.pending_desired = Some(PendingDesired {
            trees: desired_trees,
            windows: desired_windows,
            focused_domain: desired_focus_domain.clone(),
            focused_leaf: desired_focus_leaf.clone(),
            last_active: self.updated_last_active(
                &desired_focus_domain,
                &desired_focus_leaf,
                &self.trees,
                &self.windows,
            ),
            exceptions: self.exceptions.clone(),
            retained_float_geometry: self.retained_float_geometry.clone(),
            automatic_fixed: self.automatic_fixed.clone(),
            fixed_tile_override: self.fixed_tile_override.clone(),
            capacity_floats: self.capacity_floats.clone(),
            admission_order: self.admission_order.clone(),
            admission_counter: self.admission_counter,
            overlap_accepted: self.overlap_accepted.clone(),
        });
        Ok(SessionPlan {
            dispatch,
            desired_snapshot,
            desired_focus_domain,
            desired_focus_leaf,
            desired_geometry,
        })
    }
}

/// Flip one group's split axis in place, preserving child order and shares.
/// Returns false when the group id resolves to no group (never a leaf: the
/// caller passes the focused leaf's direct parent).
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
