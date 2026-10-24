#![forbid(unsafe_code)]

//! Product select handoff lane (claim-slot protocol).
//!
//! Split from [`crate::runtime`]: the armed select-only claim slot shared
//! between the engine (which stores one admitted claim per applied world
//! command) and the main-thread tile controller (which claims it,
//! submits it nonblocking to the serialized off-main Dock-swipe selection
//! owner, and reports the polled completion). The proven outcome reports
//! back under the claim's stable operation identity so the core settles
//! its planner-owned view target instead of retaining it. Refusals
//! classify honestly: only pre-actuation refusals (no owner actuation —
//! the owner never took the job, or gates/drift refused first) settle as
//! proven refusals — ambiguous mid-actuation output holds without
//! re-actuation.

use crate::runtime::{RuntimeHandle, invalidate_clock, lock_shared};
use crate::runtime_thread::EngineJob;
use crate::runtime_types::ClaimedSelect;

/// Proven outcome of one main-thread select drain, reported back under
/// the drained claim's operation identity (see
/// [`RuntimeHandle::report_select_outcome`]). Only recorded outcomes
/// cross: pre-actuation refusals (gates, drift, stale binding) report
/// `Refused`; ambiguous native effects report `PossiblyExecuted` (held,
/// never blindly repeated); verified switches report `Applied`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectDrainOutcome {
    Applied,
    Refused,
    PossiblyExecuted,
}

impl RuntimeHandle {
    /// Claim one pending product select for the main-thread drain: takes the
    /// pending claim (action plus, for indicator clicks, the immutable
    /// render-time SID/generation/revision binding) and marks the slot busy
    /// so concurrent engine enqueues serialize/refuse instead of overlapping
    /// (`None` when nothing is pending or a drain is already in progress).
    /// Short critical section only — never held across the blocking effect.
    /// Pair every `Some` with [`RuntimeHandle::release_select_after_drain`].
    pub fn claim_select_for_drain(&self) -> Option<ClaimedSelect> {
        let mut guard = lock_shared(&self.shared);
        if guard.select_busy || guard.select_pending.is_none() {
            return None;
        }
        guard.select_busy = true;
        guard.select_pending.take()
    }

    /// Release the select claim after the main-thread drain settles
    /// (success, gate refusal, drift, or no effect — every path releases
    /// exactly once; a sent press is never replayed). Pair with
    /// [`RuntimeHandle::report_select_outcome`]: the drain reports its
    /// proven native outcome under the claim's operation identity so the
    /// core settles the planner-owned view target (never replayed).
    pub fn release_select_after_drain(&self) {
        lock_shared(&self.shared).select_busy = false;
    }

    /// Proven outcome of one main-thread select drain, staged as a raw
    /// view-attempt row for the next world report (data lane, best-effort:
    /// a full or closed queue drops counted, and the retained view target
    /// settles on the next select or observation instead). Call with the
    /// `operation_id` of the drained claim (the engine ignores ids naming
    /// no retained target) plus the drain-verified native Space reads
    /// around the actuation (`None` where unread — never guessed).
    /// `Applied` carries the verified before/after Space ids; every other
    /// outcome carries no reads. The core derives confirmation, refusal,
    /// ambiguity, and supersession from the row plus the staged
    /// observation — the adapter never sends verdicts.
    ///
    /// [`SelectDrainOutcome`] maps to the setter 1:1 (`Applied` is
    /// `Accepted`, pre-actuation refusal is `Refused`, ambiguity is
    /// `Threw`); classification stays in
    /// [`classify_select_drain_error`].
    pub fn report_select_outcome(
        &self,
        operation_id: &str,
        outcome: SelectDrainOutcome,
        pre_sid: Option<u64>,
        post_sid: Option<u64>,
    ) {
        if operation_id.is_empty() {
            return;
        }
        use tiler_core::world_report::FrameSetter;
        let setter = match outcome {
            SelectDrainOutcome::Applied => FrameSetter::Accepted,
            SelectDrainOutcome::Refused => FrameSetter::Refused,
            SelectDrainOutcome::PossiblyExecuted => FrameSetter::Threw,
        };
        let job = EngineJob::ViewAttempt {
            operation_id: operation_id.to_owned(),
            setter,
            pre_sid,
            post_sid,
        };
        // Bump before enqueue (see `invalidate_clock` in runtime).
        invalidate_clock(&self.shared);
        if !self.inbox.try_send_data(job) {
            lock_shared(&self.shared).diagnostics.data_dropped_full += 1;
        }
    }

    /// Proven outcome of one settled live send, staged as a raw
    /// membership-attempt row for the next world report (same data-lane
    /// best-effort contract as [`RuntimeHandle::report_select_outcome`]).
    /// Called by the tile for settled effect completions and submit-time
    /// refusals (unsupported send shapes refuse engine-side and never reach
    /// here). `post_sid` is the drain-verified arrival Space on `Applied`
    /// only (`None` otherwise — never guessed); the core settles its
    /// retained window transfer from this plus the staged observation.
    /// Call with the `operation_id` of the drained claim (the engine
    /// ignores ids naming no retained transfer).
    pub fn report_send_outcome(
        &self,
        operation_id: &str,
        outcome: crate::workspace_send::SendDrainOutcome,
        post_sid: Option<u64>,
    ) {
        if operation_id.is_empty() {
            return;
        }
        let setter = crate::workspace_send::setter_for(outcome);
        let job = EngineJob::MemberAttempt {
            operation_id: operation_id.to_owned(),
            setter,
            post_sid,
        };
        // Bump before enqueue (see `invalidate_clock` in runtime).
        invalidate_clock(&self.shared);
        if !self.inbox.try_send_data(job) {
            lock_shared(&self.shared).diagnostics.data_dropped_full += 1;
        }
    }

    /// Drop a pending (unclaimed) product select without dispatching: the
    /// lock/secure/stale-topology paths refuse narrowly instead of running
    /// a switch under an unsafe context. Returns whether one was dropped.
    /// Never touches a drain in progress. A dropped bound claim stages a
    /// raw refusal row (best-effort) so the core holds its planner-owned
    /// target instead of retaining it blindly: the row carries no reads
    /// (the drain never touched the desktop), so the core latches hold
    /// without re-actuation rather than settling a verdict.
    pub fn discard_select_pending(&self) -> bool {
        let dropped: Option<ClaimedSelect> = {
            let mut guard = lock_shared(&self.shared);
            if guard.select_busy {
                return false;
            }
            guard.select_pending.take()
        };
        let Some(claim) = dropped else {
            return false;
        };
        if let Some(operation_id) = claim.operation_id.as_deref()
            && !operation_id.is_empty()
        {
            use tiler_core::world_report::FrameSetter;
            let job = if matches!(
                claim.action,
                crate::runtime_types::InputSemanticAction::WorkspaceSend { .. }
            ) {
                EngineJob::MemberAttempt {
                    operation_id: operation_id.to_owned(),
                    setter: FrameSetter::Refused,
                    post_sid: None,
                }
            } else {
                EngineJob::ViewAttempt {
                    operation_id: operation_id.to_owned(),
                    setter: FrameSetter::Refused,
                    pre_sid: None,
                    post_sid: None,
                }
            };
            // Bump before enqueue (see `invalidate_clock` in runtime).
            invalidate_clock(&self.shared);
            if !self.inbox.try_send_data(job) {
                lock_shared(&self.shared).diagnostics.data_dropped_full += 1;
            }
        }
        true
    }
}

/// Classify one select-drain failure for attempt feedback: prefixes naming
/// pre-actuation refusals (gates, unarmed, display count, stale binding,
/// unreadable fresh rows — the drain never touched the desktop, most
/// strings say so explicitly) settle as proven refusals; every other
/// backend/execute failure is ambiguous mid-actuation output and holds as
/// possibly-executed (never blindly repeated). Unknown strings fail safe
/// toward the hold.
///
/// macOS-only helper (plus unit tests): the sole production caller lives
/// in the macOS-gated native tile controller, so other targets exclude
/// this definition instead of carrying dead code under `-D warnings`.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn classify_select_drain_error(reason: &str) -> SelectDrainOutcome {
    for prefix in [
        "select gated:",
        "product select refuses",
        "bound select claim",
        "bound select refuses:",
        "stale bound select discarded:",
        "expected exactly one native display",
        "dock-swipe refused:",
        "select refused:",
        "select-busy:",
    ] {
        if reason.starts_with(prefix) {
            return SelectDrainOutcome::Refused;
        }
    }
    SelectDrainOutcome::PossiblyExecuted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusal_prefixes_settle_proven_and_unknown_holds() {
        // Every known pre-actuation refusal prefix (gates, unarmed,
        // display count, stale binding/drift, unreadable fresh rows)
        // settles as a proven refusal: the drain never touched the
        // desktop.
        for reason in [
            "select gated: session not unlocked",
            "select gated: Secure Input active",
            "select gated: lock screen",
            "product select refuses unarmed actions",
            "product select refuses: expected exactly one native display, found 2",
            "product select refuses: display count unreadable (row missing)",
            "product select refuses: caffeinate unspawnable (spawn failed)",
            "bound select claim carries a non-numbered action; refusing",
            "bound select refuses: fresh row unreadable (snapshot failed)",
            "stale bound select discarded: native row moved between admission and drain (row 2 changed); no select dispatched",
            "stale bound select discarded: topology revision moved between admission and drain (3 != 4); no select dispatched",
            "dock-swipe refused: session not unlocked pre-dispatch",
            "dock-swipe refused: expected exactly one native display, found 2",
            "dock-swipe refused: pinned display UUID no longer names the SLS row",
            "dock-swipe refused: cancelled before dispatch",
            "select refused: session not unlocked pre-dispatch",
            "select-busy: one Dock-swipe select already inflight; refusing overlap",
        ] {
            assert_eq!(
                classify_select_drain_error(reason),
                SelectDrainOutcome::Refused,
                "{reason}"
            );
        }
        // Backend/execute failures are ambiguous mid-actuation output and
        // hold without re-actuation; unknown strings fail safe toward the
        // hold, never toward a false proven refusal.
        for reason in [
            "workspace row/current identity unprovable",
            "display row missing",
            "previous workspace no longer exists",
            "topology drifted during open (was display 1 index 0, now 1/2); refusing stale dispatch",
            "dock-swipe unproven: posted 2 pairs but native current reads Some(22) (target 33); history unchanged; no replay",
            "dock-swipe unproven: began-without-ended partial (began=1 ended=0) release=posted; no replay",
            "dock-swipe unproven: cancelled during settlement; history unchanged; no replay",
            "",
            "select gated",
            "product select",
            "bound select",
            "dock-swipe",
        ] {
            assert_eq!(
                classify_select_drain_error(reason),
                SelectDrainOutcome::PossiblyExecuted,
                "{reason}"
            );
        }
    }
}
