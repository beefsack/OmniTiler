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
    /// unknown windows refuse as `UnknownWindow`, exception windows outside
    /// the deferred eligibility gate or an unresolved
    /// focused leaf as `NotTiled`, and a missing global focus or
    /// requested non-focused tile as `FocusMismatch`. The source domain must
    /// equal the focused domain before mutation. Intentional floats
    /// (plain or approved fullscreen/maximized combos) transfer slotless
    /// through the deferred path instead.
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
        mover_live_frame: Option<Rect>,
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
            mover_live_frame,
            session_observation,
            correlation_id,
            capabilities,
        )
    }

    /// Explicit output send (REQ-OUT-04, item 5.4): identical ordinary
    /// transfer to [`Session::propose_move_to_workspace`] (focused tiled
    /// subject through the ordinary transfer, intentional floats (plain
    /// or approved combos) through the slotless deferred transfer, sticky still
    /// ineligible as `NotTiled`;
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
        mover_live_frame: Option<Rect>,
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
            mover_live_frame,
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
        mover_live_frame: Option<Rect>,
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
        // A slotless floating exception member carries through the
        // deferred transfer (D28 hold, D16 ordinary float, D16 approved
        // combos): the held mover rides the native send-route flags while
        // the retained record holds the floating shape, and plain/combo
        // floats ride still floating with the retained size, so the
        // ordinary exact matcher below cannot bind them. Route before
        // that matcher; the deferred path binds the rest exactly plus
        // the mover exemption only.
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
        let ordinary_target = insert_tiled(
            self.policy(),
            target_tree.clone(),
            remembered.as_ref(),
            link.leaf.clone(),
            axis,
            &mut node_ids,
            window,
            base_revision,
        )
        .ok_or(ProposeError::Refused(RefusalKind::MalformedTopology))?;
        if !collect_leaves(&ordinary_target).contains(&link.leaf) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        // Best-effort destination admission (D2 destination-local, then D3):
        // the ordinary anchor above first; only a geometrically failing
        // ordinary anchor consults the D2 fallback within the destination
        // domain (never a redirect). Every candidate must pass whole-tree
        // strict plus hinted projection (the mover's own hints ride along:
        // `target_links` includes the mover leaf). D3 overlap-admits at
        // the ordinary anchor under `allow-overlap`, else capacity-floats
        // the mover slotless at the destination under the default float
        // result (no tile slot, exact current frame, diagnostic origin).
        let hints = hints_from_observed(&session_observation.windows);
        let mut target_links: BTreeMap<NodeId, WindowId> = self
            .windows
            .values()
            .filter(|l| l.output == target_key.output && l.workspace == target_key.workspace)
            .map(|l| (l.leaf.clone(), l.window.clone()))
            .collect();
        target_links.insert(link.leaf.clone(), window.clone());
        let admits = |tree: &Node| {
            crate::admission::tree_admits_candidate(
                tree,
                target_domain.bounds,
                target_domain.gap,
                &hints,
                &target_links,
            ) && {
                let mut probe = self.trees.clone();
                probe.insert(source_key.clone(), new_source.clone());
                probe.insert(target_key.clone(), Some(tree.clone()));
                let mut probe_windows = self.windows.clone();
                probe_windows.insert(
                    window.clone(),
                    WindowLink {
                        window: window.clone(),
                        leaf: link.leaf.clone(),
                        output: target_key.output.clone(),
                        workspace: target_key.workspace.clone(),
                    },
                );
                crate::session::capacity::project_affected_gated(
                    &self.domains,
                    &probe,
                    &probe_windows,
                    &[source_key.clone(), target_key.clone()],
                    &hints,
                    &|key| {
                        self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                            || self.overlap_accepted.contains(key)
                    },
                )
                .is_some()
            }
        };
        let mut new_target = ordinary_target.clone();
        let mut overlap_admit = false;
        if !admits(&ordinary_target) {
            // D2: destination-local fallback search only.
            let fallback_anchors: BTreeMap<NodeId, Rect> = match target_tree.as_ref() {
                Some(tree) => {
                    crate::geometry::project(tree, target_domain.bounds, target_domain.gap)
                        .map(|leaves| {
                            leaves
                                .into_iter()
                                .map(|leaf| (leaf.leaf, leaf.rect))
                                .collect()
                        })
                        .unwrap_or_default()
                }
                None => BTreeMap::new(),
            };
            let mut seq: u64 = 0;
            let mut group_ids = std::iter::from_fn(|| {
                seq += 1;
                let mut n = seq;
                loop {
                    let candidate = NodeId(format!("cap-send-{}-g{n}", window.0));
                    if !node_ids.contains(&candidate) {
                        node_ids.insert(candidate.clone());
                        return Some(candidate);
                    }
                    n += 1;
                }
            });
            match crate::session::capacity::select_admission_target(
                self.anchor_fallback,
                target_tree.as_ref(),
                target_domain.bounds,
                target_domain.gap,
                remembered.as_ref(),
                Some(&ordinary_target),
                fallback_anchors,
                &hints,
                &target_links,
                &link.leaf,
                &mut group_ids,
            ) {
                Ok(selected) => {
                    new_target = selected.tree;
                    if selected.anchor.as_ref() != remembered.as_ref() {
                        anchor_kind = "fallback";
                    }
                }
                Err(()) => {
                    // D3 at the named destination: overlap-admit at the
                    // ordinary anchor under `allow-overlap`, else
                    // capacity-float the mover slotless there under the
                    // default float result (exact current frame,
                    // diagnostic origin, source removal staged
                    // atomically). Only an unrepresentable frame (the
                    // source itself will not project) still refuses with
                    // the source exactly preserved.
                    if self.no_space == crate::admission::NoSpaceResult::AllowOverlap {
                        let mut probe_windows = self.windows.clone();
                        probe_windows.insert(
                            window.clone(),
                            WindowLink {
                                window: window.clone(),
                                leaf: link.leaf.clone(),
                                output: target_key.output.clone(),
                                workspace: target_key.workspace.clone(),
                            },
                        );
                        if crate::session::capacity::project_domain_overlap(
                            &target_domain,
                            &ordinary_target,
                            &probe_windows,
                            &target_key,
                            &hints,
                        )
                        .is_some()
                        {
                            new_target = ordinary_target.clone();
                            overlap_admit = true;
                        } else {
                            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
                        }
                    } else {
                        return self.stage_send_to_float(
                            window,
                            &link.leaf,
                            &source_key,
                            new_source.clone(),
                            &target_key,
                            &target_domain,
                            target_leaves,
                            follow,
                            allow_cross_output,
                            mover_live_frame,
                            session_observation,
                            correlation_id,
                            capabilities,
                        );
                    }
                }
            }
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
        // Gated projection: strict first; retained overlap allocations and
        // overlap mode project through the D9 gate so sends onto accepted
        // overlap domains keep working. Unrepresentable geometry holds
        // truthfully (refuse, source preserved).
        let desired_geometry = crate::session::capacity::project_affected_gated(
            &self.domains,
            &desired_trees,
            &desired_windows,
            &affected,
            &hints,
            &|key| {
                overlap_admit && key == &target_key
                    || self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                    || self.overlap_accepted.contains(key)
            },
        )
        .ok_or(ProposeError::Refused(RefusalKind::MalformedTopology))?;
        if !geometry_covers_affected(&desired_geometry, &desired_windows, &affected) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        // Overlap bookkeeping: an overlap-admit marks the destination;
        // a strict success clears a stale mark there. The mover lifetime
        // keeps its first admission number (moves never re-sequence).
        let mut desired_overlap = self.overlap_accepted.clone();
        if overlap_admit {
            desired_overlap.insert(target_key.clone());
        } else {
            desired_overlap.remove(&target_key);
        }
        if desired_trees.get(&source_key).cloned().flatten().is_none() {
            desired_overlap.remove(&source_key);
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
            capacity_floats: self.capacity_floats.clone(),
            admission_order: self.admission_order.clone(),
            admission_counter: self.admission_counter,
            overlap_accepted: desired_overlap,
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

    /// D3 capacity-float send: the tiled mover exhausts its named
    /// destination under the default float result, so it becomes a slotless
    /// floating exception homed at that destination (diagnostic capacity
    /// origin) instead of refusing. Staged atomically: the source leaf is
    /// removed (survivors collapse), the destination tree is untouched (no
    /// tile slot), and the mover frame travels verbatim for same-output
    /// sends — the engine-carried exact native live frame when provided,
    /// else the exact currently projected source rectangle — with no
    /// geometry write. A cross-output send never stages source
    /// coordinates as a target claim (no translate, centering, or
    /// offscreen guess): the frame stays untouched for the established
    /// native remap plus its arrival proof. Follow/stay focus, the
    /// `MoveToFloat` operation shape, and the acknowledge/verify arrival
    /// proof mirror the deferred transfer; only an unprojectable
    /// same-output source (no exact frame, survivors uncovered) still
    /// refuses with the source exactly preserved.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::session) fn stage_send_to_float(
        &mut self,
        window: &WindowId,
        mover_leaf: &NodeId,
        source_key: &DomainKey,
        new_source: Option<Node>,
        target_key: &DomainKey,
        target_domain: &OutputDomain,
        target_leaves: usize,
        follow: bool,
        allow_cross_output: bool,
        mover_live_frame: Option<Rect>,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &LifecycleCapabilities,
    ) -> Result<SessionPlan, ProposeError> {
        let hints = hints_from_observed(&session_observation.windows);
        let source_domain = self
            .domains
            .iter()
            .find(|d| &d.key() == source_key)
            .cloned()
            .ok_or(ProposeError::Refused(RefusalKind::UnknownDomain))?;
        let source_tree = self.trees.get(source_key).cloned().flatten();
        // A cross-output send still proves the source remaps: an
        // infeasible source (nothing projects, even overlap-accepted)
        // refuses with the source exactly preserved instead of staging
        // a float from broken geometry. Only the frame claim is
        // untouched there, never the transfer itself.
        if allow_cross_output {
            let allow = self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                || self.overlap_accepted.contains(source_key);
            let Some(tree) = source_tree.as_ref() else {
                return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
            };
            if crate::session::capacity::project_domain_geometry(
                &source_domain,
                tree,
                &self.windows,
                source_key,
                &hints,
                allow,
            )
            .is_none_or(|geometry| !geometry.iter().any(|g| &g.leaf == mover_leaf))
            {
                return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
            }
        }
        // Trustworthy frame first: the engine-carried native live frame
        // for a same-output send is the exact current native frame (the
        // projected allocation can differ under B6/clamped hints, so it
        // is never assumed equal). A cross-output send never trusts
        // source coordinates at the target: no translate, centering, or
        // offscreen guess is staged (the established native
        // sendClientToScreen route plus its arrival proof owns the
        // placement there). `None` there means untouched until a fresh
        // native arrival frame is available.
        let mover_frame: Option<Rect> = match (mover_live_frame, allow_cross_output) {
            (Some(live), false) => Some(live),
            (Some(_), true) | (None, true) => None,
            (None, false) => project_output_geometry(
                Some(&source_domain),
                source_tree.as_ref(),
                &self.windows,
                source_key,
                &hints,
            )
            .ok()
            .and_then(|geometry| {
                geometry
                    .into_iter()
                    .find(|g| &g.leaf == mover_leaf)
                    .map(|g| g.rect)
            })
            .or_else(|| {
                let tree = source_tree.as_ref()?;
                let allow = self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                    || self.overlap_accepted.contains(source_key);
                crate::session::capacity::project_domain_geometry(
                    &source_domain,
                    tree,
                    &self.windows,
                    source_key,
                    &hints,
                    allow,
                )
                .and_then(|geometry| {
                    geometry
                        .into_iter()
                        .find(|g| &g.leaf == mover_leaf)
                        .map(|g| g.rect)
                })
            }),
        };
        // Same-output without a live frame keeps the legacy allocation
        // derivation above; anything less is unrepresentable. A
        // cross-output `None` is honest untouched state, never a
        // refusal: the native route proves arrival adapter-side.
        let mover_frame = match (mover_frame, allow_cross_output) {
            (Some(frame), _) => Some(frame),
            (None, false) => return Err(ProposeError::Refused(RefusalKind::MalformedTopology)),
            (None, true) => None,
        };
        let mut desired_trees = self.trees.clone();
        desired_trees.insert(source_key.clone(), new_source);
        let mut desired_windows = self.windows.clone();
        desired_windows.remove(window);
        let mut desired_exceptions = self.exceptions.clone();
        desired_exceptions.insert(
            window.clone(),
            ExceptionRecord {
                window: window.clone(),
                output: target_key.output.clone(),
                workspace: target_key.workspace.clone(),
                flags: ExceptionFlags {
                    floating: true,
                    fullscreen: false,
                    maximized: false,
                    sticky: false,
                },
                floating_geometry: mover_frame,
            },
        );
        let mut desired_retained = self.retained_float_geometry.clone();
        // A fresh native frame retains; an untouched cross-output send
        // keeps prior float memory instead of recording a
        // source-coordinate claim.
        if let Some(frame) = mover_frame {
            desired_retained.insert(window.clone(), frame);
        }
        let mut desired_capacity = self.capacity_floats.clone();
        desired_capacity.insert(window.clone());
        // The mover lifetime keeps its first admission number; moves never
        // re-sequence. An emptied source drops its overlap mark.
        let mut desired_overlap = self.overlap_accepted.clone();
        if desired_trees.get(source_key).cloned().flatten().is_none() {
            desired_overlap.remove(source_key);
        }
        // Follow/stay mirror the deferred transfer: follow names the
        // target bookkeeping MRU (native follow addresses the mover ref),
        // stay falls back through the source MRU, never the target.
        let (desired_focus_domain, desired_focus_leaf) = if follow {
            match self.focus_stack_fallback(target_key, &desired_trees, &desired_windows) {
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
            match self.focus_stack_fallback(source_key, &desired_trees, &desired_windows) {
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
        let affected = vec![source_key.clone(), target_key.clone()];
        let desired_geometry = crate::session::capacity::project_affected_gated(
            &self.domains,
            &desired_trees,
            &desired_windows,
            &affected,
            &hints,
            &|key| {
                self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                    || self.overlap_accepted.contains(key)
            },
        )
        .ok_or(ProposeError::Refused(RefusalKind::MalformedTopology))?;
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
        let operation = LifecycleOperation::MoveToFloat {
            window: window.clone(),
            leaf: mover_leaf.clone(),
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
            retained_float_geometry: desired_retained,
            automatic_fixed: self.automatic_fixed.clone(),
            fixed_tile_override: self.fixed_tile_override.clone(),
            capacity_floats: desired_capacity,
            admission_order: self.admission_order.clone(),
            admission_counter: self.admission_counter,
            overlap_accepted: desired_overlap,
        });
        self.last_send_placement = Some(crate::session::SendPlacementTrace {
            anchor_kind: "capacity-float",
            anchor: None,
            axis: self.policy().admission_axis_for_rect(&target_domain.bounds),
            projected: target_domain.bounds,
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

    /// Slotless deferred send transfer for a floating exception member
    /// (D28 hold, D16 ordinary float, D16 approved combos, all option (a)).
    ///
    /// Eligibility is exactly [`Session::deferred_send_eligible`]; anything
    /// else refuses as `NotTiled`. The transfer adds/removes no
    /// window identity: the rest-match binds every non-mover plus the
    /// mover exemption and rejects extras by count. Trees, tiled links, and allocations are untouched:
    /// the mover exception simply rehomes to the target while staying
    /// slotless with flags and floating size preserved verbatim
    /// (the restart float memory is keyed by window id, so it follows
    /// with no rehoming step). Follow selects the target MRU tiled
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
        // and floating size are preserved verbatim.
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
        // Gated so floated sends onto accepted overlap domains keep
        // working; unrepresentable geometry holds truthfully.
        let affected = vec![source_key.clone(), target_key.clone()];
        let hints = hints_from_observed(&session_observation.windows);
        let desired_geometry = crate::session::capacity::project_affected_gated(
            &self.domains,
            &desired_trees,
            &desired_windows,
            &affected,
            &hints,
            &|key| {
                self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                    || self.overlap_accepted.contains(key)
            },
        )
        .ok_or(ProposeError::Refused(RefusalKind::MalformedTopology))?;
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
            capacity_floats: self.capacity_floats.clone(),
            admission_order: self.admission_order.clone(),
            admission_counter: self.admission_counter,
            overlap_accepted: self.overlap_accepted.clone(),
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
