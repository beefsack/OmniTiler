//! Session workspace operations: same-output send-to-workspace transfer.
//!
//! Responsibility: own the workspace `impl Session` family. World state,
//! shared projection/validation helpers, transaction mechanics, and tests
//! stay in the parent session module.

use super::super::*;

impl super::super::Session {
    /// Propose a portable same-output send-to-workspace transfer of the
    /// keyboard-focused tiled window into an explicit target workspace domain.
    ///
    /// Source: `cosmic-comp` `SendToWorkspace` calls `Shell::move_current`
    /// with no window id and moves only the focused element; `move_element`
    /// unmaps the source (recursive collapse) then maps the tiled element in
    /// the target. The portable gate mirrors [`Session::propose_move`]:
    /// unknown windows refuse as `UnknownWindow`, exception windows or an
    /// unresolved focused leaf as `NotTiled`, and a missing global focus or
    /// requested non-focused tile as `FocusMismatch`. The source domain must
    /// equal the focused domain before mutation.
    ///
    /// Target placement follows `map_to_tree` (`direction=None`): the valid
    /// remembered destination last-active leaf splits when still linked
    /// there (axis from its projected rect), else the valid domain-scoped
    /// focus MRU splits, else the genuine no-focus root/output-bounds
    /// fallback applies; empty targets admit a lone root.
    ///
    /// Only same-output, distinct-workspace targets plan. Same-domain targets
    /// refuse as [`RefusalKind::Unchanged`], cross-output targets as
    /// [`RefusalKind::CrossDomainMismatch`], unknown targets as
    /// [`RefusalKind::UnknownDomain`]. The mover keeps its leaf identity with
    /// a retargeted link. Follow (`follow=true`, the default) moves focus to
    /// the mover leaf in the target domain on commit. Stay (`follow=false`)
    /// leaves the source selected and visible: focus falls back through the
    /// source domain's MRU tiled focus stack exactly like a focused removal
    /// (none when the source is left empty), never selecting the target.
    /// Destination admission is identical for both. Plans commit only via
    /// acknowledge-then-[`Session::verify_lifecycle`] with complete
    /// source-plus-target geometry.
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub(in crate::session) fn propose_move_to_workspace(
        &mut self,
        window: &WindowId,
        target_output: &OutputId,
        target_workspace: &WorkspaceId,
        follow: bool,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &LifecycleCapabilities,
    ) -> Result<SessionPlan, ProposeError> {
        self.propose_send_impl(
            window,
            target_output,
            target_workspace,
            follow,
            false,
            session_observation,
            correlation_id,
            capabilities,
        )
    }

    /// Explicit output send (REQ-OUT-04, item 5.4): identical ordinary
    /// transfer to [`Session::propose_move_to_workspace`] (focused tiled
    /// subject only, sticky/intentional floats ineligible as `NotTiled`;
    /// remembered-leaf, destination focus-history, root admission; explicit
    /// follow/stay), except the target is the destination output's current
    /// workspace resolved adapter-side, so cross-output targets are admitted.
    /// Same-output targets refuse as [`RefusalKind::CrossDomainMismatch`]
    /// (that scope belongs to the workspace-send op); same-domain targets
    /// refuse as [`RefusalKind::Unchanged`], unknown targets as
    /// [`RefusalKind::UnknownDomain`]. Floating-workspace boundaries are
    /// adapter-owned (native membership transfer with tiled-side reflow, as
    /// for workspace send); this plans tiled-to-tiled transfers only.
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub(in crate::session) fn propose_move_to_output(
        &mut self,
        window: &WindowId,
        target_output: &OutputId,
        target_workspace: &WorkspaceId,
        follow: bool,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &LifecycleCapabilities,
    ) -> Result<SessionPlan, ProposeError> {
        self.propose_send_impl(
            window,
            target_output,
            target_workspace,
            follow,
            true,
            session_observation,
            correlation_id,
            capabilities,
        )
    }

    /// Shared ordinary-send implementation behind workspace send
    /// (`allow_cross_output=false`: same-output distinct-workspace only) and
    /// explicit output send (`allow_cross_output=true`: the destination
    /// output's current workspace, same-output refused).
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    fn propose_send_impl(
        &mut self,
        window: &WindowId,
        target_output: &OutputId,
        target_workspace: &WorkspaceId,
        follow: bool,
        allow_cross_output: bool,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &LifecycleCapabilities,
    ) -> Result<SessionPlan, ProposeError> {
        // Completeness: observed must equal the known tiled-plus-exception
        // set (the transfer adds/removes no window identity).
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
        // A held born-fullscreen exception member carries through the
        // slotless deferred transfer (D28 option (a)): it rides the native
        // send-route flags (never floating, still fullscreen) while the
        // retained record holds the synthetic floating shape, so the
        // ordinary exact matcher below cannot bind it. Route before that
        // matcher; the deferred path binds the rest exactly plus the held
        // exemption for the mover only.
        if self.exceptions.contains_key(window) {
            return self.propose_deferred_send(
                window,
                target_output,
                target_workspace,
                follow,
                allow_cross_output,
                session_observation,
                correlation_id,
                capabilities,
            );
        }
        if !self.observed_known_match(&session_observation.windows, None) {
            return Err(ProposeError::Refused(RefusalKind::PartialObservation));
        }
        // Exact propose_move-style focus gate: only the keyboard-focused
        // tiled element moves through the ordinary transfer below.
        if !self.windows.contains_key(window) && !self.exceptions.contains_key(window) {
            return Err(ProposeError::Refused(RefusalKind::UnknownWindow));
        }
        let link = self.windows.get(window).cloned().expect("tiled link");
        let source_key = DomainKey {
            output: link.output.clone(),
            workspace: link.workspace.clone(),
        };
        let (Some(focused_domain), Some(focused_leaf)) =
            (self.focused_domain.clone(), self.focused_leaf.clone())
        else {
            return Err(ProposeError::Refused(RefusalKind::FocusMismatch));
        };
        if focused_domain != source_key {
            return Err(ProposeError::Refused(RefusalKind::FocusMismatch));
        }
        let Some(focused_window) = self.focused_window_for(&focused_leaf, &source_key) else {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        };
        if window != &focused_window {
            return Err(ProposeError::Refused(RefusalKind::FocusMismatch));
        }
        let target_key = DomainKey {
            output: target_output.clone(),
            workspace: target_workspace.clone(),
        };
        if allow_cross_output {
            // Explicit output send is cross-output only: a same-output
            // target (even a distinct workspace) belongs to the
            // workspace-send op.
            if target_key.output == source_key.output {
                return Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch));
            }
        } else {
            if target_key == source_key {
                return Err(ProposeError::Refused(RefusalKind::Unchanged));
            }
            if target_key.output != source_key.output {
                return Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch));
            }
        }
        let Some(target_domain) = self.domains.iter().find(|d| d.key() == target_key).cloned()
        else {
            return Err(ProposeError::Refused(RefusalKind::UnknownDomain));
        };
        let base_revision = session_observation.observation.revision;
        // Source removal with recursive collapse.
        let source_tree = self.trees.get(&source_key).cloned().flatten();
        if !source_tree
            .as_ref()
            .is_some_and(|tree| collect_leaves(tree).contains(&link.leaf))
        {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let new_source = remove_leaf_from_tree(self.policy(), source_tree, &link.leaf);
        if let Some(ref tree) = new_source
            && collect_leaves(tree).contains(&link.leaf)
        {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        // COSMIC `map_to_tree` splits the destination last-active leaf when
        // still linked there; a stale last-active (departed mover, only the
        // focused target domain refreshes it) falls back to the valid
        // domain-scoped focus MRU before the genuine no-focus root fallback.
        let stored_anchor = self.remembered_leaf(&target_key);
        let mru_anchor = self.focus_stack_fallback(&target_key, &self.trees, &self.windows);
        let remembered = stored_anchor.clone().or_else(|| mru_anchor.clone());
        let target_tree = self.trees.get(&target_key).cloned().flatten();
        let target_leaves = target_tree
            .as_ref()
            .map(|tree| collect_leaves(tree).len())
            .unwrap_or(0);
        let mut anchor_kind: &'static str = "fallback";
        let mut projected = target_domain.bounds;
        let axis = remembered
            .as_ref()
            .and_then(|leaf| {
                project_output_geometry(
                    Some(&target_domain),
                    target_tree.as_ref(),
                    &self.windows,
                    &target_key,
                    &hints_from_observed(&session_observation.windows),
                )
                .ok()?
                .into_iter()
                .find(|g| &g.leaf == leaf)
                .map(|g| {
                    projected = g.rect;
                    self.policy().admission_axis_for_rect(&g.rect)
                })
            })
            .unwrap_or_else(|| self.policy().admission_axis_for_rect(&target_domain.bounds));
        if target_tree.is_none() || target_leaves == 0 {
            anchor_kind = "empty";
        } else if let Some(leaf) = remembered.as_ref() {
            if stored_anchor.as_ref() == Some(leaf) {
                anchor_kind = "remembered";
            } else if mru_anchor.as_ref() == Some(leaf) {
                anchor_kind = "mru";
            }
        }
        let mut node_ids = self.all_node_ids();
        let new_target = insert_tiled(
            self.policy(),
            target_tree,
            remembered.as_ref(),
            link.leaf.clone(),
            axis,
            &mut node_ids,
            window,
            base_revision,
        )
        .ok_or(ProposeError::Refused(RefusalKind::MalformedTopology))?;
        if !collect_leaves(&new_target).contains(&link.leaf) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let mut desired_trees = self.trees.clone();
        desired_trees.insert(source_key.clone(), new_source);
        desired_trees.insert(target_key.clone(), Some(new_target));
        let mut desired_windows = self.windows.clone();
        desired_windows.insert(
            window.clone(),
            WindowLink {
                window: window.clone(),
                leaf: link.leaf.clone(),
                output: target_key.output.clone(),
                workspace: target_key.workspace.clone(),
            },
        );
        // Follow selects the mover in the target; stay keeps the source
        // selected with the focused-removal MRU fallback, never the target.
        // Rust remains the structural authority; the adapter follows only
        // this desired focus after an exact accepted ack plus a matching
        // verified post-observation.
        let (desired_focus_domain, desired_focus_leaf) = if follow {
            (Some(target_key.clone()), Some(link.leaf.clone()))
        } else {
            match self.focus_stack_fallback(&source_key, &desired_trees, &desired_windows) {
                Some(next) => (Some(source_key.clone()), Some(next)),
                None => (None, None),
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
        let affected = vec![source_key.clone(), target_key.clone()];
        let desired_geometry = project_affected_geometry(
            &self.domains,
            &desired_trees,
            &desired_windows,
            &affected,
            &hints_from_observed(&session_observation.windows),
        )
        .map_err(|_| ProposeError::Refused(RefusalKind::MalformedTopology))?;
        if !geometry_covers_affected(&desired_geometry, &desired_windows, &affected) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let desired_snapshot = self.snapshot_for(&desired_trees, &desired_windows);
        let intent = if allow_cross_output {
            LifecycleIntent::MoveToOutput {
                window: window.clone(),
                target_output: target_key.output.clone(),
                target_workspace: target_key.workspace.clone(),
                follow,
            }
        } else {
            LifecycleIntent::MoveToWorkspace {
                window: window.clone(),
                target_output: target_key.output.clone(),
                target_workspace: target_key.workspace.clone(),
                follow,
            }
        };
        let operation = LifecycleOperation::MoveTiled {
            window: window.clone(),
            leaf: link.leaf.clone(),
            source_output: source_key.output.clone(),
            source_workspace: source_key.workspace.clone(),
            target_output: target_key.output.clone(),
            target_workspace: target_key.workspace.clone(),
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
        });
        self.last_send_placement = Some(crate::session::SendPlacementTrace {
            anchor_kind,
            anchor: remembered.clone(),
            axis,
            projected,
            target_leaves,
        });
        Ok(SessionPlan {
            dispatch,
            desired_snapshot,
            desired_focus_domain,
            desired_focus_leaf,
            desired_geometry,
        })
    }

    /// Slotless deferred send transfer for a held born-fullscreen
    /// exception member (D28 option (a)).
    ///
    /// Eligibility is exactly [`Session::deferred_send_eligible`]: any
    /// other exception (plain intentional float, sticky, maximized,
    /// exited mover, wrong homing) refuses as `NotTiled`, so the D16
    /// intentional-float gap stays refused. The transfer adds/removes no
    /// window identity: the rest-match binds every non-mover plus the
    /// mover exemption and rejects extras by count. Trees, tiled links, and allocations are untouched:
    /// the mover exception simply rehomes to the target while staying
    /// slotless and fullscreen-held. Follow selects the target MRU tiled
    /// fallback for bookkeeping (native follow runs adapter-side on the
    /// mover ref); stay falls back through the source MRU exactly like
    /// the ordinary transfer. Plans commit only
    /// via acknowledge-then-[`Session::verify_lifecycle`].
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    fn propose_deferred_send(
        &mut self,
        window: &WindowId,
        target_output: &OutputId,
        target_workspace: &WorkspaceId,
        follow: bool,
        allow_cross_output: bool,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &LifecycleCapabilities,
    ) -> Result<SessionPlan, ProposeError> {
        let record = self
            .exceptions
            .get(window)
            .cloned()
            .expect("exception mover");
        let source_key = DomainKey {
            output: record.output.clone(),
            workspace: record.workspace.clone(),
        };
        let target_key = DomainKey {
            output: target_output.clone(),
            workspace: target_workspace.clone(),
        };
        // Held exemption for the mover only; every other member binds
        // exactly like the ordinary transfer (same refusal on mismatch).
        // The rest-match below also proves completeness: it binds every
        // non-mover plus the mover exemption, and rejects any extra
        // observed identity by count.
        if !self.deferred_send_eligible(window, &source_key, &session_observation.windows) {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        }
        if !self.observed_rest_match(&session_observation.windows, window) {
            return Err(ProposeError::Refused(RefusalKind::PartialObservation));
        }
        if allow_cross_output {
            if target_key.output == source_key.output {
                return Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch));
            }
        } else {
            if target_key == source_key {
                return Err(ProposeError::Refused(RefusalKind::Unchanged));
            }
            if target_key.output != source_key.output {
                return Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch));
            }
        }
        if self
            .domains
            .iter()
            .find(|d| d.key() == target_key)
            .is_none()
        {
            return Err(ProposeError::Refused(RefusalKind::UnknownDomain));
        }
        // No tree mutation: desired trees and tiled links are exactly the
        // retained ones. Only the mover exception rehomes; its flags
        // (floating-only hold shape) are preserved verbatim.
        let desired_trees = self.trees.clone();
        let desired_windows = self.windows.clone();
        let mut desired_exceptions = self.exceptions.clone();
        desired_exceptions.insert(
            window.clone(),
            ExceptionRecord {
                window: window.clone(),
                output: target_key.output.clone(),
                workspace: target_key.workspace.clone(),
                flags: record.flags,
                floating_geometry: record.floating_geometry,
            },
        );
        // Follow selects the target MRU tiled fallback (never the mover:
        // the slotless mover has no leaf to name, and native follow runs
        // adapter-side on the mover ref). A target with no focus history yet (fresh/empty-target sends are the norm) falls
        // back to the window-id-first retained tile so the committed
        // focus stays resolvable; a member-less target leaves follow
        // focus null. Stay falls back through the source MRU exactly
        // like the ordinary transfer, never selecting the target.
        // Either way the committed focus stays a resolvable tiled leaf
        // (or null), so later reconciles keep working.
        let (desired_focus_domain, desired_focus_leaf) = if follow {
            match self.focus_stack_fallback(&target_key, &desired_trees, &desired_windows) {
                Some(next) => (Some(target_key.clone()), Some(next)),
                None => match desired_windows
                    .iter()
                    .find(|(_, link)| {
                        link.output == target_key.output && link.workspace == target_key.workspace
                    })
                    .map(|(_, link)| link.leaf.clone())
                {
                    Some(first) => (Some(target_key.clone()), Some(first)),
                    None => (None, None),
                },
            }
        } else {
            match self.focus_stack_fallback(&source_key, &desired_trees, &desired_windows) {
                Some(next) => (Some(source_key.clone()), Some(next)),
                None => (None, None),
            }
        };
        if !validate_topology(
            &self.domains,
            &desired_trees,
            &desired_windows,
            &desired_exceptions,
            &desired_focus_domain,
            &desired_focus_leaf,
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        // Projected affected geometry must still cover the retained tiled
        // members exactly: with no tree change the allocations are
        // retained, never reflowed (the slotless mover takes none).
        let affected = vec![source_key.clone(), target_key.clone()];
        let desired_geometry = project_affected_geometry(
            &self.domains,
            &desired_trees,
            &desired_windows,
            &affected,
            &hints_from_observed(&session_observation.windows),
        )
        .map_err(|_| ProposeError::Refused(RefusalKind::MalformedTopology))?;
        if !geometry_covers_affected(&desired_geometry, &desired_windows, &affected) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        if desired_geometry.iter().any(|g| &g.window == window) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let desired_snapshot = self.snapshot_for(&desired_trees, &desired_windows);
        let intent = if allow_cross_output {
            LifecycleIntent::MoveToOutput {
                window: window.clone(),
                target_output: target_key.output.clone(),
                target_workspace: target_key.workspace.clone(),
                follow,
            }
        } else {
            LifecycleIntent::MoveToWorkspace {
                window: window.clone(),
                target_output: target_key.output.clone(),
                target_workspace: target_key.workspace.clone(),
                follow,
            }
        };
        let operation = LifecycleOperation::MoveDeferred {
            window: window.clone(),
            source_output: source_key.output.clone(),
            source_workspace: source_key.workspace.clone(),
            target_output: target_key.output.clone(),
            target_workspace: target_key.workspace.clone(),
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
            exceptions: desired_exceptions,
            retained_float_geometry: self.retained_float_geometry.clone(),
            automatic_fixed: self.automatic_fixed.clone(),
            fixed_tile_override: self.fixed_tile_override.clone(),
        });
        let target_bounds = self
            .domains
            .iter()
            .find(|d| d.key() == target_key)
            .map(|d| d.bounds)
            .expect("deferred target domain");
        let target_leaves = desired_trees
            .get(&target_key)
            .cloned()
            .flatten()
            .map(|tree| collect_leaves(&tree).len())
            .unwrap_or(0);
        self.last_send_placement = Some(crate::session::SendPlacementTrace {
            anchor_kind: "deferred",
            anchor: None,
            axis: self.policy().admission_axis_for_rect(&target_bounds),
            projected: target_bounds,
            target_leaves,
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
