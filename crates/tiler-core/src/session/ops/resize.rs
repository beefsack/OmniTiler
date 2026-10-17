//! Session resize operations: keyboard/pointer proposal and verification.
//!
//! Responsibility: own the resize `impl Session` family. World state,
//! shared projection/validation helpers, transaction mechanics, and tests
//! stay in the parent session module.

use super::super::*;

impl super::super::Session {
    /// Propose COSMIC keyboard pixel resize for the selected exact
    /// opaque `(domain, window)` pair.
    ///
    /// The supplied opaque [`WindowId`] must equal the authoritative logical
    /// focused tiled window in exactly `domain`; mismatch, unknown windows,
    /// or exception windows refuse without pending. Source selection
    /// (`tiling/mod.rs` 2514-2600): the nearest matching-edge-axis ancestor
    /// wins, `direction` is the cardinal edge determining the neighbor side
    /// and `mode` is the source `ResizeDirection` (`Inwards` shrinks the
    /// focused node, `Outwards` grows it). The [`crate::cosmic_v1`] keyboard
    /// step schedule moves the shared boundary by
    /// [`crate::cosmic_v1::keyboard_step_px`](`press_index`) physical pixels
    /// (12px first operation, then 14, 16, 18, 20), source pair/child minima
    /// ([`crate::cosmic_v1::pair_admits_resize`],
    /// [`crate::cosmic_v1::clamp_keyboard_shrink_pair`]) gate and clamp the
    /// move one-sided on direct pair sums `sizes[i] + sizes[i + 1]` (never union
    /// including gap), and the resulting pixel sizes are converted to exact
    /// projector-representable integer shares applied through the reusable
    /// [`crate::directional::apply_resize_shares`] primitive. Only the two
    /// selected adjacent shares change ratios (plus an exact
    /// ratio-preserving whole-group integer scaling when pixel precision
    /// requires it); descendants/topology/order are unchanged and focus is
    /// retained exactly. The session orchestrates; all COSMIC semantics live
    /// in [`crate::cosmic_v1`].
    ///
    /// `press_index` is the explicit portable key-repeat state: 0 is the
    /// initial press (12px), each subsequent held repeat increments by one.
    /// The adapter must supply `direction` (edge), `mode`, and `press_index`;
    /// none default.
    ///
    /// Complete geometry for every tiled window in the affected domain must
    /// project before any pending is staged; unprojectable/minimum-geometry
    /// failures refuse without pending. Source no-op pairs (direct sum under
    /// the axis pair minimum) refuse as [`RefusalKind::PairBelowMinimum`];
    /// exhausted/clamped-unchanged boundaries refuse as
    /// [`RefusalKind::Unchanged`] with no plan and no pending.
    /// Stale, incomplete, malformed, pending, or unsupported-resize-capability
    /// inputs refuse or diverge fail-closed.
    #[allow(clippy::too_many_arguments)]
    pub fn propose_resize(
        &mut self,
        domain: &DomainKey,
        window: &WindowId,
        direction: Direction,
        mode: ResizeMode,
        press_index: u32,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &ResizeCapabilities,
    ) -> Result<SessionResizePlan, ProposeError> {
        if let Some(reason) = self.reconciler.divergence() {
            return Err(ProposeError::Diverged(reason));
        }
        if self.has_pending() {
            return Err(ProposeError::PendingExists);
        }
        if self.drag.is_some() {
            return Err(ProposeError::PendingExists);
        }
        if self.domains.iter().find(|d| &d.key() == domain).is_none() {
            return Err(ProposeError::Refused(RefusalKind::UnknownDomain));
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
        // Exception/floating/fullscreen/maximized/sticky focus refuses: the
        // observed entry for the focused tiled window must carry no flags
        // (tiled bindings already checked via observed_known_match, but check
        // explicitly for a stable NotTiled classification).
        if let Some(entry) = session_observation
            .windows
            .iter()
            .find(|w| w.window == focused_window)
            && entry.flags().any()
        {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        }
        if !capabilities.supports(crate::contract::ResizeCapability::KeyboardResize) {
            return Err(ProposeError::Refused(RefusalKind::UnsupportedCapability));
        }
        let Some(own_domain) = self.domains.iter().find(|d| &d.key() == domain).cloned() else {
            return Err(ProposeError::Refused(RefusalKind::UnknownDomain));
        };
        let Some(tree) = self.trees.get(domain).cloned().flatten() else {
            return Err(ProposeError::Refused(RefusalKind::FocusMismatch));
        };
        // Accepted projected geometry is the normalized current geometry for
        // the COSMIC pixel policy. Gated so accepted overlap domains keep
        // working; unrepresentable holds.
        let hints = hints_from_observed(&session_observation.windows);
        let accepted_geometry = crate::session::capacity::project_affected_gated(
            &self.domains,
            &self.trees,
            &self.windows,
            std::slice::from_ref(domain),
            &hints,
            &|key| {
                self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                    || self.overlap_accepted.contains(key)
            },
        )
        .ok_or(ProposeError::Refused(RefusalKind::MalformedTopology))?;
        if !geometry_covers_affected(
            &accepted_geometry,
            &self.windows,
            std::slice::from_ref(domain),
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let step_px = self.policy().keyboard_step_px(press_index);
        let Some(derived) = derive_keyboard_pixel_shares(
            self.policy(),
            &tree,
            &focused_leaf,
            direction,
            mode,
            step_px,
            &own_domain,
            &accepted_geometry,
        ) else {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        };
        let (target, new_shares) = match derived {
            PixelDerived::Unchanged => {
                return Err(ProposeError::Refused(RefusalKind::Unchanged));
            }
            PixelDerived::BelowMinimum => {
                return Err(ProposeError::Refused(RefusalKind::PairBelowMinimum));
            }
            PixelDerived::Planned { target, new_shares } => (target, new_shares),
        };
        let Some(updated_tree) =
            crate::directional::apply_resize_shares(&tree, &target.group_id, &new_shares)
        else {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        };
        let mut desired_trees = self.trees.clone();
        desired_trees.insert(domain.clone(), Some(updated_tree));
        if desired_trees == self.trees {
            return Err(ProposeError::Refused(RefusalKind::Unchanged));
        }
        if !validate_topology(
            &self.domains,
            &desired_trees,
            &self.windows,
            &self.exceptions,
            &Some(domain.clone()),
            &Some(focused_leaf.clone()),
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let desired_geometry = crate::session::capacity::project_affected_gated(
            &self.domains,
            &desired_trees,
            &self.windows,
            std::slice::from_ref(domain),
            &hints_from_observed(&session_observation.windows),
            &|key| {
                self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                    || self.overlap_accepted.contains(key)
            },
        )
        .ok_or(ProposeError::Refused(RefusalKind::MalformedTopology))?;
        if !geometry_covers_affected(
            &desired_geometry,
            &self.windows,
            std::slice::from_ref(domain),
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        // Minimum-size projectability on the COSMIC axis minima (keyboard
        // one-sided: only the shrink side keeps the child minimum).
        if !keyboard_minimum_holds(
            self.policy(),
            &desired_geometry,
            &target,
            domain,
            Axis::for_direction(direction),
            mode,
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let intent = ResizeIntent {
            domain_output: domain.output.clone(),
            domain_workspace: domain.workspace.clone(),
            focused_leaf: focused_leaf.clone(),
            focused_window: focused_window.clone(),
            direction,
            mode,
        };
        let operation = ResizeOperation {
            domain_output: domain.output.clone(),
            domain_workspace: domain.workspace.clone(),
            focused_leaf: focused_leaf.clone(),
            focused_window: focused_window.clone(),
            direction,
            mode,
            target_group: target.group_id.clone(),
            focused_child: target.focused_child.clone(),
            neighbor_child: target.neighbor_child.clone(),
            focused_index: target.focused_index,
            neighbor_index: target.neighbor_index,
            old_shares: target.old_shares.clone(),
            new_shares: new_shares.clone(),
        };
        let plan = ResizePlan::for_operation(intent, operation);
        // Dedicated keyboard dispatch/reconcile path (one pending slot plus
        // exact ack/post verification shared with pointer via `verify_resize`).
        // Never routes through `propose_pointer_resize`: the one-sided
        // keyboard derivation above and the fixed share/semantic keyboard
        // operation validate here before commit.
        let dispatch = match self.reconciler.propose_resize(
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
        let desired_snapshot = self.snapshot_for(&desired_trees, &self.windows);
        self.pending_desired = Some(PendingDesired {
            trees: desired_trees.clone(),
            windows: self.windows.clone(),
            focused_domain: Some(domain.clone()),
            focused_leaf: Some(focused_leaf.clone()),
            last_active: self.last_active.clone(),
            exceptions: self.exceptions.clone(),
            retained_float_geometry: self.retained_float_geometry.clone(),
            automatic_fixed: self.automatic_fixed.clone(),
            fixed_tile_override: self.fixed_tile_override.clone(),
            capacity_floats: self.capacity_floats.clone(),
            admission_order: self.admission_order.clone(),
            admission_counter: self.admission_counter,
            overlap_accepted: self.overlap_accepted.clone(),
        });
        Ok(SessionResizePlan {
            dispatch,
            resize_plan: plan,
            secondary_plan: None,
            desired_snapshot,
            desired_focus_domain: domain.clone(),
            desired_focus_leaf: focused_leaf,
            desired_geometry,
        })
    }

    /// Commit a pending resize plan after acknowledgement. On commit only the
    /// resized shares apply atomically (windows/focus/exceptions unmodified)
    /// and the accepted revision advances by exactly one. Terminal divergence
    /// clears the pending desired state. Movement plans must use
    /// [`Session::verify_move`]; lifecycle plans must use
    /// [`Session::verify_lifecycle`]; focus plans must use
    /// [`Session::verify_focus`].
    pub fn verify_resize(&mut self, post: &ResizePostObservation) -> Result<Commit, VerifyError> {
        match self.reconciler.verify_resize(post) {
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

    /// Pointer-only observation binding tolerates overlay flags on tiled
    /// members other than the dragged window; keyboard paths remain strict.
    fn pointer_observation_matches(&self, observed: &[ObservedWindow], window: &WindowId) -> bool {
        let by_id: BTreeMap<&WindowId, &ObservedWindow> =
            observed.iter().map(|w| (&w.window, w)).collect();
        for (id, link) in &self.windows {
            let Some(entry) = by_id.get(id) else {
                return false;
            };
            if entry.output != link.output || entry.workspace != link.workspace {
                return false;
            }
            if *id == *window && entry.flags().any() {
                return false;
            }
        }
        for (id, record) in &self.exceptions {
            let Some(entry) = by_id.get(id) else {
                return false;
            };
            if entry.output != record.output
                || entry.workspace != record.workspace
                || entry.flags() != record.flags
            {
                return false;
            }
        }
        true
    }

    /// Propose COSMIC pointer pixel resize for the selected exact
    /// opaque `(domain, window)` pair.
    ///
    /// Rust derives the target matching-axis split boundary and the two
    /// adjacent shares itself; the caller never supplies shares. Inputs are
    /// the requested tiled window/domain (need not be the active window),
    /// the intentional `direction` selecting which adjacent boundary of the
    /// requested leaf moves, and the
    /// absolute `proposed_boundary` coordinate in domain work-area
    /// space along the matching axis (`x` for horizontal, `y` for vertical).
    /// Structural preconditions are the complete session observation plus
    /// the accepted topology/membership/capability binding; unlike
    /// [`Session::propose_resize`], the requested window need not equal the
    /// active focused window and active/remembered focus is preserved exactly.
    ///
    /// Derivation: nearest matching-axis ancestor with a direct neighbor in
    /// `direction` (keyboard ancestor rule, outward on exhausted pairs);
    /// the proposed coordinate is mapped to desired adjacent pixel extents
    /// inside that pair region, gated by the source pair minimum and clamped
    /// two-sided to the source child minima
    /// ([`crate::cosmic_v1::pair_admits_resize`],
    /// [`crate::cosmic_v1::clamp_pair_split`]), then converted to exact
    /// pixel-projectable integer shares (only the two adjacent shares change
    /// ratios, plus an exact ratio-preserving normalization of the rest of
    /// the selected group when precision requires it; topology/order/
    /// descendants unchanged, focus retained). The session orchestrates; all
    /// COSMIC semantics live in [`crate::cosmic_v1`]. KWin/JS share math is
    /// never trusted: computed shares pass through the shared
    /// [`crate::directional::apply_resize_shares`] primitive and the full
    /// projector before any pending is staged.
    ///
    /// Fail-closed: stale/revision/membership/domain/capability/pending
    /// inputs refuse or diverge like keyboard; proposed coordinates outside
    /// the domain work-area extent refuse as [`RefusalKind::MalformedInput`];
    /// unprojectable results refuse as [`RefusalKind::MalformedTopology`];
    /// source no-op pairs, exhausted minima, and clamped-unchanged boundaries
    /// refuse as [`RefusalKind::Unchanged`] with no plan and no pending. Commits only
    /// via acknowledge-then-[`Session::verify_resize`], sharing the keyboard
    /// reconciliation boundary and operation shape.
    #[allow(clippy::too_many_arguments)]
    pub fn propose_pointer_resize(
        &mut self,
        domain: &DomainKey,
        window: &WindowId,
        direction: Direction,
        proposed_boundary: i32,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &ResizeCapabilities,
    ) -> Result<SessionResizePlan, ProposeError> {
        if let Some(reason) = self.reconciler.divergence() {
            return Err(ProposeError::Diverged(reason));
        }
        if self.has_pending() {
            return Err(ProposeError::PendingExists);
        }
        if self.drag.is_some() {
            return Err(ProposeError::PendingExists);
        }
        let Some(own_domain) = self.domains.iter().find(|d| &d.key() == domain).cloned() else {
            return Err(ProposeError::Refused(RefusalKind::UnknownDomain));
        };
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
        if !self.pointer_observation_matches(&session_observation.windows, window) {
            return Err(ProposeError::Refused(RefusalKind::PartialObservation));
        }
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
        // Pointer-only decoupling: observed overlay flags on the ACTIVE window
        // never reject a valid inactive dragged target (active/remembered
        // focus is preserved exactly). The active check below only binds the
        // coincident case, where the dragged window's own check refuses
        // identically.
        if focused_window == *window
            && let Some(entry) = session_observation
                .windows
                .iter()
                .find(|w| w.window == focused_window)
            && entry.flags().any()
        {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        }
        // Pointer-only decoupling: the dragged window need not be active.
        // Derive the resize target from its retained link; active focus is
        // preserved exactly (never synced to the dragged window here).
        let Some(target_link) = self.windows.get(window).cloned() else {
            return Err(ProposeError::Refused(RefusalKind::UnknownWindow));
        };
        if target_link.output != domain.output || target_link.workspace != domain.workspace {
            return Err(ProposeError::Refused(RefusalKind::FocusMismatch));
        }
        let target_leaf = target_link.leaf.clone();
        if let Some(entry) = session_observation
            .windows
            .iter()
            .find(|w| w.window == *window)
            && entry.flags().any()
        {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        }
        if !capabilities.supports(crate::contract::ResizeCapability::PointerResize) {
            return Err(ProposeError::Refused(RefusalKind::UnsupportedCapability));
        }
        // Normalized coordinate must land inside the domain work-area extent
        // along the matching axis; outside is malformed, never clamped.
        let wanted = Axis::for_direction(direction);
        let inside_work_area = match wanted {
            Axis::Horizontal => {
                proposed_boundary >= own_domain.bounds.x
                    && proposed_boundary <= own_domain.bounds.x + own_domain.bounds.w
            }
            Axis::Vertical => {
                proposed_boundary >= own_domain.bounds.y
                    && proposed_boundary <= own_domain.bounds.y + own_domain.bounds.h
            }
        };
        if !inside_work_area {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        let Some(tree) = self.trees.get(domain).cloned().flatten() else {
            return Err(ProposeError::Refused(RefusalKind::FocusMismatch));
        };
        if !collect_leaves(&tree).contains(&target_leaf) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        // Project the accepted topology once: source of truth for group
        // extents and adjacent pixel sizes. Gated so accepted overlap
        // domains keep working; unrepresentable holds.
        let accepted_geometry = crate::session::capacity::project_affected_gated(
            &self.domains,
            &self.trees,
            &self.windows,
            std::slice::from_ref(domain),
            &hints_from_observed(&session_observation.windows),
            &|key| {
                self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                    || self.overlap_accepted.contains(key)
            },
        )
        .ok_or(ProposeError::Refused(RefusalKind::MalformedTopology))?;
        if !geometry_covers_affected(
            &accepted_geometry,
            &self.windows,
            std::slice::from_ref(domain),
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let Some(pointer) = derive_pointer_shares(
            self.policy(),
            &tree,
            &target_leaf,
            direction,
            proposed_boundary,
            &own_domain,
            &accepted_geometry,
        ) else {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        };
        let (target, new_shares, effective_boundary, mode) = match pointer {
            PointerDerived::Unchanged => {
                return Err(ProposeError::Refused(RefusalKind::Unchanged));
            }
            PointerDerived::Planned {
                target,
                new_shares,
                effective_boundary,
                mode,
            } => (target, new_shares, effective_boundary, mode),
        };
        let Some(updated_tree) =
            crate::directional::apply_resize_shares(&tree, &target.group_id, &new_shares)
        else {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        };
        let mut desired_trees = self.trees.clone();
        desired_trees.insert(domain.clone(), Some(updated_tree));
        if desired_trees == self.trees {
            return Err(ProposeError::Refused(RefusalKind::Unchanged));
        }
        if !validate_topology(
            &self.domains,
            &desired_trees,
            &self.windows,
            &self.exceptions,
            &Some(domain.clone()),
            &Some(focused_leaf.clone()),
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let desired_geometry = crate::session::capacity::project_affected_gated(
            &self.domains,
            &desired_trees,
            &self.windows,
            std::slice::from_ref(domain),
            &hints_from_observed(&session_observation.windows),
            &|key| {
                self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                    || self.overlap_accepted.contains(key)
            },
        )
        .ok_or(ProposeError::Refused(RefusalKind::MalformedTopology))?;
        if !geometry_covers_affected(
            &desired_geometry,
            &self.windows,
            std::slice::from_ref(domain),
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        // Minimum-size projectability: every desired leaf in the affected
        // domain keeps a positive extent along the resize axis with at least
        // the portable minimum on the two resized adjacent children.
        if !pointer_minimum_holds(self.policy(), &desired_geometry, &target, domain, wanted) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        // Exact projectability: the complete projected geometry must place
        // the effective (clamped) boundary exactly after integer projection
        // (including nested/N-ary/gaps). Unclamped proposals have
        // `effective == proposed`; clamped in-work-area proposals bind the
        // clamped edge. A mismatch means the clamped result cannot represent
        // the proposal; refuse as unchanged with no plan and no pending, so
        // the adapter never faces a post-observation mismatch for a
        // native-owned source showing the effective edge. Never snap to a
        // nearby boundary.
        let Some(projected_boundary) = pointer_projected_boundary(
            &desired_geometry,
            desired_trees
                .get(domain)
                .cloned()
                .flatten()
                .as_ref()
                .expect("desired tree present"),
            &target,
            wanted,
        ) else {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        };
        if projected_boundary != effective_boundary {
            return Err(ProposeError::Refused(RefusalKind::Unchanged));
        }
        let intent = ResizeIntent {
            domain_output: domain.output.clone(),
            domain_workspace: domain.workspace.clone(),
            focused_leaf: target_leaf.clone(),
            focused_window: window.clone(),
            direction,
            mode,
        };
        let operation = ResizeOperation {
            domain_output: domain.output.clone(),
            domain_workspace: domain.workspace.clone(),
            focused_leaf: target_leaf.clone(),
            focused_window: window.clone(),
            direction,
            mode,
            target_group: target.group_id.clone(),
            focused_child: target.focused_child.clone(),
            neighbor_child: target.neighbor_child.clone(),
            focused_index: target.focused_index,
            neighbor_index: target.neighbor_index,
            old_shares: target.old_shares.clone(),
            new_shares: new_shares.clone(),
        };
        // Pointer route binds `PointerResize` explicitly: the shared operation
        // shape reports `KeyboardResize` via `required_capability()`, so the
        // plan is constructed directly instead of via `for_operation`.
        let plan = ResizePlan {
            intent,
            operation: operation.clone(),
            required_capability: crate::contract::ResizeCapability::PointerResize,
            preconditions: operation.preconditions(),
        };
        let dispatch = match self.reconciler.propose_pointer_resize(
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
        let desired_snapshot = self.snapshot_for(&desired_trees, &self.windows);
        self.pending_desired = Some(PendingDesired {
            trees: desired_trees.clone(),
            windows: self.windows.clone(),
            focused_domain: Some(domain.clone()),
            focused_leaf: Some(focused_leaf.clone()),
            last_active: self.last_active.clone(),
            exceptions: self.exceptions.clone(),
            retained_float_geometry: self.retained_float_geometry.clone(),
            automatic_fixed: self.automatic_fixed.clone(),
            fixed_tile_override: self.fixed_tile_override.clone(),
            capacity_floats: self.capacity_floats.clone(),
            admission_order: self.admission_order.clone(),
            admission_counter: self.admission_counter,
            overlap_accepted: self.overlap_accepted.clone(),
        });
        Ok(SessionResizePlan {
            dispatch,
            resize_plan: plan,
            secondary_plan: None,
            desired_snapshot,
            desired_focus_domain: domain.clone(),
            desired_focus_leaf: focused_leaf,
            desired_geometry,
        })
    }

    /// Propose an atomic corner (dual-axis) COSMIC pointer pixel resize for
    /// the selected exact opaque `(domain, window)` pair: one horizontal
    /// boundary plus one vertical boundary from a single request, committed
    /// through the single pending slot.
    ///
    /// Structural preconditions match [`Session::propose_pointer_resize`]
    /// exactly (topology/membership/capability binding; requested window
    /// need not be active, active/remembered focus preserved). Derivation
    /// runs per axis in a fixed horizontal-then-vertical order: the
    /// horizontal boundary derives against the accepted topology, applies to
    /// a tree clone, the intermediate topology is re-projected, and the
    /// vertical boundary derives against that intermediate state before the
    /// combined tree stages exactly one pending plan. Both axes must derive
    /// `Planned`; either axis `Unchanged` (no-op, exhausted minima, or
    /// unrepresentable clamped result) refuses the whole corner as
    /// [`RefusalKind::Unchanged`] with no plan and no pending, so a corner
    /// never half-applies one axis while the other silently keeps stale
    /// shares. Per-axis minimum-size projectability and exact
    /// projectability bind independently, exactly like the single-axis path.
    /// Commits only via acknowledge-then-[`Session::verify_resize`], with
    /// both operation echoes bound.
    #[allow(clippy::too_many_arguments)]
    pub fn propose_pointer_resize_corner(
        &mut self,
        domain: &DomainKey,
        window: &WindowId,
        direction_h: Direction,
        boundary_h: i32,
        direction_v: Direction,
        boundary_v: i32,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &ResizeCapabilities,
    ) -> Result<SessionResizePlan, ProposeError> {
        if let Some(reason) = self.reconciler.divergence() {
            return Err(ProposeError::Diverged(reason));
        }
        if self.has_pending() {
            return Err(ProposeError::PendingExists);
        }
        if self.drag.is_some() {
            return Err(ProposeError::PendingExists);
        }
        // Corner axes are fixed horizontal-then-vertical; the caller binds
        // them by axis before calling.
        if Axis::for_direction(direction_h) != Axis::Horizontal
            || Axis::for_direction(direction_v) != Axis::Vertical
        {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        let Some(own_domain) = self.domains.iter().find(|d| &d.key() == domain).cloned() else {
            return Err(ProposeError::Refused(RefusalKind::UnknownDomain));
        };
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
        if !self.pointer_observation_matches(&session_observation.windows, window) {
            return Err(ProposeError::Refused(RefusalKind::PartialObservation));
        }
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
        // Pointer-only decoupling: observed overlay flags on the ACTIVE window
        // never reject a valid inactive dragged target (active/remembered
        // focus is preserved exactly). The active check below only binds the
        // coincident case, where the dragged window's own check refuses
        // identically.
        if focused_window == *window
            && let Some(entry) = session_observation
                .windows
                .iter()
                .find(|w| w.window == focused_window)
            && entry.flags().any()
        {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        }
        // Pointer-only decoupling: the dragged window need not be active.
        // Derive the resize target from its retained link; active focus is
        // preserved exactly (never synced to the dragged window here).
        let Some(target_link) = self.windows.get(window).cloned() else {
            return Err(ProposeError::Refused(RefusalKind::UnknownWindow));
        };
        if target_link.output != domain.output || target_link.workspace != domain.workspace {
            return Err(ProposeError::Refused(RefusalKind::FocusMismatch));
        }
        let target_leaf = target_link.leaf.clone();
        if let Some(entry) = session_observation
            .windows
            .iter()
            .find(|w| w.window == *window)
            && entry.flags().any()
        {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        }
        if !capabilities.supports(crate::contract::ResizeCapability::PointerResize) {
            return Err(ProposeError::Refused(RefusalKind::UnsupportedCapability));
        }
        // Both normalized coordinates must land inside the domain work-area
        // extent along their matching axis; outside is malformed, never
        // clamped.
        let inside_h = boundary_h >= own_domain.bounds.x
            && boundary_h <= own_domain.bounds.x + own_domain.bounds.w;
        let inside_v = boundary_v >= own_domain.bounds.y
            && boundary_v <= own_domain.bounds.y + own_domain.bounds.h;
        if !inside_h || !inside_v {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        let Some(tree) = self.trees.get(domain).cloned().flatten() else {
            return Err(ProposeError::Refused(RefusalKind::FocusMismatch));
        };
        if !collect_leaves(&tree).contains(&target_leaf) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let accepted_geometry = project_output_geometry(
            Some(&own_domain),
            Some(&tree),
            &self.windows,
            domain,
            &hints_from_observed(&session_observation.windows),
        )
        .map_err(|_| ProposeError::Refused(RefusalKind::MalformedTopology))?;
        if !geometry_covers_affected(
            &accepted_geometry,
            &self.windows,
            std::slice::from_ref(domain),
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        // Horizontal axis first against the accepted topology.
        let Some(pointer_h) = derive_pointer_shares(
            self.policy(),
            &tree,
            &target_leaf,
            direction_h,
            boundary_h,
            &own_domain,
            &accepted_geometry,
        ) else {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        };
        let (target_h, shares_h, effective_h, mode_h) = match pointer_h {
            PointerDerived::Unchanged => {
                return Err(ProposeError::Refused(RefusalKind::Unchanged));
            }
            PointerDerived::Planned {
                target,
                new_shares,
                effective_boundary,
                mode,
            } => (target, new_shares, effective_boundary, mode),
        };
        let Some(intermediate_tree) =
            crate::directional::apply_resize_shares(&tree, &target_h.group_id, &shares_h)
        else {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        };
        // Re-project the intermediate topology: the vertical derivation must
        // see the post-horizontal pixel extents, never the accepted ones.
        let mut intermediate_trees = self.trees.clone();
        intermediate_trees.insert(domain.clone(), Some(intermediate_tree.clone()));
        let intermediate_geometry = crate::session::capacity::project_affected_gated(
            &self.domains,
            &intermediate_trees,
            &self.windows,
            std::slice::from_ref(domain),
            &hints_from_observed(&session_observation.windows),
            &|key| {
                self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                    || self.overlap_accepted.contains(key)
            },
        )
        .ok_or(ProposeError::Refused(RefusalKind::MalformedTopology))?;
        if !geometry_covers_affected(
            &intermediate_geometry,
            &self.windows,
            std::slice::from_ref(domain),
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let Some(pointer_v) = derive_pointer_shares(
            self.policy(),
            &intermediate_tree,
            &target_leaf,
            direction_v,
            boundary_v,
            &own_domain,
            &intermediate_geometry,
        ) else {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        };
        let (target_v, shares_v, effective_v, mode_v) = match pointer_v {
            PointerDerived::Unchanged => {
                return Err(ProposeError::Refused(RefusalKind::Unchanged));
            }
            PointerDerived::Planned {
                target,
                new_shares,
                effective_boundary,
                mode,
            } => (target, new_shares, effective_boundary, mode),
        };
        let Some(updated_tree) = crate::directional::apply_resize_shares(
            &intermediate_tree,
            &target_v.group_id,
            &shares_v,
        ) else {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        };
        let mut desired_trees = self.trees.clone();
        desired_trees.insert(domain.clone(), Some(updated_tree));
        if desired_trees == self.trees {
            return Err(ProposeError::Refused(RefusalKind::Unchanged));
        }
        if !validate_topology(
            &self.domains,
            &desired_trees,
            &self.windows,
            &self.exceptions,
            &Some(domain.clone()),
            &Some(focused_leaf.clone()),
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let desired_geometry = crate::session::capacity::project_affected_gated(
            &self.domains,
            &desired_trees,
            &self.windows,
            std::slice::from_ref(domain),
            &hints_from_observed(&session_observation.windows),
            &|key| {
                self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                    || self.overlap_accepted.contains(key)
            },
        )
        .ok_or(ProposeError::Refused(RefusalKind::MalformedTopology))?;
        if !geometry_covers_affected(
            &desired_geometry,
            &self.windows,
            std::slice::from_ref(domain),
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        if !pointer_minimum_holds(
            self.policy(),
            &desired_geometry,
            &target_h,
            domain,
            Axis::Horizontal,
        ) || !pointer_minimum_holds(
            self.policy(),
            &desired_geometry,
            &target_v,
            domain,
            Axis::Vertical,
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let desired_tree = desired_trees
            .get(domain)
            .cloned()
            .flatten()
            .expect("desired tree present");
        let (Some(projected_h), Some(projected_v)) = (
            pointer_projected_boundary(
                &desired_geometry,
                &desired_tree,
                &target_h,
                Axis::Horizontal,
            ),
            pointer_projected_boundary(&desired_geometry, &desired_tree, &target_v, Axis::Vertical),
        ) else {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        };
        if projected_h != effective_h || projected_v != effective_v {
            return Err(ProposeError::Refused(RefusalKind::Unchanged));
        }
        let operation_h = ResizeOperation {
            domain_output: domain.output.clone(),
            domain_workspace: domain.workspace.clone(),
            focused_leaf: target_leaf.clone(),
            focused_window: window.clone(),
            direction: direction_h,
            mode: mode_h,
            target_group: target_h.group_id.clone(),
            focused_child: target_h.focused_child.clone(),
            neighbor_child: target_h.neighbor_child.clone(),
            focused_index: target_h.focused_index,
            neighbor_index: target_h.neighbor_index,
            old_shares: target_h.old_shares.clone(),
            new_shares: shares_h.clone(),
        };
        let plan_h = ResizePlan {
            intent: ResizeIntent {
                domain_output: domain.output.clone(),
                domain_workspace: domain.workspace.clone(),
                focused_leaf: target_leaf.clone(),
                focused_window: window.clone(),
                direction: direction_h,
                mode: mode_h,
            },
            operation: operation_h.clone(),
            required_capability: crate::contract::ResizeCapability::PointerResize,
            preconditions: operation_h.preconditions(),
        };
        let operation_v = ResizeOperation {
            domain_output: domain.output.clone(),
            domain_workspace: domain.workspace.clone(),
            focused_leaf: target_leaf.clone(),
            focused_window: window.clone(),
            direction: direction_v,
            mode: mode_v,
            target_group: target_v.group_id.clone(),
            focused_child: target_v.focused_child.clone(),
            neighbor_child: target_v.neighbor_child.clone(),
            focused_index: target_v.focused_index,
            neighbor_index: target_v.neighbor_index,
            old_shares: target_v.old_shares.clone(),
            new_shares: shares_v.clone(),
        };
        let plan_v = ResizePlan {
            intent: ResizeIntent {
                domain_output: domain.output.clone(),
                domain_workspace: domain.workspace.clone(),
                focused_leaf: target_leaf.clone(),
                focused_window: window.clone(),
                direction: direction_v,
                mode: mode_v,
            },
            operation: operation_v.clone(),
            required_capability: crate::contract::ResizeCapability::PointerResize,
            preconditions: operation_v.preconditions(),
        };
        let dispatch = match self.reconciler.propose_pointer_resize_corner(
            &plan_h,
            &plan_v,
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
        let desired_snapshot = self.snapshot_for(&desired_trees, &self.windows);
        self.pending_desired = Some(PendingDesired {
            trees: desired_trees.clone(),
            windows: self.windows.clone(),
            focused_domain: Some(domain.clone()),
            focused_leaf: Some(focused_leaf.clone()),
            last_active: self.last_active.clone(),
            exceptions: self.exceptions.clone(),
            retained_float_geometry: self.retained_float_geometry.clone(),
            automatic_fixed: self.automatic_fixed.clone(),
            fixed_tile_override: self.fixed_tile_override.clone(),
            capacity_floats: self.capacity_floats.clone(),
            admission_order: self.admission_order.clone(),
            admission_counter: self.admission_counter,
            overlap_accepted: self.overlap_accepted.clone(),
        });
        Ok(SessionResizePlan {
            dispatch,
            resize_plan: plan_h,
            secondary_plan: Some(plan_v),
            desired_snapshot,
            desired_focus_domain: domain.clone(),
            desired_focus_leaf: focused_leaf,
            desired_geometry,
        })
    }
}

/// Portable pointer target: the Rust-derived matching-axis split boundary
/// plus the selected adjacent pair and its current shares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::session) struct PointerTarget {
    group_id: NodeId,
    focused_index: usize,
    neighbor_index: usize,
    focused_child: NodeId,
    neighbor_child: NodeId,
    old_shares: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::session) enum PointerDerived {
    Unchanged,
    Planned {
        target: PointerTarget,
        new_shares: Vec<u64>,
        effective_boundary: i32,
        mode: ResizeMode,
    },
}

pub(in crate::session) struct PointerLevel {
    group_id: NodeId,
    axis: Axis,
    children: Vec<NodeId>,
    shares: Vec<u64>,
    child_index: usize,
}

pub(in crate::session) fn pointer_path(
    node: &Node,
    focused: &NodeId,
    out: &mut Vec<PointerLevel>,
) -> bool {
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

/// Direct-child projected extent along `axis` for one child of the target
/// group: bounding box of its descendant accepted leaves.
pub(in crate::session) fn pointer_child_extent(
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

pub(in crate::session) fn collect_pointer_leaves(
    node: &Node,
    target: &NodeId,
    out: &mut Vec<NodeId>,
) {
    if node.id() == target {
        collect_leaves(node).into_iter().for_each(|id| out.push(id));
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

/// Group origin along `axis` from accepted descendant leaf bounds.
pub(in crate::session) fn pointer_group_origin(
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

pub(in crate::session) fn collect_pointer_group_leaves(
    node: &Node,
    group: &NodeId,
    out: &mut Vec<NodeId>,
) {
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

/// COSMIC pixel-share derivation outcome shared by keyboard and pointer
/// resize. `None` from the drivers means malformed/unrepresentable (caller
/// maps to `MalformedTopology`); `Unchanged` means no feasible boundary or a
/// no-op proposing the current boundary; `BelowMinimum` means a candidate pair
/// under the axis pair minimum prevented a plan with thresholds unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::session) enum PixelDerived {
    Unchanged,
    BelowMinimum,
    Planned {
        target: PointerTarget,
        new_shares: Vec<u64>,
    },
}

/// Exact pixel-projectable normalization for a clamped left-child size.
///
/// The projector allocates `floor(D * share / total) + 1` to every non-last
/// child with `D = avail - n` and `avail` the pixel sum. For the left child
/// at `lo` (always non-last) the desired size `clamped_left` needs
/// `floor(D * a / total') = clamped_left - 1` with `total' = K * old_total`
/// and `a + b = K * old_pair_total`. Non-pair shares scale by the same `K`
/// (exact ratio preservation); only the adjacent pair is redistributed.
/// `total' >= D` guarantees an integer `a`, so `K = ceil(D / old_total)`
/// always works. The smallest feasible scale wins (`1`, then the guaranteed
/// factor); no project share-step normalization lives on the COSMIC path. `None` means unrepresentable at any scale.
pub(in crate::session) fn pixel_shares_for_clamped(
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
        // `floor(D * a / total') = q` <=> `a in [q*total'/D, ((q+1)*total' - 1)/D]`.
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

/// Derive COSMIC keyboard pixel shares from a repeat-step move.
///
/// Walks matching-axis ancestors nearest outward; the first ancestor whose
/// pair region admits the source pair minimum and whose COSMIC-clamped move
/// changes the boundary wins. Source `tiling/mod.rs` 2576-2600: `direction`
/// (`ResizeDirection`) selects shrink/grow (`Inwards` shrinks the focused
/// node, `Outwards` grows it), `edges` (the cardinal edge) selects the
/// neighbor side, and only the shrink side is clamped one-sided to the axis
/// child minimum via [`crate::cosmic_v1::clamp_keyboard_shrink_pair`] (the
/// grow side takes exactly the amount actually removed). `None` means
/// malformed/unrepresentable; `BelowMinimum` means a candidate pair was under
/// the axis pair minimum; `Unchanged` means no feasible boundary or a clamped
/// no-op at every other candidate level.
#[allow(clippy::too_many_arguments)]
pub(in crate::session) fn derive_keyboard_pixel_shares(
    policy: &dyn LayoutPolicy,
    tree: &Node,
    focused_leaf: &NodeId,
    edge: Direction,
    mode: ResizeMode,
    step_px: i32,
    _domain: &OutputDomain,
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
            // Source no-op pair on direct sums sizes[i] + sizes[i+1]:
            // skip outward, never plan through it. Thresholds unchanged.
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

/// Derive pointer shares from the normalized boundary coordinate.
///
/// Walks matching-axis ancestors nearest outward; the first ancestor whose
/// pair region admits two minimum segments wins. For every valid/clamped
/// boundary inside that pair region, positive shares are derived such that
/// the existing projector returns exactly the proposed shared boundary.
/// Only the selected adjacent share ratio changes, plus an exact
/// ratio-preserving normalization (`new = K * old` for non-pair shares and
/// `new_pair_total = K * old_pair_total`) when representation precision
/// requires it. `None` means malformed/unrepresentable (caller maps to
/// `MalformedTopology`); `Unchanged` means no feasible boundary or a
/// no-op proposing the current boundary.
pub(in crate::session) fn derive_pointer_shares(
    policy: &dyn LayoutPolicy,
    tree: &Node,
    focused_leaf: &NodeId,
    direction: Direction,
    proposed_boundary: i32,
    domain: &OutputDomain,
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
    // `pointer_path` pushes nearest first (post-order unwind is not used;
    // levels are pushed innermost first by construction above).
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
        // Adjacent by construction (direct neighbor).
        if focused_index.abs_diff(neighbor_index) != 1 {
            return None;
        }
        // Current adjacent pixel sizes from accepted projection.
        let mut sizes: Vec<i64> = Vec::with_capacity(level.children.len());
        for child in &level.children {
            sizes.push(pointer_child_extent(tree, child, wanted, geometry)?);
            if sizes.last() == Some(&0) || sizes.last().is_some_and(|s| *s <= 0) {
                return None;
            }
        }
        let pair_avail = sizes[focused_index].checked_add(sizes[neighbor_index])?;
        // COSMIC pair gate on direct sums sizes[i] + sizes[i+1] (never union
        // including gap): source no-op pairs refuse outward, never planned.
        if !policy.pair_admits_resize(pair_avail, pair_avail, wanted) {
            feasible_exhausted = true;
            continue;
        }
        let group_origin = pointer_group_origin(tree, &level.group_id, wanted, geometry)?;
        let gap = i64::from(domain.gap);
        let mut prefix: i64 = 0;
        for size in sizes.iter().take(lo) {
            prefix = prefix.checked_add(*size)?.checked_add(gap)?;
        }
        let pair_start = group_origin.checked_add(prefix)?;
        // Desired left-child size from the proposed boundary treated as the
        // left child's end edge; the right child takes the remainder.
        // Clamping is two-sided per the COSMIC child minima.
        let left_desired = i64::from(proposed_boundary).checked_sub(pair_start)?;
        let clamped_left = match policy.clamp_pair_split(pair_avail, left_desired, wanted) {
            Some((first, _)) => first,
            None => {
                feasible_exhausted = true;
                continue;
            }
        };
        if clamped_left == sizes[lo] {
            // Proposing the current boundary at this level: noop here, try
            // outward before refusing.
            feasible_exhausted = true;
            continue;
        }
        // Exact pixel-projectable normalization shared with keyboard resize
        // (no project share-step on the COSMIC path).
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

/// Minimum-size projectability on the desired geometry: the two resized
/// adjacent children each keep at least the COSMIC axis child minimum
/// ([`crate::cosmic_v1::child_min_for_axis`]) when they are direct leaves,
/// and every desired leaf stays positive. Subgroup children were already
/// clamped pre-share, so only direct-leaf spans are rechecked here.
pub(in crate::session) fn pointer_minimum_holds(
    policy: &dyn LayoutPolicy,
    geometry: &[DesiredGeometry],
    target: &PointerTarget,
    domain: &DomainKey,
    axis: Axis,
) -> bool {
    use std::collections::BTreeMap;
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

/// Keyboard one-sided minimum: only the shrink side (focused on Inwards,
/// neighbor on Outwards) keeps the COSMIC child minimum when it is a direct
/// leaf; the grow side takes exactly the removed amount even below minimum.
/// Subgroup children were clamped pre-share, so only direct-leaf spans are
/// rechecked here. Positivity is enforced by geometry coverage.
pub(in crate::session) fn keyboard_minimum_holds(
    policy: &dyn LayoutPolicy,
    geometry: &[DesiredGeometry],
    target: &PointerTarget,
    domain: &DomainKey,
    axis: Axis,
    mode: ResizeMode,
) -> bool {
    use std::collections::BTreeMap;
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

/// Exact projected boundary of the target pair's left child after integer
/// projection: the maximum end edge over descendant desired leaves of the
/// left child (`x + w` horizontal, `y + h` vertical). This is the absolute
/// domain-space coordinate the native-driven source edge and the adjacent
/// neighbour edge must show for post-observation to bind. `None` when the
/// left child or any descendant leaf is missing from the geometry.
pub(in crate::session) fn pointer_projected_boundary(
    geometry: &[DesiredGeometry],
    tree: &Node,
    target: &PointerTarget,
    axis: Axis,
) -> Option<i32> {
    use std::collections::BTreeMap;
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
