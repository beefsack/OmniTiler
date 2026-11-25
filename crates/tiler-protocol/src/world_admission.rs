//! Checked protocol admission gate implementing [`CoreAdmissionGate`] over
//! borrowed typed reports, replies, and count-only world facts.
//!
//! Keeps the existing preflight/request sizing formulas for this cut while
//! replacing the estimate-only final refusal with exact canonical byte
//! counting via [`count_reply_v4_bytes`], handing off the ephemeral inner byte
//! count from `check_exact_inner` to `check_future_reserve` without retained
//! state and preserving the final-prepare writer validation checkpoint.

use std::cell::Cell;
use std::sync::Arc;

use crate::world_admission_sizing::{
    MAX_OPERATION_EVENT_BYTES, max_intent_desired_bound, max_intent_save_batch_bound_v4,
    max_operation_settle_reserve, max_switch_settle_reserve, reply_upper_bound_v4,
    report_estimate_v4_with_residual,
};
use crate::world_protocol::{DecodedReportV4, WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES};
use crate::world_reply_write::{
    count_operation_settled_rows_bytes, count_reply_v4_bytes, count_switch_settled_section_bytes,
};
use crate::world_report_residual::WorldReportResidual;
use crate::world_roster_wire::supports_roster;
use tiler_core::world_engine::{
    AdmissionGateDecision, CoreAdmissionGate, ExactInnerCheck, ExactInnerDecision,
    FutureReserveGateFacts, PreflightGateFacts, PreparedCandidate, ReplayResidualToken,
    RetainedResidual, TransitionResult, WakePrecheckCandidate, WorldEngine,
};
use tiler_core::world_report::{WorldCommand, WorldReplyV4, WorldReportV4};

/// Scoped checked protocol admission gate over borrowed typed reports and
/// replies.
pub struct ProtocolAdmissionGate<'a> {
    max_request_bytes: usize,
    max_reply_bytes: usize,
    residual: Option<&'a Arc<WorldReportResidual>>,
    wake_precheck: Option<&'a dyn Fn(&WakePrecheckCandidate<'_>) -> bool>,
    roster_host_mismatch: Cell<bool>,
    wake_exhausted: Cell<bool>,
    last_invalid_reply: Cell<Option<&'static str>>,
    #[cfg(test)]
    count_calls: Cell<usize>,
}

impl std::fmt::Debug for ProtocolAdmissionGate<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtocolAdmissionGate")
            .field("max_request_bytes", &self.max_request_bytes)
            .field("max_reply_bytes", &self.max_reply_bytes)
            .field("has_residual", &self.residual.is_some())
            .field("has_wake_precheck", &self.wake_precheck.is_some())
            .field("wake_exhausted", &self.wake_exhausted.get())
            .field("last_invalid_reply", &self.last_invalid_reply.get())
            .finish()
    }
}

impl Default for ProtocolAdmissionGate<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> ProtocolAdmissionGate<'a> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_request_bytes: WORLD_MAX_REQUEST_BYTES,
            max_reply_bytes: WORLD_MAX_REPLY_BYTES,
            residual: None,
            wake_precheck: None,
            roster_host_mismatch: Cell::new(false),
            wake_exhausted: Cell::new(false),
            last_invalid_reply: Cell::new(None),
            #[cfg(test)]
            count_calls: Cell::new(0),
        }
    }

    /// Construct a checked gate bound to the decoded report's protocol-owned
    /// residual evidence for exact request estimation and exact replay
    /// discrimination/retention.
    #[must_use]
    pub fn with_residual(residual: Option<&'a Arc<WorldReportResidual>>) -> Self {
        Self {
            max_request_bytes: WORLD_MAX_REQUEST_BYTES,
            max_reply_bytes: WORLD_MAX_REPLY_BYTES,
            residual: residual.filter(|r| !r.is_empty()),
            wake_precheck: None,
            roster_host_mismatch: Cell::new(false),
            wake_exhausted: Cell::new(false),
            last_invalid_reply: Cell::new(None),
            #[cfg(test)]
            count_calls: Cell::new(0),
        }
    }

    /// Dispatch using the same retained wire evidence for typed proofs, size
    /// admission and replay identity.
    pub fn dispatch_with_prepare<E>(
        &self,
        engine: &mut WorldEngine,
        report: &WorldReportV4,
        queued: &[(u64, WorldCommand)],
        prepare: impl FnOnce(PreparedCandidate<'_>) -> Result<(), E>,
    ) -> Result<TransitionResult, E> {
        let feedback = self.residual.and_then(|r| r.roster_feedback.as_ref());
        self.roster_host_mismatch.set(
            feedback.is_some()
                && (!engine.capabilities().is_some_and(supports_roster)
                    || engine.settings().workspace_mode
                        == tiler_core::workspace_scope::WorkspaceMode::Shared),
        );
        let result = match feedback {
            Some(feedback) => engine.handle_report_with_roster_feedback_and_prepare(
                report,
                queued,
                self,
                feedback.proof(),
                prepare,
            ),
            None => engine.handle_report_with_gate_and_prepare(report, queued, self, prepare),
        };
        self.roster_host_mismatch.set(false);
        result
    }

    /// Attach a pre-validation wake-epoch precheck callback to run after
    /// exact/future reserve and before mandatory semantic/host validation.
    #[must_use]
    pub fn with_wake_precheck(
        mut self,
        wake_precheck: &'a dyn Fn(&WakePrecheckCandidate<'_>) -> bool,
    ) -> Self {
        self.wake_precheck = Some(wake_precheck);
        self
    }

    /// Take whether the wake-epoch precheck refused the candidate.
    #[must_use]
    pub fn take_wake_exhausted(&self) -> bool {
        self.wake_exhausted.replace(false)
    }

    /// Take any fixed semantic or host-capability reply-validation rule
    /// recorded by the engine's outbound gate.
    #[must_use]
    pub fn take_invalid_reply(&self) -> Option<&'static str> {
        self.last_invalid_reply.take()
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn canonical_count_calls(&self) -> usize {
        self.count_calls.get()
    }

    fn count_canonical(&self, reply: &WorldReplyV4) -> usize {
        #[cfg(test)]
        self.count_calls
            .set(self.count_calls.get().saturating_add(1));
        count_reply_v4_bytes(reply)
    }
}

/// Evaluate one typed report through [`WorldEngine::handle_report_with_gate_and_prepare`]
/// under the checked [`ProtocolAdmissionGate`].
pub fn handle_report_checked(
    engine: &mut WorldEngine,
    report: &WorldReportV4,
    queued: &[(u64, WorldCommand)],
) -> TransitionResult {
    let gate = ProtocolAdmissionGate::new();
    engine
        .handle_report_with_gate_and_prepare(report, queued, &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {})
}

/// Evaluate one decoded v4 report (carrying protocol-owned residual evidence)
/// through [`WorldEngine::handle_report_with_gate_and_prepare`] under
/// [`ProtocolAdmissionGate::with_residual`].
pub fn handle_report_decoded_checked(
    engine: &mut WorldEngine,
    decoded: &DecodedReportV4,
    queued: &[(u64, WorldCommand)],
) -> TransitionResult {
    let gate = ProtocolAdmissionGate::with_residual(decoded.residual.as_ref());
    gate.dispatch_with_prepare(engine, &decoded.report, queued, |_| {
        Ok::<(), std::convert::Infallible>(())
    })
    .unwrap_or_else(|e| match e {})
}

impl CoreAdmissionGate for ProtocolAdmissionGate<'_> {
    fn check_request_size(&self, report: &WorldReportV4) -> AdmissionGateDecision {
        if self.roster_host_mismatch.get() {
            return AdmissionGateDecision::Reject {
                kind: "snapshot-invalid",
                message: "roster-host-mismatch",
            };
        }
        if report.action_outputs.len() > tiler_core::world_report::MAX_WORLD_SCREENS {
            return AdmissionGateDecision::Reject {
                kind: "snapshot-invalid",
                message: "section-over-cap",
            };
        }
        if report_estimate_v4_with_residual(report, self.residual.map(Arc::as_ref))
            > self.max_request_bytes
        {
            AdmissionGateDecision::Refuse
        } else {
            AdmissionGateDecision::Allow
        }
    }

    fn residual_matches(&self, retained: Option<&RetainedResidual>) -> bool {
        match (self.residual, retained) {
            (None, None) => true,
            (Some(incoming), Some(prev)) => prev
                .0
                .as_any()
                .downcast_ref::<WorldReportResidual>()
                .is_some_and(|rhs| incoming.as_ref() == rhs),
            _ => false,
        }
    }

    fn capture_residual(&self) -> Option<RetainedResidual> {
        self.residual
            .map(|arc| RetainedResidual(Arc::clone(arc) as Arc<dyn ReplayResidualToken>))
    }

    fn check_preflight(
        &self,
        report: &WorldReportV4,
        facts: &PreflightGateFacts<'_>,
    ) -> AdmissionGateDecision {
        let total = reply_upper_bound_v4(
            report,
            facts.retained_settlement_count,
            facts.domain_count,
            facts.screen_count,
            facts.staged_window_count,
            facts.retained_publish_footprint,
        )
        .saturating_add(max_intent_desired_bound(facts.intent_desired_count))
        .saturating_add(max_operation_settle_reserve(facts.operation_slot_count))
        .saturating_add(max_switch_settle_reserve(
            facts.switch_output_count,
            facts.switch_failed_count,
        ))
        .saturating_add(max_switch_settle_reserve(facts.issuing_selection_count, 0))
        .saturating_add(max_switch_settle_reserve(facts.policy_terminal_count, 0))
        .saturating_add(max_intent_save_batch_bound_v4(facts.effective_queued));
        if total > self.max_reply_bytes {
            AdmissionGateDecision::Refuse
        } else {
            AdmissionGateDecision::Allow
        }
    }

    fn check_exact_inner(&self, reply: &WorldReplyV4) -> ExactInnerDecision {
        let bytes = self.count_canonical(reply);
        if bytes > self.max_reply_bytes {
            ExactInnerDecision::Refuse
        } else {
            ExactInnerDecision::Allow(ExactInnerCheck::new(bytes))
        }
    }

    fn check_future_reserve(
        &self,
        reply: &WorldReplyV4,
        facts: &FutureReserveGateFacts,
        exact: ExactInnerCheck,
    ) -> AdmissionGateDecision {
        let future_op_delta = facts
            .operation_slot_count
            .saturating_mul(MAX_OPERATION_EVENT_BYTES)
            .saturating_sub(count_operation_settled_rows_bytes(&reply.operation_settled));
        let future_switch_delta =
            max_switch_settle_reserve(facts.switch_output_count, facts.switch_failed_count)
                .saturating_sub(count_switch_settled_section_bytes(&reply.switch_settled));
        let future_reserve = future_op_delta.saturating_add(future_switch_delta);
        if exact.bytes().saturating_add(future_reserve) > self.max_reply_bytes {
            AdmissionGateDecision::Refuse
        } else {
            AdmissionGateDecision::Allow
        }
    }

    fn check_wake_precheck(&self, candidate: &WakePrecheckCandidate<'_>) -> AdmissionGateDecision {
        if let Some(wake_precheck) = self.wake_precheck
            && !wake_precheck(candidate)
        {
            self.wake_exhausted.set(true);
            return AdmissionGateDecision::Reject {
                kind: "sequence-exhausted",
                message: "wake epoch exhausted",
            };
        }
        AdmissionGateDecision::Allow
    }

    fn on_invalid_reply(&self, rule: &'static str) {
        self.last_invalid_reply.set(Some(rule));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_reply_write::count_reply_v4;
    use tiler_core::directional::{OutputId, WindowId};
    use tiler_core::geometry::Rect;
    use tiler_core::world_presentation::{GroupHideReason, PresentationUnderlay};
    use tiler_core::world_report::{
        AfterShown, AfterShownMember, BorderPresentationV4, FocusRequest, FocusRequestCause,
        ScopeAuthority, StateProperty, StateTarget, WORLD_CONTRACT_VERSION_V4, WindowDesire,
        WorldPresentationV4,
    };

    fn base_reply() -> WorldReplyV4 {
        WorldReplyV4 {
            contract_version: WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: "adapter-a".to_owned(),
            core_generation: "gen-a".to_owned(),
            request_id: "req-a".to_owned(),
            based_on_observation_seq: 1,
            state_revision: 1,
            ready: true,
            command_consumed: false,
            scopes: Vec::new(),
            workspaces: Vec::new(),
            outputs: Vec::new(),
            diagnostics: Vec::new(),
            windows: Vec::new(),
            transfers: Vec::new(),
            focus_request: None,
            drag_preview: None,
            gesture_settled: None,
            operation_settled: Vec::new(),
            native_creations: Vec::new(),
            warp_request: None,
            presentation: WorldPresentationV4 {
                foreground: String::new(),
                rev: 1,
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
            },
            intent_desired: Vec::new(),
            switch_settled: Vec::new(),
        }
    }

    fn pad_win(i: usize) -> WindowDesire {
        WindowDesire {
            lifetime: WindowId(format!("w-{i:05}-{}", "x".repeat(120))),
            output: OutputId(format!("o-{}", "y".repeat(126))),
            workspace: format!("ws-{}", "z".repeat(125)),
            allocation: Some(Rect {
                x: 100,
                y: 100,
                w: 1000,
                h: 1000,
            }),
            effective_frame: Some(Rect {
                x: 100,
                y: 100,
                w: 1000,
                h: 1000,
            }),
            frame_authority: ScopeAuthority::Controlled,
            membership_authority: ScopeAuthority::Controlled,
            floating: false,
            float_origin: None,
            frame_operation: Some(format!("op-f-{i:05}-{}", "f".repeat(115))),
            command_seq: None,
            visibility: None,
            visibility_authority: ScopeAuthority::Untouched,
            state: vec![
                StateTarget {
                    property: StateProperty::Fullscreen,
                    value: false,
                    authority: ScopeAuthority::Untouched,
                    command_seq: None,
                    automatic_clear: false,
                },
                StateTarget {
                    property: StateProperty::Maximized,
                    value: false,
                    authority: ScopeAuthority::Untouched,
                    command_seq: None,
                    automatic_clear: false,
                },
                StateTarget {
                    property: StateProperty::Sticky,
                    value: false,
                    authority: ScopeAuthority::Untouched,
                    command_seq: None,
                    automatic_clear: false,
                },
                StateTarget {
                    property: StateProperty::Above,
                    value: false,
                    authority: ScopeAuthority::Untouched,
                    command_seq: None,
                    automatic_clear: false,
                },
            ],
        }
    }

    #[test]
    fn estimate_only_headroom_discriminator_fits_exact_and_refuses_genuine_limits() {
        let mut reply = base_reply();
        let members: Vec<AfterShownMember> = (0..100)
            .map(|i| AfterShownMember {
                win: WindowId(format!("w-{i}")),
                out: OutputId("o".to_owned()),
                ws: "w".to_owned(),
                frame: None,
            })
            .collect();
        reply.focus_request = Some(FocusRequest {
            id: "f-1".to_owned(),
            target: WindowId("w-0".to_owned()),
            issued_revision: 1,
            cause: FocusRequestCause::ExplicitCommand,
            output: OutputId("o".to_owned()),
            workspace: "w".to_owned(),
            cmd_seq: Some(1),
            after_shown: Some(AfterShown {
                op: "op-1".to_owned(),
                obs: 1,
                members,
                views: Vec::new(),
                selected: Vec::new(),
                departing: Vec::new(),
            }),
        });

        reply.windows.push(pad_win(0));
        let exact_one = count_reply_v4(&reply).expect("one row counts");
        reply.windows.push(pad_win(1));
        let exact_two = count_reply_v4(&reply).expect("two rows count");
        let step = exact_two - exact_one;
        let future_facts = FutureReserveGateFacts {
            operation_slot_count: 1,
            switch_output_count: 0,
            switch_failed_count: 0,
        };
        let future_reserve = max_operation_settle_reserve(future_facts.operation_slot_count);
        // Leave enough room for exact + future_reserve, while the historical legacy
        // whole-reply estimate + future_reserve exceeded WORLD_MAX_REPLY_BYTES due to
        // afterShown's worst-case per-member estimate.
        let target_exact = WORLD_MAX_REPLY_BYTES - future_reserve - 1_000;
        let extra = (target_exact - exact_two) / step;
        reply.windows.extend((2..2 + extra).map(pad_win));

        let exact = count_reply_v4(&reply).expect("discriminator reply counts");
        // Historical legacy `reply_estimate_v4(&reply)` count captured before removal
        // for this exact witness (100 short afterShown members + 3241 pad_win rows).
        const HISTORICAL_LEGACY_ESTIMATE: usize = 4_239_352;
        assert_eq!(reply.windows.len(), 3_634);
        assert_eq!(
            reply
                .focus_request
                .as_ref()
                .and_then(|req| req.after_shown.as_ref())
                .map(|shown| shown.members.len()),
            Some(100)
        );
        // Exact serialized size after retiring `"effects":[],` and per-row `"clear_op":null,`.
        assert_eq!(exact, 4_191_823);
        assert!(
            exact.saturating_add(future_reserve) <= WORLD_MAX_REPLY_BYTES,
            "exact + future reserve must fit under 4 MiB"
        );
        assert!(
            HISTORICAL_LEGACY_ESTIMATE.saturating_add(future_reserve) > WORLD_MAX_REPLY_BYTES,
            "legacy estimate + future reserve must exceed 4 MiB"
        );

        let gate = ProtocolAdmissionGate::new();
        let exact_check = match gate.check_exact_inner(&reply) {
            ExactInnerDecision::Allow(exact) => exact,
            other => panic!("expected Allow, got {other:?}"),
        };
        assert_eq!(
            gate.check_future_reserve(&reply, &future_facts, exact_check),
            AdmissionGateDecision::Allow
        );
        assert_eq!(gate.canonical_count_calls(), 1);

        // Genuine future-reserve overflow still refuses.
        let over_future = FutureReserveGateFacts {
            operation_slot_count: 10,
            switch_output_count: 0,
            switch_failed_count: 0,
        };
        let exact_check = match gate.check_exact_inner(&reply) {
            ExactInnerDecision::Allow(exact) => exact,
            other => panic!("expected Allow, got {other:?}"),
        };
        assert_eq!(
            gate.check_future_reserve(&reply, &over_future, exact_check),
            AdmissionGateDecision::Refuse
        );

        // Genuine exact-inner overflow still refuses.
        let mut over_reply = reply;
        over_reply
            .windows
            .extend((2 + extra..2 + extra + 5).map(pad_win));
        assert_eq!(
            gate.check_exact_inner(&over_reply),
            ExactInnerDecision::Refuse
        );
    }

    #[test]
    fn exact_inner_ticket_consumed_by_future_reserve_without_recounting() {
        let reply_a = base_reply();
        let reply_b = base_reply();

        let gate = ProtocolAdmissionGate::new();
        let facts = FutureReserveGateFacts {
            operation_slot_count: 0,
            switch_output_count: 0,
            switch_failed_count: 0,
        };
        let exact_a = match gate.check_exact_inner(&reply_a) {
            ExactInnerDecision::Allow(exact) => exact,
            other => panic!("expected Allow, got {other:?}"),
        };
        assert_eq!(
            gate.check_future_reserve(&reply_a, &facts, exact_a),
            AdmissionGateDecision::Allow
        );
        assert_eq!(gate.canonical_count_calls(), 1);

        let exact_b = match gate.check_exact_inner(&reply_b) {
            ExactInnerDecision::Allow(exact) => exact,
            other => panic!("expected Allow, got {other:?}"),
        };
        assert_eq!(
            gate.check_future_reserve(&reply_b, &facts, exact_b),
            AdmissionGateDecision::Allow
        );
        assert_eq!(gate.canonical_count_calls(), 2);
    }

    #[test]
    fn writer_only_validation_error_defers_to_checked_writer_checkpoint() {
        use crate::world_reply_write::{BrokerReplyBind, count_reply_v4, write_bound_reply_v4};

        let mut bad = base_reply();
        bad.presentation.border.width = Some(-1);

        let gate = ProtocolAdmissionGate::new();
        let exact = match gate.check_exact_inner(&bad) {
            ExactInnerDecision::Allow(exact) => exact,
            other => panic!("writer-only error must not fail exact_inner early: {other:?}"),
        };
        let facts = FutureReserveGateFacts {
            operation_slot_count: 0,
            switch_output_count: 0,
            switch_failed_count: 0,
        };
        assert_eq!(
            gate.check_future_reserve(&bad, &facts, exact),
            AdmissionGateDecision::Allow
        );

        let count_err = count_reply_v4(&bad).expect_err("checked counter still validates");
        assert_eq!(count_err.kind, "unknown-value");
        assert_eq!(count_err.message, "request contains an unknown value");

        let bind = BrokerReplyBind {
            source: "src-1",
            native_invalid: 1,
            clock: 1,
            gesture_assoc: None,
            gesture_settled_assoc: None,
        };
        let bound_err =
            write_bound_reply_v4(&bad, None, &bind).expect_err("bound writer still validates");
        assert_eq!(bound_err.kind, "unknown-value");
        assert_eq!(bound_err.message, "request contains an unknown value");
    }

    #[test]
    fn operation_future_reserve_credits_emitted_row_and_refuses_headroom_crossing() {
        use tiler_core::world_report::{OperationEvent, OperationKind, OperationOutcome};

        let mut reply = base_reply();
        reply.operation_settled.push(OperationEvent {
            operation_id: "op-0".to_owned(),
            window: WindowId("w-0".to_owned()),
            kind: OperationKind::Clear,
            outcome: OperationOutcome::ClearContinuedTiled,
            cause: None,
        });

        let exact = count_reply_v4(&reply).expect("reply with emitted op counts");
        let raw_one_slot = max_operation_settle_reserve(1);
        let emitted_rows = count_operation_settled_rows_bytes(&reply.operation_settled);
        let credited_delta = MAX_OPERATION_EVENT_BYTES.saturating_sub(emitted_rows);
        assert_eq!(
            credited_delta,
            MAX_OPERATION_EVENT_BYTES - emitted_rows,
            "terminal headroom delta excludes wrapper and matches legacy row budget minus emitted row"
        );
        assert_eq!(
            count_switch_settled_section_bytes(&[]),
            19,
            "empty switchSettled section matches legacy 19-byte wrapper"
        );
        assert!(
            credited_delta < raw_one_slot,
            "emitted operation row must reduce future operation reserve"
        );

        let mut gate = ProtocolAdmissionGate::new();
        gate.max_reply_bytes = exact.saturating_add(credited_delta);
        assert!(
            exact.saturating_add(raw_one_slot) > gate.max_reply_bytes,
            "without emitted-row credit, raw slot reserve must exceed budget"
        );

        // Fitting neighbor: same-key emitted-row credit fits within budget.
        let exact_check = match gate.check_exact_inner(&reply) {
            ExactInnerDecision::Allow(exact) => exact,
            other => panic!("expected Allow, got {other:?}"),
        };
        let same_key_facts = FutureReserveGateFacts {
            operation_slot_count: 1,
            switch_output_count: 0,
            switch_failed_count: 0,
        };
        assert_eq!(
            gate.check_future_reserve(&reply, &same_key_facts, exact_check),
            AdmissionGateDecision::Allow
        );

        // True headroom crossing: 30 additional reserved slots exceed budget.
        let exact_check = match gate.check_exact_inner(&reply) {
            ExactInnerDecision::Allow(exact) => exact,
            other => panic!("expected Allow, got {other:?}"),
        };
        let born_reserve_facts = FutureReserveGateFacts {
            operation_slot_count: 31,
            switch_output_count: 0,
            switch_failed_count: 0,
        };
        assert_eq!(
            gate.check_future_reserve(&reply, &born_reserve_facts, exact_check),
            AdmissionGateDecision::Refuse
        );
    }

    fn base_report() -> WorldReportV4 {
        use tiler_core::world_report::{PresentationIngressV4, PresentationRowV4};
        WorldReportV4 {
            contract_version: WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: "adapter-a".to_owned(),
            core_generation: "gen-a".to_owned(),
            observation_seq: 1,
            request_seq: 1,
            request_id: "req-a".to_owned(),
            command_seq: None,
            command: None,
            screens: Vec::new(),
            held_screens: Vec::new(),
            windows: Vec::new(),
            held_windows: Vec::new(),
            closed: Vec::new(),
            native: Vec::new(),
            held_native: Vec::new(),
            cursor: None,
            cursor_hit: None,
            focused_global: String::new(),
            foreground_known: false,
            focus_changes: None,
            action_foreground: None,
            action_cached: None,
            action_outputs: Vec::new(),
            primary_output: None,
            inventory_complete: true,
            gesture_barrier: false,
            gesture: None,
            monotonic_ms: 0,
            frames: Vec::new(),
            member_attempts: Vec::new(),
            view_attempts: Vec::new(),
            creation_attempts: Vec::new(),
            held_attempts: Vec::new(),
            state_attempts: Vec::new(),
            visibility_attempts: Vec::new(),
            focus_feedback: Vec::new(),
            presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
            intent_records: Vec::new(),
            renderer_input: None,
            pointer_activity: None,
            warp_spent: None,
        }
    }

    #[test]
    fn intent_save_canonical_widths_fit_bound_constant() {
        use crate::world_admission_rows::json_string_len;
        use crate::world_admission_sizing::{
            MAX_INTENT_DESIRED_DATA_JSON, MAX_INTENT_DESIRED_ROW_BYTES,
        };
        use tiler_core::intent_data::{IntentData, encode_intent_data};
        use tiler_core::world_report::IntentDesire;

        let base_bytes = count_reply_v4(&base_reply()).expect("base reply counts");

        let staged_sticky_shapes = [
            IntentData::new(Some(true), Some(true)),
            IntentData::new(Some(true), Some(false)),
            IntentData::new(Some(true), None),
            IntentData::new(Some(false), None),
        ];
        let max_staged_len = staged_sticky_shapes
            .iter()
            .map(|data| json_string_len(&encode_intent_data(data)))
            .max()
            .expect("non-empty staged shapes");
        assert_eq!(max_staged_len, 48);

        let all_nine_shapes = [
            (IntentData::new(None, None), 47),
            (IntentData::new(None, Some(false)), 48),
            (IntentData::new(None, Some(true)), 47),
            (IntentData::new(Some(false), None), 48),
            (IntentData::new(Some(false), Some(false)), 49),
            (IntentData::new(Some(false), Some(true)), 48),
            (IntentData::new(Some(true), None), 47),
            (IntentData::new(Some(true), Some(false)), 48),
            (IntentData::new(Some(true), Some(true)), 47),
        ];
        let mut widest = 0usize;
        for (data, expected_len) in all_nine_shapes {
            let len = json_string_len(&encode_intent_data(&data));
            assert_eq!(len, expected_len, "{data:?}");
            widest = widest.max(len);
        }
        assert_eq!(widest, MAX_INTENT_DESIRED_DATA_JSON);

        let mut with_max_row = base_reply();
        with_max_row.intent_desired.push(IntentDesire {
            win: WindowId("w".repeat(128)),
            data: Some(IntentData::new(Some(false), Some(false))),
        });
        let row_delta = count_reply_v4(&with_max_row)
            .expect("max row reply counts")
            .saturating_sub(base_bytes);
        assert!(row_delta <= MAX_INTENT_DESIRED_ROW_BYTES);

        let mut with_three_max = base_reply();
        with_three_max.intent_desired = (0..3)
            .map(|i| IntentDesire {
                win: WindowId(format!("w{i}{}", "w".repeat(126))),
                data: Some(IntentData::new(Some(false), Some(false))),
            })
            .collect();
        let three_delta = count_reply_v4(&with_three_max)
            .expect("three max rows reply counts")
            .saturating_sub(base_bytes);
        assert!(
            three_delta <= 19 + max_intent_desired_bound(3),
            "three max rows {three_delta} must fit section + bound {}",
            19 + max_intent_desired_bound(3)
        );
    }

    #[test]
    fn fullscreen_and_sticky_command_estimates_match_exact_wire() {
        use crate::world_admission_rows::command_estimate_owned;
        use tiler_core::bounds::{MAX_OPAQUE_ID_LEN, is_opaque_id};
        use tiler_core::world_report::WorldCommand;

        let max_id = "w".repeat(MAX_OPAQUE_ID_LEN);
        assert!(is_opaque_id(&max_id));

        let cases = [
            (
                WorldCommand::SetFullscreen {
                    window: tiler_core::directional::WindowSubject::Window(WindowId(
                        "win-a".to_owned(),
                    )),
                    value: true,
                },
                "{\"op\":\"set-fullscreen\",\"win\":\"win-a\",\"value\":true}".to_owned(),
            ),
            (
                WorldCommand::SetFullscreen {
                    window: tiler_core::directional::WindowSubject::Window(WindowId(
                        "w".to_owned(),
                    )),
                    value: false,
                },
                "{\"op\":\"set-fullscreen\",\"win\":\"w\",\"value\":false}".to_owned(),
            ),
            (
                WorldCommand::ToggleFullscreen {
                    window: tiler_core::directional::WindowSubject::Window(WindowId(
                        "win-a".to_owned(),
                    )),
                },
                "{\"op\":\"toggle-fullscreen\",\"win\":\"win-a\"}".to_owned(),
            ),
            (
                WorldCommand::SetSticky {
                    window: tiler_core::directional::WindowSubject::Window(WindowId(
                        "win-a".to_owned(),
                    )),
                    value: true,
                },
                "{\"op\":\"set-sticky\",\"win\":\"win-a\",\"value\":true}".to_owned(),
            ),
            (
                WorldCommand::ToggleSticky {
                    window: tiler_core::directional::WindowSubject::Window(WindowId(
                        "w".to_owned(),
                    )),
                },
                "{\"op\":\"toggle-sticky\",\"win\":\"w\"}".to_owned(),
            ),
            (
                WorldCommand::ToggleSticky {
                    window: tiler_core::directional::WindowSubject::Window(WindowId(
                        max_id.clone(),
                    )),
                },
                format!("{{\"op\":\"toggle-sticky\",\"win\":\"{max_id}\"}}"),
            ),
        ];
        for (cmd, expected_wire) in cases {
            assert_eq!(command_estimate_owned(&cmd), expected_wire.len(), "{cmd:?}");
        }
    }

    #[test]
    fn max_intent_save_bound_covers_save_commands_and_excludes_others() {
        use crate::world_admission_sizing::max_intent_save_bound_v4;
        use tiler_core::boundary::CoreCommand;
        use tiler_core::intent_data::IntentData;
        use tiler_core::world_report::{IntentDesire, WorldCommand};

        let base_bytes = count_reply_v4(&base_reply()).expect("base reply counts");
        let row_delta_for = |id: &str| {
            let mut reply = base_reply();
            reply.intent_desired.push(IntentDesire {
                win: WindowId(id.to_owned()),
                data: Some(IntentData::new(Some(false), Some(false))),
            });
            count_reply_v4_bytes(&reply).saturating_sub(base_bytes)
        };

        let with_cmd = |command: Option<WorldCommand>| {
            let mut report = base_report();
            report.command_seq = command.as_ref().map(|_| 1);
            report.command = command;
            report
        };

        for id in ["win-a", "w\"in\\q", &"w".repeat(128)] {
            let expected_delta = row_delta_for(id);
            for cmd in [
                WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: tiler_core::directional::WindowSubject::Window(WindowId(id.to_owned())),
                    float_rect: None,
                }),
                WorldCommand::SetSticky {
                    window: tiler_core::directional::WindowSubject::Window(WindowId(id.to_owned())),
                    value: true,
                },
                WorldCommand::ToggleSticky {
                    window: tiler_core::directional::WindowSubject::Window(WindowId(id.to_owned())),
                },
            ] {
                let bound = max_intent_save_bound_v4(&with_cmd(Some(cmd)));
                assert!(
                    bound >= expected_delta,
                    "id={id:?} bound={bound} < delta={expected_delta}"
                );
            }
        }

        let implicit_toggle = with_cmd(Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: tiler_core::directional::WindowSubject::CurrentFocus,
            float_rect: None,
        })));
        let max_explicit = with_cmd(Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: tiler_core::directional::WindowSubject::Window(WindowId("w".repeat(128))),
            float_rect: None,
        })));
        assert_eq!(
            max_intent_save_bound_v4(&implicit_toggle),
            max_intent_save_bound_v4(&max_explicit),
            "implicit inline save reserves max-width window id"
        );

        assert_eq!(max_intent_save_bound_v4(&with_cmd(None)), 0);
        assert_eq!(
            max_intent_save_bound_v4(&with_cmd(Some(WorldCommand::Focus {
                window: tiler_core::directional::WindowSubject::Window(WindowId(
                    "win-a".to_owned()
                )),
                direction: Some(tiler_core::directional::Direction::Left),
            }))),
            0
        );
        assert_eq!(
            max_intent_save_bound_v4(&with_cmd(Some(WorldCommand::SelectTrailing {
                output: Some(OutputId("out-a".to_owned())),
            }))),
            0
        );
    }

    #[test]
    fn max_intent_save_batch_bound_covers_distinct_and_dedups_same_key() {
        use crate::world_admission_sizing::{
            max_intent_save_batch_bound_v4, max_intent_save_bound_v4,
        };
        use tiler_core::boundary::CoreCommand;
        use tiler_core::intent_data::IntentData;
        use tiler_core::world_report::{IntentDesire, WorldCommand};

        let base_bytes = count_reply_v4(&base_reply()).expect("base reply counts");

        let mut queued: Vec<(u64, WorldCommand)> = (1..=100u64)
            .map(|n| {
                (
                    n,
                    WorldCommand::Operation(CoreCommand::ToggleFloat {
                        window: tiler_core::directional::WindowSubject::Window(WindowId(format!(
                            "w{:0127}",
                            n
                        ))),
                        float_rect: None,
                    }),
                )
            })
            .collect();
        for n in 101..=128u64 {
            queued.push((
                n,
                WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: tiler_core::directional::WindowSubject::CurrentFocus,
                    float_rect: None,
                }),
            ));
        }

        let mut max_reply = base_reply();
        max_reply.intent_desired = (0..128)
            .map(|i| IntentDesire {
                win: WindowId(format!("w{:0127}", i)),
                data: Some(IntentData::new(Some(false), Some(false))),
            })
            .collect();
        let exact_section_delta = count_reply_v4(&max_reply)
            .expect("128-row reply counts")
            .saturating_sub(base_bytes);
        let batch_bound = max_intent_save_batch_bound_v4(&queued);
        assert!(
            batch_bound >= exact_section_delta,
            "batch_bound={batch_bound} < exact_section_delta={exact_section_delta}"
        );

        let single_explicit = vec![(
            1,
            WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: tiler_core::directional::WindowSubject::Window(WindowId(
                    "win-a".to_owned(),
                )),
                float_rect: None,
            }),
        )];
        let repeated_explicit: Vec<(u64, WorldCommand)> = (1..=100u64)
            .map(|n| {
                (
                    n,
                    WorldCommand::Operation(CoreCommand::ToggleFloat {
                        window: tiler_core::directional::WindowSubject::Window(WindowId(
                            "win-a".to_owned(),
                        )),
                        float_rect: None,
                    }),
                )
            })
            .collect();
        assert_eq!(
            max_intent_save_batch_bound_v4(&repeated_explicit),
            max_intent_save_batch_bound_v4(&single_explicit),
            "repeated same-key explicit save deduplicates"
        );

        let non_saves = vec![
            (
                1,
                WorldCommand::Focus {
                    window: tiler_core::directional::WindowSubject::Window(WindowId(
                        "win-a".to_owned(),
                    )),
                    direction: None,
                },
            ),
            (
                2,
                WorldCommand::Focus {
                    window: tiler_core::directional::WindowSubject::Window(WindowId(
                        "win-b".to_owned(),
                    )),
                    direction: None,
                },
            ),
        ];
        assert_eq!(max_intent_save_batch_bound_v4(&non_saves), 0);

        // Two distinct saves: combined inline+batch bound covers canonical section,
        // and ProtocolAdmissionGate::check_preflight flips Allow -> Refuse at the
        // exact preflight boundary.
        let distinct_two = vec![
            (
                1,
                WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: tiler_core::directional::WindowSubject::Window(WindowId(
                        "win-a".to_owned(),
                    )),
                    float_rect: None,
                }),
            ),
            (
                2,
                WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: tiler_core::directional::WindowSubject::Window(WindowId(
                        "win-b".to_owned(),
                    )),
                    float_rect: None,
                }),
            ),
        ];
        let mut two_reply = base_reply();
        two_reply.intent_desired = vec![
            IntentDesire {
                win: WindowId("win-a".to_owned()),
                data: Some(IntentData::new(Some(true), None)),
            },
            IntentDesire {
                win: WindowId("win-b".to_owned()),
                data: Some(IntentData::new(Some(true), None)),
            },
        ];
        let two_delta = count_reply_v4(&two_reply)
            .expect("two-save reply counts")
            .saturating_sub(base_bytes);
        let report = base_report();
        let combined = max_intent_save_bound_v4(&report)
            .saturating_add(max_intent_save_batch_bound_v4(&distinct_two));
        assert!(combined >= two_delta);

        let base_bound = reply_upper_bound_v4(&report, 0, 0, 0, 0, 0);
        let two_bound = max_intent_save_batch_bound_v4(&distinct_two);
        let mut gate = ProtocolAdmissionGate::new();
        gate.max_reply_bytes = base_bound.saturating_add(two_bound);
        let facts_fit = PreflightGateFacts {
            effective_command: None,
            effective_queued: &distinct_two,
            retained_settlement_count: 0,
            domain_count: 0,
            screen_count: 0,
            staged_window_count: 0,
            retained_publish_footprint: 0,
            intent_desired_count: 0,
            operation_slot_count: 0,
            switch_output_count: 0,
            switch_failed_count: 0,
            issuing_selection_count: 0,
            policy_terminal_count: 0,
        };
        assert_eq!(
            gate.check_preflight(&report, &facts_fit),
            AdmissionGateDecision::Allow
        );
        gate.max_reply_bytes = gate.max_reply_bytes.saturating_sub(1);
        assert_eq!(
            gate.check_preflight(&report, &facts_fit),
            AdmissionGateDecision::Refuse
        );
    }

    #[test]
    fn switch_reserve_covers_canonical_writer_and_preflight_accounts_issuance_and_policy() {
        use crate::world_admission_sizing::{
            MAX_SWITCH_EVENT_FIXED_BYTES, MAX_SWITCH_FAILED_ROW_BYTES,
        };
        use tiler_core::world_report::{SwitchEvent, SwitchFocus, SwitchOutcome};

        let base_bytes = count_reply_v4(&base_reply()).expect("base reply counts");
        assert_eq!(max_switch_settle_reserve(0, 0), 0);
        assert_eq!(
            max_switch_settle_reserve(1, 1),
            19 + MAX_SWITCH_EVENT_FIXED_BYTES + MAX_SWITCH_FAILED_ROW_BYTES
        );

        let worst_empty = |idx: usize| SwitchEvent {
            output: OutputId(format!("o{idx:0127}")),
            operation_id: format!("op-{}", u64::MAX),
            outcome: SwitchOutcome::CompletedWithFailures,
            failed: Vec::new(),
            focus: SwitchFocus::Unproven,
            fallback: false,
        };
        for count in [1usize, 2usize] {
            let mut reply = base_reply();
            reply.switch_settled = (0..count).map(worst_empty).collect();
            let delta = count_reply_v4(&reply)
                .expect("switch reply counts")
                .saturating_sub(base_bytes);
            let reserve = max_switch_settle_reserve(count, 0);
            assert!(
                reserve >= delta,
                "reserve {reserve} must cover canonical delta {delta} for {count} empty events"
            );
        }

        let mut with_failed = base_reply();
        let mut ev = worst_empty(0);
        ev.failed = vec![WindowId("w".repeat(128))];
        with_failed.switch_settled.push(ev);
        let failed_delta = count_reply_v4(&with_failed)
            .expect("failed switch reply counts")
            .saturating_sub(base_bytes);
        assert!(
            max_switch_settle_reserve(1, 1) >= failed_delta,
            "reserve must cover canonical delta {failed_delta} with max failed id"
        );

        let report = base_report();
        let base_bound = reply_upper_bound_v4(&report, 0, 0, 0, 0, 0);
        let mixed_switch_budget = max_switch_settle_reserve(1, 2)
            .saturating_add(max_switch_settle_reserve(1, 0))
            .saturating_add(max_switch_settle_reserve(1, 0));
        let mut gate = ProtocolAdmissionGate::new();
        gate.max_reply_bytes = base_bound.saturating_add(mixed_switch_budget);
        let facts = PreflightGateFacts {
            effective_command: None,
            effective_queued: &[],
            retained_settlement_count: 0,
            domain_count: 0,
            screen_count: 0,
            staged_window_count: 0,
            retained_publish_footprint: 0,
            intent_desired_count: 0,
            operation_slot_count: 0,
            switch_output_count: 1,
            switch_failed_count: 2,
            issuing_selection_count: 1,
            policy_terminal_count: 1,
        };
        assert_eq!(
            gate.check_preflight(&report, &facts),
            AdmissionGateDecision::Allow
        );
        gate.max_reply_bytes = gate.max_reply_bytes.saturating_sub(1);
        assert_eq!(
            gate.check_preflight(&report, &facts),
            AdmissionGateDecision::Refuse
        );
    }

    #[test]
    fn switch_future_reserve_credits_emitted_section_and_refuses_headroom_crossing() {
        use tiler_core::world_report::{SwitchEvent, SwitchFocus, SwitchOutcome};

        let mut reply = base_reply();
        reply.switch_settled.push(SwitchEvent {
            output: OutputId("out-a".to_owned()),
            operation_id: "op-1".to_owned(),
            outcome: SwitchOutcome::CompletedWithFailures,
            failed: vec![WindowId("w-1".to_owned())],
            focus: SwitchFocus::Refused,
            fallback: false,
        });

        let exact = count_reply_v4(&reply).expect("reply with emitted switch counts");
        let raw_one_switch = max_switch_settle_reserve(1, 1);
        let emitted_section = count_switch_settled_section_bytes(&reply.switch_settled);
        let credited_delta = raw_one_switch.saturating_sub(emitted_section);
        assert!(credited_delta > 0, "short ids leave positive headroom");
        assert!(
            credited_delta < raw_one_switch,
            "emitted switch section must reduce future switch reserve"
        );

        let mut gate = ProtocolAdmissionGate::new();
        gate.max_reply_bytes = exact.saturating_add(credited_delta);
        assert!(
            exact.saturating_add(raw_one_switch) > gate.max_reply_bytes,
            "without emitted-section credit, raw switch reserve must exceed budget"
        );

        let exact_check = match gate.check_exact_inner(&reply) {
            ExactInnerDecision::Allow(exact) => exact,
            other => panic!("expected Allow, got {other:?}"),
        };
        let same_facts = FutureReserveGateFacts {
            operation_slot_count: 0,
            switch_output_count: 1,
            switch_failed_count: 1,
        };
        assert_eq!(
            gate.check_future_reserve(&reply, &same_facts, exact_check),
            AdmissionGateDecision::Allow
        );

        let exact_check = match gate.check_exact_inner(&reply) {
            ExactInnerDecision::Allow(exact) => exact,
            other => panic!("expected Allow, got {other:?}"),
        };
        let over_facts = FutureReserveGateFacts {
            operation_slot_count: 0,
            switch_output_count: 2,
            switch_failed_count: 1,
        };
        assert_eq!(
            gate.check_future_reserve(&reply, &over_facts, exact_check),
            AdmissionGateDecision::Refuse
        );
    }

    #[test]
    fn held_presentation_residual_discriminates_replay_whitespace_vs_changed_and_prepare_refusal() {
        use crate::world_protocol::decode_report_v4;
        use tiler_core::connection::{
            ApplicationIdentity, HandshakeRequest, HostCapabilities, SettingsRequest,
        };
        use tiler_core::world_report::PresentationRowV4;

        let mut engine = WorldEngine::new("gen-seed").expect("engine");
        let hs_reply = engine.handshake(&HandshakeRequest {
            adapter: "adapter-a".to_owned(),
            request: "hs-1".to_owned(),
            application_identity: ApplicationIdentity {
                app_id: None,
                executable: Some("test-own-executable".to_owned()),
                process_id: Some("test-own-pid".to_owned()),
            },
            capabilities: HostCapabilities::default(),
        });
        let gen_id = hs_reply.generation.expect("generation");
        engine
            .handle_settings(&SettingsRequest {
                adapter: "adapter-a".to_owned(),
                generation: gen_id.clone(),
                request: "set-1".to_owned(),
                revision: 0,
                values: Vec::new(),
            })
            .expect("settings");

        use tiler_core::world_engine::WorldOutcome;
        let fixture_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "../../test-fixtures/world-wire/v4-active/messages/invalid-witness-constant-string.json",
        );
        let base_text = std::fs::read_to_string(fixture_path).expect("held presentation fixture");
        let mut doc: serde_json::Value = serde_json::from_str(&base_text).expect("json");
        doc["adapter"] = serde_json::json!("adapter-a");
        doc["gen"] = serde_json::json!(gen_id);
        doc["reqSeq"] = serde_json::json!("1");
        doc["obs"] = serde_json::json!("1");
        doc["req"] = serde_json::json!("req-1");

        let decoded_a = decode_report_v4(&doc.to_string()).expect("held presentation decodes");
        assert!(matches!(
            decoded_a.report.presentation,
            PresentationRowV4::Held(_)
        ));
        assert!(decoded_a.residual.is_some());

        // Late prepare refusal must not install or retain replay residual.
        let gate_refuse = ProtocolAdmissionGate::with_residual(decoded_a.residual.as_ref());
        let err = engine
            .handle_report_with_gate_and_prepare(&decoded_a.report, &[], &gate_refuse, |_| {
                Err::<(), &'static str>("prepare-refused")
            })
            .expect_err("prepare error propagates");
        assert_eq!(err, "prepare-refused");
        assert_eq!(engine.state_revision(), 0);

        // Successful install retains residual.
        let outcome = handle_report_decoded_checked(&mut engine, &decoded_a, &[]).outcome;
        assert!(matches!(outcome, WorldOutcome::Accepted(_)));
        let rev_after_accept = engine.state_revision();
        assert_eq!(rev_after_accept, 1);

        // Whitespace-only difference in raw JSON decodes to equal residual -> Duplicate.
        let spaced_json = serde_json::to_string_pretty(&doc).expect("pretty json");
        let decoded_a_ws = decode_report_v4(&spaced_json).expect("spaced decodes");
        assert_eq!(decoded_a.residual, decoded_a_ws.residual);
        let ws_outcome = handle_report_decoded_checked(&mut engine, &decoded_a_ws, &[]).outcome;
        assert!(matches!(ws_outcome, WorldOutcome::Duplicate(_)));
        assert_eq!(engine.state_revision(), rev_after_accept);

        // Changed held presentation residual with identical typed HeldPresentationRow -> request-reused.
        let mut changed_doc = doc.clone();
        changed_doc["presentation"]["witness"] = serde_json::json!("other-constant");
        let decoded_b =
            decode_report_v4(&changed_doc.to_string()).expect("changed residual decodes");
        assert_eq!(decoded_a.report, decoded_b.report);
        assert_ne!(decoded_a.residual, decoded_b.residual);
        let reused_outcome = handle_report_decoded_checked(&mut engine, &decoded_b, &[]).outcome;
        assert_eq!(
            reused_outcome,
            WorldOutcome::Rejected {
                kind: "request-reused",
                message: "request sequence reused with different input",
            }
        );
        assert_eq!(engine.state_revision(), rev_after_accept);
    }

    #[test]
    fn held_screen_residual_discriminates_replay_whitespace_vs_changed_and_prepare_refusal() {
        use crate::world_protocol::decode_report_v4;
        use tiler_core::connection::{
            ApplicationIdentity, HandshakeRequest, HostCapabilities, SettingsRequest,
        };

        let mut engine = WorldEngine::new("gen-seed").expect("engine");
        let hs_reply = engine.handshake(&HandshakeRequest {
            adapter: "adapter-a".to_owned(),
            request: "hs-1".to_owned(),
            application_identity: ApplicationIdentity {
                app_id: None,
                executable: Some("test-own-executable".to_owned()),
                process_id: Some("test-own-pid".to_owned()),
            },
            capabilities: HostCapabilities::default(),
        });
        let gen_id = hs_reply.generation.expect("generation");
        engine
            .handle_settings(&SettingsRequest {
                adapter: "adapter-a".to_owned(),
                generation: gen_id.clone(),
                request: "set-1".to_owned(),
                revision: 0,
                values: Vec::new(),
            })
            .expect("settings");

        use tiler_core::world_engine::WorldOutcome;
        let fixture_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "../../test-fixtures/world-wire/v4-active/messages/invalid-screen-string-scale.json",
        );
        let base_text = std::fs::read_to_string(fixture_path).expect("held screen fixture");
        let mut doc: serde_json::Value = serde_json::from_str(&base_text).expect("json");
        doc["adapter"] = serde_json::json!("adapter-a");
        doc["gen"] = serde_json::json!(gen_id);
        doc["reqSeq"] = serde_json::json!("1");
        doc["obs"] = serde_json::json!("1");
        doc["req"] = serde_json::json!("req-1");
        // A report containing only a held screen is bootstrap-only; include
        // one usable screen to exercise installation into the replay record.
        let mut usable = doc["screens"][0].clone();
        usable["out"] = serde_json::json!("out-b");
        usable["scale"] = serde_json::json!(1.25);
        usable["identity"]["serial"] = serde_json::json!("serial-b");
        doc["screens"]
            .as_array_mut()
            .expect("screen array")
            .push(usable);

        let decoded_a = decode_report_v4(&doc.to_string()).expect("held screen decodes");
        assert_eq!(decoded_a.report.held_screens.len(), 1);
        let residual = decoded_a.residual.as_ref().expect("screen residual");
        assert!(residual.screen_fields(0).is_some());

        // Late prepare refusal must not install or retain replay residual.
        let gate_refuse = ProtocolAdmissionGate::with_residual(decoded_a.residual.as_ref());
        let err = engine
            .handle_report_with_gate_and_prepare(&decoded_a.report, &[], &gate_refuse, |_| {
                Err::<(), &'static str>("prepare-refused")
            })
            .expect_err("prepare error propagates");
        assert_eq!(err, "prepare-refused");
        assert_eq!(engine.state_revision(), 0);
        assert_eq!(Arc::strong_count(residual), 1);

        // Successful install retains residual.
        let outcome = handle_report_decoded_checked(&mut engine, &decoded_a, &[]).outcome;
        assert!(matches!(outcome, WorldOutcome::Accepted(_)));
        let rev_after_accept = engine.state_revision();
        assert_eq!(Arc::strong_count(residual), 2);

        // Whitespace-only difference in raw JSON decodes to equal residual -> Duplicate.
        let spaced_json = serde_json::to_string_pretty(&doc).expect("pretty json");
        let decoded_a_ws = decode_report_v4(&spaced_json).expect("spaced decodes");
        assert_eq!(decoded_a.residual, decoded_a_ws.residual);
        let ws_outcome = handle_report_decoded_checked(&mut engine, &decoded_a_ws, &[]).outcome;
        assert!(matches!(ws_outcome, WorldOutcome::Duplicate(_)));
        assert_eq!(engine.state_revision(), rev_after_accept);

        // Changed held screen residual with identical typed HeldScreenRow -> request-reused.
        let mut changed_doc = doc.clone();
        changed_doc["screens"][0]["scale"] = serde_json::json!("2.5");
        let decoded_b =
            decode_report_v4(&changed_doc.to_string()).expect("changed residual decodes");
        assert_eq!(decoded_a.report, decoded_b.report);
        assert_ne!(decoded_a.residual, decoded_b.residual);
        let reused_outcome = handle_report_decoded_checked(&mut engine, &decoded_b, &[]).outcome;
        assert_eq!(
            reused_outcome,
            WorldOutcome::Rejected {
                kind: "request-reused",
                message: "request sequence reused with different input",
            }
        );
        assert_eq!(engine.state_revision(), rev_after_accept);

        // Held window residual: same discrimination at reqSeq 2.
        let win_fixture_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "../../test-fixtures/world-wire/v4-active/messages/invalid-window-type-unknown.json",
        );
        let win_text = std::fs::read_to_string(win_fixture_path).expect("held window fixture");
        let mut win_doc: serde_json::Value = serde_json::from_str(&win_text).expect("json");
        win_doc["adapter"] = serde_json::json!("adapter-a");
        win_doc["gen"] = serde_json::json!(gen_id);
        win_doc["reqSeq"] = serde_json::json!("2");
        win_doc["obs"] = serde_json::json!("2");
        win_doc["req"] = serde_json::json!("req-2");

        let win_a = decode_report_v4(&win_doc.to_string()).expect("held window decodes");
        assert_eq!(win_a.report.held_windows.len(), 1);
        let win_residual = win_a.residual.as_ref().expect("window residual");
        assert!(win_residual.window_fields(0).is_some());

        let win_outcome = handle_report_decoded_checked(&mut engine, &win_a, &[]).outcome;
        assert!(matches!(win_outcome, WorldOutcome::Accepted(_)));
        let rev_after_win = engine.state_revision();
        assert_eq!(Arc::strong_count(win_residual), 2);

        let win_a_ws = decode_report_v4(&serde_json::to_string_pretty(&win_doc).expect("pretty"))
            .expect("spaced window decodes");
        assert_eq!(win_a.residual, win_a_ws.residual);
        let win_ws_outcome = handle_report_decoded_checked(&mut engine, &win_a_ws, &[]).outcome;
        assert!(matches!(win_ws_outcome, WorldOutcome::Duplicate(_)));
        assert_eq!(engine.state_revision(), rev_after_win);

        let mut win_changed_doc = win_doc.clone();
        win_changed_doc["windows"][0]["title"] = serde_json::json!("changed-title");
        let win_b = decode_report_v4(&win_changed_doc.to_string()).expect("changed window decodes");
        assert_eq!(win_a.report, win_b.report);
        assert_ne!(win_a.residual, win_b.residual);
        let win_reused_outcome = handle_report_decoded_checked(&mut engine, &win_b, &[]).outcome;
        assert_eq!(
            win_reused_outcome,
            WorldOutcome::Rejected {
                kind: "request-reused",
                message: "request sequence reused with different input",
            }
        );
        assert_eq!(engine.state_revision(), rev_after_win);
    }

    #[test]
    fn typed_admission_refuses_roster_feedback_on_non_roster_host() {
        use tiler_core::world_engine::WorldOutcome;

        let feedback_json = |generation: &str| {
            serde_json::json!({
                "op": "op-roster-1",
                "gen": generation,
                "rev": "1",
                "steps": [{
                    "step": "step-1",
                    "status": "appended",
                    "before": {"kind": "order", "order": []},
                    "after": {"kind": "order", "order": ["native-1"]},
                }],
            })
        };
        let roster_mismatch = WorldOutcome::Rejected {
            kind: "snapshot-invalid",
            message: "roster-host-mismatch",
        };

        let mut engine = WorldEngine::new_test_bound_with_capabilities(
            "gen-seed",
            "adapter-a",
            tiler_core::connection::HostCapabilities::default(),
        )
        .expect("engine");
        let generation = engine.core_generation().to_owned();
        let mut report = base_report();
        report.adapter_incarnation = "adapter-a".to_owned();
        report.core_generation = generation.clone();
        let feedback = crate::world_roster_wire::decode_feedback(&feedback_json(&generation))
            .expect("feedback decodes");
        let residual = Arc::new(WorldReportResidual {
            roster_feedback: Some(feedback),
            ..Default::default()
        });
        let gate = ProtocolAdmissionGate::with_residual(Some(&residual));
        let outcome = gate
            .dispatch_with_prepare(&mut engine, &report, &[], |_| {
                Ok::<(), std::convert::Infallible>(())
            })
            .expect("no prepare error")
            .outcome;
        assert_eq!(outcome, roster_mismatch);
        assert_eq!(engine.state_revision(), 0);

        let mut roster_engine = WorldEngine::new_test_bound_with_capabilities(
            "gen-seed-roster",
            "adapter-a",
            tiler_core::connection::HostCapabilities {
                host_owned: true,
                shared_native_roster: true,
                native_selector_scope: tiler_core::connection::NativeSelectorScope::PerOutput,
                ..tiler_core::connection::HostCapabilities::default()
            },
        )
        .expect("roster engine");
        let roster_generation = roster_engine.core_generation().to_owned();
        let mut roster_report = base_report();
        roster_report.adapter_incarnation = "adapter-a".to_owned();
        roster_report.core_generation = roster_generation.clone();
        let roster_feedback =
            crate::world_roster_wire::decode_feedback(&feedback_json(&roster_generation))
                .expect("roster feedback decodes");
        let roster_residual = Arc::new(WorldReportResidual {
            roster_feedback: Some(roster_feedback),
            ..Default::default()
        });
        let roster_gate = ProtocolAdmissionGate::with_residual(Some(&roster_residual));
        let roster_outcome = roster_gate
            .dispatch_with_prepare(&mut roster_engine, &roster_report, &[], |_| {
                Ok::<(), std::convert::Infallible>(())
            })
            .expect("no prepare error")
            .outcome;
        assert_ne!(roster_outcome, roster_mismatch);
    }
}
