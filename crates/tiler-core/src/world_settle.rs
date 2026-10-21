//! Retained transfer settlement (D3=A): view, window/migration, and float
//! settlement against staged homing, raw echoes, and attempt feedback.
//!
//! Split from [`crate::world_engine`]: same `impl WorldEngine` ownership,
//! only relocated for source size. No behavior change.

use crate::directional::WindowId;
use crate::session::DomainKey;
use crate::world_engine::{RetainedFloat, RetainedTransfer, WorldEngine};
use crate::world_report::{AttemptOutcome, TransferKind, WorldReport};

impl WorldEngine {
    /// Settle retained view targets against raw echoes before staging.
    /// Confirms move core views to the proven logical id and record
    /// history; externally moved echoes (differing from both the
    /// expectation and the last echo) are newer input and win: the flight
    /// drops and the echo is adopted as the view. Stale echoes hold.
    /// Proven refusal or external supersession feedback settles to
    /// observed. Runs before [`WorldEngine::build_staged`] so homing
    /// assigns against adapter truth, never a stale desire.
    pub(crate) fn settle_views_pre(&mut self, report: &WorldReport) {
        let mut keep: Vec<RetainedTransfer> = Vec::new();
        for retained in std::mem::take(&mut self.retained) {
            if !matches!(&retained.target.kind, TransferKind::View) {
                keep.push(retained);
                continue;
            }
            let feedback = report
                .feedback
                .iter()
                .find(|f| f.operation_id == retained.target.operation_id);
            let echo = report
                .views
                .iter()
                .find(|view| view.output == retained.target.target_output)
                .map(|view| view.view.as_str())
                .filter(|view| !view.is_empty());
            if echo == Some(retained.target.target_workspace.as_str()) {
                // Confirmed by echo: adopt as the core view.
                self.views.insert(
                    retained.target.target_output.0.clone(),
                    retained.target.target_workspace.clone(),
                );
                let ring = self
                    .mapping
                    .get(&retained.target.target_output.0)
                    .cloned()
                    .unwrap_or_default();
                self.history.observe(
                    &retained.target.target_output.0,
                    &retained.target.target_workspace,
                    &ring,
                );
            } else if matches!(
                feedback.map(|f| f.outcome),
                Some(AttemptOutcome::RefusedProven | AttemptOutcome::SupersededExternal)
            ) {
                // Settle to observed.
            } else {
                let last = self
                    .last_views
                    .get(&retained.target.target_output)
                    .map(String::as_str);
                let Some(echo) = echo else {
                    // Missing/empty echo is a gap, never external
                    // supersession and never a ring-head revert: hold the
                    // flight with the retained view intact.
                    keep.push(retained);
                    continue;
                };
                if Some(echo) != last {
                    // External view input supersedes: adopt it when it
                    // names a known logical workspace. Unknown ids hold
                    // (gap, never external proof); stale echoes hold.
                    if self
                        .mapping
                        .get(&retained.target.target_output.0)
                        .is_some_and(|ring| ring.iter().any(|id| id == echo))
                    {
                        self.views
                            .insert(retained.target.target_output.0.clone(), echo.to_owned());
                        let ring = self
                            .mapping
                            .get(&retained.target.target_output.0)
                            .cloned()
                            .unwrap_or_default();
                        self.history
                            .observe(&retained.target.target_output.0, echo, &ring);
                    } else {
                        keep.push(retained);
                    }
                } else {
                    keep.push(retained);
                }
            }
        }
        self.retained = keep;
    }

    /// Settle outstanding window and migration transfers plus explicit
    /// float placements against the staged homing, raw echoes, and explicit
    /// attempt feedback. Runs after [`WorldEngine::build_staged`] and before
    /// the sweep.
    pub(crate) fn settle_rest(&mut self, report: &WorldReport) {
        let mut keep: Vec<RetainedTransfer> = Vec::new();
        for retained in std::mem::take(&mut self.retained) {
            let feedback = report
                .feedback
                .iter()
                .find(|f| f.operation_id == retained.target.operation_id);
            let refused = matches!(
                feedback.map(|f| f.outcome),
                Some(AttemptOutcome::RefusedProven)
            );
            if refused && let TransferKind::Window(window) = &retained.target.kind {
                // Deferred until after the sweep: the unshielded sweep may
                // re-store the slot while converging back to observed.
                self.pending_forget.push((
                    DomainKey {
                        output: retained.target.target_output.clone(),
                        workspace: crate::directional::WorkspaceId(
                            retained.target.target_workspace.clone(),
                        ),
                    },
                    window.clone(),
                ));
            }
            match &retained.target.kind {
                TransferKind::Window(window) => {
                    let homing = self.staged_homing(window);
                    let closed = self.staged.closes.contains(window);
                    let source_key = DomainKey {
                        output: retained.source_output.clone(),
                        workspace: crate::directional::WorkspaceId(
                            retained.source_workspace.clone(),
                        ),
                    };
                    let target_key = DomainKey {
                        output: retained.target.target_output.clone(),
                        workspace: crate::directional::WorkspaceId(
                            retained.target.target_workspace.clone(),
                        ),
                    };
                    if matches!(
                        feedback.map(|f| f.outcome),
                        Some(AttemptOutcome::RefusedProven | AttemptOutcome::SupersededExternal)
                    ) {
                        // Dropped above; refusal slot cleanup already queued.
                    } else if homing.as_ref() == Some(&target_key) {
                        // Confirmed by staged homing -- except same-output
                        // transfers, where the committed Engine session
                        // already homes at the target before the native
                        // moves. Retained session homing must not confirm
                        // itself: a same-output send settles only on
                        // observed logical membership naming the exact
                        // target workspace (unknown or wrong membership
                        // holds the flight).
                        let same_output = retained.source_output == retained.target.target_output;
                        let observed_proves = self
                            .staged
                            .facts
                            .get(window)
                            .and_then(|fact| fact.observed_workspace.as_deref())
                            == Some(retained.target.target_workspace.as_str());
                        if same_output && !observed_proves {
                            if matches!(
                                feedback.map(|f| f.outcome),
                                Some(AttemptOutcome::PossiblyExecuted)
                            ) {
                                let mut held = retained;
                                held.target.held = true;
                                keep.push(held);
                            } else {
                                keep.push(retained);
                            }
                        }
                        // Otherwise confirmed: the flight drops.
                    } else if closed {
                        // Explicit close settles; the sweep forgets the slot.
                    } else if let Some(key) = homing {
                        if key != source_key {
                            // Newer external input off both transfer screens
                            // wins; a contradictory receipt on either screen
                            // holds (never proves arrival, never fought).
                            // Hotplug policy intents are the exception: the
                            // screen vanished under the member (no newer
                            // user input), so the intent holds instead of
                            // dropping, and the outstanding flight keeps the
                            // member homed until arrival proves it.
                            if retained.policy {
                                keep.push(retained);
                            } else if key.output == target_key.output
                                || key.output == source_key.output
                            {
                                if matches!(
                                    feedback.map(|f| f.outcome),
                                    Some(AttemptOutcome::PossiblyExecuted)
                                ) {
                                    let mut held = retained;
                                    held.target.held = true;
                                    keep.push(held);
                                } else {
                                    keep.push(retained);
                                }
                            }
                        } else if matches!(
                            feedback.map(|f| f.outcome),
                            Some(AttemptOutcome::PossiblyExecuted)
                        ) {
                            let mut held = retained;
                            held.target.held = true;
                            keep.push(held);
                        } else {
                            keep.push(retained);
                        }
                    } else if matches!(
                        feedback.map(|f| f.outcome),
                        Some(AttemptOutcome::PossiblyExecuted)
                    ) {
                        let mut held = retained;
                        held.target.held = true;
                        keep.push(held);
                    } else {
                        // Absent everywhere and not closed: never infer a
                        // close; retain across the gap.
                        keep.push(retained);
                    }
                }
                TransferKind::View => {
                    // Retained by the pre-stage view pass only when the
                    // echo is stale; nothing left to do here.
                    keep.push(retained);
                }
                TransferKind::Migration => {
                    self.settle_migration(report, retained, &mut keep);
                }
            }
        }
        self.retained = keep;
        self.settle_floats(report);
    }

    /// Settle one outstanding whole-workspace migration against staged
    /// member homing plus both view proofs (no premature settlement):
    /// every surviving member is at the target key, externally superseded,
    /// or explicitly closed; the target output echoes the destination
    /// logical id; and the source output echoes the stored refill (when one
    /// was selected). Each member proves its own arrival: homed at the
    /// target key settles it into the surviving set, an explicit close
    /// drops it in either direction, and a homing at neither source nor
    /// target drops it as newer external input (never fought). Members
    /// still homed at the source (delayed arrival) or absent everywhere
    /// without a close (partial/unreadable arrival) stay outstanding with
    /// arrived members preserved, and so does a migration whose members
    /// arrived but either view is still old: the transfer is never settled
    /// prematurely and never re-actuated. The expected refill rides the
    /// parent (never the retained refill child), so refill-child settlement
    /// before the parent never loses the proof. An originally-empty
    /// migration still verifies both views exactly like arrived ones; a
    /// non-empty migration whose members all externally superseded/closed
    /// settles to observed without fighting either view. A target view
    /// that moved externally (echo differs from both the expectation and
    /// the last echo) settles the parent instead of fighting newer input,
    /// and a settled parent never reactivates. Proven refusal or external
    /// supersession feedback settles the whole transfer; the retained
    /// logical mapping persists (core authority), while possibly-executed
    /// holds the flight without re-actuation.
    pub(crate) fn settle_migration(
        &mut self,
        report: &WorldReport,
        mut retained: RetainedTransfer,
        keep: &mut Vec<RetainedTransfer>,
    ) {
        let feedback = report
            .feedback
            .iter()
            .find(|f| f.operation_id == retained.target.operation_id);
        // Latch the terminal outcome under the operation identity so it
        // survives the feedback row being acknowledged: `held` alone
        // cannot distinguish possibly-executed ambiguity from proven
        // refusal/supersession. Only terminal outcomes latch; later
        // readable evidence reconciles against the latched outcome.
        // Possibly-executed never latches and never rolls back.
        if let Some(outcome) = feedback.map(|f| f.outcome)
            && matches!(
                outcome,
                AttemptOutcome::RefusedProven | AttemptOutcome::SupersededExternal
            )
        {
            retained.migration_terminal = Some(outcome);
        }
        let terminal = retained.migration_terminal;
        // Paired refill-child presence (child pushed before the parent, so a
        // held child is already in `keep`; a settled child is gone). Computed
        // before the `hold` closure borrows `keep`.
        let refill_child_op = format!("{}-refill", retained.target.operation_id);
        let refill_child_held = keep.iter().any(|r| {
            matches!(&r.target.kind, TransferKind::View) && r.target.operation_id == refill_child_op
        });
        let mut hold = |mut held: RetainedTransfer| {
            if matches!(
                feedback.map(|f| f.outcome),
                Some(AttemptOutcome::PossiblyExecuted)
            ) {
                held.target.held = true;
            }
            keep.push(held);
        };
        if matches!(
            terminal,
            Some(AttemptOutcome::RefusedProven | AttemptOutcome::SupersededExternal)
        ) {
            // Reconcile to observed evidence instead of holding the target
            // desires forever: classify arrivals from staged homing. Gap
            // bars apply first: never fabricate wholly-unarrived rollback
            // from absence. Explicit closes still settle across gaps.
            let target_key = DomainKey {
                output: retained.target.target_output.clone(),
                workspace: crate::directional::WorkspaceId(
                    retained.target.target_workspace.clone(),
                ),
            };
            let source_key = DomainKey {
                output: retained.source_output.clone(),
                workspace: crate::directional::WorkspaceId(retained.source_workspace.clone()),
            };
            let pruned: Vec<WindowId> = retained
                .members
                .iter()
                .filter(|m| !self.staged.closes.contains(m))
                .cloned()
                .collect();
            if pruned.is_empty()
                && !retained.migration_initial_empty
                && !retained.members.is_empty()
            {
                keep.retain(|r| {
                    !(matches!(&r.target.kind, TransferKind::View)
                        && r.target.operation_id == refill_child_op)
                });
                return;
            }
            // Gap bars: partial/unreadable inventory, incomplete screens,
            // or a held gesture never prove non-arrival. Hold narrowly
            // with identities preserved, marked held persistently so the
            // flight survives the refusal acknowledgement disappearing.
            if !report.inventory_complete {
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            if self.screen_complete(&retained.source_output) != Some(true)
                || self.screen_complete(&retained.target.target_output) != Some(true)
            {
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            if report.gesture_barrier {
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            let unreadable_member = pruned.iter().any(|m| {
                self.staged.facts.get(m).is_some_and(|f| {
                    f.rect.is_none() && !f.minimized && !self.staged.closes.contains(m)
                })
            });
            if unreadable_member {
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            // Classify only genuine evidence: arrived at the target,
            // proven at the source, or gap (absent/unreadable/unknown or
            // contradictory receipts that never prove arrival).
            let mut arrived = 0usize;
            let mut proven = 0usize;
            let mut gap = 0usize;
            for member in &pruned {
                match self.staged_homing(member) {
                    Some(key) if key == target_key => arrived += 1,
                    Some(key) if key == source_key => proven += 1,
                    Some(_) | None => gap += 1,
                }
            }
            if gap > 0 {
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            if arrived > 0 && proven > 0 {
                // Partial ambiguity: hold narrowly with identities
                // preserved, never fabricated rollback or proof. Marked
                // held persistently so the adapter never retries after the
                // refusal feedback disappears.
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            if arrived == 0 && proven == 0 && !pruned.is_empty() {
                // No genuine evidence either way (should be unreachable
                // after the gap bars, but never roll back from it).
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            if arrived == 0 && !retained.migration_initial_empty && !pruned.is_empty() {
                // Wholly proven at the source: roll the retained logical
                // unit back with stable identities, no reissued token. Gate
                // on hostability first: if the source screen cannot host
                // the session, hold narrowly through the gap instead of a
                // destructive best-effort fallback that would delete the
                // target session and reseed the tree. Tree/shares/focus
                // ride the rekey only when the relocate preserves them.
                let ws = retained.target.target_workspace.clone();
                let rollback_source = DomainKey {
                    output: retained.source_output.clone(),
                    workspace: crate::directional::WorkspaceId(ws.clone()),
                };
                let rollback_target = DomainKey {
                    output: retained.target.target_output.clone(),
                    workspace: crate::directional::WorkspaceId(ws.clone()),
                };
                if !self.can_relocate_session_for_move(&rollback_target, &rollback_source) {
                    let mut held = retained;
                    held.members = pruned;
                    held.target.held = true;
                    keep.push(held);
                    return;
                }
                if let Some(ring) = self.mapping.get_mut(&retained.target.target_output.0) {
                    ring.retain(|id| id != &ws);
                }
                {
                    let source_ring = self
                        .mapping
                        .entry(retained.source_output.0.clone())
                        .or_default();
                    if !source_ring.iter().any(|id| id == &ws) {
                        let at = retained
                            .migration_source_index
                            .unwrap_or(source_ring.len())
                            .min(source_ring.len());
                        source_ring.insert(at, ws.clone());
                    }
                }
                self.carry_tiling(
                    &ws,
                    &retained.target.target_output.0,
                    &retained.source_output.0,
                );
                self.rekey_members(
                    &ws,
                    &retained.target.target_output.0,
                    &retained.source_output.0,
                );
                // Session rollback preserves the original unit: relocate
                // the committed target session back to the source geometry.
                // Hostability was gated above, so this preserves
                // tree/shares/focus; a failed relocate here leaves state
                // exactly as validated.
                let source_key = DomainKey {
                    output: retained.source_output.clone(),
                    workspace: crate::directional::WorkspaceId(ws.clone()),
                };
                let target_key = DomainKey {
                    output: retained.target.target_output.clone(),
                    workspace: crate::directional::WorkspaceId(ws.clone()),
                };
                let relocated = self.relocate_session_for_move(&target_key, &source_key);
                debug_assert!(relocated, "rollback gated on hostability");
                let source_view = retained
                    .migration_source_view
                    .clone()
                    .filter(|view| {
                        self.mapping
                            .get(&retained.source_output.0)
                            .is_some_and(|ring| ring.iter().any(|id| id == view))
                    })
                    .unwrap_or_else(|| ws.clone());
                self.views
                    .insert(retained.source_output.0.clone(), source_view.clone());
                if let Some(prev_target) = retained.migration_target_view.clone() {
                    let known = self
                        .mapping
                        .get(&retained.target.target_output.0)
                        .is_some_and(|ring| ring.iter().any(|id| id == &prev_target));
                    if known {
                        self.views
                            .insert(retained.target.target_output.0.clone(), prev_target);
                    } else {
                        let target_ring = self
                            .mapping
                            .get(&retained.target.target_output.0)
                            .cloned()
                            .unwrap_or_default();
                        if let Some(head) = target_ring.first().cloned() {
                            self.views
                                .insert(retained.target.target_output.0.clone(), head);
                        } else {
                            self.views.remove(&retained.target.target_output.0);
                        }
                    }
                } else {
                    let target_ring = self
                        .mapping
                        .get(&retained.target.target_output.0)
                        .cloned()
                        .unwrap_or_default();
                    if let Some(head) = target_ring.first().cloned() {
                        self.views
                            .insert(retained.target.target_output.0.clone(), head);
                    } else {
                        self.views.remove(&retained.target.target_output.0);
                    }
                }
                let source_ring = self
                    .mapping
                    .get(&retained.source_output.0)
                    .cloned()
                    .unwrap_or_default();
                self.history
                    .observe(&retained.source_output.0, &source_view, &source_ring);
            }
            // Settle to observed; the retained logical mapping persists
            // (rolled back above for the wholly-unarrived case).
            keep.retain(|r| {
                !(matches!(&r.target.kind, TransferKind::View)
                    && r.target.operation_id == refill_child_op)
            });
            return;
        }
        // Fresh-report bar: partial inventory, an incomplete screen on
        // either output, or a held scope at either key holds the flight
        // with identities preserved, never settles from a gap. Explicit
        // closes are pruned even across gaps so a fully-closed migration
        // settles without waiting for views that will never arrive.
        let pruned: Vec<WindowId> = retained
            .members
            .iter()
            .filter(|m| !self.staged.closes.contains(m))
            .cloned()
            .collect();
        if pruned.is_empty() && !retained.migration_initial_empty && !retained.members.is_empty() {
            keep.retain(|r| {
                !(matches!(&r.target.kind, TransferKind::View)
                    && r.target.operation_id == refill_child_op)
            });
            return;
        }
        if !report.inventory_complete {
            let mut held = retained;
            held.members = pruned;
            hold(held);
            return;
        }
        let source_key = DomainKey {
            output: retained.source_output.clone(),
            workspace: crate::directional::WorkspaceId(retained.source_workspace.clone()),
        };
        let target_key = DomainKey {
            output: retained.target.target_output.clone(),
            workspace: crate::directional::WorkspaceId(retained.target.target_workspace.clone()),
        };
        if self.screen_complete(&retained.source_output) != Some(true)
            || self.screen_complete(&retained.target.target_output) != Some(true)
        {
            let mut held = retained;
            held.members = pruned;
            hold(held);
            return;
        }
        if report.gesture_barrier {
            let mut held = retained;
            held.members = pruned;
            hold(held);
            return;
        }
        // Unreadable member frames hold narrowly: partial/unreadable arrival
        // stays outstanding with arrived members preserved.
        let unreadable_member = pruned.iter().any(|m| {
            self.staged.facts.get(m).is_some_and(|f| {
                f.rect.is_none() && !f.minimized && !self.staged.closes.contains(m)
            })
        });
        if unreadable_member {
            let mut held = retained;
            held.members = pruned;
            hold(held);
            return;
        }
        let mut outstanding = Vec::new();
        let mut surviving = Vec::new();
        for member in &pruned {
            if self.staged.closes.contains(member) {
                continue;
            }
            match self.staged_homing(member) {
                Some(key) if key == target_key => {
                    surviving.push(member.clone());
                }
                Some(key) if key == source_key => {
                    outstanding.push(member.clone());
                    surviving.push(member.clone());
                }
                Some(key) => {
                    // Contradictory receipts on either transfer screen hold
                    // (never arrival proof); truly off-screen moves drop as
                    // newer input without a fight.
                    if key.output == target_key.output || key.output == source_key.output {
                        outstanding.push(member.clone());
                        surviving.push(member.clone());
                    }
                }
                None => {
                    outstanding.push(member.clone());
                    surviving.push(member.clone());
                }
            }
        }
        surviving.sort();
        // Deterministic outstanding order for stable retained state.
        outstanding.sort();
        if !outstanding.is_empty() {
            let mut held = retained;
            held.members = surviving;
            hold(held);
            return;
        }
        // No outstanding members: all surviving arrived, all settled
        // externally/closed, or originally empty. A non-empty migration with
        // no survivors settled every member through newer input or close:
        // settle to observed without fighting either view.
        if surviving.is_empty() && !retained.migration_initial_empty {
            keep.retain(|r| {
                !(matches!(&r.target.kind, TransferKind::View)
                    && r.target.operation_id == refill_child_op)
            });
            return;
        }
        // Both views: the adapter actuates the destination follow and the
        // stored source refill; settlement waits for each echo proof. Empty
        // migrations verify both views exactly like arrived ones. The
        // expected refill rides the parent, so refill-child settlement
        // before the parent never loses the proof: when the child is gone
        // (settled) but the source echo is not the refill, the source moved
        // externally and the parent settles instead of holding forever
        // waiting for a refill that will never be re-actuated (no
        // reactivation, never fighting newer input). When the child is still
        // held (true lag, views never proven), the parent holds and waits.
        // A target echo that moved externally (differs from both the
        // expectation and the last echo) settles instead of fighting.
        if let Some(refill) = retained.migration_refill.as_deref() {
            let source_echo = self
                .staged
                .echoes
                .get(&retained.source_output.0)
                .map(String::as_str);
            if source_echo.is_none() {
                // Missing echo is a gap: hold, never settle as external.
                let mut held = retained;
                held.members = surviving;
                hold(held);
                return;
            }
            if !refill_child_held && source_echo != Some(refill) {
                // Unknown source echoes hold (gap); only a known different
                // logical id proves external movement.
                let known = self
                    .mapping
                    .get(&retained.source_output.0)
                    .is_some_and(|ring| {
                        source_echo.is_some_and(|echo| ring.iter().any(|id| id == echo))
                    });
                if !known {
                    let mut held = retained;
                    held.members = surviving;
                    hold(held);
                    return;
                }
                keep.retain(|r| {
                    !(matches!(&r.target.kind, TransferKind::View)
                        && r.target.operation_id == refill_child_op)
                });
                return;
            }
        }
        let target_echo = self
            .staged
            .echoes
            .get(&retained.target.target_output.0)
            .map(String::as_str);
        if target_echo != Some(retained.target.target_workspace.as_str()) {
            // Missing echo is a gap: hold, never external supersession and
            // never a ring-head revert.
            if target_echo.is_none() {
                let mut held = retained;
                held.members = surviving;
                hold(held);
                return;
            }
            let last = self
                .last_views
                .get(&retained.target.target_output)
                .map(String::as_str);
            if target_echo != last {
                // Externally moved target view settles, but unknown ids
                // hold (gap, never external proof).
                let known = self
                    .mapping
                    .get(&retained.target.target_output.0)
                    .is_some_and(|ring| {
                        target_echo.is_some_and(|echo| ring.iter().any(|id| id == echo))
                    });
                if !known {
                    let mut held = retained;
                    held.members = surviving;
                    hold(held);
                    return;
                }
                // Externally moved target view: settle, drop the refill child.
                keep.retain(|r| {
                    !(matches!(&r.target.kind, TransferKind::View)
                        && r.target.operation_id == refill_child_op)
                });
                return;
            }
            let mut held = retained;
            held.members = surviving;
            hold(held);
            return;
        }
        if let Some(refill) = retained.migration_refill.as_deref()
            && self
                .staged
                .echoes
                .get(&retained.source_output.0)
                .map(String::as_str)
                != Some(refill)
        {
            let mut held = retained;
            held.members = surviving;
            hold(held);
            return;
        }
        // Settled: record both views in history and drop the refill child.
        let target_ring = self
            .mapping
            .get(&retained.target.target_output.0)
            .cloned()
            .unwrap_or_default();
        self.history.observe(
            &retained.target.target_output.0,
            &retained.target.target_workspace,
            &target_ring,
        );
        if let Some(refill) = retained.migration_refill.as_deref() {
            let source_ring = self
                .mapping
                .get(&retained.source_output.0)
                .cloned()
                .unwrap_or_default();
            self.history
                .observe(&retained.source_output.0, refill, &source_ring);
        }
        keep.retain(|r| {
            !(matches!(&r.target.kind, TransferKind::View)
                && r.target.operation_id == refill_child_op)
        });
    }

    /// Settle outstanding explicit float-placement targets against the raw
    /// observation and explicit property feedback before the sweep. Accepted
    /// (`Applied`) feedback is recorded by the adapter's read-back proof;
    /// here only the terminal outcomes and observation proofs settle:
    /// confirmation (observed frame equals the target), proven refusal or
    /// external supersession feedback, explicit close, or a native frame
    /// that matches neither the command-time source nor the target (newer
    /// input wins, never fought). Possibly-executed latches ambiguity under
    /// the operation identity so the hold survives the feedback ack;
    /// absence without a close retains across the gap.
    pub(crate) fn settle_floats(&mut self, report: &WorldReport) {
        let mut keep: Vec<RetainedFloat> = Vec::new();
        for mut retained in std::mem::take(&mut self.retained_floats) {
            let feedback = report
                .feedback
                .iter()
                .find(|f| f.operation_id == retained.operation_id);
            if matches!(
                feedback.map(|f| f.outcome),
                Some(AttemptOutcome::RefusedProven | AttemptOutcome::SupersededExternal)
            ) {
                continue;
            }
            if self.staged.closes.contains(&retained.window) {
                continue;
            }
            if matches!(
                feedback.map(|f| f.outcome),
                Some(AttemptOutcome::PossiblyExecuted)
            ) {
                retained.ambiguous = true;
            }
            match self
                .staged
                .facts
                .get(&retained.window)
                .and_then(|fact| fact.rect)
            {
                None => {
                    // Absent everywhere and not closed: never infer a
                    // close; retain across the gap.
                    keep.push(retained);
                }
                Some(current) => {
                    if current == retained.target {
                        // Confirmed by observation.
                    } else if retained.ambiguous {
                        // Ambiguous actuation (a clamp/partial setter can
                        // leave a frame matching neither source nor
                        // target): hold the target without re-actuation
                        // until confirmation, proven refusal, explicit
                        // external supersession, or close — exactly like a
                        // possibly-executed transfer. The latch survives
                        // the feedback ack, so never mistake the
                        // ambiguity for newer input.
                        keep.push(retained);
                    } else if retained.source.is_some_and(|source| current != source) {
                        // Native frame matches neither the command-time
                        // source nor the target: newer external input
                        // (manual move/resize) wins; the stale target drops
                        // rather than fighting it.
                    } else {
                        keep.push(retained);
                    }
                }
            }
        }
        self.retained_floats = keep;
    }
}
