#![forbid(unsafe_code)]

//! Product select handoff lane (main-thread drain protocol).
//!
//! Split from [`crate::runtime`]: the armed select-only claim slot shared
//! between the engine (which stores one admitted claim per applied world
//! command) and the main-thread tile controller (which claims, drains,
//! and reports it). Draining actuates natively on the main thread; the
//! proven outcome reports back under the claim's stable operation identity
//! so the core settles its planner-owned view target instead of retaining
//! it. Refusals classify honestly: only pre-actuation refusals (the drain
//! never touched the desktop) settle as proven refusals — ambiguous
//! mid-actuation output holds without re-actuation.

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

    /// Proven outcome of one main-thread select drain, staged as attempt
    /// feedback for the next world report (data lane, best-effort: a full
    /// or closed queue drops counted, and the retained view target settles
    /// on the next select or observation instead). Call with the
    /// `operation_id` of the drained claim (the core ignores ids naming no
    /// retained target).
    /// `Applied` settles on observation confirm; `Refused` settles to
    /// observed without actuation; `PossiblyExecuted` holds the target
    /// without re-actuation (ambiguous native effect — never blindly
    /// repeated).
    pub fn report_select_outcome(&self, operation_id: &str, outcome: SelectDrainOutcome) {
        if operation_id.is_empty() {
            return;
        }
        use tiler_core::world_report::{AttemptKind, AttemptOutcome};
        let outcome = match outcome {
            SelectDrainOutcome::Applied => AttemptOutcome::Applied,
            SelectDrainOutcome::Refused => AttemptOutcome::RefusedProven,
            SelectDrainOutcome::PossiblyExecuted => AttemptOutcome::PossiblyExecuted,
        };
        let job = EngineJob::AttemptFeedback {
            operation_id: operation_id.to_owned(),
            kind: AttemptKind::View,
            outcome,
            hold: None,
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
    /// Never touches a drain in progress. A dropped bound claim stages
    /// proven-refusal feedback (best-effort) so the core settles its
    /// planner-owned view target instead of retaining it.
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
            use tiler_core::world_report::{AttemptKind, AttemptOutcome};
            let job = EngineJob::AttemptFeedback {
                operation_id: operation_id.to_owned(),
                kind: AttemptKind::View,
                outcome: AttemptOutcome::RefusedProven,
                hold: None,
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
pub(crate) fn classify_select_drain_error(reason: &str) -> SelectDrainOutcome {
    for prefix in [
        "select gated:",
        "product select refuses",
        "bound select claim",
        "bound select refuses:",
        "stale bound select discarded:",
        "expected exactly one native display",
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
            "",
            "select gated",
            "product select",
            "bound select",
        ] {
            assert_eq!(
                classify_select_drain_error(reason),
                SelectDrainOutcome::PossiblyExecuted,
                "{reason}"
            );
        }
    }
}
