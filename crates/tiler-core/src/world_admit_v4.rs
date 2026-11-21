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

use crate::ids::CorrelationId;
use crate::world_engine::{
    AdmittedReplayV4, EventIssue, KIND_CORRELATION, KIND_GENERATION, KIND_OBS_STALE,
    KIND_OVERSIZED, KIND_OWNER, KIND_REPLY_SIZE, KIND_REUSED, KIND_REVISION_EXHAUSTED,
    KIND_SNAPSHOT, KIND_SUPERSEDED, MSG_CORRELATION, MSG_GENERATION, MSG_OBS_STALE, MSG_OVERSIZED,
    MSG_OWNER, MSG_REPLY_SIZE, MSG_REUSED, MSG_REVISION_EXHAUSTED, MSG_SUPERSEDED,
    PreparedCandidate, TransitionResult, ValidatedReply, WakePrecheckCandidate, WorldEngine,
    WorldState,
};
use crate::world_presentation::{GroupHideReason, PresentationUnderlay};
use crate::world_reply_validation::validate_reply_v4;
use crate::world_report::{
    AttemptLaneV4, BatchDisposition, BorderPresentationV4, CommandDisposition, ReportIncorporation,
    WORLD_CONTRACT_VERSION_V4, WorldCommand, WorldPresentationV4, WorldReplyV4, WorldReportV4,
    validate_focus_request_reason, validate_intent_desired_reason,
};
#[cfg(any(test, feature = "structural-bench"))]
use crate::world_report::{BatchSettlement, CommandResult};
use crate::world_report_v4_checks::{
    graded_effective_v4, report_digest_v4, validate_batch_v4_reason, validate_report_v4_reason,
};
use crate::world_roster::RosterFeedback;
use crate::world_run_v4::V4RunOutcome;
use crate::world_transaction::WorldTransaction;

/// Format-neutral decision returned by [`CoreAdmissionGate`] checkpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionGateDecision {
    Allow,
    Refuse,
    Reject {
        kind: &'static str,
        message: &'static str,
    },
}

/// Ephemeral exact-inner measurement produced by
/// [`CoreAdmissionGate::check_exact_inner`] and consumed once by
/// [`CoreAdmissionGate::check_future_reserve`] within the same candidate check.
/// Intentionally non-`Copy` so counts cannot be retained or reused across
/// invocations.
#[derive(Debug, PartialEq, Eq)]
pub struct ExactInnerCheck(usize);

impl ExactInnerCheck {
    #[must_use]
    pub fn new(bytes: usize) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn bytes(&self) -> usize {
        self.0
    }
}

/// Decision returned by [`CoreAdmissionGate::check_exact_inner`].
#[derive(Debug, PartialEq, Eq)]
pub enum ExactInnerDecision {
    Allow(ExactInnerCheck),
    Refuse,
    Reject {
        kind: &'static str,
        message: &'static str,
    },
}

/// Borrowed preflight inputs and count-only world facts supplied to
/// [`CoreAdmissionGate::check_preflight`] after all cheap structural, replay,
/// observation, and command-pair gates pass.
#[derive(Debug, Clone, Copy)]
pub struct PreflightGateFacts<'a> {
    pub effective_command: Option<&'a WorldCommand>,
    pub effective_queued: &'a [(u64, WorldCommand)],
    pub retained_settlement_count: usize,
    pub domain_count: usize,
    pub screen_count: usize,
    pub staged_window_count: usize,
    pub retained_publish_footprint: usize,
    pub intent_desired_count: usize,
    pub operation_slot_count: usize,
    pub switch_output_count: usize,
    pub switch_failed_count: usize,
    pub issuing_selection_count: usize,
    pub policy_terminal_count: usize,
}

/// Count-only staged settlement facts supplied to
/// [`CoreAdmissionGate::check_future_reserve`] after the exact current-inner
/// gate and staged pending operation slot hard cap pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FutureReserveGateFacts {
    pub operation_slot_count: usize,
    pub switch_output_count: usize,
    pub switch_failed_count: usize,
}

/// Opaque host/protocol replay-residual token retained alongside
/// [`AdmittedReplayV4`] only when a checked gate carries non-empty residual
/// evidence. Core never inspects its contents; equality and cloning are
/// delegated to the trait object.
pub trait ReplayResidualToken: std::fmt::Debug + Send + Sync + std::any::Any {
    fn as_any(&self) -> &dyn std::any::Any;
    fn residual_eq(&self, other: &dyn ReplayResidualToken) -> bool;
}

/// Shared opaque replay-residual handle stored on [`AdmittedReplayV4`] only
/// when a gate returns `Some(...)`. Default-empty callers pay zero allocation.
#[derive(Debug, Clone)]
pub struct RetainedResidual(pub std::sync::Arc<dyn ReplayResidualToken>);

impl PartialEq for RetainedResidual {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0) || self.0.residual_eq(other.0.as_ref())
    }
}

/// Scoped synchronous format-neutral admission gate over borrowed reports,
/// replies, and typed count/fact bundles. Core maps `Refuse` to the static
/// fixed refusal at each checkpoint.
pub trait CoreAdmissionGate {
    fn check_request_size(&self, report: &WorldReportV4) -> AdmissionGateDecision;
    fn check_preflight(
        &self,
        report: &WorldReportV4,
        facts: &PreflightGateFacts<'_>,
    ) -> AdmissionGateDecision;
    fn check_exact_inner(&self, reply: &WorldReplyV4) -> ExactInnerDecision;
    fn check_future_reserve(
        &self,
        reply: &WorldReplyV4,
        facts: &FutureReserveGateFacts,
        exact: ExactInnerCheck,
    ) -> AdmissionGateDecision;
    /// Whether the incoming report's residual evidence matches the retained
    /// residual from the prior admitted reply (`None` when the prior report
    /// had no residual). Default-empty gates match only when `retained` is
    /// `None`.
    fn residual_matches(&self, retained: Option<&RetainedResidual>) -> bool {
        retained.is_none()
    }
    /// Capture an opaque residual token for [`AdmittedReplayV4`] after the
    /// candidate passes all outbound gates and `prepare` succeeds. Default-empty
    /// gates return `None` with zero allocation.
    fn capture_residual(&self) -> Option<RetainedResidual> {
        None
    }
    /// Pre-validation wake-epoch / sequence precheck run after exact/future
    /// reserve and before mandatory semantic + host-capability validation.
    fn check_wake_precheck(&self, _candidate: &WakePrecheckCandidate<'_>) -> AdmissionGateDecision {
        AdmissionGateDecision::Allow
    }
    /// Record the fixed rule token when late semantic or host-capability reply
    /// validation refuses a staged reply before `prepare`.
    fn on_invalid_reply(&self, _rule: &'static str) {}
}

/// Structural no-op admission gate used by direct typed core tests and
/// `structural-bench` builds. Unavailable in ordinary production builds.
#[cfg(any(test, feature = "structural-bench"))]
pub(crate) struct StructuralCoreAdmissionGate;

#[cfg(any(test, feature = "structural-bench"))]
impl CoreAdmissionGate for StructuralCoreAdmissionGate {
    fn check_request_size(&self, _report: &WorldReportV4) -> AdmissionGateDecision {
        AdmissionGateDecision::Allow
    }

    fn check_preflight(
        &self,
        _report: &WorldReportV4,
        _facts: &PreflightGateFacts<'_>,
    ) -> AdmissionGateDecision {
        AdmissionGateDecision::Allow
    }

    fn check_exact_inner(&self, _reply: &WorldReplyV4) -> ExactInnerDecision {
        ExactInnerDecision::Allow(ExactInnerCheck::new(0))
    }

    fn check_future_reserve(
        &self,
        _reply: &WorldReplyV4,
        _facts: &FutureReserveGateFacts,
        _exact: ExactInnerCheck,
    ) -> AdmissionGateDecision {
        AdmissionGateDecision::Allow
    }
}

#[cfg(any(test, feature = "structural-bench"))]
pub(crate) fn dispositions_to_legacy_settlements(
    dispositions: &[BatchDisposition],
    _queued: &[(u64, WorldCommand)],
) -> Vec<BatchSettlement> {
    dispositions
        .iter()
        .filter_map(|entry| match &entry.disposition {
            CommandDisposition::Executed(result) => Some(BatchSettlement {
                seq: entry.seq,
                result: result.clone(),
            }),
            CommandDisposition::Refused(kind) => Some(BatchSettlement {
                seq: entry.seq,
                result: Some(CommandResult::Rejected { kind: *kind }),
            }),
            CommandDisposition::AlreadyConsumed | CommandDisposition::NotRun => None,
        })
        .collect()
}

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

fn not_run_dispositions(
    report: &WorldReportV4,
    queued: &[(u64, WorldCommand)],
) -> Vec<BatchDisposition> {
    let mut out = Vec::with_capacity(usize::from(report.command.is_some()) + queued.len());
    if report.command.is_some()
        && let Some(seq) = report.command_seq
    {
        out.push(BatchDisposition {
            seq,
            disposition: CommandDisposition::NotRun,
        });
    }
    for (seq, _) in queued {
        out.push(BatchDisposition {
            seq: *seq,
            disposition: CommandDisposition::NotRun,
        });
    }
    out
}

fn rejected_transition(
    kind: &'static str,
    message: &'static str,
    dispositions: Vec<BatchDisposition>,
) -> TransitionResult {
    TransitionResult {
        outcome: V4RunOutcome::Rejected { kind, message },
        incorporation: ReportIncorporation::Rejected,
        dispositions,
    }
}

fn not_admitted_transition(kind: &'static str, message: &'static str) -> TransitionResult {
    TransitionResult {
        outcome: V4RunOutcome::Rejected { kind, message },
        incorporation: ReportIncorporation::NotAdmitted,
        dispositions: Vec::new(),
    }
}

impl WorldEngine {
    #[cfg(test)]
    pub(crate) fn handle_report_v4(
        &mut self,
        report: &WorldReportV4,
        issue: &EventIssue,
    ) -> V4RunOutcome {
        let (outcome, _) = self.handle_report_v4_batched(report, issue, &[]);
        outcome
    }

    #[cfg(test)]
    pub(crate) fn handle_report_v4_batched(
        &mut self,
        report: &WorldReportV4,
        issue: &EventIssue,
        queued: &[(u64, WorldCommand)],
    ) -> (V4RunOutcome, Vec<BatchSettlement>) {
        let result = self
            .handle_report_v4_batched_with_gate_and_prepare(
                report,
                issue,
                queued,
                &StructuralCoreAdmissionGate,
                |_| Ok::<(), std::convert::Infallible>(()),
            )
            .unwrap_or_else(|e| match e {});
        (
            result.outcome,
            dispositions_to_legacy_settlements(&result.dispositions, queued),
        )
    }

    #[cfg(test)]
    pub(crate) fn handle_report_v4_batched_with_gate_and_prepare<
        E,
        G: CoreAdmissionGate + ?Sized,
    >(
        &mut self,
        report: &WorldReportV4,
        issue: &EventIssue,
        queued: &[(u64, WorldCommand)],
        gate: &G,
        prepare: impl FnOnce(PreparedCandidate<'_>) -> Result<(), E>,
    ) -> Result<TransitionResult, E> {
        self.admit_report_v4_with_roster_feedback(report, issue, queued, gate, None, prepare)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn admit_report_v4_with_roster_feedback<E, G: CoreAdmissionGate + ?Sized>(
        &mut self,
        report: &WorldReportV4,
        issue: &EventIssue,
        queued: &[(u64, WorldCommand)],
        gate: &G,
        feedback: Option<&RosterFeedback>,
        prepare: impl FnOnce(PreparedCandidate<'_>) -> Result<(), E>,
    ) -> Result<TransitionResult, E> {
        // Internal admitted-envelope defense: the bound adapter, generation,
        // owner, and request correlation must match before any estimate,
        // grading, validation, or mutation. The caller owns public
        // admission; this internal boundary re-checks the already-admitted
        // context so a stale or foreign envelope never reaches the run.
        if self.connection.adapter.as_deref() != Some(report.adapter_incarnation.as_str()) {
            return Ok(not_admitted_transition(KIND_OWNER, MSG_OWNER));
        }
        if report.core_generation != self.connection.core_generation_str {
            return Ok(not_admitted_transition(KIND_GENERATION, MSG_GENERATION));
        }
        let Some(bound_owner) = self.connection.connection_owner.clone() else {
            return Ok(not_admitted_transition(KIND_OWNER, MSG_OWNER));
        };
        if issue.owner != bound_owner {
            return Ok(not_admitted_transition(KIND_OWNER, MSG_OWNER));
        }
        if issue.correlation.as_str() != report.request_id {
            return Ok(not_admitted_transition(KIND_CORRELATION, MSG_CORRELATION));
        }
        // Core-owned grading for the typed path: invalid scale/identity and
        // metadata/process rows become narrow holds like their
        // protocol-decoded twins, then unavailable-fact contradictions hold,
        // before any estimate, validation, or mutation.
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
            || report
                .held_windows
                .iter()
                .any(|held| held.has_native_workspace)
            || report.held_attempts.iter().any(|held| match held.lane {
                AttemptLaneV4::Member => held.has_native_ref,
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
        if native_evidence && !self.connection.host_owned_workspace_domains() {
            return Ok(not_admitted_transition(
                KIND_SNAPSHOT,
                "native-without-capability",
            ));
        }
        let host_owned = self.connection.host_owned_workspace_domains();
        let arrival_host_mismatch = report.focus_feedback.iter().any(|row| {
            row.arrival
                .iter()
                .any(|bracket| bracket.location.is_native() != host_owned)
        });
        if arrival_host_mismatch {
            return Ok(not_admitted_transition(
                KIND_SNAPSHOT,
                "arrival-host-mismatch",
            ));
        }
        match gate.check_request_size(report) {
            AdmissionGateDecision::Allow => {}
            AdmissionGateDecision::Refuse => {
                return Ok(not_admitted_transition(KIND_OVERSIZED, MSG_OVERSIZED));
            }
            AdmissionGateDecision::Reject { kind, message } => {
                return Ok(not_admitted_transition(kind, message));
            }
        }
        if let Some(reason) = validate_report_v4_reason(effective) {
            return Ok(not_admitted_transition(KIND_SNAPSHOT, reason));
        }
        let digest = report_digest_v4(report);
        if let Some(replay) = &self.replay {
            // Fast digest pre-check first; full typed input equality
            let reports_equal = replay.report == *report
                && replay.roster_feedback.as_ref() == feedback
                && gate.residual_matches(replay.residual.as_ref());
            match classify_replay_v4(
                report.request_seq,
                digest,
                true,
                replay.request_seq,
                replay.digest,
                reports_equal,
            ) {
                V4Replay::Fresh => {}
                V4Replay::Stale => {
                    return Ok(not_admitted_transition(KIND_SUPERSEDED, MSG_SUPERSEDED));
                }
                V4Replay::Reused => {
                    return Ok(not_admitted_transition(KIND_REUSED, MSG_REUSED));
                }
                V4Replay::Duplicate => {
                    let cached = replay.reply.clone();
                    self.world
                        .emit_published_settlement_trace(None, &cached, true);
                    self.world.emit_published_drag_preview_trace(&cached, true);
                    return Ok(TransitionResult {
                        outcome: V4RunOutcome::Duplicate(cached),
                        incorporation: ReportIncorporation::Replay,
                        dispositions: not_run_dispositions(report, queued),
                    });
                }
            }
        }
        if let Some(reason) =
            validate_batch_v4_reason(report.command_seq, report.command.is_some(), queued)
        {
            return Ok(rejected_transition(KIND_SNAPSHOT, reason, Vec::new()));
        }
        if self
            .replay
            .as_ref()
            .is_some_and(|replay| report.observation_seq < replay.observation_seq)
        {
            return Ok(rejected_transition(
                KIND_OBS_STALE,
                MSG_OBS_STALE,
                not_run_dispositions(report, queued),
            ));
        }
        // Fresh reports carrying an already-consumed command sequence
        // reconcile observation but never re-execute the command. Stale
        // inline never suppresses fresh queued work.
        let consumed = match (report.command_seq, report.command.as_ref()) {
            (Some(seq), Some(_)) => self.world.command_highwater.is_some_and(|mark| seq <= mark),
            (Some(_), None) | (None, Some(_)) => {
                return Ok(rejected_transition(
                    KIND_SNAPSHOT,
                    "command-pair-invalid",
                    Vec::new(),
                ));
            }
            (None, None) => false,
        };
        let effective_command = if consumed {
            None
        } else {
            report.command.as_ref()
        };
        let fresh_queued: Vec<(u64, WorldCommand)> = queued
            .iter()
            .filter(|(seq, _)| {
                !self
                    .world
                    .command_highwater
                    .is_some_and(|mark| *seq <= mark)
            })
            .cloned()
            .collect();
        let effective_queued: &[(u64, WorldCommand)] = &fresh_queued;
        // Conservative pre-mutation reply bound: no observation convergence,
        // slot cleanup, or command mutation runs when the worst-case reply
        // cannot fit. All v4 bounds ride here, including the retained
        // publication footprint, the whole retained intent-desire map, and
        // selection terminal reserves. The intent term budgets the entire
        // map (including explicit null clears) transactionally, never
        // truncation; the exact-encoded staged reply stays the backstop.
        let operation_slot_count = self
            .world
            .operation_settle_slot_count(effective_command, effective_queued);
        let (switch_output_count, switch_failed_count) = self.world.switch_settle_counts();
        let preflight_facts = PreflightGateFacts {
            effective_command,
            effective_queued,
            retained_settlement_count: self.world.settlement.retained.len()
                + self.world.settlement.retained_floats.len(),
            domain_count: self.world.layout.domain_keys().len(),
            screen_count: report.screens.len(),
            staged_window_count: self
                .world
                .staged
                .facts
                .len()
                .saturating_add(self.world.staged.held_windows.len()),
            retained_publish_footprint: self.world.retained_publish_footprint(),
            intent_desired_count: self.world.intent.intent_desired.len(),
            operation_slot_count,
            switch_output_count,
            switch_failed_count,
            issuing_selection_count: self.world.switch_issuing_count(effective_command),
            policy_terminal_count: self
                .world
                .policy_terminal_count_v4(report, self.connection.workspace_mode_setting()),
        };
        match gate.check_preflight(report, &preflight_facts) {
            AdmissionGateDecision::Allow => {}
            AdmissionGateDecision::Refuse => {
                return Ok(rejected_transition(
                    KIND_REPLY_SIZE,
                    MSG_REPLY_SIZE,
                    not_run_dispositions(report, queued),
                ));
            }
            AdmissionGateDecision::Reject { kind, message } => {
                return Ok(rejected_transition(
                    kind,
                    message,
                    not_run_dispositions(report, queued),
                ));
            }
        }
        // Fixed rowcap: the whole physical retained map plus genuinely new
        // rows from accepted imports, the inline command, and the parked
        // FIFO batch must fit `MAX_INTENT_DESIRED`. Dead identities stay
        // resident until the staged prune, so the gate budgets them
        // conservatively (same as the existing byte gate): a full physical
        // map refuses before mutation with the outbound section vocabulary
        // so the caller reparks the whole batch. Replacements never grow
        // the map and are excluded from the new-row count. Bytes alone
        // cannot gate this: rows and bytes are distinct limits. Over-cap
        // refuses before mutation; accepted runs always have room, so
        // physical insertion never silently drops a desired target.
        {
            use std::collections::BTreeSet;
            let closed: BTreeSet<&crate::directional::WindowId> = report.closed.iter().collect();
            let import_new = report
                .intent_records
                .iter()
                .filter(|record| !closed.contains(&record.win))
                .map(|record| &record.win)
                .collect::<BTreeSet<_>>()
                .iter()
                .filter(|id| !self.world.intent.intent_desired.contains_key(**id))
                .count();
            let total_rows = self
                .world
                .intent
                .intent_desired
                .len()
                .saturating_add(import_new)
                .saturating_add(
                    self.world
                        .intent_desired_new_rows(effective_command, effective_queued),
                );
            if total_rows > crate::world_report::MAX_INTENT_DESIRED {
                return Ok(rejected_transition(
                    KIND_SNAPSHOT,
                    "section-over-cap",
                    not_run_dispositions(report, queued),
                ));
            }
        }
        if operation_slot_count > crate::world_report::MAX_REPLY_OPERATIONS {
            return Ok(rejected_transition(
                KIND_SNAPSHOT,
                "section-over-cap",
                not_run_dispositions(report, queued),
            ));
        }
        // Counter exhaustion refuses and re-bootstraps under a new
        // generation; it never wraps in place.
        if self.world.state_revision == u64::MAX {
            return Ok(rejected_transition(
                KIND_REVISION_EXHAUSTED,
                MSG_REVISION_EXHAUSTED,
                not_run_dispositions(report, queued),
            ));
        }
        // Readiness keys on the effective graded report, never the raw
        // input: an invalid-held screen alone is not a usable screen.
        let usable_initial_observation = usable_initial_observation_v4(effective);
        if !self.connection.initial_settings_accepted
            || (!self.world.ready_latched && !usable_initial_observation)
        {
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
            let mut staged_intent = self.world.intent.clone();
            let own_pid = self
                .connection
                .application_identity
                .as_ref()
                .and_then(|identity| identity.process_id.as_deref());
            staged_intent.ingest_rows(
                own_pid,
                &report.closed,
                &report.windows,
                &report.intent_records,
            );
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
                staged_intent.retain_pre_ready_complete(&live, &held);
            }
            let reply =
                Self::bootstrap_reply_v4(self.world.state_revision, report, &issue.correlation);
            let bootstrap_dispositions = not_run_dispositions(report, queued);
            if let Err((kind, message)) = Self::refuse_outbound_v4(&reply, gate) {
                return Ok(rejected_transition(kind, message, bootstrap_dispositions));
            }
            let active_gesture_id = self.world.gesture.retained.as_ref().map(|r| r.id);
            let wake_candidate = WakePrecheckCandidate {
                reply: &reply,
                active_gesture_id,
                dispositions: &bootstrap_dispositions,
            };
            match gate.check_wake_precheck(&wake_candidate) {
                AdmissionGateDecision::Allow => {}
                AdmissionGateDecision::Refuse => {
                    return Ok(rejected_transition(
                        KIND_REPLY_SIZE,
                        MSG_REPLY_SIZE,
                        bootstrap_dispositions,
                    ));
                }
                AdmissionGateDecision::Reject { kind, message } => {
                    return Ok(rejected_transition(kind, message, bootstrap_dispositions));
                }
            }
            if let Err((kind, message)) =
                Self::validate_prepared_reply_v4(&reply, self.connection.capabilities(), gate)
            {
                return Ok(rejected_transition(kind, message, bootstrap_dispositions));
            }
            let candidate = PreparedCandidate {
                reply: ValidatedReply::new_validated(&reply),
                active_gesture_id,
                dispositions: &bootstrap_dispositions,
                shared_roster_plan: None,
            };
            prepare(candidate)?;
            self.world.emit_published_settlement_trace(
                self.replay.as_ref().map(|replay| &replay.reply),
                &reply,
                false,
            );
            self.world.emit_published_drag_preview_trace(&reply, false);
            self.world.intent = staged_intent;
            return Ok(TransitionResult {
                outcome: V4RunOutcome::Accepted(reply),
                incorporation: ReportIncorporation::BootstrapIntentOnly,
                dispositions: bootstrap_dispositions,
            });
        }
        // Stage everything past this point on a clone. Exact-encoding
        // overflow (or any late failure) drops the candidate; `self` is
        // adopted only after the staged reply encodes within budget. Only
        // reports passing every cheap gate pay for the clone.
        let mut staged = self.world.clone();
        if usable_initial_observation {
            staged.ready_latched = true;
        }
        let next_revision = self.world.state_revision + 1;
        let prev_transfers = self
            .replay
            .as_ref()
            .map(|replay| replay.reply.transfers.as_slice())
            .unwrap_or(&[]);
        let predecessor = self
            .replay
            .as_ref()
            .map(|replay| replay.reply.state_revision);
        let (outcome, dispositions) =
            WorldTransaction::new(&self.connection, prev_transfers, &mut staged)
                .run_accepted_v4_batched(
                    report,
                    issue,
                    consumed,
                    next_revision,
                    predecessor,
                    queued,
                    feedback,
                );
        match outcome {
            V4RunOutcome::Accepted(reply) | V4RunOutcome::CommandConsumed(reply) => {
                let consumed_reply = reply.command_consumed;
                match self.finalize_v4_candidate_with_roster_feedback(
                    staged,
                    report,
                    digest,
                    reply,
                    &dispositions,
                    gate,
                    feedback,
                    prepare,
                )? {
                    V4RunOutcome::Accepted(reply) => {
                        debug_assert!(!consumed_reply);
                        Ok(TransitionResult {
                            outcome: V4RunOutcome::Accepted(reply),
                            incorporation: ReportIncorporation::Incorporated,
                            dispositions,
                        })
                    }
                    V4RunOutcome::CommandConsumed(reply) => {
                        debug_assert!(consumed_reply);
                        Ok(TransitionResult {
                            outcome: V4RunOutcome::CommandConsumed(reply),
                            incorporation: ReportIncorporation::Incorporated,
                            dispositions,
                        })
                    }
                    V4RunOutcome::Duplicate(reply) => Ok(TransitionResult {
                        outcome: V4RunOutcome::Duplicate(reply),
                        incorporation: ReportIncorporation::Replay,
                        dispositions: not_run_dispositions(report, queued),
                    }),
                    V4RunOutcome::Rejected { kind, message } => Ok(rejected_transition(
                        kind,
                        message,
                        not_run_dispositions(report, queued),
                    )),
                }
            }
            V4RunOutcome::Duplicate(_) => {
                // Staged runs never duplicate: the latest duplicate returns
                // above, before staging.
                Ok(rejected_transition(
                    KIND_SUPERSEDED,
                    MSG_SUPERSEDED,
                    not_run_dispositions(report, queued),
                ))
            }
            V4RunOutcome::Rejected { kind, message } => Ok(rejected_transition(
                kind,
                message,
                not_run_dispositions(report, queued),
            )),
        }
    }

    /// Outbound pre-count guards and exact canonical size check over one
    /// staged v4 reply. Runs before future-reserve, wake precheck, and
    /// mandatory semantic/host-capability validation so C5 size/cap precedence
    /// is preserved.
    fn refuse_outbound_v4<G: CoreAdmissionGate + ?Sized>(
        reply: &WorldReplyV4,
        gate: &G,
    ) -> Result<ExactInnerCheck, (&'static str, &'static str)> {
        // Fixed check vocabulary, never payload.
        if let Some(reason) = validate_intent_desired_reason(&reply.intent_desired) {
            return Err((KIND_SNAPSHOT, reason));
        }
        if let Some(reason) = validate_focus_request_reason(&reply.focus_request) {
            return Err((KIND_SNAPSHOT, reason));
        }
        if reply.operation_settled.len() > crate::world_report::MAX_REPLY_OPERATIONS
            || reply.switch_settled.len() > crate::world_report::MAX_REPLY_SWITCH_EVENTS
        {
            return Err((KIND_SNAPSHOT, "section-over-cap"));
        }
        match gate.check_exact_inner(reply) {
            ExactInnerDecision::Allow(exact) => Ok(exact),
            ExactInnerDecision::Refuse => Err((KIND_REPLY_SIZE, MSG_REPLY_SIZE)),
            ExactInnerDecision::Reject { kind, message } => Err((kind, message)),
        }
    }

    /// Mandatory semantic and actual-host-capability validation run after C5
    /// size/reserve/wake fences and immediately before final `prepare`.
    fn validate_prepared_reply_v4<G: CoreAdmissionGate + ?Sized>(
        reply: &WorldReplyV4,
        capabilities: Option<&crate::connection::HostCapabilities>,
        gate: &G,
    ) -> Result<(), (&'static str, &'static str)> {
        if let Err(err) = validate_reply_v4(reply, capabilities) {
            let rule = if err.message == "request contains an unknown value" {
                err.kind
            } else {
                err.message
            };
            gate.on_invalid_reply(rule);
            return Err((err.kind, err.message));
        }
        Ok(())
    }

    /// Adopt one staged v4 candidate after the outbound gates pass:
    /// install the candidate, refresh the typed replay cache, and return
    /// the publication verdict. A refused candidate leaves `self` (state,
    /// watermarks, cache) exactly unchanged.
    #[cfg(test)]
    pub(crate) fn finalize_v4_candidate(
        &mut self,
        staged: WorldState,
        report: &WorldReportV4,
        digest: u64,
        reply: WorldReplyV4,
    ) -> V4RunOutcome {
        self.finalize_v4_candidate_with_gate_and_prepare(
            staged,
            report,
            digest,
            reply,
            &[],
            &StructuralCoreAdmissionGate,
            |_| Ok::<(), std::convert::Infallible>(()),
        )
        .unwrap_or_else(|e| match e {})
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finalize_v4_candidate_with_gate_and_prepare<E, G: CoreAdmissionGate + ?Sized>(
        &mut self,
        staged: WorldState,
        report: &WorldReportV4,
        digest: u64,
        reply: WorldReplyV4,
        dispositions: &[BatchDisposition],
        gate: &G,
        prepare: impl FnOnce(PreparedCandidate<'_>) -> Result<(), E>,
    ) -> Result<V4RunOutcome, E> {
        self.finalize_v4_candidate_with_roster_feedback(
            staged,
            report,
            digest,
            reply,
            dispositions,
            gate,
            None,
            prepare,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn finalize_v4_candidate_with_roster_feedback<E, G: CoreAdmissionGate + ?Sized>(
        &mut self,
        mut staged: WorldState,
        report: &WorldReportV4,
        digest: u64,
        reply: WorldReplyV4,
        dispositions: &[BatchDisposition],
        gate: &G,
        feedback: Option<&RosterFeedback>,
        prepare: impl FnOnce(PreparedCandidate<'_>) -> Result<(), E>,
    ) -> Result<V4RunOutcome, E> {
        if staged.workspace.members.len() > crate::world_report::MAX_RETAINED_MEMBERSHIPS {
            return Ok(V4RunOutcome::Rejected {
                kind: KIND_SNAPSHOT,
                message: "membership-over-cap",
            });
        }
        let exact_inner = match Self::refuse_outbound_v4(&reply, gate) {
            Ok(exact) => exact,
            Err((kind, message)) => return Ok(V4RunOutcome::Rejected { kind, message }),
        };
        // Staged runs can mint automatic born-clear obligations without a
        // command; enforce the shared 1,024-slot cap and future terminal
        // byte budget on `staged` before adoption without double-counting
        // already-encoded `operation_settled`/`switch_settled` sections.
        let staged_op_slots = staged.operation_settle_slot_count(None, &[]);
        if staged_op_slots > crate::world_report::MAX_REPLY_OPERATIONS {
            return Ok(V4RunOutcome::Rejected {
                kind: KIND_SNAPSHOT,
                message: "section-over-cap",
            });
        }
        let (staged_switch_outputs, staged_switch_failed) = staged.switch_settle_counts();
        let future_facts = FutureReserveGateFacts {
            operation_slot_count: staged_op_slots,
            switch_output_count: staged_switch_outputs,
            switch_failed_count: staged_switch_failed,
        };
        match gate.check_future_reserve(&reply, &future_facts, exact_inner) {
            AdmissionGateDecision::Allow => {}
            AdmissionGateDecision::Refuse => {
                return Ok(V4RunOutcome::Rejected {
                    kind: KIND_REPLY_SIZE,
                    message: MSG_REPLY_SIZE,
                });
            }
            AdmissionGateDecision::Reject { kind, message } => {
                return Ok(V4RunOutcome::Rejected { kind, message });
            }
        }
        let active_gesture_id = staged.gesture.retained.as_ref().map(|retained| retained.id);
        let wake_candidate = WakePrecheckCandidate {
            reply: &reply,
            active_gesture_id,
            dispositions,
        };
        match gate.check_wake_precheck(&wake_candidate) {
            AdmissionGateDecision::Allow => {}
            AdmissionGateDecision::Refuse => {
                return Ok(V4RunOutcome::Rejected {
                    kind: KIND_REPLY_SIZE,
                    message: MSG_REPLY_SIZE,
                });
            }
            AdmissionGateDecision::Reject { kind, message } => {
                return Ok(V4RunOutcome::Rejected { kind, message });
            }
        }
        if let Err((kind, message)) =
            Self::validate_prepared_reply_v4(&reply, self.connection.capabilities(), gate)
        {
            return Ok(V4RunOutcome::Rejected { kind, message });
        }
        let candidate = PreparedCandidate {
            reply: ValidatedReply::new_validated(&reply),
            active_gesture_id,
            dispositions,
            shared_roster_plan: staged.settlement.shared_roster.plan(),
        };
        prepare(candidate)?;
        staged
            .settlement
            .emit_staged_dropped_settlement_trace(&reply);
        staged.emit_published_settlement_trace(
            self.replay.as_ref().map(|replay| &replay.reply),
            &reply,
            false,
        );
        staged.emit_published_drag_preview_trace(&reply, false);
        let consumed_reply = reply.command_consumed;
        self.world = staged;
        // Typed replay cache: mirrors the legacy finish tail, including
        // consumed-command reports; rejected candidates never reach here.
        let observation_seq = match &self.replay {
            Some(prev) => prev.observation_seq.max(report.observation_seq),
            None => report.observation_seq,
        };
        self.replay = Some(AdmittedReplayV4 {
            request_seq: report.request_seq,
            digest,
            observation_seq,
            report: report.clone(),
            residual: gate.capture_residual(),
            roster_feedback: feedback.cloned(),
            reply: reply.clone(),
        });
        if consumed_reply {
            Ok(V4RunOutcome::CommandConsumed(reply))
        } else {
            Ok(V4RunOutcome::Accepted(reply))
        }
    }

    /// Empty v4 bootstrap reply for pre-ready reports: mirrors the current
    /// revision, advances nothing, and carries no authority. The supplied
    /// correlation stamps the admitted request identity; the empty
    /// presentation (hidden border/underlay, empty pager) is constructed
    /// explicitly so retained staged/pager state can never leak in.
    fn bootstrap_reply_v4(
        state_revision: u64,
        report: &WorldReportV4,
        correlation: &CorrelationId,
    ) -> WorldReplyV4 {
        // Bootstrap publishes no intent decisions: the empty `intent_desired`
        // below trivially satisfies `validate_intent_desired_reason`, and the
        // fixed empty shape trivially satisfies the exact size bound, so the
        // pre-ready gate above stays the single enforcing boundary.
        let presentation = WorldPresentationV4 {
            foreground: String::new(),
            rev: state_revision,
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
            state_revision,
            ready: false,
            command_consumed: false,
            scopes: Vec::new(),
            workspaces: Vec::new(),
            outputs: Vec::new(),
            diagnostics: Vec::new(),
            windows: Vec::new(),
            transfers: Vec::new(),
            effects: Vec::new(),
            focus_request: None,
            drag_preview: None,
            gesture_settled: None,
            operation_settled: Vec::new(),
            native_creations: Vec::new(),
            cursor_warp: None,
            presentation,
            intent_desired: Vec::new(),
            switch_settled: Vec::new(),
        }
    }
}

#[cfg(test)]
impl WorldEngine {
    pub(crate) fn handle_report_v4_for_test(
        &mut self,
        report: &WorldReportV4,
    ) -> Result<WorldReplyV4, (&'static str, &'static str)> {
        let owner = match self.connection.connection_owner.clone() {
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
