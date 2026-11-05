//! Parallel core-internal v4 admission boundary: bootstrap, typed
//! reply/replay bookkeeping, and bounded candidate adoption (unit 6
//! sub-increment 7e).
//!
//! [`WorldEngine::handle_report_v4`] is the typed v4 counterpart of the
//! admission half of legacy [`WorldEngine::handle_report`](crate::world_engine::WorldEngine::handle_report):
//! an internal boundary over an already admitted connection/correlation
//! context. The caller supplies the [`EventIssue`](crate::world_engine::EventIssue)
//! and performs no public envelope admission here: no owner, generation,
//! or correlation parsing. Every cheap gate runs before any mutation:
//! conflicting-window grading, request-size estimate, the structural v4
//! validator, typed request replay (digest pre-check, full typed equality
//! decides), observation ordering, the conservative reply upper bound with
//! the retained-publication, rollback, and selection-terminal reserves,
//! and revision exhaustion. Accepted inputs then run to completion on a
//! staged clone via [`WorldEngine::run_accepted_v4`](crate::world_run_v4::WorldEngine::run_accepted_v4);
//! the staged reply passes the outbound intent-lane gate and the exact
//! canonical size gate before the candidate commits. A staged overflow or
//! late failure drops the candidate; `self` is adopted only after the
//! staged reply encodes within budget. Only reports passing every cheap
//! gate pay for the clone.
//!
//! The typed v4 replay cache stays separate from the legacy
//! `last_report`/`last_reply` cache and legacy request bookkeeping, while
//! the command high-watermark, state revision, and lifetime/session
//! machinery stay shared. Pre-ready reports stage intent retention on a
//! candidate and publish an empty bootstrap reply: no authority, no
//! revision advance, no command consumption.

use std::collections::BTreeSet;

use crate::ids::CorrelationId;
use crate::world_engine::{
    EventIssue, KIND_OBS_STALE, KIND_OVERSIZED, KIND_REPLY_SIZE, KIND_REUSED,
    KIND_REVISION_EXHAUSTED, KIND_SNAPSHOT, KIND_SUPERSEDED, MSG_OBS_STALE, MSG_OVERSIZED,
    MSG_REPLY_SIZE, MSG_REUSED, MSG_REVISION_EXHAUSTED, MSG_SUPERSEDED, WorldEngine,
};
use crate::world_presentation::{GroupHideReason, PresentationUnderlay};
use crate::world_report::{
    BorderPresentationV4, WORLD_CONTRACT_VERSION_V4, WORLD_MAX_REPLY_BYTES,
    WORLD_MAX_REQUEST_BYTES, WorldPresentationV4, WorldReplyV4, WorldReportV4,
    validate_intent_writes_reason,
};
use crate::world_report_v4_checks::{
    grade_conflicting_windows_v4, report_digest_v4, validate_report_v4_reason,
};
use crate::world_run_v4::V4RunOutcome;
use crate::world_sizing::max_intent_rollback_bound;
use crate::world_sizing_v4::{
    reply_estimate_v4, reply_exact_bytes_v4, reply_upper_bound_v4, report_estimate_v4,
};

/// Pure v4 replay verdict over request identity: the digest is only a fast
/// pre-check, so the caller passes full typed input equality alongside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum V4Replay {
    Fresh,
    Duplicate,
    Reused,
    Stale,
}

/// Classify one v4 request against the typed replay cache. Pure over the
/// request/observation counters: a matching digest with unequal input is a
/// reused sequence (never a duplicate), so a digest collision still
/// refuses instead of aliasing the cached reply.
#[must_use]
pub(crate) fn classify_replay_v4(
    incoming_seq: u64,
    digest: u64,
    has_last: bool,
    last_seq: u64,
    last_digest: u64,
    reports_equal: bool,
) -> V4Replay {
    if !has_last {
        return V4Replay::Fresh;
    }
    if incoming_seq < last_seq {
        return V4Replay::Stale;
    }
    if incoming_seq == last_seq {
        if digest != last_digest || !reports_equal {
            return V4Replay::Reused;
        }
        return V4Replay::Duplicate;
    }
    V4Replay::Fresh
}

/// Usable-first-observation rule for the v4 pre-ready boundary: twin of the
/// legacy rule over the v4 screen lane (complete inventory, at least one
/// screen, every screen complete with positive FULL and work-area
/// rectangles).
fn usable_initial_observation_v4(report: &WorldReportV4) -> bool {
    report.inventory_complete
        && !report.screens.is_empty()
        && report.screens.iter().all(|screen| {
            screen.complete
                && screen
                    .full_rect
                    .is_some_and(|rect| rect.w > 0 && rect.h > 0)
                && screen
                    .work_area
                    .is_some_and(|rect| rect.w > 0 && rect.h > 0)
        })
}

impl WorldEngine {
    /// Accept one already-admitted v4 report and publish one globally
    /// ordered v4 reply. Internal only: the caller owns admission and
    /// supplies the admitted [`EventIssue`]; this entry performs no public
    /// envelope owner/generation/version admission and never touches the
    /// public ingress, adapters, brokers, or services.
    ///
    /// Transactional: every cheap gate runs before any mutation. Refused
    /// reports leave the engine (including intent ledgers, revision, and
    /// command consumption) exactly unchanged. Ready reports run on a
    /// staged clone adopted only after the outbound intent-lane gate and
    /// the exact reply-size gate pass; pre-ready reports stage intent
    /// retention on a candidate and return an empty bootstrap reply with no
    /// authority and no revision advance.
    // allow(dead_code): pending later public cutover owns this entry;
    // tests drive it.
    #[allow(dead_code)]
    pub(crate) fn handle_report_v4(
        &mut self,
        report: &WorldReportV4,
        issue: &EventIssue,
    ) -> V4RunOutcome {
        // Core-owned grading for the typed path: unavailable-fact
        // contradictions become narrow holds like their protocol-decoded
        // twins before any estimate, validation, or mutation. No
        // invalid-screen/window twins: the v4 decoder already holds invalid
        // rows, and the typed-value path stays unit 7 work.
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
        let graded_conflicting = grade_conflicting_windows_v4(
            report,
            unavailable,
            native_sticky_supported,
            maximize_supported,
        );
        // The graded copy serves structural validation only. Digest,
        // equality, size, caching, and the accepted run all stay on the
        // original typed input: raw conflicting rows must never alias
        // equivalent pre-held input as duplicates.
        let effective = graded_conflicting.as_ref().map_or(report, |owned| owned);
        // Fail-closed capability gates, mirroring the legacy admission
        // order: native evidence rides only on host-owned hosts, and the
        // readiness-bracket arm matches the host kind. Checked before any
        // adoption, even when the location lanes are null.
        let native_evidence = !report.native.is_empty()
            || report
                .windows
                .iter()
                .any(|window| window.native_workspace.as_deref().is_some())
            || report.member_attempts.iter().any(|attempt| {
                matches!(
                    attempt.pre.as_ref(),
                    Some(crate::world_report::WorkspaceRef::Native { .. })
                ) || matches!(
                    attempt.post.as_ref(),
                    Some(crate::world_report::WorkspaceRef::Native { .. })
                )
            })
            || !report.view_attempts.is_empty()
            || !report.creation_attempts.is_empty();
        if native_evidence && !self.host_owned_workspace_domains() {
            return V4RunOutcome::Rejected {
                kind: KIND_SNAPSHOT,
                message: "native-without-capability",
            };
        }
        let host_owned = self.host_owned_workspace_domains();
        let arrival_host_mismatch = report.focus_feedback.iter().any(|row| {
            row.arrival
                .iter()
                .any(|bracket| bracket.location.is_native() != host_owned)
        });
        if arrival_host_mismatch {
            return V4RunOutcome::Rejected {
                kind: KIND_SNAPSHOT,
                message: "arrival-host-mismatch",
            };
        }
        if report_estimate_v4(report) > WORLD_MAX_REQUEST_BYTES {
            return V4RunOutcome::Rejected {
                kind: KIND_OVERSIZED,
                message: MSG_OVERSIZED,
            };
        }
        if let Some(reason) = validate_report_v4_reason(effective) {
            return V4RunOutcome::Rejected {
                kind: KIND_SNAPSHOT,
                // Fixed check vocabulary, never payload bytes.
                message: reason,
            };
        }
        let digest = report_digest_v4(report);
        if self.has_last_v4 {
            // Fast digest pre-check first; full typed input equality
            // decides, never the digest alone.
            let reports_equal = self
                .last_report_v4
                .as_ref()
                .is_some_and(|last| last == report);
            match classify_replay_v4(
                report.request_seq,
                digest,
                self.has_last_v4,
                self.last_request_seq_v4,
                self.last_digest_v4,
                reports_equal,
            ) {
                V4Replay::Fresh => {}
                V4Replay::Stale => {
                    return V4RunOutcome::Rejected {
                        kind: KIND_SUPERSEDED,
                        message: MSG_SUPERSEDED,
                    };
                }
                V4Replay::Reused => {
                    return V4RunOutcome::Rejected {
                        kind: KIND_REUSED,
                        message: MSG_REUSED,
                    };
                }
                V4Replay::Duplicate => {
                    let Some(cached) = self.last_reply_v4.clone() else {
                        return V4RunOutcome::Rejected {
                            kind: KIND_SUPERSEDED,
                            message: MSG_SUPERSEDED,
                        };
                    };
                    return V4RunOutcome::Duplicate(cached);
                }
            }
        }
        if self.has_last_v4 && report.observation_seq < self.last_observation_seq_v4 {
            return V4RunOutcome::Rejected {
                kind: KIND_OBS_STALE,
                message: MSG_OBS_STALE,
            };
        }
        // Conservative pre-mutation reply bound: no observation convergence,
        // slot cleanup, or command mutation runs when the worst-case reply
        // cannot fit. All v4 bounds ride here, including the retained
        // publication footprint, rollback, and selection terminal reserves.
        let retained_len = self.retained.len() + self.retained_floats.len();
        let created_len = report.screens.len();
        let staged_windows = self
            .staged
            .facts
            .len()
            .saturating_add(self.staged.held_windows.len());
        if reply_upper_bound_v4(
            report,
            retained_len,
            self.engine.domain_keys().len(),
            created_len,
            staged_windows,
            self.retained_publish_footprint(),
        )
        .saturating_add(max_intent_rollback_bound(self.rollback_reserve_count()))
        .saturating_add(self.switch_settle_reserve())
        // A select-family command can mint one zero-obligation issuing
        // terminal while old bindings settle in the same report; reserve
        // it alongside the retained bindings, never from lane slack.
        .saturating_add(self.switch_issuing_reserve(report.command.as_ref()))
        // Commandless policy views can mint at most one zero-obligation
        // terminal per fresh displaced/returned workspace at publish;
        // bound the placements knowable before convergence.
        .saturating_add(self.policy_terminal_reserve_v4(report))
            > WORLD_MAX_REPLY_BYTES
        {
            return V4RunOutcome::Rejected {
                kind: KIND_REPLY_SIZE,
                message: MSG_REPLY_SIZE,
            };
        }
        // Counter exhaustion refuses and re-bootstraps under a new
        // generation; it never wraps in place.
        if self.state_revision == u64::MAX {
            return V4RunOutcome::Rejected {
                kind: KIND_REVISION_EXHAUSTED,
                message: MSG_REVISION_EXHAUSTED,
            };
        }
        // Fresh reports carrying an already-consumed command sequence
        // reconcile observation but never re-execute the command. The
        // command-pair shape already passed the structural gate above, so
        // the mismatch arm stays defensive.
        let consumed = match (report.command_seq, report.command.as_ref()) {
            (Some(seq), Some(_)) => self.command_highwater.is_some_and(|mark| seq <= mark),
            (Some(_), None) | (None, Some(_)) => {
                return V4RunOutcome::Rejected {
                    kind: KIND_SNAPSHOT,
                    message: "command-pair-invalid",
                };
            }
            (None, None) => false,
        };
        let usable_initial_observation = usable_initial_observation_v4(report);
        if !self.initial_settings_accepted || (!self.ready_latched && !usable_initial_observation) {
            // Bootstrap still ingests validated intent rows on a staged
            // candidate so records delivered in incomplete pre-ready
            // inventories survive until the first usable fact, then adopts
            // the candidate. No admission, sweep, or writes here. Gate
            // order above guarantees every refusal already returned, so the
            // original stays unchanged on every refusal path. Complete/closed
            // cleanup applies immediately too so stale orphans and conflicts
            // cannot exhaust the joint bound before ready; retention
            // pre-ready is bounded pending data, never report or write
            // authority.
            let mut staged = self.clone();
            staged.ingest_staged_intent(&Self::capture_observation_v4(report));
            if report.inventory_complete {
                let live: BTreeSet<crate::directional::WindowId> = report
                    .windows
                    .iter()
                    .map(|fact| fact.window.clone())
                    .collect();
                let held: BTreeSet<crate::directional::WindowId> = report
                    .held_windows
                    .iter()
                    .map(|held| held.window.clone())
                    .collect();
                staged
                    .pending_intent
                    .retain(|id, _| live.contains(id) || held.contains(id));
                staged
                    .intent_conflicts
                    .retain(|id| live.contains(id) || held.contains(id));
                staged
                    .restored_floats
                    .retain(|id| live.contains(id) || held.contains(id));
                staged
                    .restored_sticky
                    .retain(|id, _| live.contains(id) || held.contains(id));
                staged
                    .intent_admitted
                    .retain(|id| live.contains(id) || held.contains(id));
                staged
                    .intent_sticky_blocks
                    .retain(|id| live.contains(id) || held.contains(id));
            }
            let reply = staged.bootstrap_reply_v4(report, &issue.correlation);
            if let Some(refused) = Self::refuse_outbound_v4(&reply) {
                return refused;
            }
            *self = staged;
            return V4RunOutcome::Accepted(reply);
        }
        // Stage everything past this point on a clone. Exact-encoding
        // overflow (or any late failure) drops the candidate; `self` is
        // adopted only after the staged reply encodes within budget. Only
        // reports passing every cheap gate pay for the clone.
        let mut staged = self.clone();
        if usable_initial_observation {
            staged.ready_latched = true;
        }
        let outcome = staged.run_accepted_v4(report, issue, consumed, self.state_revision + 1);
        match outcome {
            V4RunOutcome::Accepted(reply) | V4RunOutcome::CommandConsumed(reply) => {
                self.finalize_v4_candidate(staged, report, digest, reply)
            }
            V4RunOutcome::Duplicate(_) => {
                // Staged runs never duplicate: the latest duplicate returns
                // above, before staging.
                V4RunOutcome::Rejected {
                    kind: KIND_SUPERSEDED,
                    message: MSG_SUPERSEDED,
                }
            }
            rejected => rejected,
        }
    }

    /// Outbound gates over one staged v4 reply: intent-lane validity and
    /// the exact canonical size bound. Returns the refusal when the reply
    /// must not publish or cache, `None` when it may adopt.
    fn refuse_outbound_v4(reply: &WorldReplyV4) -> Option<V4RunOutcome> {
        // Fixed check vocabulary, never payload.
        if let Some(reason) = validate_intent_writes_reason(&reply.intent_writes) {
            return Some(V4RunOutcome::Rejected {
                kind: KIND_SNAPSHOT,
                message: reason,
            });
        }
        if reply_estimate_v4(reply) > WORLD_MAX_REPLY_BYTES
            || reply_exact_bytes_v4(reply) > WORLD_MAX_REPLY_BYTES
        {
            return Some(V4RunOutcome::Rejected {
                kind: KIND_REPLY_SIZE,
                message: MSG_REPLY_SIZE,
            });
        }
        None
    }

    /// Adopt one staged v4 candidate after the outbound gates pass:
    /// install the candidate, refresh the typed replay cache, and return
    /// the publication verdict. A refused candidate leaves `self` (state,
    /// watermarks, cache) exactly unchanged.
    pub(crate) fn finalize_v4_candidate(
        &mut self,
        staged: WorldEngine,
        report: &WorldReportV4,
        digest: u64,
        reply: WorldReplyV4,
    ) -> V4RunOutcome {
        if let Some(refused) = Self::refuse_outbound_v4(&reply) {
            return refused;
        }
        let consumed_reply = reply.command_consumed;
        *self = staged;
        // Typed replay cache: mirrors the legacy finish tail, including
        // consumed-command reports; rejected candidates never reach here.
        if report.observation_seq > self.last_observation_seq_v4 || !self.has_last_v4 {
            self.last_observation_seq_v4 = report.observation_seq;
        }
        self.has_last_v4 = true;
        self.last_request_seq_v4 = report.request_seq;
        self.last_digest_v4 = digest;
        self.last_report_v4 = Some(report.clone());
        self.last_reply_v4 = Some(reply.clone());
        if consumed_reply {
            V4RunOutcome::CommandConsumed(reply)
        } else {
            V4RunOutcome::Accepted(reply)
        }
    }

    /// Empty v4 bootstrap reply for pre-ready reports: mirrors the current
    /// revision, advances nothing, and carries no authority. The supplied
    /// correlation stamps the admitted request identity; the empty
    /// presentation (hidden border/underlay, empty pager) is constructed
    /// explicitly so retained staged/pager state can never leak in.
    fn bootstrap_reply_v4(
        &self,
        report: &WorldReportV4,
        correlation: &CorrelationId,
    ) -> WorldReplyV4 {
        // Bootstrap publishes no intent decisions: the empty `intent_writes`
        // below trivially satisfies `validate_intent_writes_reason`, and the
        // fixed empty shape trivially satisfies the exact size bound, so the
        // pre-ready gate above stays the single enforcing boundary.
        let presentation = WorldPresentationV4 {
            foreground: String::new(),
            rev: self.state_revision,
            suspended_output: None,
            border: BorderPresentationV4 {
                visible: false,
                reason: None,
                subject: None,
                outsets: None,
                width: None,
                radius: None,
                witness: None,
            },
            underlay: PresentationUnderlay::Hidden {
                base_revision: None,
                reason: GroupHideReason::NoGroup,
            },
            pager: Vec::new(),
        };
        WorldReplyV4 {
            contract_version: WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: report.adapter_incarnation.clone(),
            core_generation: report.core_generation.clone(),
            request_id: correlation.as_str().to_owned(),
            based_on_observation_seq: report.observation_seq,
            state_revision: self.state_revision,
            ready: false,
            command_consumed: false,
            scopes: Vec::new(),
            workspaces: Vec::new(),
            outputs: Vec::new(),
            diagnostics: Vec::new(),
            windows: Vec::new(),
            transfers: Vec::new(),
            effects: Vec::new(),
            command_result: None,
            gesture_settled: None,
            operation_settled: Vec::new(),
            native_creations: Vec::new(),
            cursor_warp: None,
            presentation,
            intent_writes: Vec::new(),
            switch_settled: Vec::new(),
        }
    }
}

impl WorldEngine {
    /// Test-support entry over already-admitted connection context: reuse
    /// the bound owner and the report's decoder-validated correlation to
    /// build the admitted issue, then run the internal
    /// [`WorldEngine::handle_report_v4`] boundary. Bypasses public envelope
    /// admission: the caller must establish adapter/generation binding.
    /// Missing bound owner or invalid correlation refuses before execution.
    /// Not a second public ingress: no
    /// broker, service, adapter, or version wiring. Accepted, duplicate, and
    /// command-consumed outcomes all return the typed reply for faithful
    /// canonical encoding; refusals return their kind/message without
    /// manufacturing output. Ordered protocol scenario tests use this to
    /// execute immutable fixtures end to end.
    #[doc(hidden)]
    pub fn handle_report_v4_for_test(
        &mut self,
        report: &WorldReportV4,
    ) -> Result<WorldReplyV4, (&'static str, &'static str)> {
        let owner = match self.connection_owner.clone() {
            Some(owner) => owner,
            None => return Err((KIND_SNAPSHOT, "test-issue-unbound")),
        };
        let correlation = match CorrelationId::parse(&report.request_id) {
            Some(correlation) => correlation,
            None => return Err((KIND_SNAPSHOT, "test-issue-invalid")),
        };
        match self.handle_report_v4(report, &EventIssue { owner, correlation }) {
            V4RunOutcome::Accepted(reply)
            | V4RunOutcome::Duplicate(reply)
            | V4RunOutcome::CommandConsumed(reply) => Ok(reply),
            V4RunOutcome::Rejected { kind, message } => Err((kind, message)),
        }
    }
}
