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

use crate::world_transaction::WorldTransaction;
use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::CoreCommand;
use crate::directional::{OutputId, WindowId, WindowSubject, WorkspaceId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::world_commands::CommandApplied;
use crate::world_engine::{
    EventIssue, KIND_CREATION_EXHAUSTED, KIND_REVISION_EXHAUSTED, MSG_REVISION_EXHAUSTED,
    PriorAcceptedContext, RawRouting, WorldEngine,
};
use crate::world_gesture::GestureOutcome;
use crate::world_publish_v4::PublishEnvelopeV4;
use crate::world_report::{
    BatchDisposition, CommandDisposition, CommandRejectKind, CommandResult, EffectKind, HoldReason,
    TilingMode, TransferKind, WorldCommand, WorldReplyV4, WorldReportV4,
};
use crate::world_report_v4_checks::graded_effective_v4;

fn is_implicit_float_without_frame(command: &WorldCommand) -> bool {
    matches!(
        command,
        WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: WindowSubject::CurrentFocus,
            float_rect: None,
        })
    )
}

/// A float toggle carrying no rectangle: core resolves the execution-time
/// placement instead of accepting an intent-time frame. Both implicit
/// (`CurrentFocus`) and explicit-window subjects qualify; an explicit
/// rectangle always means genuine placement.
fn is_float_without_frame(command: &WorldCommand) -> bool {
    matches!(
        command,
        WorldCommand::Operation(CoreCommand::ToggleFloat {
            float_rect: None,
            ..
        })
    )
}

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

impl WorldTransaction<'_> {
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
        let predecessor = (self.world.state_revision != 0).then_some(self.world.state_revision);
        let (outcome, _) = self.run_accepted_v4_batched(
            report,
            issue,
            consumed,
            issuing_revision,
            predecessor,
            &[],
            None,
        );
        outcome
    }

    /// Run one already-admitted v4 candidate plus a parked FIFO command
    /// batch drained inside the same capture/run/publication: native
    /// observations incorporate once, then every command executes in order
    /// with core subject state after the preceding action, and one full
    /// publication is emitted for the batch (command outcomes ride
    /// `Vec<BatchSettlement>`, not `WorldReplyV4`). The inline report command
    /// (when present) runs first; queued commands follow in FIFO order.
    /// Renderer validation applies to the inline command only: queued commands
    /// never carried render proof. Returns the single run outcome plus one
    /// per-command settlement aligned with the executed commands (inline
    /// `report.command` first if present, then `queued`). A
    /// creation-exhausted command discards the whole staged candidate with
    /// empty settlements so the caller reparks the batch.
    /// `predecessor` is the last fully incorporated reply's revision, absent
    /// at generation start. It is not the observation watermark.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_accepted_v4_batched(
        &mut self,
        report: &WorldReportV4,
        issue: &EventIssue,
        consumed: bool,
        issuing_revision: u64,
        predecessor: Option<u64>,
        queued: &[(u64, WorldCommand)],
        roster_feedback: Option<&crate::world_roster::RosterFeedback>,
    ) -> (V4RunOutcome, Vec<BatchDisposition>) {
        // Admitted-candidate precondition without implementing admission:
        // an exhausted counter refuses and re-bootstraps under a new
        // generation elsewhere; it never wraps in place.
        if self.world.state_revision == u64::MAX {
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
            self.world.state_revision + 1,
            "admitted issuing revision must be exactly the next live revision"
        );
        if !self.world.settlement.staged_dropped_operations.is_empty() {
            self.world.settlement.staged_dropped_operations.clear();
        }
        if !self.world.settlement.staged_dropped_switches.is_empty() {
            self.world.settlement.staged_dropped_switches.clear();
        }
        self.world.gesture.staged_drag_preview_reason = None;
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
            .connection
            .capabilities()
            .map(|capabilities| capabilities.unavailable_facts.as_slice())
            .unwrap_or(&[]);
        let native_sticky_supported = crate::world_state::state_property_supported(
            self.connection.capabilities(),
            crate::world_report::StateProperty::Sticky,
        );
        let maximize_supported = crate::world_state::state_property_supported(
            self.connection.capabilities(),
            crate::world_report::StateProperty::Maximized,
        );
        let graded = graded_effective_v4(
            report,
            unavailable,
            native_sticky_supported,
            maximize_supported,
        );
        let effective = graded.as_ref().unwrap_or(report);
        // A commandless native report carrying a focus interval is the cut
        // paired with queued work. A declared focus gap does not erase its
        // readable per-window facts. Freeze only pre-existing floats, before
        // this observation can admit windows or settle an in-flight float.
        // An outstanding placement can still be showing the old tile frame.
        let mut cut_float_frames: BTreeMap<&WindowId, Rect> = BTreeMap::new();
        if report.command.is_none()
            && report.focus_changes.is_some()
            && queued
                .iter()
                .any(|(_, command)| is_implicit_float_without_frame(command))
        {
            cut_float_frames.extend(effective.windows.iter().filter_map(|fact| {
                let rect = fact.rect?;
                let window = &fact.window;
                (fact.fullscreen == Some(false)
                    && fact.maximized != Some(true)
                    && self.is_ordinary_float(window)
                    && !self
                        .world
                        .settlement
                        .retained_floats
                        .iter()
                        .any(|pending| &pending.window == window))
                .then_some((window, rect))
            }));
        }
        // A first float has no remembered placement to restore, so a
        // rectangle-less toggle adopts the tiled subject's current observed
        // frame in place instead of the centered fallback. Only a truthful
        // non-overlay frame of a subject core already holds tiled qualifies:
        // a fullscreen, native- or core-composed-maximized, newly admitted,
        // re-float with remembered geometry, or pending-placement subject
        // keeps that geometry / the centered fallback because its observed
        // frame is not a float base.
        let mut first_float_frames: BTreeMap<&WindowId, Rect> = BTreeMap::new();
        if report
            .command
            .as_ref()
            .into_iter()
            .chain(queued.iter().map(|(_, command)| command))
            .any(is_float_without_frame)
        {
            first_float_frames.extend(effective.windows.iter().filter_map(|fact| {
                let rect = fact.rect?;
                let window = &fact.window;
                (fact.fullscreen == Some(false)
                    && fact.maximized != Some(true)
                    && self.world.layout.is_tiled_window_any(window)
                    && !self.world.runtime.composed_maximized.contains(window)
                    && self
                        .world
                        .layout
                        .retained_float_geometry_any(window)
                        .is_none()
                    && !self
                        .world
                        .settlement
                        .retained_floats
                        .iter()
                        .any(|pending| &pending.window == window))
                .then_some((window, rect))
            }));
        }
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
            Some(
                WorldCommand::SendRelative {
                    window: crate::directional::WindowSubject::CurrentFocus,
                    source: crate::world_report::SendSource::HomeThenAction,
                    ..
                } | WorldCommand::SendNumbered {
                    window: crate::directional::WindowSubject::CurrentFocus,
                    source: crate::world_report::SendSource::HomeThenAction,
                    ..
                }
            )
        );
        let pre_send_homes: BTreeMap<WindowId, OutputId> = if first_implicit_home_send {
            self.world
                .workspace
                .members
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
        let observation = WorldEngine::capture_observation_v4(report);
        self.ingest_staged_intent(&observation);
        self.stage_captured_v4(effective, observation);
        self.world.staged.revision = issuing_revision;
        self.settle_shared_roster(roster_feedback);
        self.world.focus.prev_cursor = self.world.focus.last_cursor;
        self.world.focus.last_cursor = self.world.staged.observation.cursor;
        self.world
            .layout
            .set_fresh_cursor(self.world.staged.observation.cursor);
        // Freeze the positive-close survivor before convergence ingests the
        // host fallback: the retained request records the pre-close MRU
        // winner while later observations stay truthful.
        self.freeze_close_focus(report, issuing_revision, predecessor);
        self.converge_logical_pre();
        self.settle_views_pre();
        self.build_staged_domains();
        self.gesture_bind(&BTreeSet::new());
        self.settle_rest(report.command.as_ref(), issue, consumed);
        self.build_staged_domains();
        self.settle_property_attempts(crate::world_attempts::PropertyAttemptPhase::States);
        self.settle_selection_feedback();
        self.settle_property_attempts(crate::world_attempts::PropertyAttemptPhase::Visibility);
        self.settle_property_attempts(crate::world_attempts::PropertyAttemptPhase::Frames);
        self.world.floating_scopes = self
            .world
            .staged
            .domains
            .iter()
            .map(|domain| domain.key.clone())
            .filter(|key| {
                self.world
                    .workspace
                    .tiling
                    .get(key)
                    .is_some_and(|mode| *mode == TilingMode::Floating)
            })
            .collect();
        self.converge_logical_post();
        self.retain_live_topology();
        let mut known_before: BTreeMap<DomainKey, BTreeSet<String>> = BTreeMap::new();
        for domain in &self.world.staged.domains {
            if !domain.readable {
                continue;
            }
            if self.world.floating_scopes.contains(&domain.key) {
                continue;
            }
            if !self.world.layout.contains_domain(&domain.key) {
                continue;
            }
            let mut known = BTreeSet::new();
            for window in self.world.layout.tiled_windows(&domain.key) {
                known.insert(window.0.clone());
            }
            for window in self.world.layout.exception_windows(&domain.key) {
                known.insert(window.0.clone());
            }
            for window in self.world.layout.minimized_windows(&domain.key) {
                known.insert(window.0.clone());
            }
            known_before.insert(domain.key.clone(), known);
        }
        let mut slotted_before: BTreeSet<WindowId> = BTreeSet::new();
        for domain in &self.world.staged.domains {
            for window in self.world.layout.tiled_windows(&domain.key) {
                slotted_before.insert(window.clone());
            }
        }
        self.world.hold_reasons.clear();
        let mut held_scopes: BTreeSet<DomainKey> = BTreeSet::new();
        if self.connection.host_owned_workspace_domains() {
            let mut fence: Vec<DomainKey> = Vec::new();
            for (window, fact) in &self.world.staged.facts {
                let authoritative = match fact.output.as_ref() {
                    Some(_) => self.native_authoritative(),
                    None => self.connection.host_owned_workspace_domains(),
                };
                if !authoritative {
                    continue;
                }
                if self.effective_logical_receipt(fact).is_some() {
                    continue;
                }
                if let Some(key) = self.world.staged.homing.get(window) {
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
                for domain in &self.world.staged.domains {
                    if !fence.contains(&domain.key) {
                        fence.push(domain.key.clone());
                    }
                }
            }
            for key in fence {
                self.world
                    .hold_reasons
                    .insert(key.clone(), HoldReason::Unreadable);
                held_scopes.insert(key);
            }
        }
        let mut held_window_scopes: Vec<DomainKey> = self
            .world
            .staged
            .held_windows
            .iter()
            .filter_map(|held| self.core_homing(&held.window))
            .collect();
        let mut held_window_set: Option<BTreeSet<&WindowId>> = None;
        for held in &self.world.staged.held_attempts {
            if held.lane == crate::world_report::AttemptLaneV4::Visibility
                && let Some(win_str) = held.identity.first()
            {
                let win = WindowId(win_str.clone());
                let present = self.world.staged.facts.contains_key(&win)
                    || (!self.world.staged.held_windows.is_empty()
                        && held_window_set
                            .get_or_insert_with(|| {
                                self.world
                                    .staged
                                    .held_windows
                                    .iter()
                                    .map(|row| &row.window)
                                    .collect()
                            })
                            .contains(&win));
                if present
                    && let Some(key) = self
                        .core_homing(&win)
                        .or_else(|| self.world.staged.homing.get(&win).cloned())
                {
                    held_window_scopes.push(key);
                }
            }
        }
        for key in &held_window_scopes {
            self.world
                .hold_reasons
                .entry(key.clone())
                .or_insert(HoldReason::Unreadable);
            held_scopes.insert(key.clone());
        }
        self.capture_excluded_intent();
        self.run_native_sticky_followups(&held_scopes);
        let domains = self.world.staged.domains.clone();
        for domain in &domains {
            if held_window_scopes.contains(&domain.key) {
                continue;
            }
            if !domain.readable {
                continue;
            }
            if self.world.floating_scopes.contains(&domain.key) {
                continue;
            }
            if !self.world.staged.observation.inventory_complete
                && self.world.settlement.retained.iter().any(|r| {
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
                self.world.hold_reasons.insert(
                    domain.key.clone(),
                    crate::world_report::HoldReason::IncompleteInventory,
                );
                held_scopes.insert(domain.key.clone());
                continue;
            }
            self.sweep_domain(domain, &mut held_scopes);
        }
        self.settle_intent_admissions();
        {
            let observed = &report.focused_global;
            if self.world.prior_context.prev_focused_global != *observed && !observed.is_empty() {
                let _ = self.record_canonical(&WindowId(observed.clone()), &held_scopes, false);
            }
            // Canonical history follows native input independently of the
            // execution subject. It cannot cancel an adjudicated request or
            // leave an abandoned logical target in the FIFO cursor.
            self.seed_focus_cursor();
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
        for (key, window) in std::mem::take(&mut self.world.settlement.pending_forget) {
            self.world.layout.forget_minimized_slot(&key, &window);
        }
        for (key, window) in self.confirmed_minimized_rehomes() {
            self.world.layout.forget_minimized_slot(&key, &window);
        }
        self.world.command_result = None;
        match self.gesture_act(issue, &mut held_scopes) {
            GestureOutcome::Absent | GestureOutcome::Held => {}
            GestureOutcome::Settled { committed, .. } => {
                if committed {
                    let was_floating = self.world.floating_scopes.clone();
                    self.resweep_retiled(&mut held_scopes, &was_floating);
                    self.build_staged_domains();
                    self.settle_intent_admissions();
                }
            }
        }
        let mut effects = self.take_admission_focus(&known_before, &held_scopes);
        // Same-capture admission enters the execution-time ordering before
        // commands: the accepted newcomer advances the logical cursor
        // immediately, so a later implicit subject resolves from core focus,
        // not stale native foreground.
        for effect in &effects {
            if effect.kind == EffectKind::Focus
                && !self.record_canonical(&effect.target, &held_scopes, false)
                && self.world.focus.logical_focus_cursor.as_ref() != Some(&effect.target)
            {
                self.world.focus.logical_focus_cursor = None;
            }
        }
        if effects
            .iter()
            .any(|effect| effect.kind == EffectKind::Focus)
        {
            self.world.staged.batch_focus_issued = true;
        }
        struct OrderedBatchEntry<'a> {
            seq: u64,
            command: &'a WorldCommand,
            renderer_input: Option<&'a crate::world_report::RendererInput>,
            already_consumed: bool,
        }
        // Admission focus effects above are history, never promises: the
        // first explicit batch Focus supersedes every unissued prior Focus
        // (admission or earlier batch), so only the latest batch Focus
        // publishes. Focus is the only effect kind. Trailing non-focus
        // commands never clear the surviving focus.
        if report.command.is_none() && queued.is_empty() {
            let _ = self.world.command_result.take();
            crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
            effects = self.drain_focus_candidates(effects, &held_scopes, issuing_revision);
            self.plan_shared_roster(&held_scopes);
            if self.connection.shared_roster_slots() {
                self.build_staged_domains();
            }
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
                &held_scopes,
            );
            return (self.finish_v4(report, issuing_revision, reply), Vec::new());
        }
        // Batch command list: the inline report command (when present)
        // runs first, then every parked command in FIFO order. The inline
        // entry carries its renderer proof; queued entries carry None. Stale
        // inline (`consumed`) records `AlreadyConsumed` and does not suppress
        // fresh queued work.
        let inline_entry = report.command.as_ref().map(|command| OrderedBatchEntry {
            seq: report.command_seq.expect("admitted v4 pair"),
            command,
            renderer_input: report.renderer_input.as_ref(),
            already_consumed: consumed,
        });
        let commands = inline_entry
            .into_iter()
            .chain(queued.iter().map(|(seq, command)| OrderedBatchEntry {
                seq: *seq,
                command,
                renderer_input: None,
                already_consumed: false,
            }));
        let mut dispositions: Vec<BatchDisposition> = Vec::new();
        // Baseline for in-batch focus resolution: post-observation session
        // state, before the first command runs. Observations applied once
        // above; every command below owns post-previous-command state.
        self.snapshot_batch_focus_baseline();
        cut_float_frames.retain(|window, _| self.is_ordinary_float(window));
        // A pending placement that settled during this observation owns the
        // frame; its pre-settlement tile echo must never become the float
        // base.
        first_float_frames.retain(|window, _| {
            !self
                .world
                .settlement
                .retained_floats
                .iter()
                .any(|pending| &pending.window == *window)
        });
        // Retained intent updates in execution order: sticky mutations
        // already updated the single owner during execution, and each
        // successful command below upserts its ordinary ToggleFloat save
        // (latest-wins per window). Refusals restore the pre-command image
        // including its retained targets, so earlier batch effects stay
        // published while the refused command's partial mutation never
        // survives. Consumed skips never executed.
        // Post-command bookkeeping identity per executed command: the
        // resolved execution subject carried from pre-execution state,
        // never focus after the action. The final publish drains with the
        // last executed command's identity.
        let mut last_bookkeeping = None;
        let mut first = true;
        let roster_in_flight = self.shared_slot_transition_unproved();
        for entry in commands {
            let command_seq = entry.seq;
            let command = entry.command;
            // Already-consumed sequences reconcile without re-executing:
            // no effect, no result, no high-watermark motion.
            if entry.already_consumed
                || self
                    .world
                    .command_highwater
                    .is_some_and(|mark| command_seq <= mark)
            {
                dispositions.push(BatchDisposition {
                    seq: command_seq,
                    disposition: CommandDisposition::AlreadyConsumed,
                });
                first = false;
                continue;
            }
            if roster_in_flight && crate::world_roster::changes_roster(command) {
                self.note_command(command_seq);
                dispositions.push(BatchDisposition {
                    seq: command_seq,
                    disposition: CommandDisposition::Refused(CommandRejectKind::TransferInFlight),
                });
                first = false;
                continue;
            }
            let latest_published_presentation_rev = self.world.presentation.latest_published_rev_v4;
            if let Some(reject) = self
                .validate_renderer_input_v4(entry.renderer_input, latest_published_presentation_rev)
            {
                crate::world_engine::emit_resize_refusal(self.command_refusal_diagnostic_v4(
                    report,
                    command,
                    command_seq,
                    reject.as_str(),
                ));
                self.note_command(command_seq);
                crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
                if effects
                    .iter()
                    .any(|effect| effect.kind == EffectKind::Focus)
                {
                    self.world.staged.batch_focus_issued = true;
                }
                dispositions.push(BatchDisposition {
                    seq: command_seq,
                    disposition: CommandDisposition::Refused(reject),
                });
                first = false;
                continue;
            }
            // Observation-only pre-command image per command: a refusal
            // restores exactly the post-previous-command image, so earlier
            // batch effects stay published while this command's partial
            // mutation never survives.
            let precommand = self.world.clone();
            // The first batch command reuses the contracted pre-observation
            // freeze above; every later subject re-freezes from staged
            // post-previous-command state.
            let (send_source, migration_source) = if first {
                (frozen_send_source.clone(), frozen_migration_source.clone())
            } else {
                let routing = RawRouting::from_staged(&self.world.staged);
                self.freeze_command_sources(Some(command), &routing)
            };
            first = false;
            let was_floating = self.world.floating_scopes.clone();
            // Resolve the execution-time subject once per command before
            // execution: both bookkeeping and dispatch share the single
            // resolved command (explicit commands borrow without cloning).
            let (resolved_focus, mut dispatched) = self.resolve_command_subject(command);
            // Execution-time float placement: a rectangle-less toggle adopts
            // the subject's current observed frame (implicit subjects are
            // already resolved to their explicit window, so one lookup covers
            // both shapes). A first float takes the tiled frame in place; a
            // pre-existing float keeps the sealed-cut lane's frame. Explicit
            // rectangles never reach this branch.
            let current_float_frame = match dispatched.as_ref() {
                WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: WindowSubject::Window(window),
                    float_rect: None,
                }) => first_float_frames
                    .get(window)
                    .or_else(|| cut_float_frames.get(window))
                    .copied(),
                _ => None,
            };
            if let Some(rect) = current_float_frame
                && let WorldCommand::Operation(CoreCommand::ToggleFloat { float_rect, .. }) =
                    dispatched.to_mut()
            {
                *float_rect = Some(rect);
            }
            // Any float toggle retires this report's captured frame for its
            // subject: a placement or unfloat changes the window's membership
            // (and possibly its native frame), so a later command in the same
            // batch must not reuse the pre-command frame even if it re-floats.
            let toggled_float_subject = match dispatched.as_ref() {
                WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: WindowSubject::Window(window),
                    ..
                }) => Some(window.clone()),
                _ => None,
            };
            let applied = self.execute_resolved_command(
                issue,
                command_seq,
                dispatched.as_ref(),
                resolved_focus,
                command.is_current_focus(),
                &held_scopes,
                send_source,
                migration_source,
            );
            // Directional actions may resolve to local layout work or to a
            // native transfer. Only the latter conflicts with a frozen plan.
            if roster_in_flight && self.world.settlement.retained != precommand.settlement.retained
            {
                *self.world = precommand;
                self.note_command(command_seq);
                dispositions.push(BatchDisposition {
                    seq: command_seq,
                    disposition: CommandDisposition::Refused(CommandRejectKind::TransferInFlight),
                });
                continue;
            }
            match applied {
                CommandApplied::Effect(effect) => {
                    self.resweep_retiled(&mut held_scopes, &was_floating);
                    let result = self.world.command_result.take();
                    self.build_staged_domains();
                    self.settle_intent_admissions();
                    self.sync_toggle_float_command(dispatched.as_ref());
                    self.finish_command_success_tail_v4(
                        dispatched.as_ref(),
                        command_seq,
                        result,
                        Some(effect),
                        &held_scopes,
                        &mut effects,
                        &mut dispositions,
                    );
                    last_bookkeeping = Some(dispatched);
                }
                CommandApplied::NoEffect => {
                    self.resweep_retiled(&mut held_scopes, &was_floating);
                    self.build_staged_domains();
                    self.settle_intent_admissions();
                    self.sync_toggle_float_command(dispatched.as_ref());
                    let result = self.world.command_result.take();
                    self.finish_command_success_tail_v4(
                        dispatched.as_ref(),
                        command_seq,
                        result,
                        None,
                        &held_scopes,
                        &mut effects,
                        &mut dispositions,
                    );
                    last_bookkeeping = Some(dispatched);
                }
                CommandApplied::Rejected { kind, message } => {
                    // Creation exhaustion is a protocol envelope failure:
                    // it discards the staged candidate with no adoption,
                    // exactly like every pre-acceptance gate. The empty
                    // dispositions let the admission boundary return NotRun.
                    if kind == KIND_CREATION_EXHAUSTED {
                        self.world.focus.batch_base_focus.clear();
                        self.world.focus.batch_command_focus.clear();
                        return (V4RunOutcome::Rejected { kind, message }, Vec::new());
                    }
                    *self.world = precommand;
                    crate::world_engine::emit_resize_refusal(self.command_refusal_diagnostic_v4(
                        report,
                        dispatched.as_ref(),
                        command_seq,
                        kind,
                    ));
                    if let Some(line) = self.resize_refusal_diagnostic_v4_for_command(
                        report,
                        dispatched.as_ref(),
                        command_seq,
                        kind,
                    ) {
                        crate::world_engine::emit_resize_refusal(line);
                    }
                    self.note_command(command_seq);
                    let typed = CommandRejectKind::parse(kind)
                        .unwrap_or(CommandRejectKind::OperationRefused);
                    crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
                    if effects
                        .iter()
                        .any(|effect| effect.kind == EffectKind::Focus)
                    {
                        self.world.staged.batch_focus_issued = true;
                    }
                    dispositions.push(BatchDisposition {
                        seq: command_seq,
                        disposition: CommandDisposition::Refused(typed),
                    });
                    last_bookkeeping = Some(dispatched);
                }
            }
            // Once a command changes a donor's state, this report's frame
            // cannot supply it again, even if a later command re-floats it.
            if let Some(window) = toggled_float_subject {
                cut_float_frames.remove(&window);
                first_float_frames.remove(&window);
            }
        }
        if dispositions
            .iter()
            .all(|d| matches!(d.disposition, CommandDisposition::AlreadyConsumed))
        {
            let _ = self.world.command_result.take();
        }
        // The final publish projects the retained map (never drains it):
        // rejected commands restored their pre-command image and
        // accumulated nothing; consumed skips never executed.
        // The final publish drains with the last executed command's
        // carried identity (resolved pre-execution subject); commands
        // that never executed fall back to the reported spelling.
        let published_command = last_bookkeeping.as_deref().or_else(|| {
            queued
                .last()
                .map(|(_, command)| command)
                .or(report.command.as_ref())
        });
        // Winner pass before publication: the final focus decision
        // supersedes the retained slot here, while the carriers stay
        // internal for the selection freeze inside publication.
        let effects = self.drain_focus_candidates(effects, &held_scopes, issuing_revision);
        self.plan_shared_roster(&held_scopes);
        if self.connection.shared_roster_slots() {
            self.build_staged_domains();
        }
        let reply = self.publish_v4(
            PublishEnvelopeV4 {
                adapter_incarnation: report.adapter_incarnation.clone(),
                request_id: report.request_id.clone(),
                observation_seq: report.observation_seq,
                command_consumed: consumed,
                issuing_revision,
            },
            published_command,
            effects,
            &held_scopes,
        );
        (
            self.finish_v4(report, issuing_revision, reply),
            dispositions,
        )
    }

    fn is_ordinary_float(&self, window: &WindowId) -> bool {
        // Composed maximize can use the same layout exception but owns a
        // distinct unfloat transition.
        !self.world.runtime.composed_maximized.contains(window)
            && self
                .world
                .workspace
                .members
                .get(window)
                .is_some_and(|member| self.world.layout.is_ordinary_float(&member.home, window))
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_command_success_tail_v4(
        &mut self,
        bookkeeping: &WorldCommand,
        command_seq: u64,
        result: Option<CommandResult>,
        effect: Option<crate::world_report::OneShotEffect>,
        held_scopes: &BTreeSet<DomainKey>,
        effects: &mut Vec<crate::world_report::OneShotEffect>,
        dispositions: &mut Vec<BatchDisposition>,
    ) {
        self.upsert_ordinary_toggle_float_save(bookkeeping, &result);
        self.restage_classification();
        self.build_staged_domains();
        if let Some(effect) = effect {
            if effect.kind == EffectKind::Focus {
                // Latest explicit focus wins: drop every unissued
                // prior Focus (admission or earlier batch). Canonical
                // command-focus failures clear `logical_focus_cursor`
                // inside `record_command_focus`, so non-recording
                // focus carriers (send/move) never wipe a valid prior
                // cursor.
                effects.retain(|existing| existing.kind != EffectKind::Focus);
            }
            effects.push(effect);
        }
        crate::world_cursor::maybe_push_focus_follows(self, held_scopes, effects);
        if effects
            .iter()
            .any(|effect| effect.kind == EffectKind::Focus)
        {
            self.world.staged.batch_focus_issued = true;
        }
        dispositions.push(BatchDisposition {
            seq: command_seq,
            disposition: CommandDisposition::Executed(result),
        });
    }

    /// Finish one v4 accepted run: lifetime retention, membership and
    /// pager convergence, then the genuine v4 publication image. Mirrors
    /// the legacy finish tail without touching legacy replay, request, or
    /// digest bookkeeping (the admission boundary owns those): no `last_report`/`last_reply`
    /// writes, no duplicate or bounds checks.
    pub(crate) fn finish_v4(
        &mut self,
        report: &WorldReportV4,
        issuing_revision: u64,
        mut reply: WorldReplyV4,
    ) -> V4RunOutcome {
        // Batch-only baseline ends with the run: clear so committed state
        // never carries an active baseline into the next capture or a
        // direct command seam (missing entry falls back to staged focus).
        self.world.focus.batch_base_focus.clear();
        self.world.focus.batch_command_focus.clear();
        self.world.staged.batch_focus_issued = false;
        self.prune_lifetime_state();
        self.note_focused_output();
        self.reconcile_members();
        let mut ringed: BTreeSet<DomainKey> = BTreeSet::new();
        for (output, ring) in &self.world.workspace.mapping {
            for id in ring {
                ringed.insert(DomainKey {
                    output: OutputId(output.clone()),
                    workspace: WorkspaceId(id.clone()),
                });
            }
        }
        // Composed-sticky membership homes are all-desktop anchors: a
        // swap/migration can move the logic workspace id off the anchor
        // output while the physical window stays put, so the recorded home
        // is legitimately off-ring. Keep such anchors while their anchor
        // output is still live; a sticky home on a vanished output with no
        // displacement record still drops (core homing would lose it
        // anyway). Sticky truth comes from the staged derived lane, never
        // the raw native fact: composed hosts report a null fact while
        // core's retained intent still proves the lifetime.
        let sticky: BTreeSet<WindowId> = self
            .world
            .workspace
            .members
            .keys()
            .filter(|window| {
                self.world
                    .staged
                    .derived
                    .get(*window)
                    .is_some_and(|derived| derived.sticky)
            })
            .cloned()
            .collect();
        self.world.workspace.members.retain(|window, state| {
            ringed.contains(&state.home)
                || self
                    .world
                    .workspace
                    .displaced
                    .contains_key(&state.home.output.0)
                || (sticky.contains(window)
                    && self
                        .world
                        .workspace
                        .mapping
                        .contains_key(&state.home.output.0))
        });
        self.latch_floating_admissions();
        // Finalized membership and floating admissions can drop windows or
        // outputs that were still in pre-reconcile `self.world.workspace.members` during
        // `publish_v4`; re-prune and re-project the settled outcome slices
        // against finalized lifetime state before returning the reply.
        self.prune_settled_outcomes();
        reply.operation_settled = self
            .world
            .settlement
            .operation_settled
            .values()
            .cloned()
            .collect();
        reply.switch_settled = self
            .world
            .settlement
            .switch_settled
            .values()
            .cloned()
            .collect();
        // Pager retention settles against converged membership, then the
        // reply carries the retained pager image.
        self.retain_pager_presentation_v4();
        reply.presentation = self.presentation_for_publish_v4();
        self.world.state_revision = issuing_revision;
        reply.state_revision = issuing_revision;
        reply.presentation.rev = issuing_revision;
        self.world.presentation.latest_published_rev_v4 = issuing_revision;
        // The prior-accepted context captures from this accepted
        // candidate's staged raw focus plus the just-published common
        // rows. Every publish path funnels here, including
        // consumed-command reports; rejected candidates never reach this
        // boundary.
        self.world.prior_context = PriorAcceptedContext::capture(
            &self.world.staged.observation.focused_global,
            &reply.workspaces,
            &reply.scopes,
            &reply.windows,
        );
        if report.inventory_complete {
            self.world.placement_bootstrapped = true;
        }
        if self.connection.host_owned_workspace_domains() {
            self.retain_native_last_views();
        }
        if reply.command_consumed {
            V4RunOutcome::CommandConsumed(reply)
        } else {
            V4RunOutcome::Accepted(reply)
        }
    }
}
