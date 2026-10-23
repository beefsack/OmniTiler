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
use crate::world_engine::{RetainedClear, RetainedFloat, WorldEngine};
use crate::world_report::{
    ClearAttempt, FrameSetter, OperationEvent, OperationOutcome, WorldReport,
};

/// Exact-correlated raw clear evidence for one retained clear operation.
/// Forged rows (operation or window mismatch) are never evidence: they
/// read as absent.
fn clear_attempt_for<'a>(
    report: &'a WorldReport,
    operation_id: &str,
    window: &WindowId,
) -> Option<&'a ClearAttempt> {
    report
        .clear_attempts
        .iter()
        .find(|attempt| attempt.operation_id == operation_id && attempt.window == *window)
}

/// Proven clear refusal: an explicit native refusal with an unchanged
/// readable maximized preimage. Anything else (accepted mismatch, throw,
/// unreadable proof, moved preimage) latches ambiguity, never refusal.
fn clear_refused(attempt: &ClearAttempt) -> bool {
    matches!(attempt.setter, FrameSetter::Refused)
        && attempt.pre_maximized == Some(true)
        && attempt.post_maximized == Some(true)
}

/// Proven clear dispatch: the setter ran (accepted, possibly completing
/// asynchronously) against a readable maximized preimage. Never proof on
/// its own: the staged observation must also show complete readable
/// unmaximized facts (checked by the caller). An async completion that
/// lands after the report still resumes: the attempt proves the core
/// dispatched this clear (never an external clear), and the later
/// unmaximized observation proves the result.
fn clear_accepted(attempt: &ClearAttempt) -> bool {
    matches!(attempt.setter, FrameSetter::Accepted) && attempt.pre_maximized == Some(true)
}

impl WorldEngine {
    /// One reused terminal-event emitter for every retained-operation
    /// verdict (clear and float alike): the original operation id plus
    /// window plus outcome (plus a narrow cause) rides the reply's one-shot
    /// `operation_settled` carrier. Holds emit nothing; the binding simply
    /// persists.
    pub(crate) fn emit_operation(
        &mut self,
        operation_id: String,
        window: WindowId,
        outcome: OperationOutcome,
        cause: Option<&str>,
    ) {
        self.operation_settled.push(OperationEvent {
            operation_id,
            window,
            outcome,
            cause: cause.map(str::to_owned),
        });
    }
}

impl WorldEngine {
    /// Settle outstanding directional maximize-clears against
    /// operation-correlated raw clear attempts plus the staged observation.
    /// Runs first in [`WorldEngine::settle_rest`] so a proven clear resumes
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
    pub(crate) fn settle_clears(&mut self, report: &WorldReport, consumed: bool) {
        use crate::world_directional::{
            effective_domain, float_half_snap_rect, half_snap_constraints_ok,
        };
        use crate::world_report::TilingMode;
        let mut keep: Vec<RetainedClear> = Vec::new();
        // Same-report supersession window (when this report carries a fresh
        // same-window intent): old bindings supersede before continuation,
        // but correlated evidence still stages above. Consumed/replayed
        // commands never supersede.
        let supersede_window: Option<crate::directional::WindowId> = if consumed {
            None
        } else {
            report
                .command
                .as_ref()
                .and_then(crate::world_operations::command_window)
        };
        for mut retained in std::mem::take(&mut self.retained_clears) {
            // Explicit close prunes even across gaps.
            if self.staged.closes.contains(&retained.window) {
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearAbandonedClosed,
                    None,
                );
                continue;
            }
            // Stage exact-correlated raw evidence before any hold.
            if let Some(attempt) =
                clear_attempt_for(report, &retained.operation_id, &retained.window)
            {
                retained.pending = Some(attempt.clone());
            }
            // Same-report supersession before continuation: a fresh
            // same-window intent drops the old binding (evidence already
            // staged above) and re-evaluates fresh in the command path.
            // No unwanted old move.
            if supersede_window.as_ref() == Some(&retained.window) {
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearSuperseded,
                    None,
                );
                continue;
            }
            // Gap bars hold with identities preserved, never settling from
            // absence and never erasing staged evidence.
            if !report.inventory_complete || report.gesture_barrier {
                if retained.pending.is_some() {
                    retained.ambiguous = true;
                }
                keep.push(retained);
                continue;
            }
            let homing = self.staged_homing(&retained.window);
            let Some(home) = homing.clone() else {
                // Complete inventory with no homing and no close: a
                // complete departure/homing loss prunes the binding.
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearAbandonedHomingChanged,
                    Some("homing-loss"),
                );
                continue;
            };
            let Some(fact) = self.staged.facts.get(&retained.window).cloned() else {
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearAbandonedHomingChanged,
                    Some("homing-loss"),
                );
                continue;
            };
            // Minimized or unreadable facts hold without weakening the
            // retained authority.
            if fact.minimized || fact.rect.is_none() {
                if retained.pending.is_some() {
                    retained.ambiguous = true;
                }
                keep.push(retained);
                continue;
            }
            // Changed lifetime/home abandons: the proof no longer names the
            // bound intent.
            if home != retained.home {
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearAbandonedHomingChanged,
                    None,
                );
                continue;
            }
            // Held-domain defers both float and tiled continuation until
            // authority returns (never silently drops). The staged home
            // domain unreadable means the scope holds.
            let home_held = self
                .staged
                .domains
                .iter()
                .find(|domain| domain.key == retained.home)
                .is_none_or(|domain| !domain.readable);
            if home_held {
                if retained.pending.is_some() {
                    retained.ambiguous = true;
                }
                keep.push(retained);
                continue;
            }
            // Changed displayed view: unknown current or unknown baseline
            // holds (cannot safely continue); only known changed abandons.
            let view = self.effective_view(&retained.home.output);
            match (&view, &retained.view) {
                (None, _) | (_, None) => {
                    if retained.pending.is_some() {
                        retained.ambiguous = true;
                    }
                    keep.push(retained);
                    continue;
                }
                (Some(current), Some(baseline)) if current != baseline => {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        retained.window.clone(),
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
            let current_focus = report.focused_global.as_str();
            let original_focus = retained.focus.as_str();
            if current_focus.is_empty() {
                if original_focus.is_empty() {
                    // Unknown baseline plus unknown current: no
                    // contradiction, hold for later proof.
                    if retained.pending.is_some() {
                        retained.ambiguous = true;
                    }
                    keep.push(retained);
                    continue;
                }
                if retained.pending.is_some() {
                    retained.ambiguous = true;
                }
                keep.push(retained);
                continue;
            }
            if original_focus.is_empty() {
                // Unknown baseline is not a contradiction, but a known
                // current focus naming another window abandons: the native
                // exact-subject proof already ties the request, so a stale
                // intent must not survive known wrong focus to resume
                // later. Only the exact bound subject permits
                // continuation with accepted proof.
                if current_focus != retained.window.0 {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        retained.window.clone(),
                        OperationOutcome::ClearAbandonedFocusChanged,
                        Some("non-subject-focus"),
                    );
                    continue;
                }
            } else {
                if current_focus != original_focus {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        retained.window.clone(),
                        OperationOutcome::ClearAbandonedFocusChanged,
                        None,
                    );
                    continue;
                }
                if current_focus != retained.window.0 {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        retained.window.clone(),
                        OperationOutcome::ClearAbandonedFocusChanged,
                        Some("baseline-non-subject"),
                    );
                    continue;
                }
            }
            // A fullscreen subject abandons: the original move can no
            // longer proceed.
            if fact.fullscreen {
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearAbandonedFullscreen,
                    None,
                );
                continue;
            }
            if fact.maximized {
                // Still maximized: never success. Proven refusal retires
                // with no loop; anything else holds.
                if retained.pending.as_ref().is_some_and(clear_refused) {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        retained.window.clone(),
                        OperationOutcome::ClearRefusedProven,
                        None,
                    );
                    continue;
                }
                if retained.pending.is_some() {
                    retained.ambiguous = true;
                }
                keep.push(retained);
                continue;
            }
            // Unmaximized, complete, readable facts: resume needs a
            // correlated accepted attempt. An external clear alone (no
            // staged attempt at all) abandons, never continues. Ambiguous
            // staged evidence (throw, accepted mismatch, unreadable proof)
            // holds with no retry below: it may yet be our own clear.
            let Some(pending) = retained.pending.clone() else {
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearAbandonedExternalClear,
                    None,
                );
                continue;
            };
            if clear_refused(&pending) {
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
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
                // completing asynchronously.
                retained.pending = Some(pending);
                retained.ambiguous = true;
                keep.push(retained);
                continue;
            }
            // Proven: correlated accepted attempt plus truthful readable
            // unmaximized facts. Resume the original intent under the
            // ORIGINAL operation id: tiled-as-tiled, float-as-float.
            // Retirement here is continuation only, never proof the final
            // frame arrived (float arrival settles separately; tiled
            // arrival proves via the transfer).
            if retained.float_route {
                let effective = effective_domain(&retained.home, fact.sticky, view.as_deref());
                if self.logical_tiling(&effective.output.0, &effective.workspace.0)
                    != TilingMode::Tiled
                {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        retained.window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("tiling-changed"),
                    );
                    continue;
                }
                // Held effective domain defers until authority returns.
                let effective_held = self
                    .staged
                    .domains
                    .iter()
                    .find(|domain| domain.key == effective)
                    .is_none_or(|domain| !domain.readable);
                if effective_held {
                    if retained.pending.is_some() {
                        retained.ambiguous = true;
                    }
                    keep.push(retained);
                    continue;
                }
                let Some(screen) = report.screens.iter().find(|s| s.output == effective.output)
                else {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        retained.window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("unknown-output"),
                    );
                    continue;
                };
                let (Some(work), gap) = (screen.work_area, screen.gap) else {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        retained.window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("unknown-output"),
                    );
                    continue;
                };
                let Some(half) = float_half_snap_rect(work, gap, retained.direction.as_str())
                else {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        retained.window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("invalid-geometry"),
                    );
                    continue;
                };
                if !half_snap_constraints_ok(half, fact.hints) {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        retained.window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("constraints"),
                    );
                    continue;
                }
                self.retained_floats
                    .retain(|held| held.window != retained.window);
                self.retained_floats.push(RetainedFloat {
                    operation_id: retained.operation_id.clone(),
                    window: retained.window.clone(),
                    target: half,
                    source: fact.rect,
                    output: effective.output.clone(),
                    workspace: effective.workspace.0.clone(),
                    ambiguous: false,
                    pending: None,
                });
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
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
                        retained.window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("invalid-direction"),
                    );
                    continue;
                };
                let owned: std::collections::BTreeMap<WindowId, EngineWindow> = self
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
                    .staged
                    .domains
                    .iter()
                    .filter(|domain| !domain.readable)
                    .map(|domain| domain.key.clone())
                    .collect();
                // Held home defers until authority returns.
                if held_scopes.contains(&retained.home) {
                    if retained.pending.is_some() {
                        retained.ambiguous = true;
                    }
                    keep.push(retained);
                    continue;
                }
                let Some(source_row) = owned.get(&retained.window).cloned() else {
                    self.emit_operation(
                        retained.operation_id.clone(),
                        retained.window.clone(),
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
                        retained.window.clone(),
                        OperationOutcome::ClearContinuationRefused,
                        Some("early-verdict"),
                    );
                    continue;
                }
                // Held effective domain defers until authority returns.
                if held_scopes.contains(&effective_now) {
                    if retained.pending.is_some() {
                        retained.ambiguous = true;
                    }
                    keep.push(retained);
                    continue;
                }
                let command_seq: u64 = retained
                    .operation_id
                    .strip_prefix("op-")
                    .and_then(|suffix| suffix.parse::<u64>().ok())
                    .unwrap_or(0);
                let pinned = match self.window_topology_target(
                    &source_row.output,
                    direction_parsed,
                    &source_row.rect,
                ) {
                    Err(_) => {
                        self.emit_operation(
                            retained.operation_id.clone(),
                            retained.window.clone(),
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
                    let command = crate::boundary::CoreCommand::Move {
                        window: retained.window.0.clone(),
                        direction: retained.direction.clone(),
                        cross_output_transfer: true,
                        same_axis_move: retained.same_axis,
                    };
                    let window_name = retained.window.0.clone();
                    let direction_name = retained.direction.clone();
                    self.operation_directional(
                        report,
                        command_seq,
                        &command,
                        &window_name,
                        direction_name.as_str(),
                        Some(&target),
                        &by_id,
                        &held_scopes,
                    )
                } else {
                    let command = crate::boundary::CoreCommand::Move {
                        window: retained.window.0.clone(),
                        direction: retained.direction.clone(),
                        cross_output_transfer: false,
                        same_axis_move: retained.same_axis,
                    };
                    self.run_local_operation(report, command_seq, &command, &by_id, &held_scopes)
                };
                // Fallible commands must not leak a stale directional
                // result into this observation-only reply.
                self.command_result = None;
                match applied {
                    crate::world_commands::CommandApplied::Rejected { kind, .. }
                        if kind == crate::world_engine::KIND_SCOPE_HELD =>
                    {
                        if retained.pending.is_some() {
                            retained.ambiguous = true;
                        }
                        keep.push(retained);
                        continue;
                    }
                    crate::world_commands::CommandApplied::Rejected { kind, .. } => {
                        self.emit_operation(
                            retained.operation_id.clone(),
                            retained.window.clone(),
                            OperationOutcome::ClearContinuationRefused,
                            Some(kind),
                        );
                        continue;
                    }
                    _ => {}
                }
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearContinuedTiled,
                    None,
                );
            }
        }
        self.retained_clears = keep;
    }
}
