//! Parallel core-internal v4 accepted-run/finish and command dispatch
//! (unit 6 sub-increment 7d).
//!
//! [`WorldEngine::run_accepted_v4`] is an internal accepted run over an
//! already admitted v4 candidate. It
//! captures the original raw v4 facts once, ingests intent exactly once
//! before capability-graded classification, then reuses every shared
//! report-free settlement/sweep/gesture helper in legacy order. Command
//! dispatch validates staged renderer input against the latest previously
//! published presentation revision before publication stamps the new one;
//! a changed target or an unissued revision refuses the command with
//! [`CommandRejectKind::StaleRenderTarget`](crate::world_report::CommandRejectKind::StaleRenderTarget),
//! never a publication hold. Refused commands restore the post-observation
//! image and consume their sequence; consumed reports reconcile only and
//! neither re-execute nor re-validate.
//!
//! The caller owns admission: this entry exposes no public ingress, performs
//! no candidate adoption, replay, budget, or envelope gates (the admission
//! boundary owns those), and never touches the legacy replay cache. Run the entry on a staged
//! clone and discard the candidate when it returns
//! [`V4RunOutcome::Rejected`], exactly like the legacy staged path.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::session::DomainKey;
use crate::world_commands::CommandApplied;
use crate::world_engine::{
    EventIssue, KIND_CREATION_EXHAUSTED, KIND_REVISION_EXHAUSTED, MSG_REVISION_EXHAUSTED,
    PriorAcceptedContext, RawRouting, WorldEngine,
};
use crate::world_gesture::GestureOutcome;
use crate::world_publish_v4::PublishEnvelopeV4;
use crate::world_report::{
    BatchSettlement, CommandRejectKind, CommandResult, EffectKind, HoldReason, IntentWrite,
    TilingMode, TransferKind, WorldCommand, WorldReplyV4, WorldReportV4,
};
use crate::world_report_v4_checks::graded_effective_v4;

/// Genuine outcome of one v4 accepted run: the v4 reply on
/// observation/command progression, or the envelope refusal kind when
/// creation exhaustion discards the staged candidate (legacy parity: the
/// candidate is dropped with no adoption, never a command result).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V4RunOutcome {
    Accepted(WorldReplyV4),
    /// Identical latest duplicate: cached v4 reply, no new revision.
    /// Only the admission boundary mints this; staged runs never do.
    Duplicate(WorldReplyV4),
    CommandConsumed(WorldReplyV4),
    Rejected {
        kind: &'static str,
        message: &'static str,
    },
}

impl WorldEngine {
    /// Run one already-admitted v4 candidate to a genuine typed v4 reply.
    /// `consumed` names an already-consumed command sequence (reconcile
    /// only, never re-execute or re-validate); `issuing_revision` stamps
    /// the resulting presentation and reply. Admitted precondition: the
    /// caller supplies `issuing_revision == state_revision + 1` (the
    /// next live revision; the admission boundary owns the counter). Revision
    /// exhaustion refuses before any mutation. The caller stages on a
    /// clone and discards it on [`V4RunOutcome::Rejected`].
    #[cfg(test)]
    pub(crate) fn run_accepted_v4(
        &mut self,
        report: &WorldReportV4,
        issue: &EventIssue,
        consumed: bool,
        issuing_revision: u64,
    ) -> V4RunOutcome {
        let (outcome, _) =
            self.run_accepted_v4_batched(report, issue, consumed, issuing_revision, &[]);
        outcome
    }

    /// Run one already-admitted v4 candidate plus a parked FIFO command
    /// batch drained inside the same capture/run/publication: native
    /// observations incorporate once, then every command executes in order
    /// with core subject state after the preceding action, and exactly one
    /// full publication carries the last command's result. The inline
    /// report command (when present) runs first; queued commands follow in
    /// FIFO order. Renderer validation applies to the inline command only:
    /// queued commands never carried render proof. Returns the single run
    /// outcome plus one per-command settlement index-aligned with `queued`
    /// (inline results ride the final publication, never a settlement). A
    /// creation-exhausted command discards the whole staged candidate with
    /// empty settlements so the caller reparks the batch.
    pub(crate) fn run_accepted_v4_batched(
        &mut self,
        report: &WorldReportV4,
        issue: &EventIssue,
        consumed: bool,
        issuing_revision: u64,
        queued: &[(u64, WorldCommand)],
    ) -> (V4RunOutcome, Vec<BatchSettlement>) {
        // Admitted-candidate precondition without implementing admission:
        // an exhausted counter refuses and re-bootstraps under a new
        // generation elsewhere; it never wraps in place.
        if self.state_revision == u64::MAX {
            return (
                V4RunOutcome::Rejected {
                    kind: KIND_REVISION_EXHAUSTED,
                    message: MSG_REVISION_EXHAUSTED,
                },
                Vec::new(),
            );
        }
        debug_assert_eq!(
            issuing_revision,
            self.state_revision + 1,
            "admitted issuing revision must be exactly the next live revision"
        );
        // Freeze implicit send sources from pre-observation retained
        // homing, before staging can admit newcomers or settle rehoming.
        // The frozen explicit output is the only source the command below
        // may use. Direction-only migration freezes here too: the frozen
        // pre-observation source view applies verbatim, so view settlement
        // in this report never retargets the current intent. The freeze
        // reads the invalid- and capability-graded effective rows, so an
        // invalid-held screen can never become command routing authority
        // merely because the raw input carried it; raw capture and replay
        // below stay verbatim.
        let unavailable: &[crate::connection::FactName] = self
            .capabilities()
            .map(|capabilities| capabilities.unavailable_facts.as_slice())
            .unwrap_or(&[]);
        let native_sticky_supported = crate::world_state::state_property_supported(
            self.capabilities(),
            crate::world_report::StateProperty::Sticky,
        );
        let maximize_supported = crate::world_state::state_property_supported(
            self.capabilities(),
            crate::world_report::StateProperty::Maximized,
        );
        let graded = graded_effective_v4(
            report,
            unavailable,
            native_sticky_supported,
            maximize_supported,
        );
        let effective = graded.as_ref().unwrap_or(report);
        let source_routing = RawRouting::from_v4(effective);
        // First-command freeze stays contracted on the first command of the
        // batch (inline else oldest queued), never on a later one.
        let first_command = report
            .command
            .as_ref()
            .or_else(|| queued.first().map(|(_, command)| command));
        let (mut frozen_send_source, frozen_migration_source) =
            self.freeze_command_sources(first_command, &source_routing);
        // Pre-observation retained homes plus the pre-observation action
        // context for the first command only: the freeze above read the
        // stale pre-native slot, while execution resolves the UPDATED
        // slot after native incorporation below. The deferred override
        // after the native-focus block re-resolves the first implicit
        // send subject from the updated slot against THIS snapshot, so
        // the first SEND keeps pre-observation vintage for the subject
        // that actually executes. Conditional to an actual first implicit
        // HomeThenAction send; explicit sends and migration freeze
        // unchanged, and no engine state is cloned.
        let first_implicit_home_send = matches!(
            first_command,
            Some(WorldCommand::Implicit(inner))
                if matches!(
                    inner.as_ref(),
                    WorldCommand::SendRelative {
                        source: crate::world_report::SendSource::HomeThenAction,
                        ..
                    } | WorldCommand::SendNumbered {
                        source: crate::world_report::SendSource::HomeThenAction,
                        ..
                    }
                )
        );
        let pre_send_homes: BTreeMap<WindowId, OutputId> = if first_implicit_home_send {
            self.members
                .iter()
                .map(|(window, state)| (window.clone(), state.home.output.clone()))
                .collect()
        } else {
            BTreeMap::new()
        };
        let pre_send_action: Option<OutputId> = if first_implicit_home_send {
            self.resolve_action_output(&source_routing)
        } else {
            None
        };
        // Exactly-once initial ingestion: capture the original raw facts,
        // ingest intent from that capture, then stage the invalid- and
        // capability-graded effective rows from the same capture, so the
        // classifier sees pending first-admission wins this same report
        // with no second ingestion. Later command restages reuse the
        // classification-only `restage_classification` and never reingest.
        let observation = Self::capture_observation_v4(report);
        self.ingest_staged_intent(&observation);
        self.stage_captured_v4(effective, observation);
        // Freeze the positive-close survivor before convergence ingests the
        // host fallback: the retained request records the pre-close MRU
        // winner while later observations stay truthful.
        self.freeze_close_focus(report, issuing_revision);
        self.converge_logical_pre();
        self.settle_views_pre();
        self.build_staged_domains();
        self.gesture_bind(&BTreeSet::new());
        self.settle_rest(report.command.as_ref(), issue, consumed);
        self.build_staged_domains();
        self.settle_states();
        self.settle_selection_feedback();
        self.settle_selection_switch();
        self.settle_frames();
        self.prev_cursor = self.last_cursor;
        self.last_cursor = self.staged.observation.cursor;
        self.engine.set_fresh_cursor(self.staged.observation.cursor);
        self.floating_scopes = self
            .staged
            .domains
            .iter()
            .map(|domain| domain.key.clone())
            .filter(|key| {
                self.tiling
                    .get(key)
                    .is_some_and(|mode| *mode == TilingMode::Floating)
            })
            .collect();
        self.converge_logical_post();
        self.retain_live_topology();
        let mut known_before: BTreeMap<DomainKey, BTreeSet<String>> = BTreeMap::new();
        for domain in &self.staged.domains {
            if !domain.readable {
                continue;
            }
            if self.floating_scopes.contains(&domain.key) {
                continue;
            }
            let Some(session) = self.engine.session(&domain.key) else {
                continue;
            };
            let mut known = BTreeSet::new();
            for link in session.snapshot().windows {
                if link.output == domain.key.output && link.workspace == domain.key.workspace {
                    known.insert(link.window.0.clone());
                }
            }
            for observed in session.exception_observed() {
                if observed.output == domain.key.output
                    && observed.workspace == domain.key.workspace
                {
                    known.insert(observed.window.0.clone());
                }
            }
            for (window, output, workspace) in session.minimized_entries() {
                if output == domain.key.output && workspace == domain.key.workspace {
                    known.insert(window.0.clone());
                }
            }
            known_before.insert(domain.key.clone(), known);
        }
        let mut slotted_before: BTreeSet<WindowId> = BTreeSet::new();
        for domain in &self.staged.domains {
            let Some(session) = self.engine.session(&domain.key) else {
                continue;
            };
            for link in session.snapshot().windows {
                slotted_before.insert(link.window.clone());
            }
        }
        self.hold_reasons.clear();
        let mut held_scopes: BTreeSet<DomainKey> = BTreeSet::new();
        if self.host_owned_workspace_domains() {
            let mut fence: Vec<DomainKey> = Vec::new();
            for (window, fact) in &self.staged.facts {
                let authoritative = match fact.output.as_ref() {
                    Some(_) => self.native_authoritative(),
                    None => self.host_owned_workspace_domains(),
                };
                if !authoritative {
                    continue;
                }
                if self.effective_logical_receipt(fact).is_some() {
                    continue;
                }
                if let Some(key) = self.staged.homing.get(window) {
                    // Retained all-desktop home: positive sticky with no
                    // single native workspace keeps its already-retained
                    // same-lifetime home without fencing the scope.
                    // Unknown/missing membership and newcomers with no
                    // home stay fenced.
                    let all_desktop = fact.sticky == Some(true)
                        && fact.native_workspace.as_deref().is_none()
                        && self.core_homing(window).is_some();
                    if !all_desktop {
                        fence.push(key.clone());
                    }
                }
            }
            if self.native_order_conflict() {
                for domain in &self.staged.domains {
                    if !fence.contains(&domain.key) {
                        fence.push(domain.key.clone());
                    }
                }
            }
            for key in fence {
                self.hold_reasons
                    .insert(key.clone(), HoldReason::Unreadable);
                held_scopes.insert(key);
            }
        }
        let held_window_scopes: Vec<DomainKey> = self
            .staged
            .held_windows
            .iter()
            .filter_map(|held| self.core_homing(&held.window))
            .collect();
        for key in &held_window_scopes {
            self.hold_reasons
                .entry(key.clone())
                .or_insert(HoldReason::Unreadable);
            held_scopes.insert(key.clone());
        }
        self.capture_excluded_intent();
        self.run_native_sticky_followups(issue, &held_scopes);
        let domains = self.staged.domains.clone();
        for domain in &domains {
            if held_window_scopes.contains(&domain.key) {
                continue;
            }
            if !domain.readable {
                continue;
            }
            if self.floating_scopes.contains(&domain.key) {
                continue;
            }
            if !self.staged.observation.inventory_complete
                && self.retained.iter().any(|r| {
                    matches!(&r.target.kind, TransferKind::Migration)
                        && (DomainKey {
                            output: r.source_output.clone(),
                            workspace: crate::directional::WorkspaceId(r.source_workspace.clone()),
                        } == domain.key
                            || DomainKey {
                                output: r.target.target_output.clone(),
                                workspace: crate::directional::WorkspaceId(
                                    r.target.target_workspace.clone(),
                                ),
                            } == domain.key)
                })
            {
                self.hold_reasons.insert(
                    domain.key.clone(),
                    crate::world_report::HoldReason::IncompleteInventory,
                );
                held_scopes.insert(domain.key.clone());
                continue;
            }
            self.sweep_domain(domain, &issue.owner, &issue.correlation, &mut held_scopes);
        }
        self.settle_intent_admissions();
        {
            let prev = self.prior_context.prev_focused_global.clone();
            let current_focused = self.staged.observation.focused_global.clone();
            if !current_focused.is_empty() && prev != current_focused {
                let active = WindowId(current_focused.clone());
                let _ = self.record_canonical(&active, &held_scopes, false);
                // A genuine new native focus event beats prior core intent:
                // when the fail-closed record above skipped, the stale slot
                // must not survive under the new event.
                if self
                    .logical_focus_cursor
                    .as_ref()
                    .is_none_or(|slot| slot.0 != current_focused)
                {
                    self.logical_focus_cursor = None;
                }
            } else if current_focused.is_empty() {
                // Genuine native focus loss clears the execution-time
                // subject. Per-workspace canonical memory
                // (`last_active_window`) is untouched: it stays best-effort
                // for workspace return.
                self.logical_focus_cursor = None;
            }
            // Close or departure clears a slot target that left the live
            // set; unchanged re-reports keep an unlanded command choice.
            if let Some(slot) = self.logical_focus_cursor.clone()
                && (self.staged.closes.contains(&slot) || !self.staged.homing.contains_key(&slot))
            {
                self.logical_focus_cursor = None;
            }
            // A live retained request preserves its target as the cursor
            // across an unchanged-baseline report: stale observed A must
            // not overwrite pending B.
            {
                let observed = self.staged.observation.focused_global.clone();
                self.preserve_live_pending_cursor(&observed);
            }
        }
        // Deferred first-command subject re-resolution: the pre-observation
        // freeze read the stale slot, but native incorporation above moved
        // it. Re-resolve the first implicit HomeThenAction send subject from
        // the UPDATED slot against the pre-observation snapshot (retained
        // home else pre-observation action context), never staged postimage
        // state: same-report rehoming of the execution subject still refuses
        // instead of retargeting, and an unresolvable subject freezes
        // nothing so execution refuses without inventing focus.
        if first_implicit_home_send {
            frozen_send_source = self.execution_subject().and_then(|subject| {
                pre_send_homes
                    .get(&subject)
                    .cloned()
                    .or(pre_send_action.clone())
            });
        }
        {
            let mut pre_known: BTreeSet<WindowId> = BTreeSet::new();
            for known in known_before.values() {
                for id in known {
                    pre_known.insert(WindowId(id.clone()));
                }
            }
            self.mint_born_clears_v4(&pre_known, &slotted_before);
        }
        for (key, window) in std::mem::take(&mut self.pending_forget) {
            self.engine.forget_minimized_slot(&key, &window);
        }
        for (key, window) in self.confirmed_minimized_rehomes() {
            self.engine.forget_minimized_slot(&key, &window);
        }
        self.command_result = None;
        match self.gesture_act(issue, &mut held_scopes) {
            GestureOutcome::Absent | GestureOutcome::Held => {}
            GestureOutcome::Settled { committed, result } => {
                if committed {
                    let was_floating = self.floating_scopes.clone();
                    self.resweep_retiled(
                        &issue.owner,
                        &issue.correlation,
                        &mut held_scopes,
                        &was_floating,
                    );
                    self.build_staged_domains();
                    self.settle_intent_admissions();
                }
                self.command_result = result.map(|boxed| *boxed);
            }
        }
        let mut effects = self.take_admission_focus(&known_before, &held_scopes);
        // Same-capture admission enters the execution-time ordering before
        // commands: the accepted newcomer advances the logical cursor
        // immediately, so a later implicit subject resolves from core focus,
        // not stale native foreground.
        for effect in &effects {
            if effect.kind == EffectKind::Focus {
                let _ = self.record_canonical(&effect.target, &held_scopes, false);
            }
        }
        if consumed {
            // Consumed sequences reconcile only, never re-execute or
            // re-validate: the result never re-emits, while a gesture
            // result stays the gesture's own.
            crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
            let result = self.command_result.take();
            effects = self.drain_focus_candidates(effects, &held_scopes, report, issuing_revision);
            let reply = self.publish_v4(
                PublishEnvelopeV4 {
                    adapter_incarnation: report.adapter_incarnation.clone(),
                    request_id: report.request_id.clone(),
                    observation_seq: report.observation_seq,
                    command_consumed: true,
                    issuing_revision,
                },
                report.command.as_ref(),
                effects,
                result,
                &held_scopes,
            );
            return (self.finish_v4(report, issuing_revision, reply), Vec::new());
        }
        // Batch command list: the inline report command (when present)
        // runs first, then every parked command in FIFO order. The inline
        // entry carries its renderer proof; queued entries never do.
        let mut commands: Vec<(u64, WorldCommand, bool)> = Vec::new();
        if let Some(command) = report.command.as_ref() {
            commands.push((
                report.command_seq.expect("admitted v4 pair"),
                command.clone(),
                true,
            ));
        }
        for (seq, command) in queued {
            commands.push((*seq, command.clone(), false));
        }
        // Admission focus effects above are history, never promises: the
        // first explicit batch Focus supersedes every unissued prior Focus
        // (admission or earlier batch), so only the latest batch Focus
        // publishes. Focus is the only effect kind. Trailing non-focus
        // commands never clear the surviving focus.
        if commands.is_empty() {
            let result = self.command_result.take();
            crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
            effects = self.drain_focus_candidates(effects, &held_scopes, report, issuing_revision);
            let reply = self.publish_v4(
                PublishEnvelopeV4 {
                    adapter_incarnation: report.adapter_incarnation.clone(),
                    request_id: report.request_id.clone(),
                    observation_seq: report.observation_seq,
                    command_consumed: false,
                    issuing_revision,
                },
                report.command.as_ref(),
                effects,
                result,
                &held_scopes,
            );
            return (self.finish_v4(report, issuing_revision, reply), Vec::new());
        }
        let mut settlements: Vec<BatchSettlement> = Vec::new();
        let mut last_result: Option<CommandResult> = None;
        // Baseline for in-batch focus resolution: post-observation session
        // state, before the first command runs. Observations applied once
        // above; every command below owns post-previous-command state.
        self.snapshot_batch_focus_baseline();
        // Explicit batch intent accumulator, latest-wins per window. The
        // pre-existing sticky refusal rollback saves stay untouched in
        // their map until the single final merge below, so intermediate
        // commands never consume, duplicate, or drop them.
        let mut batch_saves: BTreeMap<WindowId, IntentWrite> = BTreeMap::new();
        // Post-command bookkeeping identity per executed command: the
        // resolved execution subject carried from pre-execution state,
        // never focus after the action. The final publish drains with the
        // last executed command's identity.
        let mut last_bookkeeping: Option<WorldCommand> = None;
        let mut first = true;
        let last_index = commands.len() - 1;
        for (index, (command_seq, command, is_inline)) in commands.iter().enumerate() {
            let command_seq = *command_seq;
            let is_inline = *is_inline;
            // Already-consumed sequences reconcile without re-executing:
            // no effect, no result, no high-watermark motion.
            if self
                .command_highwater
                .is_some_and(|mark| command_seq <= mark)
            {
                settlements.push(BatchSettlement {
                    seq: command_seq,
                    result: None,
                });
                first = false;
                continue;
            }
            // Observation-only pre-command image per command: a refusal
            // restores exactly the post-previous-command image, so earlier
            // batch effects stay published while this command's partial
            // mutation never survives.
            let precommand = self.clone();
            if is_inline
                && let Some(reject) = self.validate_renderer_input_v4(
                    report.renderer_input.as_ref(),
                    precommand.latest_published_presentation_rev_v4,
                )
            {
                *self = precommand;
                crate::world_engine::emit_resize_refusal(self.command_refusal_diagnostic_v4(
                    report,
                    command,
                    command_seq,
                    reject.as_str(),
                ));
                self.note_command(command_seq);
                let result = Some(CommandResult::Rejected { kind: reject });
                crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
                settlements.push(BatchSettlement {
                    seq: command_seq,
                    result: result.clone(),
                });
                last_result = result;
                first = false;
                continue;
            }
            // The first batch command reuses the contracted pre-observation
            // freeze above; every later subject re-freezes from staged
            // post-previous-command state.
            let (send_source, migration_source) = if first {
                (frozen_send_source.clone(), frozen_migration_source.clone())
            } else {
                let routing = RawRouting::from_staged(&self.staged);
                self.freeze_command_sources(Some(command), &routing)
            };
            first = false;
            let was_floating = self.floating_scopes.clone();
            // Resolve the bookkeeping identity before execution: an
            // implicit command maps to its explicit form against the
            // current (pre-execution) subject slot.
            let bookkeeping = self.bookkeeping_command(command);
            match self.execute_command(
                issue,
                command_seq,
                command,
                &held_scopes,
                send_source,
                migration_source,
            ) {
                CommandApplied::Effect(effect) => {
                    self.resweep_retiled(
                        &issue.owner,
                        &issue.correlation,
                        &mut held_scopes,
                        &was_floating,
                    );
                    let result = self.command_result.take();
                    self.build_staged_domains();
                    self.settle_intent_admissions();
                    self.sync_toggle_float_command(&bookkeeping);
                    if index == last_index {
                        // Final command: its staged saves stay for the
                        // single final publish drain, preserving exact
                        // single-command semantics (including queued-match
                        // ordinary suppression).
                    } else {
                        self.accumulate_batch_command_save(&mut batch_saves, &bookkeeping, &result);
                    }
                    if effect.kind == EffectKind::Focus {
                        // Latest explicit focus wins: drop every unissued
                        // prior Focus (admission or earlier batch).
                        effects.retain(|existing| existing.kind != EffectKind::Focus);
                    }
                    effects.push(effect);
                    crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
                    settlements.push(BatchSettlement {
                        seq: command_seq,
                        result: result.clone(),
                    });
                    last_result = result;
                    last_bookkeeping = Some(bookkeeping);
                }
                CommandApplied::NoEffect => {
                    self.resweep_retiled(
                        &issue.owner,
                        &issue.correlation,
                        &mut held_scopes,
                        &was_floating,
                    );
                    self.build_staged_domains();
                    self.settle_intent_admissions();
                    self.sync_toggle_float_command(&bookkeeping);
                    let result = self.command_result.take();
                    if index != last_index {
                        self.accumulate_batch_command_save(&mut batch_saves, &bookkeeping, &result);
                    }
                    crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
                    settlements.push(BatchSettlement {
                        seq: command_seq,
                        result: result.clone(),
                    });
                    last_result = result;
                    last_bookkeeping = Some(bookkeeping);
                }
                CommandApplied::Rejected { kind, message } => {
                    // Creation exhaustion is a protocol envelope failure:
                    // it discards the staged candidate with no adoption,
                    // exactly like every pre-acceptance gate. The empty
                    // settlements leave the whole batch parked.
                    if kind == KIND_CREATION_EXHAUSTED {
                        self.batch_base_focus.clear();
                        self.batch_command_focus.clear();
                        return (V4RunOutcome::Rejected { kind, message }, Vec::new());
                    }
                    *self = precommand;
                    crate::world_engine::emit_resize_refusal(self.command_refusal_diagnostic_v4(
                        report,
                        &bookkeeping,
                        command_seq,
                        kind,
                    ));
                    if let Some(line) = self.resize_refusal_diagnostic_v4_for_command(
                        report,
                        &bookkeeping,
                        command_seq,
                        kind,
                    ) {
                        crate::world_engine::emit_resize_refusal(line);
                    }
                    self.note_command(command_seq);
                    let typed = CommandRejectKind::parse(kind)
                        .unwrap_or(CommandRejectKind::OperationRefused);
                    let result = Some(CommandResult::Rejected { kind: typed });
                    crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
                    settlements.push(BatchSettlement {
                        seq: command_seq,
                        result: result.clone(),
                    });
                    last_result = result;
                    last_bookkeeping = Some(bookkeeping);
                }
            }
        }
        // Single final intent merge: earlier batch saves win over the
        // pre-existing refusal rollback map latest-wins per window. The
        // final command's own staged saves stay staged, so the publish
        // drain below emits everything exactly once with one knowledge
        // update. Rejected commands restored their pre-command image and
        // accumulated nothing; consumed skips never executed.
        for (win, write) in batch_saves {
            self.sticky_rollback_saves.insert(win, write);
        }
        // The final publish drains with the last executed command's
        // carried identity (resolved pre-execution subject); commands
        // that never executed fall back to the reported spelling.
        let published_command: Option<WorldCommand> = last_bookkeeping
            .clone()
            .or_else(|| commands.last().map(|(_, command, _)| command.clone()));
        // Winner pass before publication: the final focus decision
        // supersedes the retained slot here, while the carriers stay
        // internal for the selection freeze inside publication.
        let effects = self.drain_focus_candidates(effects, &held_scopes, report, issuing_revision);
        let reply = self.publish_v4(
            PublishEnvelopeV4 {
                adapter_incarnation: report.adapter_incarnation.clone(),
                request_id: report.request_id.clone(),
                observation_seq: report.observation_seq,
                command_consumed: false,
                issuing_revision,
            },
            published_command.as_ref(),
            effects,
            last_result,
            &held_scopes,
        );
        (self.finish_v4(report, issuing_revision, reply), settlements)
    }

    /// Finish one v4 accepted run: lifetime retention, membership and
    /// pager convergence, then the genuine v4 publication image. Mirrors
    /// the legacy finish tail without touching legacy replay, request, or
    /// digest bookkeeping (the admission boundary owns those): no `last_report`/`last_reply`
    /// writes, no duplicate or bounds checks.
    fn finish_v4(
        &mut self,
        report: &WorldReportV4,
        issuing_revision: u64,
        mut reply: WorldReplyV4,
    ) -> V4RunOutcome {
        // Batch-only baseline ends with the run: clear so committed state
        // never carries an active baseline into the next capture or a
        // direct command seam (missing entry falls back to staged focus).
        self.batch_base_focus.clear();
        self.batch_command_focus.clear();
        self.prune_lifetime_state();
        self.note_focused_output();
        self.reconcile_members();
        let mut ringed: BTreeSet<DomainKey> = BTreeSet::new();
        for (output, ring) in &self.mapping {
            for id in ring {
                ringed.insert(DomainKey {
                    output: OutputId(output.clone()),
                    workspace: WorkspaceId(id.clone()),
                });
            }
        }
        self.members.retain(|_, state| {
            ringed.contains(&state.home) || self.displaced.contains_key(&state.home.output.0)
        });
        self.latch_floating_admissions();
        // Pager retention settles against converged membership, then the
        // reply carries the retained pager image.
        self.retain_pager_presentation_v4();
        reply.presentation = self.presentation_for_publish_v4();
        self.state_revision = issuing_revision;
        reply.state_revision = issuing_revision;
        reply.presentation.rev = issuing_revision;
        // The prior-accepted context captures from this accepted
        // candidate's staged raw focus plus the just-published common
        // rows. Every publish path funnels here, including
        // consumed-command reports; rejected candidates never reach this
        // boundary.
        self.prior_context = PriorAcceptedContext::capture(
            &self.staged.observation.focused_global,
            &reply.workspaces,
            &reply.scopes,
            &reply.windows,
        );
        if report.inventory_complete {
            self.placement_bootstrapped = true;
        }
        if self.host_owned_workspace_domains() {
            self.retain_native_last_views();
        }
        if reply.command_consumed {
            V4RunOutcome::CommandConsumed(reply)
        } else {
            V4RunOutcome::Accepted(reply)
        }
    }
}
