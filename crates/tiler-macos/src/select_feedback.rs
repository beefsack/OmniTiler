#![forbid(unsafe_code)]

//! Product select handoff lane (claim-slot protocol).
//!
//! Split from [`crate::runtime`]: the armed select-only claim slot shared
//! between the engine (which stores one admitted claim per applied world
//! command) and the main-thread tile controller (which claims it,
//! submits it nonblocking to the serialized off-main Dock-swipe selection
//! owner, and reports the polled completion). The proven outcome reports
//! back under the claim's stable operation identity so the core settles
//! its planner-owned view target instead of retaining it. The drain
//! returns the typed setter disposition plus genuine native reads on every
//! path - the core owns every verdict from the staged row plus the
//! observation.

use crate::runtime::{RuntimeHandle, invalidate_clock, lock_shared};
use crate::runtime_thread::EngineJob;
use crate::runtime_types::ClaimedSelect;
use tiler_core::world_report::FrameSetter;

impl RuntimeHandle {
    /// Claim one pending product select for the main-thread drain: takes the
    /// pending claim (action plus, for indicator clicks, the immutable
    /// render-time SID/generation/revision binding) and marks the slot busy
    /// so concurrent engine enqueues serialize/refuse instead of overlapping
    /// (`None` when nothing is pending or a drain is already in progress).
    /// Short critical section only - never held across the blocking effect.
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
    /// (success, gate refusal, drift, or no effect - every path releases
    /// exactly once; a sent press is never replayed). Pair with
    /// [`RuntimeHandle::report_select_attempt`]: the drain reports its
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
    /// no retained target) plus the drain's typed setter disposition and
    /// genuine native Space reads around the actuation (`None` where
    /// unread - never guessed) and the observation-time baseline (`None`
    /// where unread at dispatch - never guessed). The core derives
    /// confirmation, refusal, ambiguity, and supersession from the row
    /// plus the staged observation - the adapter never sends verdicts.
    pub fn report_select_attempt(
        &self,
        operation_id: &str,
        setter: FrameSetter,
        pre_sid: Option<u64>,
        post_sid: Option<u64>,
        baseline_sid: Option<u64>,
    ) {
        if operation_id.is_empty() {
            return;
        }
        let job = EngineJob::ViewAttempt {
            operation_id: operation_id.to_owned(),
            setter,
            pre_sid,
            post_sid,
            baseline_sid,
        };
        // Bump before enqueue (see `invalidate_clock` in runtime).
        invalidate_clock(&self.shared);
        if !self.inbox.try_send_data(job) {
            lock_shared(&self.shared).diagnostics.data_dropped_full += 1;
        }
    }

    /// Proven outcome of one native creation leg, staged as a raw native
    /// creation attempt row for the next world report (same data-lane
    /// best-effort contract as [`RuntimeHandle::report_select_attempt`]).
    /// Call with the desire's operation identity (the core consumes only
    /// the first exact-correlated attempt per op) plus the create leg's
    /// typed setter and genuine native orders around it. `order_after`
    /// is the first readable full post-order, never a substituted preimage.
    /// `created_sid` names the verified
    /// appended desktop only (`None` on refusal/unknown, never guessed).
    /// The core owns binding and settlement over this plus the fresh
    /// `native[]` readback.
    #[allow(clippy::too_many_arguments)]
    pub fn report_creation_attempt(
        &self,
        operation_id: &str,
        logical: &str,
        output: &str,
        setter: FrameSetter,
        order_before: &[u64],
        order_after: &[u64],
        created_sid: Option<u64>,
        attempt_ms: u64,
    ) {
        use tiler_core::directional::OutputId;
        if operation_id.is_empty() || logical.is_empty() {
            return;
        }
        let job = EngineJob::NativeCreation {
            operation_id: operation_id.to_owned(),
            logical: logical.to_owned(),
            output: OutputId(output.to_owned()),
            setter,
            order_before: order_before.to_vec(),
            order_after: order_after.to_vec(),
            created_sid,
            attempt_ms,
        };
        // Bump before enqueue (see `invalidate_clock` in runtime).
        invalidate_clock(&self.shared);
        if !self.inbox.try_send_data(job) {
            lock_shared(&self.shared).diagnostics.data_dropped_full += 1;
        }
    }

    /// Proven outcome of one settled live send, staged as raw
    /// membership-attempt (plus, for follows, raw follow-view-attempt) rows
    /// for the next world report (same data-lane best-effort contract as
    /// [`RuntimeHandle::report_select_attempt`]). Called by the tile for
    /// settled effect completions and submit-time refusals. `setter` is the
    /// owner's raw native disposition captured at the actual write boundary
    /// — staged verbatim, never derived from any arrival outcome.
    /// `pre_member` is the genuine pre-write homing read around the dispatch
    /// (`None` unread - never guessed); `post_member` is the
    /// drain-verified arrival Space (`Some` only on a verified exact
    /// arrival, `None` otherwise - never guessed). Follows additionally
    /// stage the genuine view-current reads plus the observation-time
    /// baseline under `follow_view_operation` — the reply-published follow
    /// view flight identity carried by the claim (the existing view-attempt
    /// lane correlates it against that retained View transfer, dropping
    /// silently when the core minted no follow flight, as for stays);
    /// `None` stages no view and invents none. The core settles its retained window
    /// transfer (plus the follow view flight) from this plus the staged
    /// observation. Call with the `operation_id` of the drained claim (the
    /// engine ignores ids naming no retained transfer).
    #[allow(clippy::too_many_arguments)]
    pub fn report_send_outcome(
        &self,
        operation_id: &str,
        setter: FrameSetter,
        pre_member: Option<u64>,
        post_member: Option<u64>,
        pre_view: Option<u64>,
        post_view: Option<u64>,
        baseline_sid: Option<u64>,
        follow_view_operation: Option<&str>,
    ) {
        if operation_id.is_empty() {
            return;
        }
        let job = EngineJob::MemberAttempt {
            operation_id: operation_id.to_owned(),
            setter,
            pre_sid: pre_member,
            post_sid: post_member,
        };
        // Bump before enqueue (see `invalidate_clock` in runtime).
        invalidate_clock(&self.shared);
        if !self.inbox.try_send_data(job) {
            lock_shared(&self.shared).diagnostics.data_dropped_full += 1;
        }
        if let Some(follow_op) = follow_view_operation {
            self.report_select_attempt(follow_op, setter, pre_view, post_view, baseline_sid);
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
                    pre_sid: None,
                    post_sid: None,
                }
            } else {
                EngineJob::ViewAttempt {
                    operation_id: operation_id.to_owned(),
                    setter: FrameSetter::Refused,
                    pre_sid: None,
                    post_sid: None,
                    baseline_sid: None,
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
