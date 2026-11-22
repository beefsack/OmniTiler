//! Directional maximize-clear settlement: one outstanding authorized clear
//! per window, keyed by the stable originating command operation identity.
//!
//! Split from [`crate::world_settle`]: same `impl WorldEngine` ownership,
//! only relocated for source size. Owns the clear helpers, the one reused
//! operation-event emitter shared with float settlement, and the clear
//! settlement itself. Float settlement stays in [`crate::world_settle`].
//! No behavior change from the move itself.

use crate::directional::WindowId;
use crate::seed::EngineWindow;
use crate::session::DomainKey;
use crate::world_attempts::{PropertyAttempt, clear_accepted, clear_refused};
use crate::world_engine::{RetainedFloat, WorldEngine, WorldState};
use crate::world_report::{OperationEvent, OperationOutcome, StateProperty, WorldCommand};
use crate::world_state::ClearProvenance;
use crate::world_transaction::WorldTransaction;

impl WorldState {
    /// Occupied plus pending-reserved `(window, kind)` operation slots across
    /// the retained map, in-flight clear/float bindings, and any new
    /// operation-capable commands in the inline/queued batch. Two phase slots
    /// (`Clear`, `Float`) share the single [`MAX_REPLY_OPERATIONS`] cap.
    #[must_use]
    pub(crate) fn operation_settle_slot_count(
        &self,
        command: Option<&WorldCommand>,
        queued: &[(u64, WorldCommand)],
    ) -> usize {
        use crate::boundary::CoreCommand;
        use crate::world_report::OperationKind;
        use std::collections::BTreeSet;
        let mut slots: BTreeSet<(&str, OperationKind)> = BTreeSet::new();
        for (window, kind) in self.settlement.operation_settled.keys() {
            slots.insert((window.0.as_str(), *kind));
        }
        for binding in self.settlement.retained_states.values() {
            match binding.clear_meta.as_ref() {
                Some(ClearProvenance::Directional(meta)) => {
                    slots.insert((binding.target.window.0.as_str(), OperationKind::Clear));
                    if meta.float_route {
                        slots.insert((binding.target.window.0.as_str(), OperationKind::Float));
                    }
                }
                Some(ClearProvenance::Born(_)) => {
                    slots.insert((binding.target.window.0.as_str(), OperationKind::Clear));
                }
                None => {}
            }
        }
        for retained in &self.settlement.retained_floats {
            slots.insert((retained.window.0.as_str(), OperationKind::Float));
        }
        let mut implicit_slots = 0usize;
        let mut no_focus_directional = false;
        for cmd in command.into_iter().chain(queued.iter().map(|(_, c)| c)) {
            match cmd {
                WorldCommand::FocusDirectional {
                    window: crate::world_report::FocusDirectionalSubject::CurrentFocus,
                    ..
                }
                | WorldCommand::MoveDirectional {
                    window: crate::directional::WindowSubject::CurrentFocus,
                    ..
                } => {
                    implicit_slots = implicit_slots.saturating_add(2);
                }
                WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: crate::directional::WindowSubject::CurrentFocus,
                    ..
                })
                | WorldCommand::ToggleMaximize {
                    window: crate::directional::WindowSubject::CurrentFocus,
                }
                | WorldCommand::ToggleSticky {
                    window: crate::directional::WindowSubject::CurrentFocus,
                } => {
                    implicit_slots = implicit_slots.saturating_add(1);
                }
                WorldCommand::FocusDirectional {
                    window: crate::world_report::FocusDirectionalSubject::Window(window),
                    ..
                }
                | WorldCommand::MoveDirectional {
                    window: crate::directional::WindowSubject::Window(window),
                    ..
                } => {
                    slots.insert((window.0.as_str(), OperationKind::Clear));
                    slots.insert((window.0.as_str(), OperationKind::Float));
                }
                WorldCommand::FocusDirectional {
                    window: crate::world_report::FocusDirectionalSubject::NoFocus,
                    ..
                } => {
                    no_focus_directional = true;
                }
                WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: crate::directional::WindowSubject::Window(window),
                    ..
                })
                | WorldCommand::ToggleMaximize {
                    window: crate::directional::WindowSubject::Window(window),
                }
                | WorldCommand::ToggleSticky {
                    window: crate::directional::WindowSubject::Window(window),
                } => {
                    slots.insert((window.0.as_str(), OperationKind::Float));
                }
                _ => {}
            }
        }
        slots
            .len()
            .saturating_add(implicit_slots)
            .saturating_add(if no_focus_directional { 2 } else { 0 })
    }
}

impl WorldTransaction<'_> {
    /// One reused terminal-event emitter for every retained-operation
    /// verdict (clear and float alike): updates the `(window, kind)` slot
    /// in `operation_settled` latest-wins. Holds emit nothing; the binding
    /// simply persists.
    pub(crate) fn emit_operation(
        &mut self,
        operation_id: String,
        window: WindowId,
        outcome: OperationOutcome,
        cause: Option<&str>,
    ) {
        let kind = outcome.kind();
        let event = OperationEvent {
            window: window.clone(),
            kind,
            operation_id,
            outcome,
            cause: cause.map(str::to_owned),
        };
        self.world
            .settlement
            .operation_settled
            .insert((window, kind), event);
    }
}

impl WorldTransaction<'_> {
    /// Settle outstanding directional maximize-clears against
    /// operation-correlated canonical clear evidence plus the staged
    /// observation, staged by the shared clear phase. Runs first in
    /// [`WorldEngine::settle_rest`] so a proven clear resumes
    /// its original intent in the same pass (tiled engine move or retained
    /// float half); the sweep and publish below then apply the continuation
    /// coherently in one reply under the ORIGINAL operation id. No second
    /// command ever consumes a binding: settlement itself is the sole
    /// supersession choke point (same-report same-window intents supersede
    /// above before any continuation, even when the new command later
    /// refuses; consumed replays never supersede), and observation-only
    /// reports drive every continuation here.
    ///
    /// Evidence stages BEFORE any readability or authority hold: a gapped
    /// report never erases knowledge the sequencer pending map forgets
    /// after acknowledgement. Every verdict below except close needs
    /// truthful readable complete data.
    ///
    /// - Explicit closes prune immediately, even across gaps. A window
    ///   absent from homing with a complete inventory (and not closed)
    ///   prunes as a complete departure/homing loss; absent facts across a
    ///   gap hold (never an inferred close).
    /// - Changed lifetime/home/view/focus abandons the binding: the proof
    ///   no longer names the bound intent, so the clear retires with no
    ///   continuation. A fullscreen subject abandons the same way.
    /// - A still-maximized subject holds (never success). Proven refusal
    ///   (explicit refusal with an unchanged readable maximized preimage)
    ///   retires the binding with no loop and no re-actuation.
    /// - An unmaximized subject needs BOTH complete readable unmaximized
    ///   facts AND a correlated accepted attempt to resume: an external
    ///   clear alone (no staged attempt at all) abandons, never continues.
    ///   A refused attempt against unmaximized facts abandons the same way:
    ///   the refusal proves the native clear did not run, so the later
    ///   unmaximized state names newer external input. Anything else fresh
    ///   and correlated (accepted mismatch, throw, unreadable proof) holds
    ///   no-reactuation ambiguity: the spent operation never retries and
    ///   never resumes without proof, without ever inferring arrival from
    ///   an ack or a desire.
    /// - Held (unreadable scope, incomplete inventory, gesture barrier,
    ///   minimized or unreadable fact, unknown current focus/view) holds
    ///   with identities preserved and no weakening of authority.
    pub(crate) fn settle_clears(
        &mut self,
        current_command: Option<&WorldCommand>,
        issue: &crate::world_engine::EventIssue,
        consumed: bool,
    ) {
        use crate::world_directional::{
            effective_domain, float_half_snap_rect, half_snap_constraints_ok,
        };
        use crate::world_report::TilingMode;
        let directional_keys: Vec<(WindowId, StateProperty)> = self
            .world
            .settlement
            .retained_states
            .iter()
            .filter_map(|(key, binding)| {
                matches!(binding.clear_meta, Some(ClearProvenance::Directional(_)))
                    .then_some(key.clone())
            })
            .collect();
        let mut keep: Vec<(
            (WindowId, StateProperty),
            crate::world_state::RetainedStateBinding,
        )> = Vec::new();
        // Same-report supersession window (when this report carries a fresh
        // same-window intent): old bindings supersede before continuation,
        // but correlated evidence still stages above. Consumed/replayed
        // commands never supersede.
        let supersede_window: Option<crate::directional::WindowId> = if consumed {
            None
        } else {
            current_command.and_then(crate::world_operations::command_window)
        };
        for key in directional_keys {
            let Some(mut binding) = self.world.settlement.retained_states.remove(&key) else {
                continue;
            };
            let Some(ClearProvenance::Directional(mut retained)) = binding.clear_meta.take() else {
                self.world.settlement.retained_states.insert(key, binding);
                continue;
            };
            let window = binding.target.window.clone();
            let push_keep =
                |keep: &mut Vec<(
                    (WindowId, StateProperty),
                    crate::world_state::RetainedStateBinding,
                )>,
                 mut binding: crate::world_state::RetainedStateBinding,
                 retained: crate::world_state::DirectionalClearMeta| {
                    binding.clear_meta = Some(ClearProvenance::Directional(retained));
                    keep.push((key.clone(), binding));
                };
            // Explicit close prunes even across gaps.
            if self.world.staged.closes.contains(&window) {
                self.emit_operation(
                    retained.operation_id.clone(),
                    window.clone(),
                    OperationOutcome::ClearAbandonedClosed,
                    None,
                );
                continue;
            }
            // Exact-correlated canonical evidence was staged by the clear
            // phase before any hold; close pruned above before intake.
            // Same-report supersession before continuation: a fresh
            // same-window intent drops the old binding (evidence already
            // staged above) and re-evaluates fresh in the command path.
            // No unwanted old move.
            if supersede_window.as_ref() == Some(&window) {
                self.emit_operation(
                    retained.operation_id.clone(),
                    window.clone(),
                    OperationOutcome::ClearSuperseded,
                    None,
                );
                continue;
            }
            // Gap bars hold with identities preserved, never settling from
            // absence and never erasing staged evidence.
            if !self.world.staged.observation.inventory_complete
                || self.world.staged.gesture_barrier
            {
                if binding.pending.is_some() {
                    retained.ambiguous = true;
                }
                push_keep(&mut keep, binding, retained);
                continue;
            }
            // Narrowly held rows and safety exclusions keep the binding:
            // live presence with no usable facts is never a departure,
            // and pending clears must not write an excluded window. Only
            // an explicit close prunes above.
            if self.is_held_window(&window) || self.is_excluded_window(&window) {
                if binding.pending.is_some() {
                    retained.ambiguous = true;
                }
                push_keep(&mut keep, binding, retained);
                continue;
            }
            let homing = self.staged_homing(&window);
            let Some(home) = homing.clone() else {
                // Complete inventory with no homing and no close: a
                // complete departure/homing loss prunes the binding.
                self.emit_operation(
                    retained.operation_id.clone(),
                    window.clone(),
                    OperationOutcome::ClearAbandonedHomingChanged,
                    Some("homing-loss"),
                );
                continue;
            };
            let Some(fact) = self.world.staged.facts.get(&window).cloned() else {
                self.emit_operation(
                    retained.operation_id.clone(),
                    window.clone(),
                    OperationOutcome::ClearAbandonedHomingChanged,
                    Some("homing-loss"),
                );
                continue;
            };
            // Minimized/hidden, held, excluded, or unreadable facts hold
            // without weakening the retained authority. Raw null overlay
            // already holds via the derived hold; unknown minimized holds
            // its dependent clear without revoking membership.
            let derived_clear = self.derived_for(&window);
            if derived_clear.hidden
                || derived_clear.hold
                || derived_clear.excluded
                || fact.rect.is_none()
            {
                if binding.pending.is_some() {
                    retained.ambiguous = true;
                }
                push_keep(&mut keep, binding, retained);
                continue;
            }
            // Changed lifetime/home abandons: the proof no longer names the
            // bound intent.
            if home != retained.home {
                self.emit_operation(
                    retained.operation_id.clone(),
                    window.clone(),
                    OperationOutcome::ClearAbandonedHomingChanged,
                    None,
                );
                continue;
            }
            // Held-domain defers both float and tiled continuation until
            // authority returns (never silently drops). The staged home
            // domain unreadable means the scope holds.
            let home_held = self
                .world
                .staged
                .domains
                .iter()
                .find(|domain| domain.key == retained.home)
                .is_none_or(|domain| !domain.readable);
            if home_held {
                if binding.pending.is_some() {
                    retained.ambiguous = true;
                }
                push_keep(&mut keep, binding, retained);
                continue;
            }
            // Changed displayed view: unknown current or unknown baseline
            // holds (cannot safely continue); only known changed abandons.
            let view = self.effective_view(&retained.home.output);
            match (&view, &retained.view) {
                (None, _) | (_, None) => {
                    if binding.pending.is_some() {
                        retained.ambiguous = true;
                    }
                    push_keep(&mut keep, binding, retained);
                    continue;
                }
                (Some(current), Some(baseline)) if current != baseline => {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        window.clone(),
                        OperationOutcome::ClearAbandonedViewChanged,
                        None,
                    );
                    continue;
                }
                _ => {}
            }
            // Focus semantics: unknown CURRENT focus holds (cannot safely
            // continue); only known focus retires or continues. Retained
            // focus stays immutable (never guessed/latched). An original
            // baseline naming a non-subject window retires promptly with
            // `baseline-non-subject`: no stale intent survives to resume
            // later under a focus that was never its subject.
            let current_focus = self.world.staged.observation.focused_global.as_str();
            let original_focus = retained.focus.as_str();
            if current_focus.is_empty() {
                if original_focus.is_empty() {
                    // Unknown baseline plus unknown current: no
                    // contradiction, hold for later proof.
                    if binding.pending.is_some() {
                        retained.ambiguous = true;
                    }
                    push_keep(&mut keep, binding, retained);
                    continue;
                }
                if binding.pending.is_some() {
                    retained.ambiguous = true;
                }
                push_keep(&mut keep, binding, retained);
                continue;
            }
            if original_focus.is_empty() {
                // Unknown baseline is not a contradiction, but a known
                // current focus naming another window abandons: the native
                // exact-subject proof already ties the request, so a stale
                // intent must not survive known wrong focus to resume
                // later. Only the exact bound subject permits
                // continuation with accepted proof.
                if current_focus != window.0 {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        window.clone(),
                        OperationOutcome::ClearAbandonedFocusChanged,
                        Some("non-subject-focus"),
                    );
                    continue;
                }
            } else {
                if current_focus != original_focus {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        window.clone(),
                        OperationOutcome::ClearAbandonedFocusChanged,
                        None,
                    );
                    continue;
                }
                if current_focus != window.0 {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        window.clone(),
                        OperationOutcome::ClearAbandonedFocusChanged,
                        Some("baseline-non-subject"),
                    );
                    continue;
                }
            }
            // A fullscreen subject abandons: the original move can no
            // longer proceed. Only proven `Some(true)` abandons; raw null
            // overlay already holds above via the derived hold.
            if fact.fullscreen == Some(true) {
                self.emit_operation(
                    retained.operation_id.clone(),
                    window.clone(),
                    OperationOutcome::ClearAbandonedFullscreen,
                    None,
                );
                continue;
            }
            if derived_clear.overlay_maximized {
                // Still maximized: never success. Proven refusal retires
                // with no loop; anything else holds.
                if binding
                    .pending
                    .as_ref()
                    .and_then(PropertyAttempt::as_state)
                    .is_some_and(clear_refused)
                {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        window.clone(),
                        OperationOutcome::ClearRefusedProven,
                        None,
                    );
                    continue;
                }
                if binding.pending.is_some() {
                    retained.ambiguous = true;
                }
                push_keep(&mut keep, binding, retained);
                continue;
            }
            // Unmaximized, complete, readable facts: resume needs a
            // correlated accepted attempt. An external clear alone (no
            // staged attempt at all) abandons, never continues. Ambiguous
            // staged evidence (throw, accepted mismatch, unreadable proof)
            // holds with no retry below: it may yet be our own clear.
            let Some(pending) = binding
                .pending
                .as_ref()
                .and_then(PropertyAttempt::as_state)
                .cloned()
            else {
                self.emit_operation(
                    retained.operation_id.clone(),
                    window.clone(),
                    OperationOutcome::ClearAbandonedExternalClear,
                    None,
                );
                continue;
            };
            if clear_refused(&pending) {
                self.emit_operation(
                    retained.operation_id.clone(),
                    window.clone(),
                    OperationOutcome::ClearAbandonedExternalClear,
                    Some("refused-with-unmaximized"),
                );
                continue;
            }
            if !clear_accepted(&pending) {
                // Ambiguous evidence (throw, accepted mismatch, unreadable
                // proof) plus unmaximized facts holds with no retry and no
                // continuation: the spent connector cannot emit another
                // attempt for the same op, the binding persists with its
                // staged evidence unchanged, and a fresh same-window intent
                // supersedes it independently. Never inferred as an external
                // clear: the correlated attempt may yet be our own clear
                // completing asynchronously. The canonical evidence already
                // sits in the retained slot; only ambiguity is latched.
                retained.ambiguous = true;
                push_keep(&mut keep, binding, retained);
                continue;
            }
            // Proven: correlated accepted attempt plus truthful readable
            // unmaximized facts. Resume the original intent under the
            // ORIGINAL operation id: tiled-as-tiled, float-as-float.
            // Retirement here is continuation only, never proof the final
            // frame arrived (float arrival settles separately; tiled
            // arrival proves via the transfer).
            if retained.float_route {
                let effective =
                    effective_domain(&retained.home, derived_clear.sticky, view.as_deref());
                if self.logical_tiling(&effective.output.0, &effective.workspace.0)
                    != TilingMode::Tiled
                {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("tiling-changed"),
                    );
                    continue;
                }
                // Held effective domain defers until authority returns.
                let effective_held = self
                    .world
                    .staged
                    .domains
                    .iter()
                    .find(|domain| domain.key == effective)
                    .is_none_or(|domain| !domain.readable);
                if effective_held {
                    if binding.pending.is_some() {
                        retained.ambiguous = true;
                    }
                    push_keep(&mut keep, binding, retained);
                    continue;
                }
                let Some(screen) = self
                    .world
                    .staged
                    .screens
                    .iter()
                    .find(|s| s.output == effective.output)
                else {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("unknown-output"),
                    );
                    continue;
                };
                let Some(work) = screen.work_area else {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("unknown-output"),
                    );
                    continue;
                };
                let Some(gap) =
                    WorldEngine::scale_once(self.connection.gap_setting(), screen.scale)
                else {
                    // Unscalable geometry refuses the continuation without
                    // mutation, never a clamped gap.
                    self.emit_operation(
                        retained.operation_id.clone(),
                        window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("invalid-geometry"),
                    );
                    continue;
                };
                let Some(half) = float_half_snap_rect(work, gap, retained.direction.as_str())
                else {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("invalid-geometry"),
                    );
                    continue;
                };
                if !fact
                    .hints
                    .is_some_and(|hints| half_snap_constraints_ok(half, hints))
                {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("constraints"),
                    );
                    continue;
                }
                self.world
                    .settlement
                    .retained_floats
                    .retain(|held| held.window != window);
                self.world.settlement.retained_floats.push(RetainedFloat {
                    operation_id: retained.operation_id.clone(),
                    command_seq: retained.command_seq,
                    window: window.clone(),
                    target: half,
                    source: fact.rect,
                    output: effective.output.clone(),
                    workspace: effective.workspace.0.clone(),
                    ambiguous: false,
                    pending: None,
                });
                self.emit_operation(
                    retained.operation_id.clone(),
                    window.clone(),
                    OperationOutcome::ClearContinuedFloat,
                    None,
                );
            } else {
                // Tiled resume through the same core-owned pair/local path
                // the command would have taken, under the original identity.
                // All immutable reads complete before the engine mutation
                // below so the mutable borrow never overlaps staged borrows.
                // Held scopes defer (never silently drop); other failures
                // retire with a `continuation-refused` event carrying the
                // original op/window/reason. Success retires with a
                // `continued-tiled` event: the transfer proves the resume,
                // never the final frame arrival.
                let Some(direction_parsed) =
                    crate::world_operations::parse_direction_token(retained.direction.as_str())
                else {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("invalid-direction"),
                    );
                    continue;
                };
                let owned: std::collections::BTreeMap<WindowId, EngineWindow> = self
                    .world
                    .staged
                    .domains
                    .iter()
                    .flat_map(|domain| {
                        domain
                            .windows
                            .iter()
                            .cloned()
                            .map(|window| (window.window.clone(), window))
                    })
                    .collect();
                let held_scopes: std::collections::BTreeSet<DomainKey> = self
                    .world
                    .staged
                    .domains
                    .iter()
                    .filter(|domain| !domain.readable)
                    .map(|domain| domain.key.clone())
                    .collect();
                // Held home defers until authority returns.
                if held_scopes.contains(&retained.home) {
                    if binding.pending.is_some() {
                        retained.ambiguous = true;
                    }
                    push_keep(&mut keep, binding, retained);
                    continue;
                }
                let Some(source_row) = owned.get(&window).cloned() else {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("unknown-window"),
                    );
                    continue;
                };
                let view_now = self.effective_view(&retained.home.output);
                let effective_now =
                    effective_domain(&retained.home, source_row.sticky, view_now.as_deref());
                let tiling_now =
                    self.logical_tiling(&effective_now.output.0, &effective_now.workspace.0);
                if let Some(crate::world_directional::DirectionalVerdict::Refused { .. }) =
                    crate::world_directional::early_verdict(&source_row, tiling_now, true)
                {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("early-verdict"),
                    );
                    continue;
                }
                // Held effective domain defers until authority returns.
                if held_scopes.contains(&effective_now) {
                    if binding.pending.is_some() {
                        retained.ambiguous = true;
                    }
                    push_keep(&mut keep, binding, retained);
                    continue;
                }
                let command_seq: u64 = retained.command_seq;
                let pinned = match self.window_topology_target(
                    &source_row.output,
                    direction_parsed,
                    &source_row.rect,
                ) {
                    Err(_) => {
                        self.emit_operation(
                            retained.operation_id.clone(),
                            window.clone(),
                            OperationOutcome::ClearContinuationRefused,
                            Some("topology-unreadable"),
                        );
                        continue;
                    }
                    Ok(target) => target,
                };
                let by_id: std::collections::BTreeMap<&WindowId, &EngineWindow> =
                    owned.iter().collect();
                let applied = if let Some(target) = pinned {
                    let window_name = window.clone();
                    let command = crate::boundary::CoreCommand::Move {
                        window: window_name.clone(),
                        direction: direction_parsed,
                        cross_output_transfer: true,
                        same_axis_move: retained.same_axis,
                    };
                    self.operation_directional(
                        issue,
                        command_seq,
                        &command,
                        &window_name,
                        direction_parsed,
                        Some(&target),
                        &by_id,
                        &held_scopes,
                    )
                } else {
                    let command = crate::boundary::CoreCommand::Move {
                        window: window.clone(),
                        direction: direction_parsed,
                        cross_output_transfer: false,
                        same_axis_move: retained.same_axis,
                    };
                    self.run_local_operation(command_seq, &command, &by_id, &held_scopes)
                };
                // Fallible commands must not leak a stale directional
                // result into this observation-only reply.
                self.world.command_result = None;
                match applied {
                    crate::world_commands::CommandApplied::Rejected { kind, .. }
                        if kind == crate::world_engine::KIND_SCOPE_HELD =>
                    {
                        if binding.pending.is_some() {
                            retained.ambiguous = true;
                        }
                        push_keep(&mut keep, binding, retained);
                        continue;
                    }
                    crate::world_commands::CommandApplied::Rejected { kind, .. } => {
                        self.emit_operation(
                            retained.operation_id.clone(),
                            window.clone(),
                            OperationOutcome::ClearContinuationRefused,
                            Some(kind),
                        );
                        continue;
                    }
                    _ => {}
                }
                self.emit_operation(
                    retained.operation_id.clone(),
                    window.clone(),
                    OperationOutcome::ClearContinuedTiled,
                    None,
                );
            }
        }
        for (key, binding) in keep {
            self.world.settlement.retained_states.insert(key, binding);
        }
    }
}
