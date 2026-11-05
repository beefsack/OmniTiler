//! Parallel core-internal v4 accepted-run/finish and command dispatch
//! (unit 6 sub-increment 7d).
//!
//! [`WorldEngine::run_accepted_v4`] is the typed v4 counterpart of the
//! legacy [`WorldEngine::run_accepted`](crate::world_engine::WorldEngine::run_accepted):
//! an internal accepted run over an already admitted v4 candidate. It
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
    CommandRejectKind, CommandResult, HoldReason, TilingMode, TransferKind, WorldReplyV4,
    WorldReportV4,
};
use crate::world_report_v4_checks::graded_effective_v4;

/// Internal outcome of one v4 accepted run: the genuine v4 reply on
/// observation/command progression, or the envelope refusal kind when
/// creation exhaustion discards the staged candidate (legacy parity: the
/// candidate is dropped with no adoption, never a command result).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum V4RunOutcome {
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
    pub(crate) fn run_accepted_v4(
        &mut self,
        report: &WorldReportV4,
        issue: &EventIssue,
        consumed: bool,
        issuing_revision: u64,
    ) -> V4RunOutcome {
        // Admitted-candidate precondition without implementing admission:
        // an exhausted counter refuses and re-bootstraps under a new
        // generation elsewhere; it never wraps in place.
        if self.state_revision == u64::MAX {
            return V4RunOutcome::Rejected {
                kind: KIND_REVISION_EXHAUSTED,
                message: MSG_REVISION_EXHAUSTED,
            };
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
        let (frozen_send_source, frozen_migration_source) =
            self.freeze_command_sources(report.command.as_ref(), &source_routing);
        // Exactly-once initial ingestion: capture the original raw facts,
        // ingest intent from that capture, then stage the invalid- and
        // capability-graded effective rows from the same capture, so the
        // classifier sees pending first-admission wins this same report
        // with no second ingestion. Later command restages reuse the
        // classification-only `restage_classification` and never reingest.
        let observation = Self::capture_observation_v4(report);
        self.ingest_staged_intent(&observation);
        self.stage_captured_v4(effective, observation);
        self.converge_logical_pre();
        self.settle_views_pre();
        self.build_staged_domains();
        self.gesture_bind(&BTreeSet::new());
        self.settle_rest(report.command.as_ref(), issue, consumed);
        self.build_staged_domains();
        self.settle_states();
        self.settle_focus_feedback();
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
                    fence.push(key.clone());
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
                let active = WindowId(current_focused);
                let _ = self.record_canonical(&active, &held_scopes, false);
            }
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
        if consumed {
            // Consumed sequences reconcile only, never re-execute or
            // re-validate: the result never re-emits, while a gesture
            // result stays the gesture's own.
            crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
            let result = self.command_result.take();
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
            return self.finish_v4(report, issuing_revision, reply);
        }
        if let Some(command) = report.command.as_ref() {
            let command_seq = report.command_seq.expect("admitted v4 pair");
            let was_floating = self.floating_scopes.clone();
            // Observation-only pre-command image: settlement, sweep,
            // gesture, admission focus, and intent effects above are
            // trusted observations. A command refusal restores exactly
            // this image so no partial command mutation survives, while
            // pre-command observation effects stay published. Renderer
            // validation reads the latest previously published revision,
            // never the pending one the publication below will stamp.
            let precommand = self.clone();
            if let Some(reject) = self.validate_renderer_input_v4(
                report.renderer_input.as_ref(),
                precommand.latest_published_presentation_rev_v4,
            ) {
                *self = precommand;
                self.note_command(command_seq);
                self.command_result = Some(CommandResult::Rejected { kind: reject });
                crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
                let result = self.command_result.take();
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
                return self.finish_v4(report, issuing_revision, reply);
            }
            match self.execute_command(
                issue,
                command_seq,
                command,
                &held_scopes,
                frozen_send_source.clone(),
                frozen_migration_source.clone(),
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
                    self.sync_toggle_float_command(command);
                    if effect.kind == crate::world_report::EffectKind::Focus {
                        effects.retain(|existing| {
                            !(existing.kind == crate::world_report::EffectKind::Focus
                                && existing.target == effect.target)
                        });
                    }
                    effects.push(effect);
                    crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
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
                    return self.finish_v4(report, issuing_revision, reply);
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
                    self.sync_toggle_float_command(command);
                    let result = self.command_result.take();
                    crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
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
                    return self.finish_v4(report, issuing_revision, reply);
                }
                CommandApplied::Rejected { kind, message } => {
                    // Creation exhaustion is a protocol envelope failure:
                    // it discards the staged candidate with no adoption,
                    // exactly like every pre-acceptance gate.
                    if kind == KIND_CREATION_EXHAUSTED {
                        return V4RunOutcome::Rejected { kind, message };
                    }
                    *self = precommand;
                    self.note_command(command_seq);
                    let typed = CommandRejectKind::parse(kind)
                        .unwrap_or(CommandRejectKind::OperationRefused);
                    self.command_result = Some(CommandResult::Rejected { kind: typed });
                    crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
                    let result = self.command_result.take();
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
                    return self.finish_v4(report, issuing_revision, reply);
                }
            }
        }
        let result = self.command_result.take();
        crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
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
        self.finish_v4(report, issuing_revision, reply)
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
