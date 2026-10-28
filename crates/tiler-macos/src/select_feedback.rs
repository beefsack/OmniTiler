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

use std::collections::VecDeque;

use crate::runtime::{RuntimeHandle, invalidate_clock, lock_shared};
use crate::runtime_thread::EngineJob;
use crate::runtime_types::ClaimedSelect;
use tiler_core::world_report::FrameSetter;

impl RuntimeHandle {
    /// Claim the oldest pending product select for the main-thread drain:
    /// pops the queue front (action plus, for indicator clicks, the
    /// immutable render-time SID/generation/revision binding) and marks
    /// the slot busy so concurrent claims serialize instead of overlapping
    /// (`None` when the queue is empty or a drain is already in progress).
    /// Queued entries wait instead of refusing: every admitted command
    /// drains oldest-first with no loss, no duplication, and no
    /// reordering. Short critical section only - never held across the
    /// blocking effect.
    /// Taking a claim consumes the publish wake (`select_claim_pending`)
    /// only when the queue empties: the wake that ran this body early is
    /// spent for an empty queue, while a still-queued entry keeps the next
    /// pump early via the release re-arm — nothing is lost either way.
    /// Pair every `Some` with [`RuntimeHandle::release_select_after_drain`].
    pub fn claim_select_for_drain(&self) -> Option<ClaimedSelect> {
        let mut guard = lock_shared(&self.shared);
        if guard.select_busy {
            return None;
        }
        let claim = guard.select_pending.pop_front()?;
        guard.select_busy = true;
        guard.select_claim_pending = !guard.select_pending.is_empty();
        Some(claim)
    }

    /// Consume the published-claim wake hint once: true when the engine
    /// stored an actionable claim since the last take (or the last claim,
    /// drop, or settle that cleared it). The host-woken main loop checks
    /// this on every iteration to run the cadence body early; the drain
    /// site clears it by claiming. Coalesced across bursts; never blocks.
    /// Steady state (no publication) stays quiet, so idle polling never
    /// quickens.
    pub fn take_select_claim_pending(&self) -> bool {
        std::mem::replace(&mut lock_shared(&self.shared).select_claim_pending, false)
    }

    /// Release the select claim after the main-thread drain settles
    /// (success, gate refusal, drift, or no effect - every path releases
    /// exactly once; a sent press is never replayed). When entries are
    /// still queued, the publish wake re-arms plus the shared main wake
    /// signals (flag-set then signal), so the next drain runs on the
    /// completion event instead of waiting out the cadence: FIFO dequeue
    /// on causal completion. Pair with
    /// [`RuntimeHandle::report_select_attempt`]: the drain reports its
    /// proven native outcome under the claim's operation identity so the
    /// core settles the planner-owned view target (never replayed).
    pub fn release_select_after_drain(&self) {
        let wake = {
            let mut guard = lock_shared(&self.shared);
            guard.select_busy = false;
            if guard.select_pending.is_empty() {
                None
            } else {
                guard.select_claim_pending = true;
                guard.main_wake.clone()
            }
        };
        if let Some(wake) = wake {
            wake.signal();
        }
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

    /// Release one deferred selection focus after the native selection
    /// proved arrival (main thread, on the matching select completion's
    /// verified post): the post must name the bound native Space, the app
    /// must be live at its bound generation, and the foreground must not
    /// have moved to another app. Anything else drops the intent counted
    /// instead of dispatching stale; a repeat completion finds nothing (no
    /// replay). See
    /// [`crate::world_dispatch::release_selection_focus_after_arrival`].
    pub(crate) fn release_selection_focus(
        &self,
        completion_op: Option<&str>,
        post_sid: Option<u64>,
        frontmost_pid: Option<u32>,
    ) -> crate::world_dispatch::SelectionFocusRelease {
        crate::world_dispatch::release_selection_focus_after_arrival(
            &self.shared,
            &self.focus_queue,
            completion_op,
            post_sid,
            frontmost_pid,
        )
    }

    /// Drop any deferred selection focus naming one operation (refused
    /// select completion): arrival never proved, so the intent must never
    /// fire. Returns true when an intent was dropped.
    pub fn drop_selection_focus(&self, operation_id: Option<&str>) -> bool {
        crate::world_dispatch::drop_selection_focus_for_operation(&self.shared, operation_id)
    }

    /// Drop all queued (unclaimed) product selects without dispatching: the
    /// lock/secure/stale-topology paths refuse narrowly instead of running
    /// switches under an unsafe context. Returns whether any were dropped.
    /// Never touches a drain in progress. Every dropped bound claim stages
    /// a raw refusal row (best-effort) so the core holds its planner-owned
    /// target instead of retaining it blindly: each row carries no reads
    /// (the drain never touched the desktop), so the core latches hold
    /// without re-actuation rather than settling a verdict.
    pub fn discard_select_pending(&self) -> bool {
        let dropped: VecDeque<ClaimedSelect> = {
            let mut guard = lock_shared(&self.shared);
            if guard.select_busy {
                return false;
            }
            guard.select_claim_pending = false;
            std::mem::take(&mut guard.select_pending)
        };
        if dropped.is_empty() {
            return false;
        }
        for claim in &dropped {
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
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::Arc;
    use std::time::Instant;

    use crate::runtime::Shared;
    use crate::runtime_types::{InputSemanticAction, RuntimeConfig};
    use crate::world_sequencer::ObservationClock;

    struct StubAx;
    impl crate::runtime_backend::AxBackendFactory for StubAx {
        fn create(&self, _pid: u32) -> Result<Box<dyn crate::runtime_backend::AxBackend>, String> {
            Err("test stub creates nothing".to_owned())
        }
    }

    fn test_handle() -> RuntimeHandle {
        let shared = Arc::new(std::sync::Mutex::new(Shared {
            config: RuntimeConfig::default(),
            handles_live: 0,
            generation_counter: 0,
            log_counter: 0,
            start: Instant::now(),
            apps: BTreeMap::new(),
            clock: Arc::new(ObservationClock::new()),
            midflight_hook: std::sync::Mutex::new(None),
            post_accept_hook: std::sync::Mutex::new(None),
            topology: None,
            display_inputs: None,
            select_wake: None,
            main_wake: None,
            topology_revision: 0,
            native_context: None,
            native_cell: crate::runtime_shared::NativeContextCell::new(),
            select_pending: VecDeque::new(),
            select_busy: false,
            select_claim_pending: false,
            selection_focus_pending: None,
            selection_focus_floor_seq: 0,
            focus_evidence: None,
            zombies: Vec::new(),
            engine_handle: None,
            input_handle: None,
            logger_handle: None,
            input_thread: None,
            terminal: None,
            admission_closed: false,
            stop_engine: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            stop_input: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            stop_logger: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            log_tx: None,
            diagnostics: crate::runtime_types::RuntimeDiagnostics::default(),
            log: VecDeque::new(),
            last_desired: Vec::new(),
            test_wake: None,
            test_seq: 0,
            last_geometry_key: None,
            last_presentation: tiler_core::world_presentation::WorldPresentation::default(),
            last_pager_bindings: Vec::new(),
            presentation_style: crate::active_border::PresentationStylePx::default(),
        }));
        RuntimeHandle {
            shared: Arc::clone(&shared),
            inbox: Arc::new(crate::runtime_thread::Inbox::new(16)),
            focus_queue: Arc::new(std::sync::Mutex::new(Vec::new())),
            ax_factory: Arc::new(StubAx),
        }
    }

    /// Engine-publication shape: the store sets the slot and the wake under
    /// one lock, so a take and a claim never disagree about what is pending.
    fn publish(handle: &RuntimeHandle, operation_id: &str) {
        let mut guard = lock_shared(&handle.shared);
        let mut claim = ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 2 });
        claim.operation_id = Some(operation_id.to_owned());
        guard.select_pending.push_back(claim);
        guard.select_claim_pending = true;
    }

    #[test]
    fn claim_wake_quiets_steady_state_and_spends_once() {
        // Steady state (no publication) wakes nothing: the gate stays shut
        // and the body runs only on the cadence backstop.
        let handle = test_handle();
        assert!(!handle.take_select_claim_pending());
        assert!(handle.claim_select_for_drain().is_none());
        assert!(!handle.take_select_claim_pending());
    }

    #[test]
    fn claim_between_check_and_drain_is_not_lost() {
        // The take consumes the wake while the slot stays the truth: a claim
        // stored before the take still drains after it (check-then-drain
        // loses nothing), and the claim take clears the wake so no spurious
        // extra body follows.
        let handle = test_handle();
        publish(&handle, "op-1");
        assert!(handle.take_select_claim_pending(), "publication wakes once");
        assert!(
            !handle.take_select_claim_pending(),
            "wake consumed at most once"
        );
        let claimed = handle
            .claim_select_for_drain()
            .expect("slot is truth: consumed wake loses no claim");
        assert_eq!(claimed.operation_id.as_deref(), Some("op-1"));
        assert!(
            !handle.take_select_claim_pending(),
            "claim take leaves no residual wake"
        );
        handle.release_select_after_drain();
        // A later publication re-arms (no stuck latch).
        publish(&handle, "op-2");
        assert!(handle.take_select_claim_pending());
    }

    #[test]
    fn release_rearm_pokes_installed_main_wake() {
        // Production completion publisher: releasing the claim after the
        // drain re-arms the sticky flag AND pokes the installed main wake
        // (flag-set then signal) when entries stay queued, so the next
        // drain runs on the completion event. An empty queue re-arms
        // nothing and pokes nothing. Deterministic: state only, no timing.
        let handle = test_handle();
        let signal = crate::ffi::MainWakeSignal::detached();
        lock_shared(&handle.shared).main_wake = Some(signal.clone());
        publish(&handle, "op-1");
        publish(&handle, "op-2");
        assert!(handle.take_select_claim_pending());
        handle
            .claim_select_for_drain()
            .expect("oldest claim drains first");
        let before = signal.generation();
        handle.release_select_after_drain();
        assert!(
            lock_shared(&handle.shared).select_claim_pending,
            "queued remainder re-arms the sticky flag"
        );
        assert_eq!(
            signal.generation(),
            before + 1,
            "re-arm pokes the installed wake exactly once"
        );
        handle
            .claim_select_for_drain()
            .expect("second claim drains next");
        let drained = signal.generation();
        handle.release_select_after_drain();
        assert!(
            !lock_shared(&handle.shared).select_claim_pending,
            "empty queue re-arms nothing"
        );
        assert_eq!(signal.generation(), drained, "empty release pokes nothing");
    }

    #[test]
    fn queued_publications_drain_oldest_first_and_discard_clears() {
        // FIFO: burst publications queue in order behind one coalesced
        // wake; the drain takes the oldest first, then the next.
        let handle = test_handle();
        publish(&handle, "op-1");
        publish(&handle, "op-2");
        assert!(
            handle.take_select_claim_pending(),
            "bursts coalesce into one wake"
        );
        assert!(!handle.take_select_claim_pending());
        let first = handle
            .claim_select_for_drain()
            .expect("oldest claim drains first");
        assert_eq!(first.operation_id.as_deref(), Some("op-1"));
        // A queued remainder keeps the wake armed past the claim; release
        // re-arms for the next pump and the second claim drains next.
        assert!(
            handle.take_select_claim_pending(),
            "queued remainder keeps the wake armed"
        );
        handle.release_select_after_drain();
        let second = handle
            .claim_select_for_drain()
            .expect("second claim drains next");
        assert_eq!(second.operation_id.as_deref(), Some("op-2"));
        handle.release_select_after_drain();
        // Discard drops every queued claim at once (multi-drop) and clears
        // the wake, so the pump never runs an early body for claims that
        // will never drain. A drain in progress is never touched.
        publish(&handle, "op-3");
        publish(&handle, "op-4");
        assert!(handle.discard_select_pending(), "queued claims dropped");
        assert!(
            !handle.take_select_claim_pending(),
            "drop clears the wake: no lost-wake gap, no spurious early body"
        );
        assert!(handle.claim_select_for_drain().is_none());
        assert!(!handle.discard_select_pending(), "empty drop stays quiet");
    }

    #[test]
    fn queued_claim_waits_behind_busy_drain_and_releases_in_order() {
        // While a drain holds the front claim, a queued claim waits (never
        // lost, never overlapping): the concurrent claim returns None, the
        // queued remainder keeps the wake armed, and release dequeues it
        // next in order.
        let handle = test_handle();
        publish(&handle, "op-1");
        assert!(handle.take_select_claim_pending());
        let first = handle.claim_select_for_drain().expect("front drains");
        assert_eq!(first.operation_id.as_deref(), Some("op-1"));
        assert!(handle.claim_select_for_drain().is_none(), "busy serializes");
        publish(&handle, "op-2");
        assert!(handle.claim_select_for_drain().is_none(), "still busy");
        assert!(
            handle.take_select_claim_pending(),
            "queued remainder keeps the wake armed behind the drain"
        );
        handle.release_select_after_drain();
        let second = handle
            .claim_select_for_drain()
            .expect("queued claim drains after release");
        assert_eq!(second.operation_id.as_deref(), Some("op-2"));
        handle.release_select_after_drain();
        assert!(handle.claim_select_for_drain().is_none());
    }
}
