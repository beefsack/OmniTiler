//! Session lifecycle operations: admit/remove command proposal and observation binding.
//!
//! Responsibility: own the lifecycle `impl Session` family. World state,
//! shared projection/validation helpers, transaction mechanics, and tests
//! stay in the parent session module.

use super::super::*;

impl super::super::Session {
    /// Propose a lifecycle command against a complete session observation.
    ///
    /// Transactional: refusals leave state unchanged and usable; terminal
    /// divergences (stale revision, capability, binding mismatch) clear pending
    /// like the reconciler. At most one pending plan.
    pub fn propose(
        &mut self,
        command: &SessionCommand,
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
        // Shape checks (non-divergent refusals).
        if !valid_command_shapes(command) {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        if !valid_observed_shapes(&session_observation.windows) {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        // Cross-domain checks before duplicate/unknown so unknown domains
        // classify deterministically.
        if let Err(kind) = self.check_domains(command, &session_observation.windows) {
            return Err(ProposeError::Refused(kind));
        }
        match command {
            SessionCommand::Admit {
                window,
                output,
                workspace,
                exceptions,
                exception_behavior,
                placement_bounds,
                suppress_fixed_float,
            } => self.propose_admit(
                window,
                output,
                workspace,
                *exceptions,
                *exception_behavior,
                *placement_bounds,
                *suppress_fixed_float,
                session_observation,
                correlation_id,
                capabilities,
            ),
            SessionCommand::Remove { window } => {
                self.propose_remove(window, session_observation, correlation_id, capabilities)
            }
            SessionCommand::MoveToWorkspace {
                window,
                target_output,
                target_workspace,
                follow,
            } => self.propose_move_to_workspace(
                window,
                target_output,
                target_workspace,
                *follow,
                None,
                session_observation,
                correlation_id,
                capabilities,
            ),
            SessionCommand::MoveToOutput {
                window,
                target_output,
                target_workspace,
                follow,
            } => self.propose_move_to_output(
                window,
                target_output,
                target_workspace,
                *follow,
                None,
                session_observation,
                correlation_id,
                capabilities,
            ),
            SessionCommand::ToggleFloat {
                window,
                float_geometry,
            } => self.propose_toggle_float(
                window,
                *float_geometry,
                session_observation,
                correlation_id,
                capabilities,
            ),
            SessionCommand::ToggleOrientation { window } => self.propose_toggle_orientation(
                window,
                session_observation,
                correlation_id,
                capabilities,
            ),
        }
    }

    /// Propose a workspace/output send with a trustworthy engine-carried
    /// native live frame for the mover (`Some` for same-output sends,
    /// `None` when unavailable or cross-output). Same guards as
    /// [`Session::propose`]; only `MoveToWorkspace`/`MoveToOutput`
    /// commands are accepted (anything else refuses as `MalformedInput`).
    /// The live frame becomes the capacity-float exception frame
    /// verbatim for same-output sends; cross-output sends never stage a
    /// source-coordinate claim (the established native `sendClientToScreen`
    /// route plus its arrival proof owns placement there).
    pub fn propose_send_with_live_frame(
        &mut self,
        command: &SessionCommand,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &LifecycleCapabilities,
        mover_live_frame: Option<Rect>,
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
        if !valid_command_shapes(command) {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        if !valid_observed_shapes(&session_observation.windows) {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        if let Err(kind) = self.check_domains(command, &session_observation.windows) {
            return Err(ProposeError::Refused(kind));
        }
        match command {
            SessionCommand::MoveToWorkspace {
                window,
                target_output,
                target_workspace,
                follow,
            } => self.propose_move_to_workspace(
                window,
                target_output,
                target_workspace,
                *follow,
                mover_live_frame,
                session_observation,
                correlation_id,
                capabilities,
            ),
            SessionCommand::MoveToOutput {
                window,
                target_output,
                target_workspace,
                follow,
            } => self.propose_move_to_output(
                window,
                target_output,
                target_workspace,
                *follow,
                mover_live_frame,
                session_observation,
                correlation_id,
                capabilities,
            ),
            _ => Err(ProposeError::Refused(RefusalKind::MalformedInput)),
        }
    }

    pub(in crate::session) fn check_domains(
        &self,
        command: &SessionCommand,
        observed: &[ObservedWindow],
    ) -> Result<(), RefusalKind> {
        match command {
            SessionCommand::Admit {
                output, workspace, ..
            } => {
                if self.domain_for(output, workspace).is_none() {
                    return Err(RefusalKind::CrossDomainMismatch);
                }
            }
            SessionCommand::Remove { .. } => {}
            // Same-output workspace transfer targets are validated precisely
            // inside `propose_move_to_workspace` (`UnknownDomain` for unknown
            // targets, `CrossDomainMismatch` for cross-output); explicit
            // output-send targets are validated precisely inside
            // `propose_move_to_output` (`UnknownDomain` for unknown targets,
            // `CrossDomainMismatch` for same-output); only observed
            // entries are checked here.
            SessionCommand::MoveToWorkspace { .. } | SessionCommand::MoveToOutput { .. } => {}
            SessionCommand::ToggleFloat { .. } => {}
            SessionCommand::ToggleOrientation { .. } => {}
        }
        for entry in observed {
            if self.domain_for(&entry.output, &entry.workspace).is_none() {
                return Err(RefusalKind::CrossDomainMismatch);
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::session) fn propose_admit(
        &mut self,
        window: &WindowId,
        output: &OutputId,
        workspace: &WorkspaceId,
        exceptions: ExceptionFlags,
        exception_behavior: Option<ExceptionBehavior>,
        placement_bounds: Rect,
        suppress_fixed_float: bool,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &LifecycleCapabilities,
    ) -> Result<SessionPlan, ProposeError> {
        if self.windows.contains_key(window) || self.exceptions.contains_key(window) {
            return Err(ProposeError::Refused(RefusalKind::DuplicateWindow));
        }
        // Completeness: observed must equal known plus the admitted window.
        let mut known: BTreeSet<&WindowId> =
            self.windows.keys().chain(self.exceptions.keys()).collect();
        known.insert(window);
        let observed_ids: BTreeSet<&WindowId> = session_observation
            .windows
            .iter()
            .map(|w| &w.window)
            .collect();
        if observed_ids != known {
            return Err(ProposeError::Refused(RefusalKind::PartialObservation));
        }
        let Some(entry) = session_observation
            .windows
            .iter()
            .find(|w| &w.window == window)
        else {
            return Err(ProposeError::Refused(RefusalKind::PartialObservation));
        };
        // R-MAX-06 fresh-max carrier: a maximized-only birth with no caller
        // exceptions admits as an ordinary tile (the adapter clears the
        // native overlay once, then the approved allocation applies).
        // Every other admission binds the full observed flags exactly, so
        // an explicit deferred maximize-only (alone or in a floating+max
        // combo) keeps its flags. Maximized never selects exception
        // behavior, matching `converge_observation` normal admission which
        // tiles maximized windows; floating, fullscreen, and sticky still
        // require explicit behavior selection below.
        let entry_flags = entry.flags();
        let max_only_tile_carrier = !exceptions.any()
            && entry_flags
                == crate::session::world::ExceptionFlags {
                    floating: false,
                    fullscreen: false,
                    maximized: true,
                    sticky: false,
                };
        let observed_binding = if max_only_tile_carrier {
            crate::session::world::ExceptionFlags {
                maximized: false,
                ..entry_flags
            }
        } else {
            entry_flags
        };
        if &entry.output != output
            || &entry.workspace != workspace
            || observed_binding != exceptions
        {
            return Err(ProposeError::Refused(RefusalKind::PartialObservation));
        }
        // Known-window observation bindings must match stored state.
        if !self.observed_known_match(&session_observation.windows, Some(window)) {
            return Err(ProposeError::Refused(RefusalKind::PartialObservation));
        }
        if exceptions.any() && exception_behavior.is_none() {
            return Err(ProposeError::Refused(
                RefusalKind::ExceptionBehaviorUnselected,
            ));
        }
        if !exceptions.any() && exception_behavior.is_some() {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        if !valid_rect_shape(&placement_bounds) {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        let deferred = exceptions.any();
        if deferred {
            // Exception deferral: no topology effect, exception set grows.
            let mut desired_exceptions = self.exceptions.clone();
            desired_exceptions.insert(
                window.clone(),
                ExceptionRecord {
                    window: window.clone(),
                    output: output.clone(),
                    workspace: workspace.clone(),
                    flags: exceptions,
                    floating_geometry: None,
                },
            );
            let desired_snapshot = self.snapshot_for(&self.trees, &self.windows);
            if desired_exceptions == self.exceptions {
                return Err(ProposeError::Refused(RefusalKind::Unchanged));
            }
            let intent = LifecycleIntent::Admit {
                window: window.clone(),
                output: output.clone(),
                workspace: workspace.clone(),
            };
            let operation = LifecycleOperation::AdmitDeferred {
                window: window.clone(),
                output: output.clone(),
                workspace: workspace.clone(),
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
            self.pending_desired = Some(PendingDesired {
                trees: self.trees.clone(),
                windows: self.windows.clone(),
                focused_domain: self.focused_domain.clone(),
                focused_leaf: self.focused_leaf.clone(),
                last_active: self.last_active.clone(),
                exceptions: desired_exceptions,
                retained_float_geometry: self.retained_float_geometry.clone(),
                automatic_fixed: self.automatic_fixed.clone(),
                fixed_tile_override: self.fixed_tile_override.clone(),
                capacity_floats: self.capacity_floats.clone(),
                admission_order: self.admission_order.clone(),
                admission_counter: self.admission_counter,
                overlap_accepted: self.overlap_accepted.clone(),
            });
            return Ok(SessionPlan {
                dispatch,
                desired_snapshot,
                desired_focus_domain: self.focused_domain.clone(),
                desired_focus_leaf: self.focused_leaf.clone(),
                desired_geometry: Vec::new(),
            });
        }
        // Q2 fixed-size admission (D1): a normal tiled admission carrying
        // fixed hints floats as an automatic membership-only exception (D8)
        // instead of taking a tiled slot. Born fullscreen bypasses (D5);
        // an explicit user tile wins, from the retained set or this
        // command's suppress origin (D3); born maximized floats under its
        // native maximize overlay with no reserved tile (D4). The observed
        // suppress signal bypasses like a retained override. Later hint
        // changes never reclassify (D2): only this admission branch and
        // observation convergence classify.
        let override_wins = self.fixed_tile_override.contains(window) || suppress_fixed_float;
        if self.fixed_admission
            && !override_wins
            && !entry.floating
            && !entry.fullscreen
            && !entry.sticky
            && !entry.fixed_suppress
            && crate::size_hints::is_fixed_size_with(entry.hints, self.fixed_predicate)
        {
            let mut desired_exceptions = self.exceptions.clone();
            desired_exceptions.insert(
                window.clone(),
                ExceptionRecord {
                    window: window.clone(),
                    output: output.clone(),
                    workspace: workspace.clone(),
                    flags: ExceptionFlags {
                        floating: true,
                        fullscreen: false,
                        maximized: false,
                        sticky: false,
                    },
                    floating_geometry: None,
                },
            );
            let desired_snapshot = self.snapshot_for(&self.trees, &self.windows);
            let intent = LifecycleIntent::Admit {
                window: window.clone(),
                output: output.clone(),
                workspace: workspace.clone(),
            };
            let operation = LifecycleOperation::AdmitDeferred {
                window: window.clone(),
                output: output.clone(),
                workspace: workspace.clone(),
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
            // Membership only: no tile, no geometry, no focus change (D8).
            // The automatic marker stages transactionally in the pending
            // desired state: refusals never stage it, and acknowledgement
            // or verification failure discards it with the plan.
            let mut desired_automatic = self.automatic_fixed.clone();
            desired_automatic.insert(window.clone());
            self.pending_desired = Some(PendingDesired {
                trees: self.trees.clone(),
                windows: self.windows.clone(),
                focused_domain: self.focused_domain.clone(),
                focused_leaf: self.focused_leaf.clone(),
                last_active: self.last_active.clone(),
                exceptions: desired_exceptions,
                retained_float_geometry: self.retained_float_geometry.clone(),
                automatic_fixed: desired_automatic,
                fixed_tile_override: self.fixed_tile_override.clone(),
                capacity_floats: self.capacity_floats.clone(),
                admission_order: self.admission_order.clone(),
                admission_counter: self.admission_counter,
                overlap_accepted: self.overlap_accepted.clone(),
            });
            return Ok(SessionPlan {
                dispatch,
                desired_snapshot,
                desired_focus_domain: self.focused_domain.clone(),
                desired_focus_leaf: self.focused_leaf.clone(),
                desired_geometry: Vec::new(),
            });
        }
        // Normal tiled admission stays fail-closed on overlays, except a
        // maximized-only birth (R-MAX-06 fresh-max carrier): it admits as
        // an ordinary tile so the ready reply seeds the approved
        // allocation, and the adapter clears the native overlay once
        // before applying it. An observed floating/fullscreen/sticky
        // window without an explicit exception selection is refused
        // exactly as before.
        if entry.floating || entry.fullscreen || entry.sticky {
            return Err(ProposeError::Refused(RefusalKind::PartialObservation));
        }
        // Normal tiled admission under best effort (D2/D3): the ordinary
        // anchor first, then the D2 fallback, then the D3 no-space result.
        // Every candidate must pass whole-tree strict plus hinted
        // projection; minimum flags alone never disqualify (D1, B6 kept).
        let orientation = self.policy().admission_axis_for_rect(&placement_bounds);
        let mut node_ids = self.all_node_ids();
        let base_revision = session_observation.observation.revision;
        let key = DomainKey {
            output: output.clone(),
            workspace: workspace.clone(),
        };
        let target_tree = self.trees.get(&key).cloned().flatten();
        let eligible_focus = self.eligible_focus_in(&key);
        let hints = hints_from_observed(&session_observation.windows);
        let current_links: BTreeMap<NodeId, WindowId> = self
            .windows
            .values()
            .filter(|l| l.output == *output && l.workspace == *workspace)
            .map(|l| (l.leaf.clone(), l.window.clone()))
            .collect();
        let domain_bounds = self
            .domain_for(output, workspace)
            .map(|d| d.bounds)
            .unwrap_or(placement_bounds);
        let domain_gap = self
            .domain_for(output, workspace)
            .map(|d| d.gap)
            .unwrap_or(0);
        let leg = crate::session::capacity::plan_admission_leg(
            self.policy(),
            self.anchor_fallback,
            target_tree,
            domain_bounds,
            domain_gap,
            eligible_focus,
            orientation,
            window,
            base_revision,
            &mut node_ids,
            &hints,
            &current_links,
        );
        let leaf_id = leg.leaf_id;
        let ordinary_tree = leg.ordinary_tree;
        match leg.selected {
            Ok(target) => self.stage_tiled_admission(
                window,
                output,
                workspace,
                leaf_id,
                key,
                target.tree,
                &hints,
                suppress_fixed_float,
                entry,
                session_observation,
                correlation_id,
                capabilities,
            ),
            Err(()) => self.stage_no_space_admission(
                window,
                output,
                workspace,
                key,
                leaf_id,
                ordinary_tree,
                &hints,
                suppress_fixed_float,
                entry,
                session_observation,
                correlation_id,
                capabilities,
            ),
        }
    }

    /// Stage a successful tiled admission (ordinary anchor or D2 fallback).
    #[allow(clippy::too_many_arguments)]
    pub(in crate::session) fn stage_tiled_admission(
        &mut self,
        window: &WindowId,
        output: &OutputId,
        workspace: &WorkspaceId,
        leaf_id: NodeId,
        key: DomainKey,
        new_tree: Node,
        hints: &BTreeMap<WindowId, crate::size_hints::WindowSizeHints>,
        suppress_fixed_float: bool,
        entry: &ObservedWindow,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &LifecycleCapabilities,
    ) -> Result<SessionPlan, ProposeError> {
        let mut desired_trees = self.trees.clone();
        desired_trees.insert(key.clone(), Some(new_tree));
        // Desired window links (sorted via BTreeMap).
        let mut desired_windows = self.windows.clone();
        desired_windows.insert(
            window.clone(),
            WindowLink {
                window: window.clone(),
                leaf: leaf_id.clone(),
                output: output.clone(),
                workspace: workspace.clone(),
            },
        );
        let desired_snapshot = self.snapshot_for(&desired_trees, &desired_windows);
        if !validate_topology(
            &self.domains,
            &desired_trees,
            &desired_windows,
            &self.exceptions,
            &Some(key.clone()),
            &Some(leaf_id.clone()),
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        if desired_trees == self.trees
            && desired_windows == self.windows
            && self.focused_domain.as_ref() == Some(&key)
            && self.focused_leaf.as_ref() == Some(&leaf_id)
        {
            return Err(ProposeError::Refused(RefusalKind::Unchanged));
        }
        let desired_geometry = project_output_geometry(
            self.domain_for(output, workspace),
            desired_trees.get(&key).cloned().flatten().as_ref(),
            &desired_windows,
            &key,
            hints,
        )
        .map_err(|_| ProposeError::Refused(RefusalKind::MalformedTopology))?;
        if desired_geometry.is_empty() {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let intent = LifecycleIntent::Admit {
            window: window.clone(),
            output: output.clone(),
            workspace: workspace.clone(),
        };
        let operation = LifecycleOperation::Admit {
            window: window.clone(),
            leaf: leaf_id.clone(),
            output: output.clone(),
            workspace: workspace.clone(),
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
        // An explicit suppress origin (unfloat command or observed
        // adapter signal) stages its tile win transactionally with the
        // tiled admission: the override commits at verify and vanishes
        // on any refusal or divergence.
        let mut desired_override = self.fixed_tile_override.clone();
        let mut desired_automatic = self.automatic_fixed.clone();
        if suppress_fixed_float || (self.fixed_admission && entry.fixed_suppress) {
            desired_override.insert(window.clone());
            desired_automatic.remove(window);
        }
        // A successful tile clears capacity origin and records the
        // first-successful-tiled-admission sequence (moves and re-tiles
        // keep the first number). Strict success clears any overlap
        // acceptance for this domain.
        let mut desired_capacity = self.capacity_floats.clone();
        desired_capacity.remove(window);
        let mut desired_order = self.admission_order.clone();
        let mut desired_counter = self.admission_counter;
        if !desired_order.contains_key(window) {
            desired_order.insert(window.clone(), desired_counter);
            desired_counter = desired_counter.saturating_add(1);
        }
        let mut desired_overlap = self.overlap_accepted.clone();
        desired_overlap.remove(&key);
        self.pending_desired = Some(PendingDesired {
            trees: desired_trees.clone(),
            windows: desired_windows.clone(),
            focused_domain: Some(key.clone()),
            focused_leaf: Some(leaf_id.clone()),
            last_active: self.updated_last_active(
                &Some(key.clone()),
                &Some(leaf_id.clone()),
                &desired_trees,
                &desired_windows,
            ),
            exceptions: self.exceptions.clone(),
            retained_float_geometry: self.retained_float_geometry.clone(),
            automatic_fixed: desired_automatic,
            fixed_tile_override: desired_override,
            capacity_floats: desired_capacity,
            admission_order: desired_order,
            admission_counter: desired_counter,
            overlap_accepted: desired_overlap,
        });
        Ok(SessionPlan {
            dispatch,
            desired_snapshot,
            desired_focus_domain: Some(key),
            desired_focus_leaf: Some(leaf_id),
            desired_geometry,
        })
    }

    /// Stage the D3 no-space result after ordinary and fallback anchors
    /// exhaust: overlap-admit at the ordinary anchor under `allow-overlap`,
    /// else a slotless capacity float at the native opening frame (D4).
    #[allow(clippy::too_many_arguments)]
    pub(in crate::session) fn stage_no_space_admission(
        &mut self,
        window: &WindowId,
        output: &OutputId,
        workspace: &WorkspaceId,
        key: DomainKey,
        leaf_id: NodeId,
        ordinary_tree: Option<Node>,
        hints: &BTreeMap<WindowId, crate::size_hints::WindowSizeHints>,
        suppress_fixed_float: bool,
        entry: &ObservedWindow,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &LifecycleCapabilities,
    ) -> Result<SessionPlan, ProposeError> {
        if self.no_space == crate::admission::NoSpaceResult::AllowOverlap {
            // Overlap keeps tiled membership at the ordinary anchor with
            // positive overflowing geometry (D9); B6 minimum flags ride
            // along unchanged.
            let Some(candidate) = ordinary_tree else {
                return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
            };
            let mut desired_windows = self.windows.clone();
            desired_windows.insert(
                window.clone(),
                WindowLink {
                    window: window.clone(),
                    leaf: leaf_id.clone(),
                    output: output.clone(),
                    workspace: workspace.clone(),
                },
            );
            let domain = self.domain_for(output, workspace);
            let desired_geometry = match domain {
                Some(domain) => crate::session::capacity::project_domain_overlap(
                    domain,
                    &candidate,
                    &desired_windows,
                    &key,
                    hints,
                ),
                None => None,
            };
            let Some(desired_geometry) = desired_geometry else {
                return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
            };
            if desired_geometry.is_empty() {
                return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
            }
            let mut desired_trees = self.trees.clone();
            desired_trees.insert(key.clone(), Some(candidate));
            let desired_snapshot = self.snapshot_for(&desired_trees, &desired_windows);
            if !validate_topology(
                &self.domains,
                &desired_trees,
                &desired_windows,
                &self.exceptions,
                &Some(key.clone()),
                &Some(leaf_id.clone()),
            ) {
                return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
            }
            let intent = LifecycleIntent::Admit {
                window: window.clone(),
                output: output.clone(),
                workspace: workspace.clone(),
            };
            let operation = LifecycleOperation::Admit {
                window: window.clone(),
                leaf: leaf_id.clone(),
                output: output.clone(),
                workspace: workspace.clone(),
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
            let mut desired_override = self.fixed_tile_override.clone();
            let mut desired_automatic = self.automatic_fixed.clone();
            if suppress_fixed_float || (self.fixed_admission && entry.fixed_suppress) {
                desired_override.insert(window.clone());
                desired_automatic.remove(window);
            }
            let mut desired_capacity = self.capacity_floats.clone();
            desired_capacity.remove(window);
            let mut desired_order = self.admission_order.clone();
            let mut desired_counter = self.admission_counter;
            if !desired_order.contains_key(window) {
                desired_order.insert(window.clone(), desired_counter);
                desired_counter = desired_counter.saturating_add(1);
            }
            let mut desired_overlap = self.overlap_accepted.clone();
            desired_overlap.insert(key.clone());
            self.pending_desired = Some(PendingDesired {
                trees: desired_trees.clone(),
                windows: desired_windows.clone(),
                focused_domain: Some(key.clone()),
                focused_leaf: Some(leaf_id.clone()),
                last_active: self.updated_last_active(
                    &Some(key.clone()),
                    &Some(leaf_id.clone()),
                    &desired_trees,
                    &desired_windows,
                ),
                exceptions: self.exceptions.clone(),
                retained_float_geometry: self.retained_float_geometry.clone(),
                automatic_fixed: desired_automatic,
                fixed_tile_override: desired_override,
                capacity_floats: desired_capacity,
                admission_order: desired_order,
                admission_counter: desired_counter,
                overlap_accepted: desired_overlap,
            });
            return Ok(SessionPlan {
                dispatch,
                desired_snapshot,
                desired_focus_domain: Some(key),
                desired_focus_leaf: Some(leaf_id),
                desired_geometry,
            });
        }
        // Capacity float (D4): slotless membership with diagnostic-only
        // capacity origin, distinct from automatic fixed. No tile slot,
        // no allocation or frame authority: the adapter keeps the native
        // opening frame untouched. Otherwise an ordinary float: the
        // newcomer is the float target the adapter natively focuses, and
        // core tiled focus (prior MRU) is preserved. No automatic
        // re-tile: the float persists until an explicit tile retry (D5).
        let mut desired_exceptions = self.exceptions.clone();
        desired_exceptions.insert(
            window.clone(),
            ExceptionRecord {
                window: window.clone(),
                output: output.clone(),
                workspace: workspace.clone(),
                flags: ExceptionFlags {
                    floating: true,
                    fullscreen: false,
                    maximized: false,
                    sticky: false,
                },
                floating_geometry: None,
            },
        );
        let desired_snapshot = self.snapshot_for(&self.trees, &self.windows);
        let intent = LifecycleIntent::Admit {
            window: window.clone(),
            output: output.clone(),
            workspace: workspace.clone(),
        };
        let operation = LifecycleOperation::AdmitDeferred {
            window: window.clone(),
            output: output.clone(),
            workspace: workspace.clone(),
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
        // A capacity re-float never reports a false fixed-tile override:
        // a retained explicit win stays with the lifetime, a genuinely
        // fixed-size window may earn one, and anything else carries none
        // even when the unfloat command asserted suppress.
        let fixed_now = crate::size_hints::is_fixed_size_with(entry.hints, self.fixed_predicate);
        let mut desired_override = self.fixed_tile_override.clone();
        if !desired_override.contains(window) {
            if (suppress_fixed_float || entry.fixed_suppress) && fixed_now {
                desired_override.insert(window.clone());
            } else {
                desired_override.remove(window);
            }
        }
        let mut desired_capacity = self.capacity_floats.clone();
        desired_capacity.insert(window.clone());
        self.pending_desired = Some(PendingDesired {
            trees: self.trees.clone(),
            windows: self.windows.clone(),
            focused_domain: self.focused_domain.clone(),
            focused_leaf: self.focused_leaf.clone(),
            last_active: self.last_active.clone(),
            exceptions: desired_exceptions,
            retained_float_geometry: self.retained_float_geometry.clone(),
            automatic_fixed: self.automatic_fixed.clone(),
            fixed_tile_override: desired_override,
            capacity_floats: desired_capacity,
            admission_order: self.admission_order.clone(),
            admission_counter: self.admission_counter,
            overlap_accepted: self.overlap_accepted.clone(),
        });
        Ok(SessionPlan {
            dispatch,
            desired_snapshot,
            desired_focus_domain: self.focused_domain.clone(),
            desired_focus_leaf: self.focused_leaf.clone(),
            desired_geometry: Vec::new(),
        })
    }

    pub(in crate::session) fn propose_remove(
        &mut self,
        window: &WindowId,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &LifecycleCapabilities,
    ) -> Result<SessionPlan, ProposeError> {
        let tiled = self.windows.get(window).cloned();
        let deferred = self.exceptions.get(window).cloned();
        if tiled.is_none() && deferred.is_none() {
            return Err(ProposeError::Refused(RefusalKind::UnknownWindow));
        }
        if tiled.is_some() && deferred.is_some() {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        // Completeness: observed must equal the known set (includes removed).
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
        if let Some(record) = deferred {
            // Exception removal: topology unchanged, exception set shrinks.
            let mut desired_exceptions = self.exceptions.clone();
            desired_exceptions.remove(window);
            let desired_snapshot = self.snapshot_for(&self.trees, &self.windows);
            let (mut focus_domain, mut focus_leaf) =
                (self.focused_domain.clone(), self.focused_leaf.clone());
            // Exception windows never hold leaf focus; preserve focus as long
            // as it still resolves, else fall back.
            if !self.focus_resolves(&focus_domain, &focus_leaf, &self.trees, &self.windows) {
                let fallback = first_leaf_global(&self.domains, &self.trees);
                focus_domain = fallback.clone().map(|(k, _)| k);
                focus_leaf = fallback.map(|(_, l)| l);
            }
            let intent = LifecycleIntent::Remove {
                window: window.clone(),
            };
            let operation = LifecycleOperation::RemoveDeferred {
                window: window.clone(),
                output: record.output.clone(),
                workspace: record.workspace.clone(),
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
            self.pending_desired = Some(PendingDesired {
                trees: self.trees.clone(),
                windows: self.windows.clone(),
                focused_domain: focus_domain.clone(),
                focused_leaf: focus_leaf.clone(),
                last_active: self.updated_last_active(
                    &focus_domain,
                    &focus_leaf,
                    &self.trees,
                    &self.windows,
                ),
                exceptions: desired_exceptions,
                retained_float_geometry: self.retained_float_geometry.clone(),
                automatic_fixed: self.automatic_fixed.clone(),
                fixed_tile_override: self.fixed_tile_override.clone(),
                capacity_floats: self.capacity_floats.clone(),
                admission_order: self.admission_order.clone(),
                admission_counter: self.admission_counter,
                overlap_accepted: self.overlap_accepted.clone(),
            });
            return Ok(SessionPlan {
                dispatch,
                desired_snapshot,
                desired_focus_domain: focus_domain,
                desired_focus_leaf: focus_leaf,
                desired_geometry: Vec::new(),
            });
        }
        let link = tiled.clone().expect("tiled link present");
        let key = DomainKey {
            output: link.output.clone(),
            workspace: link.workspace.clone(),
        };
        let leaf = link.leaf.clone();
        let mut desired_trees = self.trees.clone();
        let target_tree = desired_trees.get(&key).cloned().flatten();
        let new_target = remove_leaf_from_tree(self.policy(), target_tree, &leaf);
        desired_trees.insert(key.clone(), new_target.clone());
        let mut desired_windows = self.windows.clone();
        desired_windows.remove(window);
        let desired_snapshot = self.snapshot_for(&desired_trees, &desired_windows);
        // Focus: preserve unless the removed leaf was focused.
        let was_focused =
            self.focused_leaf.as_ref() == Some(&leaf) && self.focused_domain.as_ref() == Some(&key);
        let (desired_focus_domain, desired_focus_leaf) = if was_focused {
            match self.focus_stack_fallback(&key, &desired_trees, &desired_windows) {
                Some(next) => (Some(key.clone()), Some(next)),
                // No MRU history (e.g. floating the only-ever-focused
                // window): fall back to a surviving leaf in the same
                // domain, never focus-none with live tiles here and never
                // a cross-domain jump. An emptied domain keeps focus none
                // (removal retains empty domains); a focus-none session
                // with live tiles would refuse every follow-up sweep,
                // silently stalling retained targets.
                None => match desired_trees.get(&key).cloned().flatten().map(|tree| {
                    collect_leaves(&tree).into_iter().find(|leaf| {
                        self.focus_resolves(
                            &Some(key.clone()),
                            &Some(leaf.clone()),
                            &desired_trees,
                            &desired_windows,
                        )
                    })
                }) {
                    Some(Some(leaf)) => (Some(key.clone()), Some(leaf)),
                    _ => (None, None),
                },
            }
        } else {
            // Removing an unfocused window leaves the active focus unchanged.
            if self.focus_resolves(
                &self.focused_domain,
                &self.focused_leaf,
                &desired_trees,
                &desired_windows,
            ) {
                (self.focused_domain.clone(), self.focused_leaf.clone())
            } else {
                (None, None)
            }
        };
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
        if desired_trees == self.trees
            && desired_windows == self.windows
            && desired_focus_domain == self.focused_domain
            && desired_focus_leaf == self.focused_leaf
        {
            return Err(ProposeError::Refused(RefusalKind::Unchanged));
        }
        // Complete desired geometry for the affected domain when tiled leaves
        // remain; empty when the domain is now empty. Overlap-accepted
        // domains (or overlap mode) keep projecting through the D9 gate
        // so closes never refuse on retained overflow.
        let desired_geometry = match (
            self.domain_for(&key.output, &key.workspace),
            new_target.as_ref(),
        ) {
            (Some(domain), Some(tree)) => {
                let allow_overlap = self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                    || self.overlap_accepted.contains(&key);
                crate::session::capacity::project_domain_geometry(
                    domain,
                    tree,
                    &desired_windows,
                    &key,
                    &hints_from_observed(&session_observation.windows),
                    allow_overlap,
                )
                .ok_or(ProposeError::Refused(RefusalKind::MalformedTopology))?
            }
            _ => Vec::new(),
        };
        let intent = LifecycleIntent::Remove {
            window: window.clone(),
        };
        let operation = LifecycleOperation::Remove {
            window: window.clone(),
            leaf: leaf.clone(),
            output: key.output.clone(),
            workspace: key.workspace.clone(),
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
        self.pending_desired = Some(PendingDesired {
            trees: desired_trees.clone(),
            windows: desired_windows.clone(),
            focused_domain: desired_focus_domain.clone(),
            focused_leaf: desired_focus_leaf.clone(),
            last_active: self.updated_last_active(
                &desired_focus_domain,
                &desired_focus_leaf,
                &desired_trees,
                &desired_windows,
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

pub(in crate::session) fn valid_command_shapes(command: &SessionCommand) -> bool {
    match command {
        SessionCommand::Admit {
            window,
            output,
            workspace,
            placement_bounds,
            ..
        } => {
            !window.0.is_empty()
                && !output.0.is_empty()
                && !workspace.0.is_empty()
                && valid_rect_shape(placement_bounds)
        }
        SessionCommand::Remove { window } => !window.0.is_empty(),
        SessionCommand::MoveToWorkspace {
            window,
            target_output,
            target_workspace,
            ..
        }
        | SessionCommand::MoveToOutput {
            window,
            target_output,
            target_workspace,
            ..
        } => !window.0.is_empty() && !target_output.0.is_empty() && !target_workspace.0.is_empty(),
        SessionCommand::ToggleFloat {
            window,
            float_geometry,
        } => !window.0.is_empty() && float_geometry.as_ref().is_none_or(valid_rect_shape),
        SessionCommand::ToggleOrientation { window } => !window.0.is_empty(),
    }
}

pub(in crate::session) fn generate_leaf_id(
    window: &WindowId,
    existing: &mut BTreeSet<NodeId>,
) -> NodeId {
    let base = format!("leaf-{}", window.0);
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

pub(in crate::session) fn first_leaf_global(
    domains: &[OutputDomain],
    trees: &BTreeMap<DomainKey, Option<Node>>,
) -> Option<(DomainKey, NodeId)> {
    for domain in domains {
        if let Some(tree) = trees.get(&domain.key()).cloned().flatten()
            && let Some(first) = collect_leaves(&tree).into_iter().next()
        {
            return Some((domain.key(), first));
        }
    }
    None
}
