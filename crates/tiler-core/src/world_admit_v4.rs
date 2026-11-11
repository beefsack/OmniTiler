//! Parallel core-internal v4 admission boundary: bootstrap, typed
//! reply/replay bookkeeping, and bounded candidate adoption (unit 6
//! sub-increment 7e).
//!
//! [`WorldEngine::handle_report_v4`] is the typed v4 counterpart of the
//! admission half of legacy [`WorldEngine::handle_report`](crate::world_engine::WorldEngine::handle_report):
//! an internal boundary over an already admitted connection/correlation
//! context. The caller supplies the [`EventIssue`](crate::world_engine::EventIssue)
//! and this entry defends that admitted envelope (bound adapter,
//! generation, owner, and request correlation) before any mutation. Every
//! cheap gate runs before any mutation: admitted-envelope defense,
//! invalid-screen/window plus conflicting-window grading, request-size
//! estimate, the structural v4 validator, typed request replay (digest
//! pre-check, full typed equality decides), observation ordering, the conservative reply upper bound with
//! the retained-publication, rollback, and selection-terminal reserves,
//! and revision exhaustion. Accepted inputs then run to completion on a
//! staged clone via [`WorldEngine::run_accepted_v4`](crate::world_run_v4::WorldEngine::run_accepted_v4);
//! the staged reply passes the outbound intent-lane gate and the exact
//! canonical size gate before the candidate commits. A staged overflow or
//! late failure drops the candidate; `self` is adopted only after the
//! staged reply encodes within budget. Only reports passing every cheap
//! gate pay for the clone.
//!
//! The typed v4 replay cache (`last_report_v4`/`last_reply_v4`) is v4-only
//! with no legacy twin, while the command high-watermark, state revision,
//! and lifetime/session machinery stay shared. Pre-ready reports stage intent retention on a
//! candidate and publish an empty bootstrap reply: no authority, no
//! revision advance, no command consumption.

use std::collections::BTreeSet;

use crate::connection::RawSettingValue;
use crate::ids::CorrelationId;
use crate::world_engine::{
    EventIssue, KIND_CORRELATION, KIND_GENERATION, KIND_OBS_STALE, KIND_OVERSIZED, KIND_OWNER,
    KIND_REPLY_SIZE, KIND_REUSED, KIND_REVISION_EXHAUSTED, KIND_SNAPSHOT, KIND_SUPERSEDED,
    MSG_CORRELATION, MSG_GENERATION, MSG_OBS_STALE, MSG_OVERSIZED, MSG_OWNER, MSG_REPLY_SIZE,
    MSG_REUSED, MSG_REVISION_EXHAUSTED, MSG_SUPERSEDED, WorldEngine,
};
use crate::world_presentation::{GroupHideReason, PresentationUnderlay};
use crate::world_report::{
    AttemptLaneV4, BatchSettlement, BorderPresentationV4, WORLD_CONTRACT_VERSION_V4,
    WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES, WorldCommand, WorldPresentationV4,
    WorldReplyV4, WorldReportV4, validate_intent_writes_reason,
};
use crate::world_report_v4_checks::{
    graded_effective_v4, report_digest_v4, validate_report_v4_reason,
};
use crate::world_run_v4::V4RunOutcome;
use crate::world_sizing::max_intent_rollback_bound;
use crate::world_sizing_v4::{
    is_intent_save_command_v4, max_intent_save_batch_bound_v4, max_intent_save_batch_rows_v4,
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
    /// ordered v4 reply. Internal only: the caller supplies the admitted
    /// [`EventIssue`]; this entry defends the already-admitted envelope
    /// (bound adapter/generation plus issue owner/correlation) before any
    /// mutation and never touches the public ingress, adapters, brokers,
    /// or services.
    ///
    /// Transactional: every cheap gate runs before any mutation. Refused
    /// reports leave the engine (including intent ledgers, revision, and
    /// command consumption) exactly unchanged. Ready reports run on a
    /// staged clone adopted only after the outbound intent-lane gate and
    /// the exact reply-size gate pass; pre-ready reports stage intent
    /// retention on a candidate and return an empty bootstrap reply with no
    /// authority and no revision advance.
    pub(crate) fn handle_report_v4(
        &mut self,
        report: &WorldReportV4,
        issue: &EventIssue,
    ) -> V4RunOutcome {
        let (outcome, _) = self.handle_report_v4_batched(report, issue, &[]);
        outcome
    }

    /// Accept one already-admitted v4 report plus a parked FIFO command
    /// batch drained inside the same capture/run/publication. The report
    /// carries the fresh native observations; `queued` holds the parked
    /// `(command sequence, command)` pairs in drain order. Returns the
    /// single admission outcome plus one per-command settlement
    /// index-aligned with `queued`. Any refusal adopts nothing, settles
    /// nothing, and returns empty settlements so the caller reparks the
    /// whole batch.
    pub(crate) fn handle_report_v4_batched(
        &mut self,
        report: &WorldReportV4,
        issue: &EventIssue,
        queued: &[(u64, WorldCommand)],
    ) -> (V4RunOutcome, Vec<BatchSettlement>) {
        // Internal admitted-envelope defense: the bound adapter, generation,
        // owner, and request correlation must match before any estimate,
        // grading, validation, or mutation. The caller owns public
        // admission; this internal boundary re-checks the already-admitted
        // context so a stale or foreign envelope never reaches the run.
        if self.adapter.as_deref() != Some(report.adapter_incarnation.as_str()) {
            return (
                V4RunOutcome::Rejected {
                    kind: KIND_OWNER,
                    message: MSG_OWNER,
                },
                Vec::new(),
            );
        }
        if report.core_generation != self.core_generation_str {
            return (
                V4RunOutcome::Rejected {
                    kind: KIND_GENERATION,
                    message: MSG_GENERATION,
                },
                Vec::new(),
            );
        }
        let Some(bound_owner) = self.connection_owner.clone() else {
            return (
                V4RunOutcome::Rejected {
                    kind: KIND_OWNER,
                    message: MSG_OWNER,
                },
                Vec::new(),
            );
        };
        if issue.owner != bound_owner {
            return (
                V4RunOutcome::Rejected {
                    kind: KIND_OWNER,
                    message: MSG_OWNER,
                },
                Vec::new(),
            );
        }
        if issue.correlation.as_str() != report.request_id {
            return (
                V4RunOutcome::Rejected {
                    kind: KIND_CORRELATION,
                    message: MSG_CORRELATION,
                },
                Vec::new(),
            );
        }
        // Core-owned grading for the typed path: invalid scale/identity and
        // metadata/process rows become narrow holds like their
        // protocol-decoded twins, then unavailable-fact contradictions hold,
        // before any estimate, validation, or mutation.
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
        let graded_effective = graded_effective_v4(
            report,
            unavailable,
            native_sticky_supported,
            maximize_supported,
        );
        // The graded copy supplies validation and initial readiness. Digest,
        // equality, size, caching, and the accepted run keep the original
        // typed input; the run grades its effective authority rows separately.
        // Raw rows must never alias equivalent pre-held input as duplicates.
        let effective = graded_effective.as_ref().map_or(report, |owned| owned);
        // Fail-closed capability gates, mirroring the legacy admission
        // order: native evidence rides only on host-owned hosts, and the
        // readiness-bracket arm matches the host kind. Checked before any
        // adoption, even when the location lanes are null. Already-held
        // native evidence counts conservatively: a held native row, a held
        // window retaining a non-empty native workspace, or a held
        // view/creation attempt is still native evidence, while a held
        // member attempt counts only when a retained `pre`/`post` field
        // carries the native reference kind.
        let held_native_evidence = !report.held_native.is_empty()
            || report.held_windows.iter().any(|held| {
                held.fields.iter().any(|(key, value)| {
                    key == "nativeWs"
                        && matches!(value, crate::connection::RawSettingValue::String(id) if !id.is_empty())
                })
            })
            || report.held_attempts.iter().any(|held| match held.lane {
                AttemptLaneV4::Member => held.fields.iter().any(|(key, value)| {
                    (key == "pre" || key == "post")
                        && matches!(value, RawSettingValue::Object(entries)
                            if entries.iter().any(|(key, kind)| key == "kind"
                                && matches!(kind, RawSettingValue::String(kind) if kind == "native")))
                }),
                AttemptLaneV4::View | AttemptLaneV4::Creation => true,
                _ => false,
            });
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
            || !report.creation_attempts.is_empty()
            || held_native_evidence;
        if native_evidence && !self.host_owned_workspace_domains() {
            return (
                V4RunOutcome::Rejected {
                    kind: KIND_SNAPSHOT,
                    message: "native-without-capability",
                },
                Vec::new(),
            );
        }
        let host_owned = self.host_owned_workspace_domains();
        let arrival_host_mismatch = report.focus_feedback.iter().any(|row| {
            row.arrival
                .iter()
                .any(|bracket| bracket.location.is_native() != host_owned)
        });
        if arrival_host_mismatch {
            return (
                V4RunOutcome::Rejected {
                    kind: KIND_SNAPSHOT,
                    message: "arrival-host-mismatch",
                },
                Vec::new(),
            );
        }
        if report_estimate_v4(report) > WORLD_MAX_REQUEST_BYTES {
            return (
                V4RunOutcome::Rejected {
                    kind: KIND_OVERSIZED,
                    message: MSG_OVERSIZED,
                },
                Vec::new(),
            );
        }
        if let Some(reason) = validate_report_v4_reason(effective) {
            return (
                V4RunOutcome::Rejected {
                    kind: KIND_SNAPSHOT,
                    // Fixed check vocabulary, never payload bytes.
                    message: reason,
                },
                Vec::new(),
            );
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
                    return (
                        V4RunOutcome::Rejected {
                            kind: KIND_SUPERSEDED,
                            message: MSG_SUPERSEDED,
                        },
                        Vec::new(),
                    );
                }
                V4Replay::Reused => {
                    return (
                        V4RunOutcome::Rejected {
                            kind: KIND_REUSED,
                            message: MSG_REUSED,
                        },
                        Vec::new(),
                    );
                }
                V4Replay::Duplicate => {
                    let Some(cached) = self.last_reply_v4.clone() else {
                        return (
                            V4RunOutcome::Rejected {
                                kind: KIND_SUPERSEDED,
                                message: MSG_SUPERSEDED,
                            },
                            Vec::new(),
                        );
                    };
                    return (V4RunOutcome::Duplicate(cached), Vec::new());
                }
            }
        }
        if self.has_last_v4 && report.observation_seq < self.last_observation_seq_v4 {
            return (
                V4RunOutcome::Rejected {
                    kind: KIND_OBS_STALE,
                    message: MSG_OBS_STALE,
                },
                Vec::new(),
            );
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
        // Drained FIFO batch saves: the inline bound above covers at most
        // the report command; queued save-capable commands (explicit
        // distinct plus implicit max-width) stage additional latest-wins
        // rows merged into the single final publication.
        .saturating_add(max_intent_save_batch_bound_v4(queued))
            > WORLD_MAX_REPLY_BYTES
        {
            return (
                V4RunOutcome::Rejected {
                    kind: KIND_REPLY_SIZE,
                    message: MSG_REPLY_SIZE,
                },
                Vec::new(),
            );
        }
        // Fixed rowcap: the merged final section (pre-existing rollback
        // plus inline plus distinct batch saves, latest-wins collapsed only
        // at publish) must fit `MAX_INTENT_WRITES`. Bytes alone cannot gate
        // this: rows and bytes are distinct limits. Over-cap refuses before
        // mutation with the outbound section vocabulary so the caller
        // reparks the whole batch.
        {
            let inline_rows = usize::from(
                report
                    .command
                    .as_ref()
                    .is_some_and(is_intent_save_command_v4),
            );
            let total_rows = self
                .rollback_reserve_count()
                .saturating_add(inline_rows)
                .saturating_add(max_intent_save_batch_rows_v4(queued));
            if total_rows > crate::world_report::MAX_INTENT_WRITES {
                return (
                    V4RunOutcome::Rejected {
                        kind: KIND_SNAPSHOT,
                        message: "section-over-cap",
                    },
                    Vec::new(),
                );
            }
        }
        // Counter exhaustion refuses and re-bootstraps under a new
        // generation; it never wraps in place.
        if self.state_revision == u64::MAX {
            return (
                V4RunOutcome::Rejected {
                    kind: KIND_REVISION_EXHAUSTED,
                    message: MSG_REVISION_EXHAUSTED,
                },
                Vec::new(),
            );
        }
        // Fresh reports carrying an already-consumed command sequence
        // reconcile observation but never re-execute the command. The
        // command-pair shape already passed the structural gate above, so
        // the mismatch arm stays defensive.
        let consumed = match (report.command_seq, report.command.as_ref()) {
            (Some(seq), Some(_)) => self.command_highwater.is_some_and(|mark| seq <= mark),
            (Some(_), None) | (None, Some(_)) => {
                return (
                    V4RunOutcome::Rejected {
                        kind: KIND_SNAPSHOT,
                        message: "command-pair-invalid",
                    },
                    Vec::new(),
                );
            }
            (None, None) => false,
        };
        // Readiness keys on the effective graded report, never the raw
        // input: an invalid-held screen alone is not a usable screen.
        let usable_initial_observation = usable_initial_observation_v4(effective);
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
                return (refused, Vec::new());
            }
            *self = staged;
            return (V4RunOutcome::Accepted(reply), Vec::new());
        }
        // Stage everything past this point on a clone. Exact-encoding
        // overflow (or any late failure) drops the candidate; `self` is
        // adopted only after the staged reply encodes within budget. Only
        // reports passing every cheap gate pay for the clone.
        let mut staged = self.clone();
        if usable_initial_observation {
            staged.ready_latched = true;
        }
        let (outcome, settlements) = staged.run_accepted_v4_batched(
            report,
            issue,
            consumed,
            self.state_revision + 1,
            queued,
        );
        match outcome {
            V4RunOutcome::Accepted(reply) | V4RunOutcome::CommandConsumed(reply) => {
                let consumed_reply = reply.command_consumed;
                match self.finalize_v4_candidate(staged, report, digest, reply) {
                    V4RunOutcome::Accepted(reply) => {
                        debug_assert!(!consumed_reply);
                        (V4RunOutcome::Accepted(reply), settlements)
                    }
                    V4RunOutcome::CommandConsumed(reply) => {
                        debug_assert!(consumed_reply);
                        (V4RunOutcome::CommandConsumed(reply), settlements)
                    }
                    V4RunOutcome::Duplicate(reply) => (V4RunOutcome::Duplicate(reply), Vec::new()),
                    rejected => (rejected, Vec::new()),
                }
            }
            V4RunOutcome::Duplicate(_) => {
                // Staged runs never duplicate: the latest duplicate returns
                // above, before staging.
                (
                    V4RunOutcome::Rejected {
                        kind: KIND_SUPERSEDED,
                        message: MSG_SUPERSEDED,
                    },
                    Vec::new(),
                )
            }
            rejected => (rejected, Vec::new()),
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
            close_focus: None,
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
    /// [`WorldEngine::handle_report_v4`] boundary. Bypasses public ingress
    /// only: the caller must establish adapter/generation binding, and the
    /// internal boundary still defends the bound adapter, generation,
    /// owner, and correlation before any mutation. Missing bound owner or
    /// invalid correlation refuses before execution.
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
